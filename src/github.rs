use anyhow::{anyhow, Result};
use lazy_static::lazy_static;
use serde::Deserialize;
use std::collections::HashMap;
use std::env;
use tokio::sync::Mutex;
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

lazy_static! {
    static ref CACHE: Mutex<HashMap<String, CacheEntry>> = Mutex::new(HashMap::new());
}

#[instrument(skip(token))]
pub async fn get_repository_info(repo_path: &str, token: Option<String>) -> Result<Repository> {
    let mut cache = CACHE.lock().await;

    if let Some(entry) = cache.get(repo_path) {
        match entry {
            CacheEntry::Valid(repo) => {
                debug!("Cache hit for {}", repo_path);
                return Ok(repo.clone());
            }
            CacheEntry::Invalid(count) if *count >= 3 => {
                info!("Cache hit for invalid repo {} (retries exhausted)", repo_path);
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
            cache.insert(repo_path.to_string(), CacheEntry::Valid(repo.clone()));
            Ok(repo)
        }
        _ => {
            let new_count = if let Some(CacheEntry::Invalid(count)) = cache.get(repo_path) {
                count + 1
            } else {
                1
            };
            info!(
                "Failed to fetch repo info for {}, attempt {}",
                repo_path, new_count
            );
            cache.insert(repo_path.to_string(), CacheEntry::Invalid(new_count));
            Err(anyhow!("Failed to fetch repository information."))
        }
    }
}
