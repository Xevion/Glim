use anyhow::Result;
use clap::Parser;
use serde::Deserialize;
use std::env;
use std::path::PathBuf;

use crate::image;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    /// The repository to generate a card for, in the format `owner/repo`.
    pub repository: String,

    /// The output path for the generated card.
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// GitHub token to use for API requests.
    #[arg(short, long)]
    pub token: Option<String>,
}

#[derive(Deserialize)]
pub struct Repository {
    pub name: String,
    pub description: Option<String>,
    pub language: Option<String>,
    pub stargazers_count: u32,
    pub forks_count: u32,
}

pub async fn run() -> Result<()> {
    let cli = Cli::parse();
    let repo_url = format!("https://api.github.com/repos/{}", cli.repository);

    let token = cli.token.or_else(|| env::var("GITHUB_TOKEN").ok());

    let mut headers = reqwest::header::HeaderMap::new();
    if let Some(token) = token {
        headers.insert(
            "Authorization",
            format!("Bearer {}", token).parse().unwrap(),
        );
    }

    let client = reqwest::Client::builder()
        .user_agent("livecards-generator")
        .default_headers(headers)
        .build()?;

    let repo: Repository = client.get(&repo_url).send().await?.json().await?;

    let output_path = match cli.output {
        Some(path) => path,
        None => {
            let repo_name = cli.repository.split('/').last().unwrap_or("card");
            PathBuf::from(format!("{}.png", repo_name))
        }
    };

    image::generate_image(
        &repo.name,
        &repo.description.unwrap_or_default(),
        &repo.language.unwrap_or_default(),
        &repo.stargazers_count.to_string(),
        &repo.forks_count.to_string(),
        &output_path.to_string_lossy(),
    )?;

    Ok(())
}
