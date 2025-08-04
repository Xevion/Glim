use anyhow::{anyhow, Result};
use moka::future::Cache;
use once_cell::sync::Lazy;
use serde::Deserialize;
use std::env;
use std::time::Duration;
use tracing::{debug, info, instrument};

#[derive(Deserialize, Clone, Debug)]
pub struct Repository {
    pub name: String,
    pub description: Option<String>,
    pub language: Option<String>,
    pub stargazers_count: u32,
    pub forks_count: u32,
}

#[derive(Clone, Debug)]
pub enum CacheEntry {
    Valid(Repository),
    Invalid(u8),
}

static CACHE: Lazy<Cache<String, CacheEntry>> = Lazy::new(|| {
    Cache::builder()
        .time_to_live(Duration::from_secs(30 * 60)) // 30 minutes TTL
        .build()
});

#[instrument(skip(token))]
pub async fn get_repository_info(repo_path: &str, token: Option<String>) -> Result<Repository> {
    let repo_path_string = repo_path.to_string();

    if let Some(entry) = CACHE.get(&repo_path_string).await {
        match entry {
            CacheEntry::Valid(repo) => {
                debug!("Cache hit for {}", repo_path);
                return Ok(repo);
            }
            CacheEntry::Invalid(count) if count >= 3 => {
                info!(
                    "Cache hit for invalid repo {} (retries exhausted)",
                    repo_path
                );
                return Err(anyhow!("Repository not found or API rate limit exceeded."));
            }
            _ => {}
        }
    }

    info!("Cache miss for {}", repo_path);

    let token = token.or_else(|| env::var("GITHUB_TOKEN").ok());
    let repo_url = format!("https://api.github.com/repos/{}", repo_path);

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

    match client.get(&repo_url).send().await {
        Ok(response) if response.status().is_success() => {
            let repo: Repository = response.json().await?;
            debug!("Fetched repo info for {}", repo_path);
            CACHE
                .insert(repo_path_string, CacheEntry::Valid(repo.clone()))
                .await;
            Ok(repo)
        }
        _ => {
            let old_count =
                if let Some(CacheEntry::Invalid(count)) = CACHE.get(&repo_path_string).await {
                    count
                } else {
                    0
                };
            let new_count = old_count + 1;
            info!(
                "Failed to fetch repo info for {}, attempt {}",
                repo_path, new_count
            );
            CACHE
                .insert(repo_path_string, CacheEntry::Invalid(new_count))
                .await;
            Err(anyhow!("Failed to fetch repository information."))
        }
    }
}
