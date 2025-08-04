mod cli;
mod colors;
mod github;
mod image;
mod server;

use anyhow::Result;
use clap::Parser;
use tracing_subscriber::FmtSubscriber;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = cli::Cli::parse();

    let subscriber = FmtSubscriber::builder()
        .with_max_level(cli.log_level)
        .finish();

    tracing::subscriber::set_global_default(subscriber)
        .expect("setting default subscriber failed");

    if let Some(addr) = cli.server.as_ref() {
        server::run(Some(addr.clone())).await;
    } else if cli.repository.is_some() {
        cli::run(cli).await?;
    } else {
        tracing::error!("Please provide a repository or start the server with --server.");
    }

    Ok(())
}
