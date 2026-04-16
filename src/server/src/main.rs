use server::cli::parse_cli_args;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config =
        parse_cli_args().map_err(|e| anyhow::anyhow!("Failed to load configuration: {e}"))?;
    server::run(config).await
}
