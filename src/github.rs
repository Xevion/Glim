//! GitHub API client with caching support.
//!
//! This module handles fetching repository information from the GitHub API
//! with intelligent caching to minimize API calls and handle rate limits.

use crate::errors::{GitHubError, Result};
use moka::future::Cache;
use once_cell::sync::Lazy;
use serde::Deserialize;
use std::env;
use std::time::Duration;
use tracing::{debug, info, instrument};

/// Repository information retrieved from the GitHub API.
#[derive(Deserialize, Clone, Debug)]
pub struct Repository {
    /// Repository name
    pub name: String,
    /// Repository description
    pub description: Option<String>,
    /// Primary programming language
    pub language: Option<String>,
    /// Number of stars
    pub stargazers_count: u32,
    /// Number of forks
    pub forks_count: u32,
}

/// Cache entry type for tracking both successful and failed requests.
#[derive(Clone, Debug)]
pub enum CacheEntry {
    /// Valid repository data
    Valid(Repository),
    /// Invalid request with error type and retry count
    Invalid(crate::errors::GitHubError, u8),
}

/// Global cache for repository data with 30-minute TTL.
static CACHE: Lazy<Cache<String, CacheEntry>> = Lazy::new(|| {
    Cache::builder()
        .time_to_live(Duration::from_secs(30 * 60)) // 30 minutes TTL
        .build()
});

/// Fetches repository information from GitHub API with caching.
///
/// # Arguments
/// * `repo_path` - Repository path in format "owner/repo"
/// * `token` - Optional GitHub token for authentication
///
/// # Returns
/// Repository information or specific error type
#[instrument(skip(token))]
pub async fn get_repository_info(repo_path: &str, token: Option<String>) -> Result<Repository> {
    let repo_path_string = repo_path.to_string();

    if let Some(entry) = CACHE.get(&repo_path_string).await {
        match entry {
            CacheEntry::Valid(repo) => {
                debug!("Cache hit for {}", repo_path);
                return Ok(repo);
            }
            CacheEntry::Invalid(error, count) if count >= 3 => {
                info!(
                    "Cache hit for invalid repo {} (retries exhausted)",
                    repo_path
                );
                return Err(crate::errors::LivecardsError::GitHub(error));
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
        .build()
        .map_err(|_| crate::errors::LivecardsError::GitHub(GitHubError::NetworkError))?;

    match client.get(&repo_url).send().await {
        Ok(response) => {
            let status = response.status();
            if status.is_success() {
                let repo: Repository = response.json().await.map_err(|_| {
                    crate::errors::LivecardsError::GitHub(GitHubError::NetworkError)
                })?;
                debug!("Fetched repo info for {}", repo_path);
                CACHE
                    .insert(repo_path_string, CacheEntry::Valid(repo.clone()))
                    .await;
                Ok(repo)
            } else {
                let error = match status.as_u16() {
                    404 => GitHubError::NotFound,
                    403 => GitHubError::RateLimited,
                    code => GitHubError::ApiError(code),
                };

                let old_count = if let Some(CacheEntry::Invalid(_, count)) =
                    CACHE.get(&repo_path_string).await
                {
                    count
                } else {
                    0
                };
                let new_count = old_count + 1;
                info!(
                    "Failed to fetch repo info for {}, attempt {}, status: {}",
                    repo_path, new_count, status
                );
                CACHE
                    .insert(
                        repo_path_string,
                        CacheEntry::Invalid(error.clone(), new_count),
                    )
                    .await;
                Err(crate::errors::LivecardsError::GitHub(error))
            }
        }
        Err(_) => {
            let error = GitHubError::NetworkError;
            let old_count =
                if let Some(CacheEntry::Invalid(_, count)) = CACHE.get(&repo_path_string).await {
                    count
                } else {
                    0
                };
            let new_count = old_count + 1;
            info!(
                "Network error for repo {}, attempt {}",
                repo_path, new_count
            );
            CACHE
                .insert(
                    repo_path_string,
                    CacheEntry::Invalid(error.clone(), new_count),
                )
                .await;
            Err(crate::errors::LivecardsError::GitHub(error))
        }
    }
}
