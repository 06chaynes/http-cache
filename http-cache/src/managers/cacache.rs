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

#[derive(Debug, Deserialize, Serialize)]
struct Store {
    response: HttpResponse,
    policy: CachePolicy,
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
            Ok(d) => bincode::deserialize(&d)?,
            Err(_e) => {
                return Ok(None);
            }
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
        let bytes = bincode::serialize(&data)?;
        cacache::write(&self.path, cache_key, bytes).await?;
        Ok(data.response)
    }

    async fn delete(&self, cache_key: &str) -> Result<()> {
        self.remove_opts.clone().remove(&self.path, cache_key).await?;
        Ok(())
    }
}
