//! GitHub API client with intelligent caching, circuit breaker, and error handling.

use crate::errors::{self, GitHubError, Result};
use axum::http::header;
use failsafe::{
    backoff::{self},
    failure_policy::{self, ConsecutiveFailures, OrElse, SuccessRateOverTimeWindow},
    Config, FailurePolicy, StateMachine,
};
use moka::future::Cache;
use once_cell::sync::Lazy;
use reqwest::Client;
use serde::Deserialize;
use std::env;
use std::time::Duration;
use tracing::{debug, info, instrument, warn};

const DEFAULT_API_RETRIES: u8 = 3;

/// Type alias for the circuit breaker implementation
type DefaultCircuitBreaker = StateMachine<
    OrElse<
        SuccessRateOverTimeWindow<backoff::FullJittered>,
        ConsecutiveFailures<backoff::FullJittered>,
    >,
    (),
>;

// Global GitHub client instance
pub static GITHUB_CLIENT: Lazy<GitHubClient> = Lazy::new(GitHubClient::new);

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
    /// Whether the repository is private
    pub private: bool,
}

/// Cache entry for tracking successful and failed requests.
#[derive(Clone, Debug)]
pub enum CacheEntry {
    /// Successfully fetched repository data (cached for 30 minutes)
    Valid { data: Repository },
    /// Failed request with retry counter (up to 3 attempts)
    Invalid {
        error: errors::GitHubError,
        remaining: u8,
    },
    /// Permanently failed request with original error preserved
    InvalidExhausted { error: errors::GitHubError },
}

/// GitHub API client with circuit breaker and caching.
#[derive(Clone)]
pub struct GitHubClient {
    /// HTTP client for making requests
    http_client: Client,
    /// Circuit breaker for handling failures
    circuit_breaker: DefaultCircuitBreaker,
    /// Cache for repository data
    pub cache: Cache<String, CacheEntry>,
}

impl GitHubClient {
    /// Creates a new GitHub client with circuit breaker and caching.
    pub fn new() -> Self {
        let mut headers = header::HeaderMap::new();
        headers.insert(
            header::ACCEPT,
            header::HeaderValue::from_static("application/vnd.github+json"),
        );
        headers.insert(
            "X-GitHub-Api-Version",
            header::HeaderValue::from_static("2022-11-28"),
        );

        // Add authorization header if token is available
        if let Ok(token) = env::var("GITHUB_TOKEN") {
            let mut auth_value =
                header::HeaderValue::from_str(&format!("Bearer {}", token)).unwrap();
            auth_value.set_sensitive(true);
            headers.insert(header::AUTHORIZATION, auth_value);
        }

        // Create HTTP client with default headers
        let http_client = Client::builder()
            // Set user agent to glim/version
            .user_agent(format!("glim/{}", env!("CARGO_PKG_VERSION")))
            .default_headers(headers)
            .build()
            .expect("Failed to create HTTP client");

        // Create circuit breaker with success rate + consecutive failures policy, full jitter backoff
        let circuit_breaker = Config::new()
            .failure_policy(
                failure_policy::success_rate_over_time_window(
                    0.8,
                    5,
                    Duration::from_secs(30),
                    backoff::full_jittered(Duration::from_secs(10), Duration::from_secs(300)),
                )
                .or_else(failure_policy::consecutive_failures(
                    5,
                    backoff::full_jittered(Duration::from_secs(10), Duration::from_secs(300)),
                )),
            )
            .build();

        // Create cache
        let cache = Cache::builder()
            .time_to_live(Duration::from_secs(30 * 60)) // 30 minutes TTL
            .build();

        Self {
            http_client,
            circuit_breaker,
            cache,
        }
    }

    /// Determines if an error should trigger the circuit breaker.
    /// Only network errors, 5xx errors, and rate limits should trigger it.
    /// 404s and other client errors should not trigger the circuit breaker.
    pub fn should_trigger_circuit_breaker(error: &GitHubError) -> bool {
        match error {
            GitHubError::NetworkError => true,
            GitHubError::RateLimited => true,
            GitHubError::ApiError(code) => {
                // Only 5xx errors should trigger circuit breaker
                *code >= 500
            }
            GitHubError::NotFound => false, // 404s should not trigger circuit breaker
            GitHubError::InvalidFormat(_) => false, // Client errors should not trigger
            GitHubError::AuthError(_) => false, // Auth errors should not trigger
            GitHubError::CircuitBreakerOpen => false, // N/A
        }
    }

    /// Fetches repository information from GitHub API with circuit breaker and caching.
    ///
    /// # Arguments
    /// * `repo_path` - Repository path in format "owner/repo"
    /// * `token` - Optional GitHub token for authentication
    ///
    /// # Returns
    /// Repository information or specific error type
    ///
    /// # Circuit Breaker Behavior
    /// - Network errors, 5xx errors, and rate limits trigger the circuit breaker
    /// - 404s and other client errors do not trigger the circuit breaker
    /// - When circuit breaker is open, returns a 503 Service Unavailable error
    #[instrument(skip(self))]
    pub async fn get_repository_info(&self, repo_path: &str) -> Result<Repository> {
        // Check cache for existing entry
        if let Some(entry) = self.cache.get(repo_path).await {
            match entry {
                // Valid entry: return the data
                CacheEntry::Valid { data } => {
                    debug!("Cache hit for {}", repo_path);
                    return Ok(data);
                }
                // Invalid exhausted entry: return the error
                CacheEntry::InvalidExhausted { error } => {
                    debug!("Cache hit for invalid exhausted repo {}", repo_path);
                    return Err(errors::GlimError::GitHub(error));
                }
                // Invalid entry with remaining retries: try to make the API call
                CacheEntry::Invalid {
                    error: _,
                    remaining: _,
                } => {}
            }
        }

        // Check if the circuit breaker is open
        if !self.circuit_breaker.is_call_permitted() {
            info!("Request blocked by circuit breaker for {}", repo_path);
            return Err(errors::GlimError::GitHub(GitHubError::CircuitBreakerOpen));
        }

        // Invoke the API call
        debug!("Cache miss for {}", repo_path);
        let result = self.fetch_repository_info(repo_path).await;

        match result {
            // Success, cache the result
            Ok(repo) => {
                self.cache
                    .insert(
                        repo_path.to_string(),
                        CacheEntry::Valid { data: repo.clone() },
                    )
                    .await;

                // Inform the circuit breaker of the success
                self.circuit_breaker.on_success();

                Ok(repo)
            }
            Err(glim_error) => {
                // Extract GitHub error from GlimError
                let github_error = match glim_error {
                    errors::GlimError::GitHub(github_error) => github_error,
                    _ => {
                        // Unexpected error type - treat as network error
                        GitHubError::NetworkError
                    }
                };

                // Inform the circuit breaker of the error if it's appropriate
                if Self::should_trigger_circuit_breaker(&github_error) {
                    self.circuit_breaker.on_error();

                    // Check if it opened (disabled) the circuit breaker
                    if !self.circuit_breaker.is_call_permitted() {
                        warn!(
                            "Circuit breaker opened for GitHub API after error: {:?}",
                            github_error
                        );
                    }
                }

                // Handle the error
                self.handle_github_error(repo_path, &github_error).await
            }
        }
    }

    /// Makes the actual GitHub API request.
    #[instrument(skip(self))]
    pub async fn fetch_repository_info(&self, repo_path: &str) -> Result<Repository> {
        // Build request
        let url = format!("https://api.github.com/repos/{}", repo_path);
        let request = self.http_client.get(&url);

        debug!("GET {}", url);

        let response = request
            .send()
            .await
            .map_err(|_| errors::GlimError::GitHub(GitHubError::NetworkError))?;

        let status = response.status();
        info!(
            status = format!(
                "{}{}",
                status.as_u16(),
                status
                    .canonical_reason()
                    .map(|reason| format!(" {}", reason))
                    .unwrap_or_default()
            ),
            "Response received"
        );

        if status.is_success() {
            let repo: Repository = response
                .json()
                .await
                .map_err(|_| errors::GlimError::GitHub(GitHubError::NetworkError))?;
            debug!("Fetched repo info for {}", repo_path);

            if repo.private {
                warn!("A private repository was fetched: {}", repo_path);

                // Return a 404 as if the repository was not found
                return Err(errors::GlimError::GitHub(GitHubError::NotFound));
            }

            Ok(repo)
        } else {
            let error = match status.as_u16() {
                404 => GitHubError::NotFound,
                403 => GitHubError::RateLimited,
                code => GitHubError::ApiError(code),
            };

            Err(errors::GlimError::GitHub(error))
        }
    }

    /// Handles GitHub API errors with caching logic.
    async fn handle_github_error(
        &self,
        repo_path: &str,
        error: &GitHubError,
    ) -> Result<Repository> {
        // 404 errors are immediately exhausted (no retries for non-zexistent repos)
        if matches!(error, GitHubError::NotFound) {
            info!(
                "Repository not found: {} (immediately exhausted)",
                repo_path
            );
            self.cache
                .insert(
                    repo_path.to_string(),
                    CacheEntry::InvalidExhausted {
                        error: error.clone(),
                    },
                )
                .await;

            return Err(errors::GlimError::GitHub(error.clone()));
        }

        // Decrement remaining retries for other errors
        let new_count = if let Some(CacheEntry::Invalid {
            error: _,
            remaining: count,
        }) = self.cache.get(repo_path).await
        {
            count.saturating_sub(1)
        } else {
            DEFAULT_API_RETRIES
        };

        info!(
            "Failed to fetch repo info for {}, attempt {}, error: {:?}",
            repo_path, new_count, error
        );

        // Exhaust after 3 attempts, otherwise decrement counter
        let cache_entry = if new_count == 0 {
            CacheEntry::InvalidExhausted {
                error: error.clone(),
            }
        } else {
            CacheEntry::Invalid {
                error: error.clone(),
                remaining: new_count,
            }
        };

        self.cache.insert(repo_path.to_string(), cache_entry).await;
        Err(errors::GlimError::GitHub(error.clone()))
    }

    /// Gets the current circuit breaker status for monitoring.
    pub fn circuit_breaker(&self) -> &DefaultCircuitBreaker {
        &self.circuit_breaker
    }

    /// Returns true if the circuit breaker is disabled (open)
    pub fn disabled(&self) -> bool {
        !self.circuit_breaker.is_call_permitted()
    }
}

impl Default for GitHubClient {
    fn default() -> Self {
        Self::new()
    }
}
