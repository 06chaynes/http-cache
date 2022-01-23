use crate::{CacheManager, HttpResponse, Result};

use std::{fmt, sync::Arc};

use http_cache_semantics::CachePolicy;
use moka::future::{Cache, ConcurrentCacheExt};
use serde::{Deserialize, Serialize};
use url::Url;

/// Implements [`CacheManager`] with [`moka`](https://github.com/moka-rs/moka) as the backend.
#[derive(Clone)]
pub struct MokaManager {
    /// The instance of `moka::future::Cache`
    pub cache: Cache<String, Arc<Vec<u8>>>,
}

impl fmt::Debug for MokaManager {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        // need to add more data, anything helpful
        f.debug_struct("MokaManager").finish_non_exhaustive()
    }
}

impl Default for MokaManager {
    fn default() -> Self {
        MokaManager { cache: Cache::new(42) }
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct Store {
    response: HttpResponse,
    policy: CachePolicy,
}

fn req_key(method: &str, url: &Url) -> String {
    format!("{}:{}", method, url)
}

impl MokaManager {
    /// Clears out the entire cache.
    pub async fn clear(&self) -> Result<()> {
        self.cache.invalidate_all();
        self.cache.sync();
        Ok(())
    }
}

#[async_trait::async_trait]
impl CacheManager for Arc<MokaManager> {
    async fn get(
        &self,
        method: &str,
        url: &Url,
    ) -> Result<Option<(HttpResponse, CachePolicy)>> {
        let store: Store = match self.cache.get(&req_key(method, url)) {
            Some(d) => bincode::deserialize(&d)?,
            None => return Ok(None),
        };
        Ok(Some((store.response, store.policy)))
    }

    async fn put(
        &self,
        method: &str,
        url: &Url,
        response: HttpResponse,
        policy: CachePolicy,
    ) -> Result<HttpResponse> {
        let data = Store { response: response.clone(), policy };
        let bytes = bincode::serialize(&data)?;
        self.cache.insert(req_key(method, url), Arc::new(bytes)).await;
        self.cache.sync();
        Ok(response)
    }

    async fn delete(&self, method: &str, url: &Url) -> Result<()> {
        self.cache.invalidate(&req_key(method, url)).await;
        self.cache.sync();
        Ok(())
    }
}
