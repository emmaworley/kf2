use clap::Parser;
use config::{Config, Environment, File};
use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct AppConfig {
    pub database: DatabaseConfig,
    pub server: ServerConfig,
    pub projector: FrontendConfig,
    pub remocon: FrontendConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DatabaseConfig {
    pub path: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}

impl ServerConfig {
    /// `host:port` string suitable for `TcpListener::bind`.
    pub fn listen_addr(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

/// Configuration for a frontend SPA.
///
/// `root` is either a filesystem path to a built `dist` directory (production
/// mode — static file serving with index.html fallback) or an `http://` /
/// `https://` URL pointing at a Vite dev server (development mode — requests
/// are reverse-proxied for HMR support).
#[derive(Debug, Deserialize, Clone)]
pub struct FrontendConfig {
    pub root: String,
}

impl FrontendConfig {
    /// True when `root` is an HTTP(S) URL and should be reverse-proxied.
    pub fn is_dev_server(&self) -> bool {
        let r = self.root.trim_start();
        r.starts_with("http://") || r.starts_with("https://")
    }
}

#[derive(Parser, Debug)]
#[command(name = "kf2", version, about = "KF2 Karaoke Server")]
pub struct CliArgs {
    /// Path to config file
    #[arg(short, long, default_value = "kf2.toml")]
    pub config: String,

    /// Database file path (overrides config)
    #[arg(long)]
    pub db_path: Option<String>,

    /// Server host (overrides config)
    #[arg(long)]
    pub host: Option<String>,

    /// Server port (overrides config)
    #[arg(short, long)]
    pub port: Option<u16>,

    /// Projector frontend root — dist path or Vite dev server URL (overrides config)
    #[arg(long)]
    pub projector_root: Option<String>,

    /// Remocon frontend root — dist path or Vite dev server URL (overrides config)
    #[arg(long)]
    pub remocon_root: Option<String>,
}

pub fn load_config() -> Result<AppConfig, Box<dyn std::error::Error>> {
    let cli = CliArgs::parse();

    let mut builder = Config::builder()
        .set_default("database.path", "kf2.db")?
        .set_default("server.host", "127.0.0.1")?
        .set_default("server.port", 3000)?
        .set_default("projector.root", "src/frontend/packages/projector/dist")?
        .set_default("remocon.root", "src/frontend/packages/remocon/dist")?
        .add_source(File::with_name(&cli.config).required(false))
        .add_source(Environment::with_prefix("KF2").separator("__"));

    if let Some(db_path) = cli.db_path {
        builder = builder.set_override("database.path", db_path)?;
    }
    if let Some(host) = cli.host {
        builder = builder.set_override("server.host", host)?;
    }
    if let Some(port) = cli.port {
        builder = builder.set_override("server.port", port as i64)?;
    }
    if let Some(projector_root) = cli.projector_root {
        builder = builder.set_override("projector.root", projector_root)?;
    }
    if let Some(remocon_root) = cli.remocon_root {
        builder = builder.set_override("remocon.root", remocon_root)?;
    }

    let config = builder.build()?;
    Ok(config.try_deserialize()?)
}
