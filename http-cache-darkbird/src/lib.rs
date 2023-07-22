mod error;

use http_cache::{CacheManager, HttpResponse, Result};

use std::{fmt, sync::Arc, time::SystemTime};

use darkbird::{
    document::{self, RangeField},
    Options, Storage, StorageType,
};
use http_cache_semantics::CachePolicy;
use serde::{Deserialize, Serialize};

/// Implements [`CacheManager`] with [`darkbird`](https://github.com/Rustixir/darkbird) as the backend.
#[derive(Clone)]
pub struct DarkbirdManager {
    /// The instance of `darkbird::Storage<String, Store>`
    pub cache: Arc<Storage<String, Store>>,
    /// Whether full text search should be enabled
    pub full_text: bool,
}

impl fmt::Debug for DarkbirdManager {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        // need to add more data, anything helpful
        f.debug_struct("DarkbirdManager").finish_non_exhaustive()
    }
}

/// The data stored in the cache
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Store {
    /// The HTTP response
    pub response: HttpResponse,
    /// The cache policy generated for the response
    pub policy: CachePolicy,
    /// The cache key for this entry
    pub cache_key: String,
    full_text: bool,
}

impl document::Document for Store {}

impl document::Indexer for Store {
    fn extract(&self) -> Vec<String> {
        vec![self.cache_key.clone()]
    }
}

impl document::Tags for Store {
    fn get_tags(&self) -> Vec<String> {
        vec![self.response.url.to_string()]
    }
}

impl document::Range for Store {
    fn get_fields(&self) -> Vec<RangeField> {
        vec![
            RangeField {
                name: String::from("age"),
                value: self.policy.age(SystemTime::now()).as_secs().to_string(),
            },
            RangeField {
                name: String::from("time_to_live"),
                value: self
                    .policy
                    .time_to_live(SystemTime::now())
                    .as_secs()
                    .to_string(),
            },
        ]
    }
}

impl document::MaterializedView for Store {
    fn filter(&self) -> Option<String> {
        if self.policy.is_stale(SystemTime::now()) {
            Some(String::from("stale"))
        } else {
            None
        }
    }
}

impl document::FullText for Store {
    fn get_content(&self) -> Option<String> {
        if self.full_text {
            Some(String::from_utf8_lossy(&self.response.body).to_string())
        } else {
            None
        }
    }
}

impl DarkbirdManager {
    /// Create a new manager with provided options
    pub async fn new(options: Options<'_>, full_text: bool) -> Result<Self> {
        Ok(Self {
            cache: Arc::new(Storage::<String, Store>::open(options).await?),
            full_text,
        })
    }

    /// Create a new manager with default options
    pub async fn new_with_defaults() -> Result<Self> {
        let ops = Options::new(
            ".",
            "http-darkbird",
            42,
            StorageType::RamCopies,
            true,
        );
        Self::new(ops, false).await
    }
}

#[async_trait::async_trait]
impl CacheManager for DarkbirdManager {
    async fn get(
        &self,
        cache_key: &str,
    ) -> Result<Option<(HttpResponse, CachePolicy)>> {
        let store: Store = match self.cache.lookup(&cache_key.to_string()) {
            Some(d) => d.value().clone(),
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
        let data = Store {
            response: response.clone(),
            policy,
            cache_key: cache_key.clone(),
            full_text: self.full_text,
        };
        let mut exists = false;
        if self.cache.lookup(&cache_key.to_string()).is_some() {
            exists = true;
        }
        if exists {
            self.delete(&cache_key).await?;
        }
        match self.cache.insert(cache_key, data).await {
            Ok(_) => {}
            Err(e) => {
                return Err(Box::new(error::Error::Put(e.to_string())));
            }
        };
        Ok(response)
    }

    async fn delete(&self, cache_key: &str) -> Result<()> {
        match self.cache.remove(cache_key.to_string()).await {
            Ok(_) => {}
            Err(e) => {
                return Err(Box::new(error::Error::Delete(e.to_string())));
            }
        };
        Ok(())
    }
}
