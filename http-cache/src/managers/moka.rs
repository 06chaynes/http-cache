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

// Modern store format (postcard) - includes metadata field
#[cfg(feature = "postcard")]
#[derive(Debug, Deserialize, Serialize)]
struct Store {
    response: HttpResponse,
    policy: CachePolicy,
}

// Legacy store format (bincode) - HttpResponse without metadata field
#[cfg(feature = "bincode")]
#[derive(Debug, Deserialize, Serialize)]
struct BincodeStore {
    response: LegacyHttpResponse,
    policy: CachePolicy,
}

#[cfg(feature = "bincode")]
use crate::{HttpHeaders, HttpVersion, Url};

#[cfg(feature = "bincode")]
#[derive(Debug, Clone, Deserialize, Serialize)]
struct LegacyHttpResponse {
    body: Vec<u8>,
    #[cfg(feature = "http-headers-compat")]
    headers: std::collections::HashMap<String, String>,
    #[cfg(not(feature = "http-headers-compat"))]
    headers: std::collections::HashMap<String, Vec<String>>,
    status: u16,
    url: Url,
    version: HttpVersion,
}

#[cfg(feature = "bincode")]
impl From<LegacyHttpResponse> for HttpResponse {
    fn from(legacy: LegacyHttpResponse) -> Self {
        #[cfg(feature = "http-headers-compat")]
        let headers = HttpHeaders::Legacy(legacy.headers);
        #[cfg(not(feature = "http-headers-compat"))]
        let headers = HttpHeaders::Modern(legacy.headers);

        HttpResponse {
            body: legacy.body,
            headers,
            status: legacy.status,
            url: legacy.url,
            version: legacy.version,
            metadata: None,
        }
    }
}

#[cfg(feature = "bincode")]
impl From<HttpResponse> for LegacyHttpResponse {
    fn from(response: HttpResponse) -> Self {
        #[cfg(feature = "http-headers-compat")]
        let headers = match response.headers {
            HttpHeaders::Legacy(h) => h,
            HttpHeaders::Modern(h) => {
                h.into_iter().map(|(k, v)| (k, v.join(", "))).collect()
            }
        };
        #[cfg(not(feature = "http-headers-compat"))]
        let headers = match response.headers {
            HttpHeaders::Modern(h) => h,
        };

        LegacyHttpResponse {
            body: response.body,
            headers,
            status: response.status,
            url: response.url,
            version: response.version,
        }
    }
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
        let d = match self.cache.get(cache_key).await {
            Some(d) => d,
            None => return Ok(None),
        };

        // When both postcard and bincode are enabled, try postcard first
        // then fall back to bincode (for reading legacy cache entries).
        // When only one format is enabled, use that format directly.
        #[cfg(feature = "postcard")]
        {
            match postcard::from_bytes::<Store>(&d) {
                Ok(store) => return Ok(Some((store.response, store.policy))),
                Err(_e) => {
                    #[cfg(feature = "bincode")]
                    {
                        match bincode::deserialize::<BincodeStore>(&d) {
                            Ok(store) => {
                                return Ok(Some((
                                    store.response.into(),
                                    store.policy,
                                )));
                            }
                            Err(e) => {
                                log::warn!(
                                    "Failed to deserialize cache entry for key '{}': {}",
                                    cache_key,
                                    e
                                );
                                return Ok(None);
                            }
                        }
                    }
                    #[cfg(not(feature = "bincode"))]
                    {
                        log::warn!(
                            "Failed to deserialize cache entry for key '{}': {}",
                            cache_key,
                            _e
                        );
                        return Ok(None);
                    }
                }
            }
        }
        #[cfg(all(feature = "bincode", not(feature = "postcard")))]
        {
            match bincode::deserialize::<BincodeStore>(&d) {
                Ok(store) => Ok(Some((store.response.into(), store.policy))),
                Err(e) => {
                    log::warn!(
                        "Failed to deserialize cache entry for key '{}': {}",
                        cache_key,
                        e
                    );
                    Ok(None)
                }
            }
        }
    }

    async fn put(
        &self,
        cache_key: String,
        response: HttpResponse,
        policy: CachePolicy,
    ) -> Result<HttpResponse> {
        // Always write with postcard when available (modern format).
        // Only use bincode when postcard is not enabled.
        #[cfg(feature = "postcard")]
        let data = Store { response, policy };
        #[cfg(all(feature = "bincode", not(feature = "postcard")))]
        let data = BincodeStore { response: response.into(), policy };

        #[cfg(feature = "postcard")]
        let bytes = postcard::to_allocvec(&data)?;
        #[cfg(all(feature = "bincode", not(feature = "postcard")))]
        let bytes = bincode::serialize(&data)?;

        self.cache.insert(cache_key, Arc::new(bytes)).await;
        self.cache.run_pending_tasks().await;

        #[cfg(feature = "postcard")]
        {
            Ok(data.response)
        }
        #[cfg(all(feature = "bincode", not(feature = "postcard")))]
        {
            Ok(data.response.into())
        }
    }

    async fn delete(&self, cache_key: &str) -> Result<()> {
        self.cache.invalidate(cache_key).await;
        self.cache.run_pending_tasks().await;
        Ok(())
    }
}
