mod cli;
mod colors;
mod github;
mod image;
mod server;

use anyhow::Result;
use clap::Parser;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = cli::Cli::parse();

    if let Some(addr) = cli.server.as_ref() {
        server::run(Some(addr.clone())).await;
    } else if cli.repository.is_some() {
        cli::run(cli).await?;
    } else {
        println!("Please provide a repository or start the server with --server.");
    }

    Ok(())
}