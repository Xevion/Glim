//! GitHub API client with intelligent caching and error handling.

use crate::errors::{self, GitHubError, Result};
use moka::future::Cache;
use once_cell::sync::Lazy;
use reqwest::Client;
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

/// Cache entry for tracking successful and failed requests.
#[derive(Clone, Debug)]
pub enum CacheEntry {
    /// Successfully fetched repository data (cached for 30 minutes)
    Valid(Repository),
    /// Failed request with retry counter (up to 3 attempts)
    Invalid(errors::GitHubError, u8),
    /// Permanently failed request with original error preserved
    InvalidExhausted(errors::GitHubError),
}

/// Global cache for repository data with 30-minute TTL.
static CACHE: Lazy<Cache<String, CacheEntry>> = Lazy::new(|| {
    Cache::builder()
        .time_to_live(Duration::from_secs(30 * 60)) // 30 minutes TTL
        .build()
});

/// Shared HTTP client for GitHub API requests.
static HTTP_CLIENT: Lazy<Client> = Lazy::new(|| {
    Client::builder()
        .user_agent("glim-generator")
        .build()
        .expect("Failed to create HTTP client")
});

/// GitHub API base URL.
const GITHUB_API_BASE: &str = "https://api.github.com/repos/";

/// Fetches repository information from GitHub API with caching.
///
/// # Arguments
/// * `repo_path` - Repository path in format "owner/repo"
/// * `token` - Optional GitHub token for authentication
///
/// # Returns
/// Repository information or specific error type
///
/// # Caching Strategy
/// - Valid entries: Return immediately (30 min TTL)
/// - InvalidExhausted entries: Return original error immediately
/// - Invalid entries (count < 3): Retry API call, increment counter
/// - 404 errors: Immediately cache as InvalidExhausted (no retries)
/// - Other errors: Retry up to 3 times before exhaustion
#[instrument(skip(token))]
pub async fn get_repository_info(repo_path: &str, token: Option<String>) -> Result<Repository> {
    // Check cache for existing entry (avoid string allocation if cache hit)
    if let Some(entry) = CACHE.get(repo_path).await {
        match entry {
            CacheEntry::Valid(repo) => {
                debug!("Cache hit for {}", repo_path);
                return Ok(repo);
            }
            CacheEntry::InvalidExhausted(error) => {
                info!(
                    "Cache hit for invalid repo {} (retries exhausted)",
                    repo_path
                );
                return Err(errors::GlimError::GitHub(error));
            }
            // Invalid entry with count < 3: continue to API call
            _ => {}
        }
    }

    info!("Cache miss for {}", repo_path);

    // Build request with minimal allocations
    let mut request = HTTP_CLIENT.get(format!("{}{}", GITHUB_API_BASE, repo_path));

    // Add authorization header if token is available
    if let Some(token) = token.or_else(|| env::var("GITHUB_TOKEN").ok()) {
        request = request.header("Authorization", format!("Bearer {}", token));
    }

    match request.send().await {
        Ok(response) => {
            let status = response.status();
            if status.is_success() {
                let repo: Repository = response
                    .json()
                    .await
                    .map_err(|_| errors::GlimError::GitHub(GitHubError::NetworkError))?;
                debug!("Fetched repo info for {}", repo_path);
                CACHE
                    .insert(repo_path.to_string(), CacheEntry::Valid(repo.clone()))
                    .await;
                Ok(repo)
            } else {
                let error = match status.as_u16() {
                    404 => GitHubError::NotFound,
                    403 => GitHubError::RateLimited,
                    code => GitHubError::ApiError(code),
                };

                // 404 errors are immediately exhausted (no retries for non-existent repos)
                if status.as_u16() == 404 {
                    info!(
                        "Repository not found: {} (immediately exhausted)",
                        repo_path
                    );
                    CACHE
                        .insert(
                            repo_path.to_string(),
                            CacheEntry::InvalidExhausted(error.clone()),
                        )
                        .await;
                    return Err(errors::GlimError::GitHub(error));
                }

                // Increment retry count for other errors
                let old_count =
                    if let Some(CacheEntry::Invalid(_, count)) = CACHE.get(repo_path).await {
                        count
                    } else {
                        0
                    };
                let new_count = old_count + 1;

                info!(
                    "Failed to fetch repo info for {}, attempt {}, status: {}",
                    repo_path, new_count, status
                );

                // Exhaust after 3 attempts, otherwise increment counter
                let cache_entry = if new_count >= 3 {
                    CacheEntry::InvalidExhausted(error.clone())
                } else {
                    CacheEntry::Invalid(error.clone(), new_count)
                };

                CACHE.insert(repo_path.to_string(), cache_entry).await;
                Err(errors::GlimError::GitHub(error))
            }
        }
        Err(_) => {
            let error = GitHubError::NetworkError;
            let old_count = if let Some(CacheEntry::Invalid(_, count)) = CACHE.get(repo_path).await
            {
                count
            } else {
                0
            };
            let new_count = old_count + 1;
            info!(
                "Network error for repo {}, attempt {}",
                repo_path, new_count
            );

            // Exhaust after 3 attempts, otherwise increment counter
            let cache_entry = if new_count >= 3 {
                CacheEntry::InvalidExhausted(error.clone())
            } else {
                CacheEntry::Invalid(error.clone(), new_count)
            };

            CACHE.insert(repo_path.to_string(), cache_entry).await;
            Err(errors::GlimError::GitHub(error))
        }
    }
}
