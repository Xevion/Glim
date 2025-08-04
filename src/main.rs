//! Livecards - Generate beautiful GitHub repository cards.
//!
//! A command-line tool and HTTP server for creating dynamic repository cards
//! that display GitHub repository information in a clean, visual format.

pub mod colors;
pub mod encode;
pub mod errors;
pub mod github;
pub mod image;
pub mod ratelimit;
pub mod server;

#[cfg(feature = "cli")]
pub mod cli;

use crate::errors::Result;
use tracing_subscriber::FmtSubscriber;

#[tokio::main]
async fn main() -> Result<()> {
    #[cfg(feature = "cli")]
    {
        use clap::Parser;
        let cli = cli::Cli::parse();

        let subscriber = FmtSubscriber::builder()
            .with_max_level(cli.log_level)
            .finish();

        tracing::subscriber::set_global_default(subscriber)
            .expect("setting default subscriber failed");

        if let Some(addr) = cli.server.as_ref() {
            if let Err(e) = server::run(Some(addr.clone())).await {
                tracing::error!("Server error: {}", e);
                std::process::exit(1);
            }
        } else if cli.repository.is_some() {
            cli::run(cli).await?;
        } else {
            tracing::error!("Please provide a repository or start the server with --server.");
        }
    }

    #[cfg(not(feature = "cli"))]
    {
        // Server-only mode
        let subscriber = FmtSubscriber::builder()
            .with_max_level(tracing::Level::INFO)
            .finish();

        tracing::subscriber::set_global_default(subscriber)
            .expect("setting default subscriber failed");

        // Parse command line arguments manually for server address
        let args: Vec<String> = std::env::args().collect();
        let server_addr = args.get(1).cloned();

        if let Err(e) = server::run(server_addr).await {
            eprintln!("Error starting server: {}", e);
            std::process::exit(1);
        }
    }

    Ok(())
}
