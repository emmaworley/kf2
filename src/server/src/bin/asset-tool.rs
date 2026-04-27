//! Integration tester for kf2 providers and the asset cache.
//!
//! Drives the `server` library in-process so proxy settings apply to upstream
//! HTTP traffic. Provider-specific commands (`asset-tool dam ...`) hit only
//! upstream APIs and never touch the local sqlite database; cache commands
//! (`asset-tool cache ...`) lazily open the DB and migrate it. This split
//! lets `asset-tool dam login` work on a fresh checkout without creating
//! `kf2.db` as a side effect — the DB only materializes when the user
//! actually exercises the cache.
//!
//! Per-provider auth state lives in `./.asset-tool.<provider>.json`; today
//! that's just `.asset-tool.dam.json`. Future providers add their own files.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result, anyhow};
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use server::asset_cache::AssetCache;
use server::db::{self, DbPool};
use server::provider::ProviderSession;
use server::provider::dam::{DamConfig, DamProvider, DamProviderSession, LoginData};
use server::provider::types::{AssetId, MediaStream, ProviderId};
use server::repo::AssetCacheRepo;
use server::repo::diesel_impl::DieselAssetCacheRepo;
use server::tools::hls::{HlsEvent, ProgressReporter};

const DAM_DOTFILE: &str = ".asset-tool.dam.json";

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

/// Per-provider on-disk auth state. Tool-local so the wire types in
/// `server::provider::dam` don't need `Serialize` just to be persisted here.
#[derive(Debug, Serialize, Deserialize)]
struct DamStoredSession {
    user_code: String,
    damtomo_id: String,
    auth_token: String,
}

impl DamStoredSession {
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
    name = "asset-tool",
    about = "Integration tool for kf2 providers and the asset cache",
    long_about = "Drives kf2 provider sessions and the application-wide asset cache against \
                  the real upstream APIs. Cache subcommands open the kf2 sqlite database; \
                  provider subcommands (e.g. `dam`) talk to upstream HTTP only."
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

    /// Log every provider HTTP request and response to stderr. Sensitive
    /// values (password, authToken) are redacted in request logs.
    #[arg(short, long, global = true)]
    debug: bool,

    /// Path to the kf2 sqlite database. Used only by cache subcommands.
    #[arg(long, env = "KF2_DB_PATH", global = true, default_value = "kf2.db")]
    db_path: String,

    /// Asset cache directory. Used only by cache subcommands.
    #[arg(
        long,
        env = "KF2_CACHE_DIR",
        global = true,
        default_value = ".kf2-cache"
    )]
    cache_dir: String,

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
    /// Print proxy/auth state and configured DB/cache paths. Side-effect-free:
    /// reports paths but does not open the DB or create the cache directory.
    Status,

    /// Asset cache operations against the kf2 sqlite database.
    #[command(subcommand)]
    Cache(CacheCommand),

    /// DAM provider operations. No DB or cache is touched.
    #[command(subcommand)]
    Dam(DamCommand),
}

#[derive(Subcommand, Debug)]
enum CacheCommand {
    /// Resolve `<provider> <song_id>` into an Asset and download it into the
    /// asset cache (or short-circuit to the existing local file if cached).
    Fetch {
        /// Provider id, e.g. `dam`. Must match a registered ProviderId.
        provider: String,
        /// Provider-native song id (e.g. DAM `requestNo`).
        song_id: String,
    },
    /// List rows in the asset_cache table.
    List,
    /// Remove a cached asset's row and on-disk directory by URN.
    Evict {
        /// Asset URN, e.g. `urn:dam:contentsId:5778352`.
        asset_id: String,
    },
}

#[derive(Subcommand, Debug)]
enum DamCommand {
    /// Log in to DAM and print the response. Writes to ./.asset-tool.dam.json
    /// on success; use `--no-write` to suppress this.
    Login {
        #[arg(short, long, env = "DAM_USERNAME")]
        username: String,
        #[arg(short, long, env = "DAM_PASSWORD", num_args = 0..=1, default_missing_value = "")]
        password: Option<String>,
        #[arg(long)]
        no_write: bool,
    },
    /// Search the DAM catalog.
    #[command(subcommand)]
    Search(DamSearchCommand),
    /// Fetch song data (metadata, asset, scoring) from the DAM catalog.
    Song {
        song_id: String,
        #[command(subcommand)]
        command: DamSongCommand,
    },
}

#[derive(Subcommand, Debug)]
enum DamSearchCommand {
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
        #[arg(long, default_value_t = 0)]
        page: u32,
    },
    /// List songs for a given artist id.
    ByArtist {
        artist_id: String,
        #[arg(long, default_value_t = 0)]
        page: u32,
    },
}

#[derive(Subcommand, Debug)]
enum DamSongCommand {
    /// Fetch a song's metadata by id.
    Metadata,
    /// Fetch a song's Asset (URN + remote stream URLs) by id. With `--output`,
    /// downloads the HLS stream to the given directory; this bypasses the
    /// cache layer and is intended for raw diagnostics — use `cache fetch`
    /// for the full integration path.
    Asset {
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
        db_path,
        cache_dir,
        cmd,
    } = cli;
    if debug {
        // server::provider::dam reads DAM_DEBUG once via LazyLock, so this
        // must be set before the first request fires. SAFETY: asset-tool is
        // single-threaded before dispatch.
        unsafe { std::env::set_var("DAM_DEBUG", "1") };
    }
    let proxy = ProxyArgs {
        url: proxy_url.as_deref(),
        user: proxy_user.as_deref(),
        pass: proxy_pass.as_deref(),
    };
    match cmd {
        Commands::Status => run_status(proxy, &db_path, &cache_dir),
        Commands::Cache(c) => run_cache(c, proxy, &db_path, &cache_dir).await,
        Commands::Dam(c) => run_dam(c, proxy).await,
    }
}

// ---------------------------------------------------------------------------
// `status` — read-only summary, no DB / cache side effects.
// ---------------------------------------------------------------------------

fn run_status(proxy: ProxyArgs<'_>, db_path: &str, cache_dir: &str) -> Result<()> {
    println!("proxy:        {}", proxy.url.unwrap_or("<not set>"));
    println!(
        "proxy auth:   {}",
        match (proxy.user, proxy.pass) {
            (Some(u), Some(_)) => format!("username={u}"),
            (Some(u), None) => format!("username={u} (no password)"),
            (None, _) => "<none>".into(),
        }
    );
    println!("db path:      {db_path} (not opened)");
    println!("cache dir:    {cache_dir} (not opened)");
    let dam_path = PathBuf::from(DAM_DOTFILE);
    println!(
        "dam dotfile:  {}",
        dam_path.canonicalize().unwrap_or(dam_path).display()
    );
    match load_dam_session()? {
        Some(s) => println!("dam session:  damtomoId={}", s.damtomo_id),
        None => println!("dam session:  <none>"),
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// `cache` — opens DB + cache lazily on first call.
// ---------------------------------------------------------------------------

struct CacheState {
    cache: Arc<AssetCache>,
    repo: Arc<DieselAssetCacheRepo>,
    cache_dir: PathBuf,
}

async fn cache_state(db_path: &str, cache_dir: &str) -> Result<CacheState> {
    let pool = open_pool(db_path).await?;
    let repo = Arc::new(DieselAssetCacheRepo::new(pool));
    let cache_dir = PathBuf::from(cache_dir);
    tokio::fs::create_dir_all(&cache_dir)
        .await
        .with_context(|| format!("creating cache dir {}", cache_dir.display()))?;
    let cache = Arc::new(AssetCache::new(
        cache_dir.clone(),
        repo.clone() as Arc<dyn AssetCacheRepo>,
        reqwest::Client::new(),
    ));
    Ok(CacheState {
        cache,
        repo,
        cache_dir,
    })
}

async fn open_pool(db_path: &str) -> Result<DbPool> {
    let pool = db::create_pool(&server::DatabaseConfig {
        path: db_path.to_string(),
    })
    .with_context(|| format!("opening sqlite db at {db_path}"))?;
    db::run_migrations(&pool)
        .await
        .with_context(|| format!("migrating db at {db_path}"))?;
    Ok(pool)
}

async fn run_cache(
    cmd: CacheCommand,
    proxy: ProxyArgs<'_>,
    db_path: &str,
    cache_dir: &str,
) -> Result<()> {
    let state = cache_state(db_path, cache_dir).await?;
    match cmd {
        CacheCommand::Fetch { provider, song_id } => {
            let provider: ProviderId = provider.parse().map_err(|e| {
                anyhow!("unknown provider `{provider}`: {e} (try one of: dam, joysound, youtube)")
            })?;
            // Outer re-mint loop: DAM hands out signed playlist URLs that
            // are one-time-use, so a 403 burns the URL and the only way to
            // recover is to call get_asset again to mint a fresh one. The
            // inner per-HTTP retries inside `download_hls` cover transient
            // CDN hiccups; this loop covers the burned-URL case.
            const MAX_REMINT_ATTEMPTS: usize = 3;
            let mut last_err: Option<anyhow::Error> = None;
            let mut cached = None;
            let mut last_asset_source = None;
            for attempt in 1..=MAX_REMINT_ATTEMPTS {
                let asset = match provider {
                    ProviderId::Dam => {
                        let session = dam_session(proxy)?;
                        session
                            .get_asset(&song_id)
                            .await
                            .with_context(|| format!("dam.get_asset({song_id})"))?
                    }
                    other => {
                        return Err(anyhow!(
                            "provider `{}` is not yet wired up in asset-tool",
                            other.as_str()
                        ));
                    }
                };
                eprintln!(
                    "resolved asset (attempt {attempt}/{MAX_REMINT_ATTEMPTS}): {}",
                    asset.id
                );
                match state.cache.fetch(&asset, &StderrProgress).await {
                    Ok(c) => {
                        last_asset_source = Some(asset.source);
                        cached = Some(c);
                        break;
                    }
                    Err(e) => {
                        eprintln!("attempt {attempt} failed: {e}");
                        last_err = Some(e.into());
                    }
                }
            }
            let cached = cached.ok_or_else(|| {
                last_err.unwrap_or_else(|| anyhow!("cache fetch failed without an error"))
            })?;
            println!("urn:          {}", cached.id);
            println!("local path:   {}", cached.local_path.display());
            println!(
                "source:       {}",
                last_asset_source
                    .as_ref()
                    .map(describe_source)
                    .unwrap_or("unknown")
            );
        }
        CacheCommand::List => {
            let entries = state.repo.list().await?;
            if entries.is_empty() {
                println!("(empty)");
            } else {
                for e in entries {
                    println!("{}\t{}", e.asset_id, e.relative_path);
                }
            }
        }
        CacheCommand::Evict { asset_id } => {
            let id: AssetId = asset_id
                .parse()
                .with_context(|| format!("parsing asset id `{asset_id}`"))?;
            let removed = state.cache.evict(&id).await?;
            if removed {
                println!("evicted {id} from cache at {}", state.cache_dir.display());
            } else {
                println!(
                    "no row found for {id} (cache dir: {})",
                    state.cache_dir.display()
                );
            }
        }
    }
    Ok(())
}

fn describe_source(s: &MediaStream) -> &'static str {
    match s {
        MediaStream::Hls { .. } => "hls",
        MediaStream::DirectVideo { .. } => "direct_video",
        MediaStream::YouTubeDownload { .. } => "youtube_download",
    }
}

// ---------------------------------------------------------------------------
// `dam` — provider-specific operations, no DB.
// ---------------------------------------------------------------------------

async fn run_dam(cmd: DamCommand, proxy: ProxyArgs<'_>) -> Result<()> {
    match cmd {
        DamCommand::Login {
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
                save_dam_session(&DamStoredSession::from_login(
                    cfg.username.clone(),
                    &resp.data,
                ))?;
                eprintln!("wrote auth token to {}", dam_dotfile_path().display());
            }
            Ok(())
        }
        DamCommand::Search(s) => {
            let session = dam_session(proxy)?;
            let searchable = session.as_searchable().expect("DAM is Searchable");
            match s {
                DamSearchCommand::Song { query, page } => {
                    let r = searchable.search_songs(&query, page).await?;
                    println!("{r:#?}");
                }
                DamSearchCommand::Artists { query, page } => {
                    let r = searchable.search_artists(&query, page).await?;
                    println!("{r:#?}");
                }
                DamSearchCommand::ByArtist { artist_id, page } => {
                    let r = searchable.songs_by_artist(&artist_id, page).await?;
                    println!("{r:#?}");
                }
            }
            Ok(())
        }
        DamCommand::Song { song_id, command } => match command {
            DamSongCommand::Metadata => {
                let session = dam_session(proxy)?;
                let song = session.get_song(&song_id).await?;
                println!("{song:#?}");
                Ok(())
            }
            DamSongCommand::Asset { output } => {
                let session = dam_session(proxy)?;
                let asset = session.get_asset(&song_id).await?;
                let Some(output) = output else {
                    println!("{asset:#?}");
                    return Ok(());
                };

                let url = match asset.source {
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
                let result = server::tools::hls::download_hls(
                    &hls_client,
                    &url,
                    &output_dir,
                    &StderrProgress,
                )
                .await?;
                eprintln!(
                    "wrote {} segments to {}",
                    result.segment_count,
                    result.playlist_path.display()
                );
                Ok(())
            }
            DamSongCommand::Scoring => {
                let session = dam_session(proxy)?;
                let scoring = session
                    .as_scoring_provider()
                    .ok_or_else(|| anyhow!("DAM session does not expose the scoring capability"))?;
                let r = scoring.get_scoring(&song_id).await?;
                println!("{r:#?}");
                Ok(())
            }
        },
    }
}

// ---------------------------------------------------------------------------
// Shared helpers.
// ---------------------------------------------------------------------------

fn build_client(proxy: ProxyArgs<'_>) -> Result<reqwest::Client> {
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

fn dam_session(proxy: ProxyArgs<'_>) -> Result<Arc<dyn ProviderSession>> {
    let stored = load_dam_session()?
        .ok_or_else(|| anyhow!("no DAM session on file — run `asset-tool dam login` first"))?;
    let client = build_client(proxy)?;
    Ok(Arc::new(DamProviderSession::from_tokens(
        client,
        stored.user_code,
        stored.auth_token,
    )))
}

fn dam_dotfile_path() -> PathBuf {
    PathBuf::from(DAM_DOTFILE)
}

fn load_dam_session() -> Result<Option<DamStoredSession>> {
    let path = dam_dotfile_path();
    load_dotfile(&path)
}

fn save_dam_session(s: &DamStoredSession) -> Result<()> {
    let path = dam_dotfile_path();
    let body = serde_json::to_string_pretty(s).context("serializing DAM session")?;
    fs::write(&path, body).with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

fn load_dotfile<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<Option<T>> {
    match fs::read_to_string(path) {
        Ok(body) => Ok(Some(
            serde_json::from_str::<T>(&body)
                .with_context(|| format!("parsing {}", path.display()))?,
        )),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).with_context(|| format!("reading {}", path.display())),
    }
}
