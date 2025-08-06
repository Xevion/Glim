use glim::cache::{CacheConfig, CacheManager, Meaning};
use tempfile::tempdir;

#[tokio::test]
async fn test_cache_basic_functionality() -> glim::cache::Result<()> {
    let temp_dir = tempdir().map_err(|e| {
        glim::cache::CacheError::Create(anyhow::anyhow!("Failed to create temp dir: {}", e))
    })?;
    let config = CacheConfig {
        disk_capacity: 1024 * 1024, // 1 MB
        disk_path: temp_dir.path().to_string_lossy().to_string(),
    };

    let cache_manager = CacheManager::new(config).await?;

    let meaning = Meaning {
        owner: "test_owner".to_string(),
        repo: "test_repo".to_string(),
        theme: "dark".to_string(),
    };

    // Test cache miss and creation
    let result = cache_manager
        .get_or_create(meaning.clone(), || async {
            Ok(b"test_image_data".to_vec())
        })
        .await?;

    assert_eq!(result.image_data, b"test_image_data");
    assert_eq!(result.meaning, meaning);
    assert_eq!(result.access_count, 1);

    // Test cache hit
    let result2 = cache_manager
        .get_or_create(meaning, || async {
            panic!("This should not be called on cache hit");
        })
        .await?;

    assert_eq!(result2.image_data, b"test_image_data");

    Ok(())
}
