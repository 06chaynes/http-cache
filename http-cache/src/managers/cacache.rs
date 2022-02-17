use crate::{CacheManager, HttpResponse, Result};

use http_cache_semantics::CachePolicy;
use serde::{Deserialize, Serialize};
use url::Url;

/// Implements [`CacheManager`] with [`cacache`](https://github.com/zkat/cacache-rs) as the backend.
#[cfg_attr(docsrs, doc(cfg(feature = "manager-cacache")))]
#[derive(Debug, Clone)]
pub struct CACacheManager {
    /// Directory where the cache will be stored.
    pub path: String,
}

impl Default for CACacheManager {
    fn default() -> Self {
        Self { path: "./http-cacache".into() }
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

#[allow(dead_code)]
impl CACacheManager {
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
        method: &str,
        url: &Url,
    ) -> Result<Option<(HttpResponse, CachePolicy)>> {
        let store: Store =
            match cacache::read(&self.path, &req_key(method, url)).await {
                Ok(d) => bincode::deserialize(&d)?,
                Err(_e) => {
                    return Ok(None);
                }
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
        cacache::write(&self.path, &req_key(method, url), bytes).await?;
        Ok(response)
    }

    async fn delete(&self, method: &str, url: &Url) -> Result<()> {
        Ok(cacache::remove(&self.path, &req_key(method, url)).await?)
    }
}
