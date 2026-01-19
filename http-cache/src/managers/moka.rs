use crate::{CacheManager, HttpResponse, Result};

use std::{fmt, sync::Arc};

use http_cache_semantics::CachePolicy;
use moka::future::Cache;
use serde::{Deserialize, Serialize};

/// Implements [`CacheManager`] with [`moka`](https://github.com/moka-rs/moka) as the backend.
#[cfg_attr(docsrs, doc(cfg(feature = "manager-moka")))]
#[derive(Clone)]
pub struct MokaManager {
    /// The instance of `moka::future::Cache`
    pub cache: Arc<Cache<String, Arc<Vec<u8>>>>,
}

impl fmt::Debug for MokaManager {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        // need to add more data, anything helpful
        f.debug_struct("MokaManager").finish_non_exhaustive()
    }
}

impl Default for MokaManager {
    fn default() -> Self {
        Self::new(Cache::new(42))
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct Store {
    response: HttpResponse,
    policy: CachePolicy,
}

impl MokaManager {
    /// Create a new manager from a pre-configured Cache
    pub fn new(cache: Cache<String, Arc<Vec<u8>>>) -> Self {
        Self { cache: Arc::new(cache) }
    }
    /// Clears out the entire cache.
    pub async fn clear(&self) -> Result<()> {
        self.cache.invalidate_all();
        self.cache.run_pending_tasks().await;
        Ok(())
    }
}

#[async_trait::async_trait]
impl CacheManager for MokaManager {
    async fn get(
        &self,
        cache_key: &str,
    ) -> Result<Option<(HttpResponse, CachePolicy)>> {
        let store: Store = match self.cache.get(cache_key).await {
            Some(d) => {
                #[cfg(feature = "postcard")]
                {
                    postcard::from_bytes(&d)?
                }
                #[cfg(all(feature = "bincode", not(feature = "postcard")))]
                {
                    bincode::deserialize(&d)?
                }
            }
            None => return Ok(None),
        };
        Ok(Some((store.response, store.policy)))
    }

    async fn put(
        &self,
        cache_key: String,
        response: HttpResponse,
        policy: CachePolicy,
    ) -> Result<HttpResponse> {
        let data = Store { response, policy };
        #[cfg(feature = "postcard")]
        let bytes = postcard::to_allocvec(&data)?;
        #[cfg(all(feature = "bincode", not(feature = "postcard")))]
        let bytes = bincode::serialize(&data)?;
        self.cache.insert(cache_key, Arc::new(bytes)).await;
        self.cache.run_pending_tasks().await;
        Ok(data.response)
    }

    async fn delete(&self, cache_key: &str) -> Result<()> {
        self.cache.invalidate(cache_key).await;
        self.cache.run_pending_tasks().await;
        Ok(())
    }
}
