use std::path::PathBuf;

use crate::{CacheManager, HttpResponse, Result};

use http_cache_semantics::CachePolicy;
use serde::{Deserialize, Serialize};

/// Implements [`CacheManager`] with [`cacache`](https://github.com/zkat/cacache-rs) as the backend.
#[derive(Clone)]
pub struct CACacheManager {
    /// Directory where the cache will be stored.
    pub path: PathBuf,
    /// Options for removing cache entries.
    pub remove_opts: cacache::RemoveOpts,
}

impl std::fmt::Debug for CACacheManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CACacheManager").field("path", &self.path).finish()
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
// The metadata field was added alongside postcard, so bincode cache
// data will never contain it.
#[cfg(all(feature = "bincode", not(feature = "postcard")))]
#[derive(Debug, Deserialize, Serialize)]
struct Store {
    response: LegacyHttpResponse,
    policy: CachePolicy,
}

#[cfg(all(feature = "bincode", not(feature = "postcard")))]
use crate::{HttpHeaders, HttpVersion, Url};

#[cfg(all(feature = "bincode", not(feature = "postcard")))]
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

#[cfg(all(feature = "bincode", not(feature = "postcard")))]
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

#[cfg(all(feature = "bincode", not(feature = "postcard")))]
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

#[allow(dead_code)]
impl CACacheManager {
    /// Creates a new [`CACacheManager`] with the given path.
    pub fn new(path: PathBuf, remove_fully: bool) -> Self {
        Self {
            path,
            remove_opts: cacache::RemoveOpts::new().remove_fully(remove_fully),
        }
    }

    /// Clears out the entire cache.
    pub async fn clear(&self) -> Result<()> {
        cacache::clear(&self.path).await?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl CacheManager for CACacheManager {
    async fn get(
        &self,
        cache_key: &str,
    ) -> Result<Option<(HttpResponse, CachePolicy)>> {
        let store: Store = match cacache::read(&self.path, cache_key).await {
            Ok(d) => {
                #[cfg(feature = "postcard")]
                {
                    postcard::from_bytes(&d)?
                }
                #[cfg(all(feature = "bincode", not(feature = "postcard")))]
                {
                    bincode::deserialize(&d)?
                }
            }
            Err(_e) => {
                return Ok(None);
            }
        };

        #[cfg(feature = "postcard")]
        {
            Ok(Some((store.response, store.policy)))
        }
        #[cfg(all(feature = "bincode", not(feature = "postcard")))]
        {
            Ok(Some((store.response.into(), store.policy)))
        }
    }

    async fn put(
        &self,
        cache_key: String,
        response: HttpResponse,
        policy: CachePolicy,
    ) -> Result<HttpResponse> {
        #[cfg(feature = "postcard")]
        let data = Store { response, policy };
        #[cfg(all(feature = "bincode", not(feature = "postcard")))]
        let data = Store { response: response.into(), policy };

        #[cfg(feature = "postcard")]
        let bytes = postcard::to_allocvec(&data)?;
        #[cfg(all(feature = "bincode", not(feature = "postcard")))]
        let bytes = bincode::serialize(&data)?;

        cacache::write(&self.path, cache_key, bytes).await?;

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
        self.remove_opts.clone().remove(&self.path, cache_key).await?;
        Ok(())
    }
}
