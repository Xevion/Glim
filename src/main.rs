//! Glim - Generate beautiful GitHub repository cards.
//!
//! A command-line tool and HTTP server for creating dynamic repository cards
//! that display GitHub repository information in a clean, visual format.

pub mod cache;
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
use std::net::SocketAddr;
use std::net::{IpAddr, Ipv4Addr};
use tracing_subscriber::FmtSubscriber;

const DEFAULT_HOST: IpAddr = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
const DEFAULT_PORT: u16 = 8080;

/// A helper method for invoking the address parser, and filling in the missing parts of the address.
///
/// If no port is provided, use 8080. Works for both IPv4 and IPv6.
/// If no host is provided, defaults to IPv4 at 127.0.0.1.
///
/// # Errors
///
/// Returns an error if the address is invalid.
fn get_address(addr: &str) -> Result<SocketAddr> {
    match server::parse_address_components(addr) {
        Ok(value) => match value.to_enum() {
            terrors::E3::A(addr) => Ok(addr),
            terrors::E3::B(ip) => Ok(SocketAddr::from((ip, DEFAULT_PORT))),
            terrors::E3::C(port) => Ok(SocketAddr::from((DEFAULT_HOST, port))),
        },
        Err(value) => match value.to_enum() {
            terrors::E3::A(e) => return Err(e.into()),
            terrors::E3::B(e) => return Err(e.into()),
            terrors::E3::C(e) => return Err(e.into()),
        },
    }
}
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

        if let Some(addr_argument) = cli.server.as_ref() {
            // If no address is provided, use the default address
            let addr = addr_argument.as_ref().map_or(
                Ok(SocketAddr::new(DEFAULT_HOST, DEFAULT_PORT)),
                // If an argument is provided, use it
                |addr| get_address(addr),
            )?;

            server::start_server(addr).await;
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

        if let Some(addr) = server_addr {
            server::start_server(get_address(&addr)?).await;
        } else {
            tracing::error!("Please provide a server address or enable the 'cli' feature.");
        }
    }

    Ok(())
}
