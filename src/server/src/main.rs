use server::config::load_config;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = load_config().map_err(|e| anyhow::anyhow!("Failed to load configuration: {e}"))?;
    server::run(config).await
}
