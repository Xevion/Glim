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
/// If no port is provided, use the provided default_port. Works for both IPv4 and IPv6.
/// If no host is provided, defaults to IPv4 at 127.0.0.1.
/// Multiple addresses can be provided, separated by commas.
///
/// # Arguments
/// * `addr` - Address string to parse
/// * `default_port` - Default port to use when no port is specified
///
/// # Errors
///
/// Returns an error if any address is invalid.
fn get_addresses(addr: &str, default_port: u16) -> Result<Vec<SocketAddr>> {
    let addresses: Vec<Result<SocketAddr>> = addr
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| match server::parse_address_components(s) {
            Ok(value) => match value.to_enum() {
                terrors::E3::A(addr) => Ok(addr),
                terrors::E3::B(ip) => Ok(SocketAddr::from((ip, default_port))),
                terrors::E3::C(port) => Ok(SocketAddr::from((DEFAULT_HOST, port))),
            },
            Err(value) => match value.to_enum() {
                terrors::E3::A(e) => Err(e.into()),
                terrors::E3::B(e) => Err(e.into()),
                terrors::E3::C(e) => Err(e.into()),
            },
        })
        .collect();

    addresses.into_iter().collect()
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
            let default_port = cli.port.unwrap_or_else(|| {
                if let Ok(port_str) = std::env::var("PORT") {
                    port_str.parse::<u16>().unwrap_or_else(|_| {
                        tracing::error!("Invalid PORT environment variable: {}", port_str);
                        std::process::exit(1);
                    })
                } else {
                    DEFAULT_PORT
                }
            });
            let addrs = addr_argument.as_ref().map_or(
                Ok(vec![SocketAddr::new(DEFAULT_HOST, default_port)]),
                // If an argument is provided, use it
                |addr| get_addresses(addr, default_port),
            )?;

            if let Some(Err(e)) = server::start_server(addrs).await {
                tracing::error!("Server error: {}", e);
                return Err(crate::errors::GlimError::General(e));
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

        // Get default port from environment variable or use constant
        let default_port = if let Ok(port_str) = std::env::var("PORT") {
            port_str.parse::<u16>().unwrap_or_else(|_| {
                tracing::error!("Invalid PORT environment variable: {}", port_str);
                std::process::exit(1);
            })
        } else {
            DEFAULT_PORT
        };

        if let Some(addr) = server_addr {
            if let Some(Err(e)) = server::start_server(get_addresses(&addr, default_port)?).await {
                tracing::error!("Server error: {}", e);
                return Err(crate::errors::GlimError::General(e));
            }
        } else {
            tracing::error!("Please provide a server address or enable the 'cli' feature.");
        }
    }

    Ok(())
}
