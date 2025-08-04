mod cli;
mod colors;
mod image;

use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    cli::run().await
}