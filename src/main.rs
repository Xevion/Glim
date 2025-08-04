use anyhow::Result;
use clap::Parser;

mod cli;
mod colors;
mod image;
mod server;

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
