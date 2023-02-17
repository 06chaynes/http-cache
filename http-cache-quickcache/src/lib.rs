use http_cache::{CacheManager, HttpResponse, Result};

use std::{fmt, sync::Arc};

use http_cache_semantics::CachePolicy;
use quick_cache::sync::Cache;
use serde::{Deserialize, Serialize};
use url::Url;

/// Implements [`CacheManager`] with [`quick-cache`](https://github.com/arthurprs/quick-cache) as the backend.
#[derive(Clone)]
pub struct QuickManager {
    /// The instance of `quick_cache::sync::Cache`
    pub cache: Arc<Cache<String, Arc<Vec<u8>>>>,
}

impl fmt::Debug for QuickManager {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        // need to add more data, anything helpful
        f.debug_struct("QuickManager").finish_non_exhaustive()
    }
}

impl Default for QuickManager {
    fn default() -> Self {
        Self::new(Cache::new(42))
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct Store {
    response: HttpResponse,
    policy: CachePolicy,
}

fn req_key(method: &str, url: &Url) -> String {
    format!("{method}:{url}")
}

impl QuickManager {
    /// Create a new manager from a pre-configured Cache
    pub fn new(cache: Cache<String, Arc<Vec<u8>>>) -> Self {
        Self { cache: Arc::new(cache) }
    }
}

#[async_trait::async_trait]
impl CacheManager for QuickManager {
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
        self.cache.insert(req_key(method, url), Arc::new(bytes));
        Ok(response)
    }

    async fn delete(&self, method: &str, url: &Url) -> Result<()> {
        self.cache.remove(&req_key(method, url));
        Ok(())
    }
}
