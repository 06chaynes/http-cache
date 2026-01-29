use std::fmt;

use crate::{CacheManager, HttpResponse, Result};

use foyer::{HybridCache, HybridCacheBuilder};
use http_cache_semantics::CachePolicy;
use serde::{Deserialize, Serialize};

/// Implements [`CacheManager`] with [`foyer`](https://github.com/foyer-rs/foyer) as the backend.
///
/// Foyer is a hybrid in-memory + disk cache that provides:
/// - Memory cache with configurable eviction strategies (w-TinyLFU, S3-FIFO, SIEVE)
/// - Optional disk cache for persistent storage
/// - Request deduplication
/// - Tokio-native async operations
///
/// # Example
///
/// ```rust,ignore
/// use http_cache::FoyerManager;
/// use foyer::HybridCacheBuilder;
/// use std::path::PathBuf;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// // Build a hybrid cache with memory and disk storage
/// let cache = HybridCacheBuilder::new()
///     .memory(64)
///     .storage()
///     .with_device_config(
///         foyer::DirectFsDeviceOptions::new(PathBuf::from("./cache"))
///             .with_capacity(256 * 1024 * 1024)
///     )
///     .build()
///     .await?;
///
/// let manager = FoyerManager::new(cache);
/// # Ok(())
/// # }
/// ```
#[cfg_attr(docsrs, doc(cfg(feature = "manager-foyer")))]
#[derive(Clone)]
pub struct FoyerManager {
    cache: HybridCache<String, Vec<u8>>,
}

impl fmt::Debug for FoyerManager {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("FoyerManager").finish_non_exhaustive()
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct Store {
    response: HttpResponse,
    policy: CachePolicy,
}

impl FoyerManager {
    /// Creates a new [`FoyerManager`] from a pre-configured [`HybridCache`].
    ///
    /// Use [`HybridCacheBuilder`] to configure memory size, disk storage,
    /// eviction strategy, and other options before passing to this constructor.
    pub fn new(cache: HybridCache<String, Vec<u8>>) -> Self {
        Self { cache }
    }

    /// Creates a new in-memory only [`FoyerManager`] with the specified capacity.
    ///
    /// This is a convenience method for creating a simple memory-only cache.
    /// For disk-backed caches or advanced configuration, use [`FoyerManager::new`]
    /// with a custom [`HybridCacheBuilder`].
    ///
    /// # Arguments
    ///
    /// * `capacity` - The number of entries the memory cache can hold
    pub async fn in_memory(capacity: usize) -> Result<Self> {
        let cache: HybridCache<String, Vec<u8>> = HybridCacheBuilder::new()
            .memory(capacity)
            .storage() // noop storage = memory-only mode
            .build()
            .await
            .map_err(|e| crate::HttpCacheError::cache(e.to_string()))?;
        Ok(Self { cache })
    }

    /// Closes the cache gracefully.
    ///
    /// This should be called before the application exits to ensure
    /// all data is flushed to disk (if using disk storage).
    pub async fn close(&self) -> Result<()> {
        self.cache
            .close()
            .await
            .map_err(|e| crate::HttpCacheError::cache(e.to_string()))?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl CacheManager for FoyerManager {
    async fn get(
        &self,
        cache_key: &str,
    ) -> Result<Option<(HttpResponse, CachePolicy)>> {
        match self.cache.get(&cache_key.to_string()).await {
            Ok(Some(entry)) => {
                let store: Store = postcard::from_bytes(entry.value())?;
                Ok(Some((store.response, store.policy)))
            }
            Ok(None) => Ok(None),
            Err(_) => Ok(None),
        }
    }

    async fn put(
        &self,
        cache_key: String,
        response: HttpResponse,
        policy: CachePolicy,
    ) -> Result<HttpResponse> {
        let data = Store { response, policy };
        let bytes = postcard::to_allocvec(&data)?;
        self.cache.insert(cache_key, bytes);
        Ok(data.response)
    }

    async fn delete(&self, cache_key: &str) -> Result<()> {
        self.cache.remove(&cache_key.to_string());
        Ok(())
    }
}
