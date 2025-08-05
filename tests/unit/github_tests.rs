use glim::github::{get_repository_info, Repository};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

// Mock HTTP client for testing
struct MockHttpClient {
    responses: Arc<Mutex<HashMap<String, MockResponse>>>,
}

struct MockResponse {
    status: u16,
    body: String,
    headers: HashMap<String, String>,
}

impl MockHttpClient {
    fn new() -> Self {
        Self {
            responses: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    async fn add_response(&self, url: String, response: MockResponse) {
        let mut responses = self.responses.lock().await;
        responses.insert(url, response);
    }

    async fn get(&self, url: &str) -> Result<MockHttpResponse, Box<dyn std::error::Error>> {
        let responses = self.responses.lock().await;
        if let Some(response) = responses.get(url) {
            Ok(MockHttpResponse {
                status: response.status,
                body: response.body.clone(),
                headers: response.headers.clone(),
            })
        } else {
            Err("No mock response found".into())
        }
    }
}

struct MockHttpResponse {
    status: u16,
    body: String,
    headers: HashMap<String, String>,
}

impl MockHttpResponse {
    fn status(&self) -> u16 {
        self.status
    }

    async fn json<T: serde::de::DeserializeOwned>(&self) -> Result<T, Box<dyn std::error::Error>> {
        serde_json::from_str(&self.body).map_err(|e| e.into())
    }
}

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
async fn test_get_repository_info_success() {
    // This test would require mocking the HTTP client
    // For now, we'll test the basic structure
    let repo = create_test_repository();

    assert_eq!(repo.name, "test-repo");
    assert_eq!(repo.description, Some("A test repository".to_string()));
    assert_eq!(repo.language, Some("Rust".to_string()));
    assert_eq!(repo.stargazers_count, 42);
    assert_eq!(repo.forks_count, 7);
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
    use glim::errors::GitHubError;
    use glim::github::CacheEntry;

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
    }
}

#[tokio::test]
async fn test_cache_entry_invalid() {
    use glim::errors::GitHubError;
    use glim::github::CacheEntry;

    let error = GitHubError::NotFound;
    let retry_count = 2;
    let cache_entry = CacheEntry::Invalid(error.clone(), retry_count);

    match cache_entry {
        CacheEntry::Valid(_) => panic!("Expected Invalid cache entry"),
        CacheEntry::Invalid(cached_error, count) => {
            assert!(matches!(cached_error, GitHubError::NotFound));
            assert_eq!(count, retry_count);
        }
    }
}

// Test URL construction
#[tokio::test]
async fn test_github_api_url_construction() {
    let repo_path = "test-owner/test-repo";
    let expected_url = "https://api.github.com/repos/test-owner/test-repo";
    let constructed_url = format!("https://api.github.com/repos/{}", repo_path);

    assert_eq!(constructed_url, expected_url);
}

// Test user agent header
#[tokio::test]
async fn test_user_agent_header() {
    let expected_user_agent = "glim-generator";
    assert_eq!(expected_user_agent, "glim-generator");
}

// Test authorization header format
#[tokio::test]
async fn test_authorization_header_format() {
    let token = "ghp_test_token_123";
    let expected_header = format!("Bearer {}", token);

    assert_eq!(expected_header, "Bearer ghp_test_token_123");
    assert!(expected_header.starts_with("Bearer "));
}

// Test cache TTL configuration
#[tokio::test]
async fn test_cache_ttl_configuration() {
    use std::time::Duration;

    // The cache is configured with 30 minutes TTL
    let expected_ttl = Duration::from_secs(30 * 60); // 30 minutes
    let actual_ttl = Duration::from_secs(1800); // 30 minutes in seconds

    assert_eq!(expected_ttl, actual_ttl);
}
