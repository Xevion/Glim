//! Command-line interface for glim.
//!
//! Handles CLI argument parsing and execution logic for generating repository cards.

use crate::errors::Result;
use clap::Parser;
use std::fs::File;
use std::io::BufWriter;
use std::path::PathBuf;
use tracing::Level;

use crate::{
    encode::{create_encoder, Encoder, ImageFormat},
    github,
};

/// Command-line arguments for glim.
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
        value_name = "HOST:PORT[,HOST:PORT[,...]]",
        num_args = 0..=1,
        require_equals = false
    )]
    pub server: Option<Option<String>>,

    /// Set the logging level.
    #[arg(long, short = 'L', value_name = "LEVEL", default_value_t = if cfg!(debug_assertions) { Level::DEBUG } else { Level::INFO })]
    pub log_level: Level,

    /// Port to use for the server (defaults to 8080).
    #[arg(short, long)]
    pub port: Option<u16>,
}

/// Formats the SVG template with repository data.
///
/// # Arguments
/// * `name` - Repository name
/// * `description` - Repository description
/// * `language` - Primary programming language
/// * `stars` - Star count as string
/// * `forks` - Fork count as string
///
/// # Returns
/// Formatted SVG string
fn format_svg_template(
    name: &str,
    description: &str,
    language: &str,
    stars: &str,
    forks: &str,
) -> String {
    let svg_template = include_str!("../card.svg");
    let wrapped_description = crate::image::wrap_text(description, 65);
    let language_color =
        crate::colors::get_color(language).unwrap_or_else(|| "#f1e05a".to_string());

    let formatted_stars = crate::image::format_count(stars);
    let formatted_forks = crate::image::format_count(forks);

    svg_template
        .replace("{{name}}", name)
        .replace("{{description}}", &wrapped_description)
        .replace("{{language}}", language)
        .replace("{{language_color}}", &language_color)
        .replace("{{stars}}", &formatted_stars)
        .replace("{{forks}}", &formatted_forks)
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
    let repo = github::GITHUB_CLIENT.get_repository_info(repo_path).await?;

    let output_path = match cli.output {
        Some(path) => path,
        None => {
            let repo_name = repo_path.split('/').next_back().unwrap_or("card");
            PathBuf::from(format!("{}.png", repo_name))
        }
    };

    let file = File::create(&output_path)?;
    let mut writer = BufWriter::new(file);

    // Start timing for image generation
    let start_time = std::time::Instant::now();

    // Format the SVG template
    let formatted_svg = format_svg_template(
        &repo.name,
        &repo.description.unwrap_or_default(),
        &repo.language.unwrap_or_default(),
        &repo.stargazers_count.to_string(),
        &repo.forks_count.to_string(),
    );

    // Create encoder and encode
    let encoder = create_encoder(ImageFormat::Png);
    let encoding_timing = encoder.encode(&formatted_svg, &mut writer, None)?;

    // Calculate timing
    let duration = start_time.elapsed();
    let duration_ms = duration.as_millis();

    let svg_template_duration = duration - encoding_timing.total;

    tracing::debug!(
        repo_path = repo_path,
        svg_template_duration = ?svg_template_duration,
        rasterization_duration = ?encoding_timing.rasterization,
        encoding_duration = ?encoding_timing.encoding,
        total_duration = ?duration,
        "CLI image generation completed"
    );

    if duration_ms > 1000 {
        tracing::warn!(
            repo_path = repo_path,
            svg_template_duration = ?svg_template_duration,
            rasterization_duration = ?encoding_timing.rasterization,
            encoding_duration = ?encoding_timing.encoding,
            total_duration = ?duration,
            "Slow CLI image generation"
        );
    }

    tracing::info!("Successfully generated {}.", output_path.to_string_lossy());

    Ok(())
}
