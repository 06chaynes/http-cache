use http_cache::{CacheManager, HttpResponse, Result};

use std::{fmt, sync::Arc};

use http_cache_semantics::CachePolicy;
use moka::future::Cache;

#[derive(Clone)]
pub struct MokaManager {
    pub cache: Arc<Cache<String, Store>>,
}

impl fmt::Debug for MokaManager {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("MokaManager").finish_non_exhaustive()
    }
}

impl Default for MokaManager {
    fn default() -> Self {
        Self::new(Cache::new(42))
    }
}

#[derive(Clone, Debug)]
pub struct Store {
    response: HttpResponse,
    policy: CachePolicy,
}

impl MokaManager {
    pub fn new(cache: Cache<String, Store>) -> Self {
        Self { cache: Arc::new(cache) }
    }
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
            Some(d) => d,
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
        let store = Store { response: response.clone(), policy };
        self.cache.insert(cache_key, store).await;
        self.cache.run_pending_tasks().await;
        Ok(response)
    }

    async fn delete(&self, cache_key: &str) -> Result<()> {
        self.cache.invalidate(cache_key).await;
        self.cache.run_pending_tasks().await;
        Ok(())
    }
}

#[cfg(test)]
mod test;