//! The caching system for `glim`.
//!
//! This module provides a two-tier caching system (L1 In-Memory, L2 On-Disk)
//! for storing generated card images. It uses a "meaning-based" keying system
//! where the cache key is a hash of the parameters used to generate the image.
//!
//! The core of this system is the `foyer` hybrid cache library, which handles
//! the complexities of memory/disk tiers, eviction, and concurrency.
//!
//! # Example
//!
//! ```rust
//! use glim::cache::{init, cache, CacheConfig, RepositoryCard};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Initialize the cache
//!     let config = CacheConfig {
//!         disk_capacity: 1024 * 1024 * 1024, // 1 GB
//!         disk_path: "/tmp/glim_cache".to_string(),
//!     };
//!     init(config).await?;
//!
//!     // Use the cache
//!     let meaning = RepositoryCard {
//!         owner: "rust-lang".to_string(),
//!         repo: "rust".to_string(),
//!         theme: "dark".to_string(),
//!     };
//!
//!     let image_data = cache()
//!         .get_or_create(meaning, || async {
//!             // This function only runs on cache miss
//!             // Generate the image here...
//!             Ok(b"generated_image_data".to_vec())
//!         })
//!         .await?;
//!
//!     println!("Image size: {} bytes", image_data.image_data.len());
//!     Ok(())
//! }
//! ```
//!
//! # Custom Cacheable Types
//!
//! You can implement the `Cacheable` trait for your own types:
//!
//! ```rust
//! use glim::cache::Cacheable;
//!
//! #[derive(Clone, Serialize, Deserialize)]
//! struct MyCustomType {
//!     pub user_id: String,
//!     pub template: String,
//!     pub settings: Vec<String>,
//! }
//!
//! impl Cacheable for MyCustomType {
//!     fn cache_key(&self) -> String {
//!         format!("user:{}:template:{}:settings:{}",
//!                 self.user_id, self.template, self.settings.join(","))
//!     }
//!
//!     fn owner(&self) -> &str { &self.user_id }
//!     fn repo(&self) -> &str { "custom" }
//!     fn theme(&self) -> &str { &self.template }
//! }
//! ```

use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::Arc;
use std::time::SystemTime;

use foyer::{HybridCache, HybridCacheBuilder};
use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};

#[derive(thiserror::Error, Debug)]
pub enum CacheError {
    #[error("Failed to build or initialize cache: {0}")]
    Init(String),
    #[error("Error from the image generation function: {0}")]
    Create(#[from] anyhow::Error),
    #[error("Failed to serialize cache key: {0}")]
    Serialization(#[from] bincode::error::EncodeError),
    #[error("Foyer cache error: {0}")]
    Foyer(#[from] foyer::Error),
}

pub type Result<T> = std::result::Result<T, CacheError>;

/// Global static cache instance, initialized once at runtime.
static CACHE: OnceCell<CacheManager<RepositoryCard>> = OnceCell::new();

/// Initializes the global cache. This must be called once at application startup.
pub async fn init(config: CacheConfig) -> Result<()> {
    let manager = CacheManager::new(config).await?;
    CACHE
        .set(manager)
        .map_err(|_| CacheError::Init("Cache already initialized".to_string()))?;
    Ok(())
}

/// Returns a handle to the globally initialized cache.
/// Panics if the cache has not been initialized with `init()`.
pub fn cache() -> &'static CacheManager<RepositoryCard> {
    CACHE.get().expect("Cache has not been initialized")
}

/// Configuration for the caching system.
#[derive(Debug, Clone)]
pub struct CacheConfig {
    /// The maximum capacity of the on-disk cache in bytes.
    pub disk_capacity: u64,
    /// The path to the directory where the on-disk cache will be stored.
    pub disk_path: String,
}

/// A cloneable, thread-safe handle to the cache system.
#[derive(Clone)]
pub struct CacheManager<
    T: Cacheable + Send + Sync + Serialize + for<'de> Deserialize<'de> + Clone + 'static,
> {
    inner: Arc<HybridCache<u64, CacheValue<T>>>,
}

impl<T: Cacheable + Send + Sync + Serialize + for<'de> Deserialize<'de> + Clone + 'static>
    CacheManager<T>
{
    /// Creates a new `CacheManager` and initializes the underlying hybrid cache.
    pub async fn new(config: CacheConfig) -> Result<Self> {
        let hybrid = HybridCacheBuilder::new()
            .memory(128 * 1024 * 1024) // 128 MiB in-memory cache
            .with_weighter(|_key, value: &CacheValue<T>| {
                // Less valuable items have a higher cost, so they take up more "space"
                // in the cache and are evicted sooner.
                let value_score = value.meaning.owner().len() + value.meaning.repo().len(); // Placeholder for stars/forks
                let cost = (10000.0 / (value_score + 1) as f32) as u32;
                cost.max(1) as usize // Cost must be at least 1.
            })
            .storage(foyer::Engine::Large(foyer::LargeEngineOptions::new()))
            .with_device_options(
                foyer::DirectFsDeviceOptions::new(config.disk_path)
                    .with_capacity(config.disk_capacity.try_into().unwrap()),
            )
            .build()
            .await
            .map_err(CacheError::Foyer)?;

        Ok(Self {
            inner: Arc::new(hybrid),
        })
    }

    /// Gets a cached item or creates it if it doesn't exist.
    ///
    /// This is the primary API for interacting with the cache. It will:
    /// 1. Hash the provided `meaning` to generate a stable cache key
    /// 2. Check if the item exists in the cache (L1 memory or L2 disk)
    /// 3. If it exists, return the cached value
    /// 4. If it doesn't exist, execute the `create_fn` to generate the value
    /// 5. Store the newly created value in the cache and return it
    ///
    /// The `create_fn` is an async closure that takes no parameters and returns
    /// a `Result<Vec<u8>>` containing the image data.
    pub async fn get_or_create<F, Fut>(&self, meaning: T, create_fn: F) -> Result<CacheValue<T>>
    where
        F: FnOnce() -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = Result<Vec<u8>>> + Send,
    {
        let key = hash_cacheable(&meaning);
        let meaning_clone = meaning.clone();

        let cache_entry = match self
            .inner
            .fetch(key, move || async move {
                // This block only runs on a cache miss.
                let image_data = match create_fn().await {
                    Ok(data) => data,
                    Err(e) => {
                        return Err(foyer::Error::other(anyhow::anyhow!(
                            "Image generation failed: {}",
                            e
                        )))
                    }
                };

                let value = CacheValue {
                    image_data,
                    meaning: meaning_clone,
                    access_count: 1,
                    created_at: SystemTime::now(),
                };

                Ok(value)
            })
            .await
        {
            Ok(entry) => entry,
            Err(e) => return Err(CacheError::Foyer(e)),
        };

        Ok(cache_entry.value().clone())
    }
}

/// A trait that defines the "meaning" of a cacheable item.
/// Implementations should contain all parameters that define the final output.
/// This allows different types to define their own caching semantics.
pub trait Cacheable {
    /// Returns a unique identifier for this cacheable item.
    /// This is used as the basis for the cache key.
    fn cache_key(&self) -> String;

    /// Returns the repository owner (for value-based weighting).
    fn owner(&self) -> &str;

    /// Returns the repository name (for value-based weighting).
    fn repo(&self) -> &str;

    /// Returns the theme or style identifier.
    fn theme(&self) -> &str;
}

/// A concrete implementation of `Cacheable` for repository cards.
#[derive(Debug, Serialize, Deserialize, Hash, PartialEq, Eq, Clone)]
pub struct RepositoryCard {
    pub owner: String,
    pub repo: String,
    pub theme: String,
}

impl Cacheable for RepositoryCard {
    fn cache_key(&self) -> String {
        format!("{}:{}/{}:{}", self.owner, self.repo, self.theme, "v1")
    }

    fn owner(&self) -> &str {
        &self.owner
    }

    fn repo(&self) -> &str {
        &self.repo
    }

    fn theme(&self) -> &str {
        &self.theme
    }
}

/// Legacy type alias for backward compatibility.
pub type Meaning = RepositoryCard;

/// The value stored in the cache, containing the image data and metadata for TTL calculation.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CacheValue<T: Cacheable> {
    /// The raw bytes of the generated WEBP image.
    pub image_data: Vec<u8>,
    /// The original meaning used to generate this value.
    pub meaning: T,
    /// A count of how many times this entry has been accessed.
    pub access_count: u32,
    /// The timestamp of when this entry was first created.
    pub created_at: SystemTime,
}

/// Calculates a stable, 64-bit hash for a given `Cacheable` item to use as a cache key.
fn hash_cacheable<T: Cacheable>(item: &T) -> u64 {
    let mut hasher = DefaultHasher::new();
    item.cache_key().hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_cache_basic_functionality() -> Result<()> {
        let temp_dir = tempdir()
            .map_err(|e| CacheError::Create(anyhow::anyhow!("Failed to create temp dir: {}", e)))?;
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
}
