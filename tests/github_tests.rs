use livecards::errors::GitHubError;
use livecards::github::{CacheEntry, Repository};

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
    let cache_entry = CacheEntry::Valid(repo.clone());

    match cache_entry {
        CacheEntry::Valid(cached_repo) => {
            assert_eq!(cached_repo.name, repo.name);
            assert_eq!(cached_repo.description, repo.description);
            assert_eq!(cached_repo.language, repo.language);
            assert_eq!(cached_repo.stargazers_count, repo.stargazers_count);
            assert_eq!(cached_repo.forks_count, repo.forks_count);
        }
        CacheEntry::Invalid(_, _) => panic!("Expected Valid cache entry"),
        CacheEntry::InvalidExhausted(_) => panic!("Expected Valid cache entry"),
    }
}

#[tokio::test]
async fn test_cache_entry_invalid() {
    let error = GitHubError::NotFound;
    let retry_count = 2;
    let cache_entry = CacheEntry::Invalid(error.clone(), retry_count);

    match cache_entry {
        CacheEntry::Valid(_) => panic!("Expected Invalid cache entry"),
        CacheEntry::Invalid(cached_error, count) => {
            assert!(matches!(cached_error, GitHubError::NotFound));
            assert_eq!(count, retry_count);
        }
        CacheEntry::InvalidExhausted(_) => {
            panic!("Expected Invalid cache entry, not InvalidExhausted")
        }
    }
}

#[tokio::test]
async fn test_cache_entry_invalid_exhausted() {
    let error = GitHubError::NotFound;
    let cache_entry = CacheEntry::InvalidExhausted(error.clone());

    match cache_entry {
        CacheEntry::Valid(_) => panic!("Expected InvalidExhausted cache entry"),
        CacheEntry::Invalid(_, _) => panic!("Expected InvalidExhausted cache entry, not Invalid"),
        CacheEntry::InvalidExhausted(cached_error) => {
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

    let valid_entry = CacheEntry::Valid(repo.clone());
    let invalid_entry = CacheEntry::Invalid(not_found_error.clone(), 2);
    let exhausted_entry = CacheEntry::InvalidExhausted(network_error.clone());

    // Test pattern matching works for all variants
    match valid_entry {
        CacheEntry::Valid(_) => (), // No assertion needed for successful match
        _ => panic!("Expected Valid"),
    }

    match invalid_entry {
        CacheEntry::Invalid(error, count) => {
            assert!(matches!(error, GitHubError::NotFound));
            assert_eq!(count, 2);
        }
        _ => panic!("Expected Invalid"),
    }

    match exhausted_entry {
        CacheEntry::InvalidExhausted(error) => {
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
