use crate::AppConfig;
use clap::Parser;
use config::{Config, Environment, File};

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

pub fn parse_cli_args() -> Result<AppConfig, Box<dyn std::error::Error>> {
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
