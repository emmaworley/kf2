//! Manual tester for the DAM provider.
//!
//! Drives `server::provider::dam` in-process so proxy settings apply to the
//! real upstream HTTP calls DAM makes. The damtomoId and auth token from a
//! successful login are persisted to `./.dam-tool.json` by
//! `dam-tool login --write` so follow-up invocations don't need to re-auth.
//! This tool bypasses the gRPC server, the provider cache, and the database
//! entirely.

use std::fs;
use std::io;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result, anyhow};
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use server::provider::dam::{DamConfig, DamProvider, DamProviderSession, LoginData};
use server::provider::{ProviderSession, Searchable};

const DOTFILE_NAME: &str = ".dam-tool.json";

/// On-disk shape for `./.dam-tool.json`. Tool-local so the wire types in
/// `server::provider::dam` don't need `Serialize` just to be persisted here.
#[derive(Debug, Serialize, Deserialize)]
struct StoredSession {
    /// The login username. Sent as `userCode` in Minsei requests — see
    /// `DamProviderSession::user_code`.
    user_code: String,
    damtomo_id: String,
    auth_token: String,
}

impl StoredSession {
    fn from_login(user_code: String, data: &LoginData) -> Self {
        Self {
            user_code,
            damtomo_id: data.damtomo_id.clone(),
            auth_token: data.auth_token.clone(),
        }
    }
}

#[derive(Parser, Debug)]
#[command(
    name = "dam-tool",
    about = "CLI tool for DAM",
    long_about = "DAM provider direct query utility. \
                  Session tokens persist in ./.dam-tool.json via `login --write`."
)]
struct Cli {
    /// Upstream HTTP proxy, e.g. http://127.0.0.1:8080. Applies to this
    /// invocation only — not persisted.
    #[arg(long, env = "DAM_PROXY", global = true)]
    proxy: Option<String>,

    /// Username for proxy basic auth. Requires --proxy.
    #[arg(long, env = "DAM_PROXY_USER", global = true, requires = "proxy")]
    proxy_user: Option<String>,

    /// Password for proxy basic auth. Requires --proxy-user.
    #[arg(long, env = "DAM_PROXY_PASS", global = true, requires = "proxy_user")]
    proxy_pass: Option<String>,

    /// Log every DAM HTTP request and response to stderr. Sensitive values
    /// (password, authKey, authToken) are redacted in request logs; response
    /// bodies are printed verbatim and may contain your session tokens, so
    /// avoid pasting them publicly.
    #[arg(short, long, global = true)]
    debug: bool,

    #[command(subcommand)]
    cmd: Commands,
}

#[derive(Debug, Clone, Copy)]
struct ProxyArgs<'a> {
    url: Option<&'a str>,
    user: Option<&'a str>,
    pass: Option<&'a str>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Log in to DAM and print the response. Use `--write` to persist the
    /// credentials to ./.dam-tool.json on success.
    Login {
        #[arg(short, long, env = "DAM_USERNAME")]
        username: String,
        #[arg(short, long, env = "DAM_PASSWORD", num_args = 0..=1, default_missing_value = "")]
        password: Option<String>,
        /// Persist the credentials to ./.dam-tool.json on successful login.
        #[arg(long)]
        write: bool,
    },
    /// Print current proxy and stored-credential state.
    Status,

    /// Search songs by query string.
    SearchSongs {
        query: String,
        /// Zero-indexed page number (page 0 is the first page).
        #[arg(long, default_value_t = 0)]
        page: u32,
    },
    /// Search artists by query string.
    SearchArtists {
        query: String,
        /// Zero-indexed page number (page 0 is the first page).
        #[arg(long, default_value_t = 0)]
        page: u32,
    },
    /// List songs for a given artist id.
    SongsByArtist {
        artist_id: String,
        /// Zero-indexed page number (page 0 is the first page).
        #[arg(long, default_value_t = 0)]
        page: u32,
    },
    /// Fetch a song's metadata by id.
    GetSong { song_id: String },
    /// Fetch a song's media stream URL(s) by id.
    GetStream { song_id: String },
    /// Fetch a song's scoring / piano-roll data by id.
    GetScoring { song_id: String },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    dispatch(cli).await
}

async fn dispatch(cli: Cli) -> Result<()> {
    let Cli {
        proxy: proxy_url,
        proxy_user,
        proxy_pass,
        debug,
        cmd,
    } = cli;
    if debug {
        // The server::provider::dam module reads DAM_DEBUG once via
        // LazyLock, so this must be set before the first request fires.
        // SAFETY: dam-tool is single-threaded before dispatch.
        unsafe { std::env::set_var("DAM_DEBUG", "1") };
    }
    let proxy = ProxyArgs {
        url: proxy_url.as_deref(),
        user: proxy_user.as_deref(),
        pass: proxy_pass.as_deref(),
    };
    match cmd {
        Commands::Login {
            username,
            password,
            write,
        } => {
            let password = match password.as_deref() {
                Some(p) if !p.is_empty() => p.to_string(),
                _ => rpassword::prompt_password(format!("DAM password for {username}: "))
                    .context("reading password from stdin")?,
            };
            let client = build_client(proxy)?;
            let cfg = DamConfig { username, password };
            let resp = DamProvider::login(&client, &cfg).await?;
            println!("{resp:#?}");
            if write {
                save_session(&StoredSession::from_login(cfg.username.clone(), &resp.data))?;
                eprintln!("warning: wrote auth token to {}", dotfile_path().display());
            }
            Ok(())
        }
        Commands::Status => {
            println!("proxy:       {}", proxy.url.unwrap_or("<not set>"));
            println!(
                "proxy auth:  {}",
                match (proxy.user, proxy.pass) {
                    (Some(u), Some(_)) => format!("username={u}"),
                    (Some(u), None) => format!("username={u} (no password)"),
                    (None, _) => "<none>".into(),
                }
            );
            let path = dotfile_path();
            println!(
                "dotfile:     {}",
                path.canonicalize().unwrap_or(path).display()
            );
            match load_session()? {
                Some(s) => println!("session:     damtomoId={}", s.damtomo_id),
                None => println!("session:     <none>"),
            }
            Ok(())
        }
        Commands::SearchSongs { query, page } => {
            let session = session(proxy)?;
            let searchable = as_searchable(&session);
            let result = searchable.search_songs(&query, page).await?;
            println!("{result:#?}");
            Ok(())
        }
        Commands::SearchArtists { query, page } => {
            let session = session(proxy)?;
            let searchable = as_searchable(&session);
            let result = searchable.search_artists(&query, page).await?;
            println!("{result:#?}");
            Ok(())
        }
        Commands::SongsByArtist { artist_id, page } => {
            let session = session(proxy)?;
            let searchable = as_searchable(&session);
            let result = searchable.songs_by_artist(&artist_id, page).await?;
            println!("{result:#?}");
            Ok(())
        }
        Commands::GetSong { song_id } => {
            let session = session(proxy)?;
            let result = session.get_song(&song_id).await?;
            println!("{result:#?}");
            Ok(())
        }
        Commands::GetStream { song_id } => {
            let session = session(proxy)?;
            let result = session.get_stream(&song_id).await?;
            println!("{result:#?}");
            Ok(())
        }
        Commands::GetScoring { song_id } => {
            let session = session(proxy)?;
            let scoring = session
                .as_scoring_provider()
                .ok_or_else(|| anyhow!("DAM session does not expose the scoring capability"))?;
            let result = scoring.get_scoring(&song_id).await?;
            println!("{result:#?}");
            Ok(())
        }
    }
}

fn build_client(proxy: ProxyArgs) -> Result<reqwest::Client> {
    let mut builder = DamProvider::client_builder();
    if let Some(url) = proxy.url {
        let mut p = reqwest::Proxy::all(url).with_context(|| format!("invalid proxy URL {url}"))?;
        if let Some(user) = proxy.user {
            p = p.basic_auth(user, proxy.pass.unwrap_or(""));
        }
        builder = builder.proxy(p);
    }
    builder.build().context("building reqwest client")
}

fn session(proxy: ProxyArgs) -> Result<Arc<dyn ProviderSession>> {
    let stored = load_session()?
        .ok_or_else(|| anyhow!("no session on file — run `dam-tool login --write` first"))?;
    let client = build_client(proxy)?;
    Ok(Arc::new(DamProviderSession::from_tokens(
        client,
        stored.user_code,
        stored.auth_token,
    )))
}

fn as_searchable(session: &Arc<dyn ProviderSession>) -> &dyn Searchable {
    session.as_searchable().expect("DAM is Searchable")
}

fn dotfile_path() -> PathBuf {
    PathBuf::from(DOTFILE_NAME)
}

fn load_session() -> Result<Option<StoredSession>> {
    let path = dotfile_path();
    match fs::read_to_string(&path) {
        Ok(body) => {
            let s: StoredSession = serde_json::from_str(&body)
                .with_context(|| format!("parsing {}", path.display()))?;
            Ok(Some(s))
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).with_context(|| format!("reading {}", path.display())),
    }
}

fn save_session(s: &StoredSession) -> Result<()> {
    let path = dotfile_path();
    let body = serde_json::to_string_pretty(s).context("serializing session")?;
    fs::write(&path, body).with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}
