//! Command-line interface for livecards.
//!
//! Handles CLI argument parsing and execution logic for generating repository cards.

use crate::errors::Result;
use clap::Parser;
use std::fs::File;
use std::io::BufWriter;
use std::path::PathBuf;
use tracing::Level;

use crate::{github, image};

/// Command-line arguments for livecards.
#[derive(Parser)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    /// The repository to generate a card for, in the format `owner/repo`.
    pub repository: Option<String>,

    /// The output path for the generated card.
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// GitHub token to use for API requests.
    #[arg(short, long)]
    pub token: Option<String>,

    /// Start the HTTP server.
    #[arg(
        short,
        long,
        value_name = "HOST:PORT",
        default_missing_value = Some("127.0.0.1:8000"),
        num_args = 0..=1,
        require_equals = true
    )]
    pub server: Option<String>,
    /// Set the logging level.
    #[arg(long, short = 'L', value_name = "LEVEL", default_value_t = if cfg!(debug_assertions) { Level::DEBUG } else { Level::INFO })]
    pub log_level: Level,
}

/// Executes the CLI command to generate a repository card.
///
/// # Arguments
/// * `cli` - Parsed command-line arguments
///
/// # Returns
/// Result indicating success or failure of card generation
pub async fn run(cli: Cli) -> Result<()> {
    let repo_path = cli.repository.as_ref().unwrap();
    let repo = github::get_repository_info(repo_path, cli.token).await?;

    let output_path = match cli.output {
        Some(path) => path,
        None => {
            let repo_name = repo_path.split('/').last().unwrap_or("card");
            PathBuf::from(format!("{}.png", repo_name))
        }
    };

    let file = File::create(&output_path)?;
    let writer = BufWriter::new(file);

    image::generate_image(
        &repo.name,
        &repo.description.unwrap_or_default(),
        &repo.language.unwrap_or_default(),
        &repo.stargazers_count.to_string(),
        &repo.forks_count.to_string(),
        writer,
    )?;

    tracing::info!("Successfully generated {}.", output_path.to_string_lossy());

    Ok(())
}
