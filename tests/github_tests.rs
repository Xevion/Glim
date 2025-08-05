use glim::errors::GitHubError;
use glim::github::{CacheEntry, Repository};

// Test fixtures
fn create_test_repository() -> Repository {
    Repository {
        name: "test-repo".to_string(),
        description: Some("A test repository".to_string()),
        language: Some("Rust".to_string()),
        stargazers_count: 42,
        forks_count: 7,
    }
}

fn create_test_repository_json() -> String {
    r#"{
        "name": "test-repo",
        "description": "A test repository",
        "language": "Rust",
        "stargazers_count": 42,
        "forks_count": 7
    }"#
    .to_string()
}

#[tokio::test]
async fn test_repository_deserialization() {
    let json = create_test_repository_json();
    let repo: Repository = serde_json::from_str(&json).unwrap();

    assert_eq!(repo.name, "test-repo");
    assert_eq!(repo.description, Some("A test repository".to_string()));
    assert_eq!(repo.language, Some("Rust".to_string()));
    assert_eq!(repo.stargazers_count, 42);
    assert_eq!(repo.forks_count, 7);
}

#[tokio::test]
async fn test_repository_with_null_fields() {
    let json = r#"{
        "name": "test-repo",
        "description": null,
        "language": null,
        "stargazers_count": 0,
        "forks_count": 0
    }"#;

    let repo: Repository = serde_json::from_str(json).unwrap();

    assert_eq!(repo.name, "test-repo");
    assert_eq!(repo.description, None);
    assert_eq!(repo.language, None);
    assert_eq!(repo.stargazers_count, 0);
    assert_eq!(repo.forks_count, 0);
}

#[tokio::test]
async fn test_cache_entry_valid() {
    let repo = create_test_repository();
    let cache_entry = CacheEntry::Valid { data: repo.clone() };

    match cache_entry {
        CacheEntry::Valid { data: cached_repo } => {
            assert_eq!(cached_repo.name, repo.name);
            assert_eq!(cached_repo.description, repo.description);
            assert_eq!(cached_repo.language, repo.language);
            assert_eq!(cached_repo.stargazers_count, repo.stargazers_count);
            assert_eq!(cached_repo.forks_count, repo.forks_count);
        }
        CacheEntry::Invalid {
            error: _,
            remaining: _,
        } => panic!("Expected Valid cache entry"),
        CacheEntry::InvalidExhausted { error: _ } => panic!("Expected Valid cache entry"),
    }
}

#[tokio::test]
async fn test_cache_entry_invalid() {
    let error = GitHubError::NotFound;
    let retry_count = 2;
    let cache_entry = CacheEntry::Invalid {
        error: error.clone(),
        remaining: retry_count,
    };

    match cache_entry {
        CacheEntry::Valid { data: _ } => panic!("Expected Invalid cache entry"),
        CacheEntry::Invalid {
            error: cached_error,
            remaining: count,
        } => {
            assert!(matches!(cached_error, GitHubError::NotFound));
            assert_eq!(count, retry_count);
        }
        CacheEntry::InvalidExhausted { error: _ } => {
            panic!("Expected Invalid cache entry, not InvalidExhausted")
        }
    }
}

#[tokio::test]
async fn test_cache_entry_invalid_exhausted() {
    let error = GitHubError::NotFound;
    let cache_entry = CacheEntry::InvalidExhausted {
        error: error.clone(),
    };

    match cache_entry {
        CacheEntry::Valid { data: _ } => panic!("Expected InvalidExhausted cache entry"),
        CacheEntry::Invalid {
            error: _,
            remaining: _,
        } => panic!("Expected InvalidExhausted cache entry, not Invalid"),
        CacheEntry::InvalidExhausted {
            error: cached_error,
        } => {
            assert!(matches!(cached_error, GitHubError::NotFound));
        }
    }
}

#[tokio::test]
async fn test_cache_entry_variants() {
    // Test that we can handle all cache entry variants
    let repo = create_test_repository();
    let not_found_error = GitHubError::NotFound;
    let network_error = GitHubError::NetworkError;

    let valid_entry = CacheEntry::Valid { data: repo.clone() };
    let invalid_entry = CacheEntry::Invalid {
        error: not_found_error.clone(),
        remaining: 2,
    };
    let exhausted_entry = CacheEntry::InvalidExhausted {
        error: network_error.clone(),
    };

    // Test pattern matching works for all variants
    match valid_entry {
        CacheEntry::Valid { data: _ } => (), // No assertion needed for successful match
        _ => panic!("Expected Valid"),
    }

    match invalid_entry {
        CacheEntry::Invalid {
            error,
            remaining: count,
        } => {
            assert!(matches!(error, GitHubError::NotFound));
            assert_eq!(count, 2);
        }
        _ => panic!("Expected Invalid"),
    }

    match exhausted_entry {
        CacheEntry::InvalidExhausted { error } => {
            assert!(matches!(error, GitHubError::NetworkError));
        }
        _ => panic!("Expected InvalidExhausted"),
    }
}

// Test error handling scenarios
#[tokio::test]
async fn test_github_error_variants() {
    let test_cases = [
        (GitHubError::NotFound, "Repository not found"),
        (GitHubError::RateLimited, "GitHub API rate limit exceeded"),
        (GitHubError::ApiError(500), "GitHub API error: 500"),
        (
            GitHubError::NetworkError,
            "Network error while contacting GitHub API",
        ),
        (
            GitHubError::InvalidFormat("invalid/repo/format".to_string()),
            "Invalid repository format: invalid/repo/format",
        ),
        (
            GitHubError::AuthError("Invalid token".to_string()),
            "Authentication failed: Invalid token",
        ),
    ];

    for (error, expected_message) in test_cases {
        assert_eq!(error.to_string(), expected_message);
    }
}

#[test]
fn test_should_trigger_circuit_breaker_logic() {
    use glim::errors::GitHubError;
    use glim::github::GitHubClient;

    let test_cases = [
        (GitHubError::NetworkError, true),
        (GitHubError::RateLimited, true),
        (GitHubError::ApiError(500), true),
        (GitHubError::ApiError(502), true),
        (GitHubError::ApiError(503), true),
        (GitHubError::NotFound, false),
        (GitHubError::ApiError(400), false),
        (GitHubError::ApiError(401), false),
        (GitHubError::ApiError(404), false),
        (GitHubError::ApiError(422), false),
        (GitHubError::InvalidFormat("test".to_string()), false),
        (GitHubError::AuthError("test".to_string()), false),
    ];

    for (error, should_trigger) in test_cases {
        assert_eq!(
            GitHubClient::should_trigger_circuit_breaker(&error),
            should_trigger
        );
    }
}

// Circuit breaker tests
#[tokio::test]
async fn test_circuit_breaker_initial_state() {
    use glim::github::GitHubClient;

    let client = GitHubClient::new();

    // Circuit breaker should be closed initially (allowing calls)
    assert!(!client.disabled());
}

#[tokio::test]
async fn test_circuit_breaker_opens_after_failures() {
    use glim::github::GitHubClient;

    let client = GitHubClient::new();

    // Simulate consecutive failures that should trigger circuit breaker
    // The circuit breaker needs more failures to open (configured for 5 consecutive failures)
    for _ in 0..20 {
        client.circuit_breaker().on_error();
    }

    // Circuit breaker should be open after multiple failures
    assert!(client.disabled());
}

#[tokio::test]
async fn test_circuit_breaker_success_tracking() {
    use glim::github::GitHubClient;

    let client = GitHubClient::new();

    // Test that circuit breaker starts in closed state (allowing calls)
    assert!(client.circuit_breaker().is_call_permitted());

    // Simulate some successes
    for _ in 0..5 {
        client.circuit_breaker().on_success();
    }

    // Circuit breaker should still allow calls after successes
    assert!(!client.disabled());
}

// GitHub API integration tests
#[tokio::test]
async fn test_github_client_creation() {
    use glim::github::GitHubClient;

    let client = GitHubClient::new();

    // Client should be created successfully
    assert!(!client.disabled());
}

#[tokio::test]
async fn test_cache_hit_and_miss() {
    use glim::github::{CacheEntry, GitHubClient};

    let client = GitHubClient::new();
    let repo_path = "test/owner";

    // Initially, cache should be empty
    assert!(client.cache.get(repo_path).await.is_none());

    // Insert a valid cache entry
    let test_repo = create_test_repository();
    client
        .cache
        .insert(
            repo_path.to_string(),
            CacheEntry::Valid {
                data: test_repo.clone(),
            },
        )
        .await;

    // Cache should now have the entry
    let cached = client.cache.get(repo_path).await;
    assert!(cached.is_some());

    match cached.unwrap() {
        CacheEntry::Valid { data } => {
            assert_eq!(data.name, test_repo.name);
            assert_eq!(data.stargazers_count, test_repo.stargazers_count);
        }
        _ => panic!("Expected Valid cache entry"),
    }
}

#[tokio::test]
async fn test_cache_invalid_entry_retry_logic() {
    use glim::errors::GitHubError;
    use glim::github::{CacheEntry, GitHubClient};

    let client = GitHubClient::new();
    let repo_path = "test/owner";

    // Insert an invalid entry with remaining retries
    client
        .cache
        .insert(
            repo_path.to_string(),
            CacheEntry::Invalid {
                error: GitHubError::NetworkError,
                remaining: 2,
            },
        )
        .await;

    // Cache should have the invalid entry
    let cached = client.cache.get(repo_path).await;
    assert!(cached.is_some());

    match cached.unwrap() {
        CacheEntry::Invalid { error, remaining } => {
            assert!(matches!(error, GitHubError::NetworkError));
            assert_eq!(remaining, 2);
        }
        _ => panic!("Expected Invalid cache entry"),
    }
}

#[tokio::test]
async fn test_cache_exhausted_entry() {
    use glim::errors::GitHubError;
    use glim::github::{CacheEntry, GitHubClient};

    let client = GitHubClient::new();
    let repo_path = "test/owner";

    // Insert an exhausted entry
    client
        .cache
        .insert(
            repo_path.to_string(),
            CacheEntry::InvalidExhausted {
                error: GitHubError::NotFound,
            },
        )
        .await;

    // Cache should have the exhausted entry
    let cached = client.cache.get(repo_path).await;
    assert!(cached.is_some());

    match cached.unwrap() {
        CacheEntry::InvalidExhausted { error } => {
            assert!(matches!(error, GitHubError::NotFound));
        }
        _ => panic!("Expected InvalidExhausted cache entry"),
    }
}

// Test error handling with circuit breaker
#[tokio::test]
async fn test_circuit_breaker_with_network_errors() {
    use glim::errors::GitHubError;
    use glim::github::GitHubClient;

    let _client = GitHubClient::new();

    // Test that network errors trigger circuit breaker
    assert!(GitHubClient::should_trigger_circuit_breaker(
        &GitHubError::NetworkError
    ));

    // Test that rate limit errors trigger circuit breaker
    assert!(GitHubClient::should_trigger_circuit_breaker(
        &GitHubError::RateLimited
    ));

    // Test that 5xx errors trigger circuit breaker
    assert!(GitHubClient::should_trigger_circuit_breaker(
        &GitHubError::ApiError(500)
    ));
    assert!(GitHubClient::should_trigger_circuit_breaker(
        &GitHubError::ApiError(502)
    ));
    assert!(GitHubClient::should_trigger_circuit_breaker(
        &GitHubError::ApiError(503)
    ));
}

#[tokio::test]
async fn test_circuit_breaker_with_client_errors() {
    use glim::errors::GitHubError;
    use glim::github::GitHubClient;

    let _client = GitHubClient::new();

    // Test that 4xx errors do NOT trigger circuit breaker
    assert!(!GitHubClient::should_trigger_circuit_breaker(
        &GitHubError::NotFound
    ));
    assert!(!GitHubClient::should_trigger_circuit_breaker(
        &GitHubError::ApiError(400)
    ));
    assert!(!GitHubClient::should_trigger_circuit_breaker(
        &GitHubError::ApiError(401)
    ));
    assert!(!GitHubClient::should_trigger_circuit_breaker(
        &GitHubError::ApiError(422)
    ));

    // Test that auth errors do NOT trigger circuit breaker
    assert!(!GitHubClient::should_trigger_circuit_breaker(
        &GitHubError::AuthError("test".to_string())
    ));

    // Test that format errors do NOT trigger circuit breaker
    assert!(!GitHubClient::should_trigger_circuit_breaker(
        &GitHubError::InvalidFormat("test".to_string())
    ));
}

// Test cache TTL behavior
#[tokio::test]
async fn test_cache_ttl_behavior() {
    use glim::github::{CacheEntry, GitHubClient};

    let client = GitHubClient::new();
    let repo_path = "test/ttl";

    // Insert a valid entry
    let test_repo = create_test_repository();
    client
        .cache
        .insert(
            repo_path.to_string(),
            CacheEntry::Valid {
                data: test_repo.clone(),
            },
        )
        .await;

    // Entry should be immediately available
    assert!(client.cache.get(repo_path).await.is_some());

    // Note: We can't easily test TTL expiration in unit tests without time manipulation
    // This would require integration tests with time mocking
}

// Test concurrent access to cache
#[tokio::test]
async fn test_cache_concurrent_access() {
    use glim::github::{CacheEntry, GitHubClient};
    use std::sync::Arc;
    use tokio::task;

    let client = Arc::new(GitHubClient::new());
    let repo_path = "test/concurrent";

    // Spawn multiple tasks to access cache concurrently
    let handles: Vec<_> = (0..10)
        .map(|i| {
            let client = Arc::clone(&client);
            let path = format!("{}/{}", repo_path, i);
            task::spawn(async move {
                let test_repo = create_test_repository();
                client
                    .cache
                    .insert(path.clone(), CacheEntry::Valid { data: test_repo })
                    .await;

                // Verify we can retrieve it
                client.cache.get(&path).await.is_some()
            })
        })
        .collect();

    // Wait for all tasks to complete
    for handle in handles {
        let result = handle.await;
        assert!(result.unwrap());
    }
}
