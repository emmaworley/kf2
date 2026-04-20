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
use server::hls::{HlsEvent, ProgressReporter};
use server::provider::dam::{DamConfig, DamProvider, DamProviderSession, LoginData};
use server::provider::types::MediaStream;
use server::provider::{ProviderSession, Searchable};

const DOTFILE_NAME: &str = ".dam-tool.json";

struct StderrProgress;

impl ProgressReporter for StderrProgress {
    fn report(&self, event: HlsEvent<'_>) {
        match event {
            HlsEvent::PlaylistParsed { segment_count } => {
                eprintln!("playlist: {segment_count} segments");
            }
            HlsEvent::SegmentComplete { index, total } => {
                eprintln!("[{}/{}] seg_{:05}.ts", index + 1, total, index);
            }
            HlsEvent::InitSegmentComplete { url } => {
                eprintln!("init segment: {url}");
            }
        }
    }
}

/// On-disk shape for `./.dam-tool.json`. Tool-local so the wire types in
/// `server::provider::dam` don't need `Serialize` just to be persisted here.
#[derive(Debug, Serialize, Deserialize)]
struct StoredSession {
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
    long_about = "DAM provider direct query utility."
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
    /// (password, authToken) are redacted in request logs.
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
    /// Log in to DAM and print the response. Writes to ./.dam-tool.json on success, use
    /// `--no-write` to suppress this behavior.
    Login {
        #[arg(short, long, env = "DAM_USERNAME")]
        username: String,
        #[arg(short, long, env = "DAM_PASSWORD", num_args = 0..=1, default_missing_value = "")]
        password: Option<String>,
        #[arg(long)]
        no_write: bool,
    },
    /// Print current proxy and stored-credential state.
    Status,

    /// Search the DAM catalog.
    #[command(subcommand)]
    Search(SearchCommand),

    /// Fetch song data (metadata, media stream, etc) from the DAM catalog
    Song {
        song_id: String,
        #[command(subcommand)]
        command: SongCommand,
    },
}

#[derive(Subcommand, Debug)]
enum SearchCommand {
    /// Search songs by query string.
    Song {
        query: String,
        /// Zero-indexed page number (page 0 is the first page).
        #[arg(long, default_value_t = 0)]
        page: u32,
    },
    /// Search artists by query string.
    Artists {
        query: String,
        /// Zero-indexed page number (page 0 is the first page).
        #[arg(long, default_value_t = 0)]
        page: u32,
    },
    /// List songs for a given artist id.
    ByArtist {
        artist_id: String,
        /// Zero-indexed page number (page 0 is the first page).
        #[arg(long, default_value_t = 0)]
        page: u32,
    },
}

#[derive(Subcommand, Debug)]
enum SongCommand {
    /// Fetch a song's metadata by id.
    Metadata,
    /// Fetch a song's media stream URL(s) by id.
    Stream {
        /// Output directory. If no directory is provided, defaults to `./<song_id>/`.
        #[arg(short, long, num_args = 0..=1, require_equals = true, default_missing_value = "")]
        output: Option<String>,
    },
    /// Fetch a song's scoring / piano-roll data by id.
    Scoring,
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
            no_write,
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
            if !no_write {
                save_session(&StoredSession::from_login(cfg.username.clone(), &resp.data))?;
                eprintln!("wrote auth token to {}", dotfile_path().display());
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
        Commands::Search(search) => match search {
            SearchCommand::Song { query, page } => {
                let session = session(proxy)?;
                let searchable = as_searchable(&session);
                let result = searchable.search_songs(&query, page).await?;
                println!("{result:#?}");
                Ok(())
            }
            SearchCommand::Artists { query, page } => {
                let session = session(proxy)?;
                let searchable = as_searchable(&session);
                let result = searchable.search_artists(&query, page).await?;
                println!("{result:#?}");
                Ok(())
            }
            SearchCommand::ByArtist { artist_id, page } => {
                let session = session(proxy)?;
                let searchable = as_searchable(&session);
                let result = searchable.songs_by_artist(&artist_id, page).await?;
                println!("{result:#?}");
                Ok(())
            }
        },
        Commands::Song { song_id, command } => match command {
            SongCommand::Metadata => {
                let session = session(proxy)?;
                let result = session.get_song(&song_id).await?;
                println!("{result:#?}");
                Ok(())
            }
            SongCommand::Stream { output } => {
                let session = session(proxy)?;
                let stream = session.get_stream(&song_id).await?;

                let Some(output) = output else {
                    println!("{stream:#?}");
                    return Ok(());
                };

                let url = match stream {
                    MediaStream::Hls { url_high, .. } => url_high,
                    other => return Err(anyhow!("expected HLS stream, got {other:?}")),
                };
                let output_dir = PathBuf::from(if output.is_empty() {
                    song_id.as_str()
                } else {
                    output.as_str()
                });

                eprintln!("downloading: {url}");
                let hls_client = build_client(proxy)?;
                let result =
                    server::hls::download_hls(&hls_client, &url, &output_dir, &StderrProgress)
                        .await?;
                eprintln!(
                    "wrote {} segments to {}",
                    result.segment_count,
                    result.playlist_path.display()
                );
                Ok(())
            }
            SongCommand::Scoring => {
                let session = session(proxy)?;
                let scoring = session
                    .as_scoring_provider()
                    .ok_or_else(|| anyhow!("DAM session does not expose the scoring capability"))?;
                let result = scoring.get_scoring(&song_id).await?;
                println!("{result:#?}");
                Ok(())
            }
        },
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
