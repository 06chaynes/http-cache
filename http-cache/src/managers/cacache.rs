use std::path::PathBuf;

use crate::{CacheManager, HttpResponse, Parts, Result};

use http_cache_semantics::CachePolicy;
use serde::{Deserialize, Serialize};

/// Implements [`CacheManager`] with [`cacache`](https://github.com/zkat/cacache-rs) as the backend.
#[cfg_attr(docsrs, doc(cfg(feature = "manager-cacache")))]
#[derive(Debug, Clone)]
pub struct CACacheManager {
    /// Directory where the cache will be stored.
    pub path: PathBuf,
}

impl Default for CACacheManager {
    fn default() -> Self {
        Self { path: "./http-cacache".into() }
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct Store {
    response: HttpResponse<Vec<u8>>,
    policy: CachePolicy,
}

// Cache binary value layout:
// [u32 - size of the NoBodyStore][NoBodyStore][response body bytes]
// bincode works with pre-defined slice of bytes, so we need this u32 in front.

// FIXME: this layout needs to be used for both CacheManager impls.
// Otherwise user won't be able to use different impls with the same cache instance.

#[derive(Debug, Deserialize, Serialize)]
struct NoBodyStore {
    parts: Parts,
    policy: CachePolicy,
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
        cache_key: &str,
    ) -> Result<Option<(HttpResponse<Vec<u8>>, CachePolicy)>> {
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
        response: HttpResponse<Vec<u8>>,
        policy: CachePolicy,
    ) -> Result<HttpResponse<Vec<u8>>> {
        let data = Store { response: response.clone(), policy };
        let bytes = bincode::serialize(&data)?;
        cacache::write(&self.path, cache_key, bytes).await?;
        Ok(response)
    }

    async fn delete(&self, cache_key: &str) -> Result<()> {
        Ok(cacache::remove(&self.path, cache_key).await?)
    }
}

#[cfg(feature = "cacache-async-std")]
use futures_lite::{AsyncReadExt, AsyncWriteExt};
#[cfg(feature = "cacache-tokio")]
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[async_trait::async_trait]
impl CacheManager<cacache::Reader> for CACacheManager {
    async fn get(
        &self,
        cache_key: &str,
    ) -> Result<Option<(HttpResponse<cacache::Reader>, CachePolicy)>> {
        // FIXME: match "entry not found" error to return None if occured
        let mut reader = cacache::Reader::open(&self.path, cache_key).await?;

        // Reading "header" part length
        let mut buf = [0u8; 4];
        reader.read_exact(&mut buf).await?;
        let store_len = u32::from_le_bytes(buf);

        // Reading "header" part
        let mut buf = Vec::<u8>::with_capacity(store_len as usize);
        reader.read_exact(buf.as_mut_slice()).await?;
        let store: NoBodyStore = bincode::deserialize(&buf)?;

        Ok(Some((HttpResponse::from_parts(store.parts, reader), store.policy)))
    }

    async fn put(
        &self,
        cache_key: String,
        response: HttpResponse<cacache::Reader>,
        policy: CachePolicy,
    ) -> Result<HttpResponse<cacache::Reader>> {
        let mut writer =
            cacache::Writer::create(&self.path, &cache_key).await?;
        let (parts, mut body) = response.into_parts();
        let data = NoBodyStore { parts, policy };
        let bytes = bincode::serialize(&data)?;
        writer.write_all(&bytes).await?;

        // I don't know why cacache::Reader does not implement AsyncBufRead
        // Perform bufferized reading manually
        const BUF_SIZE: usize = 1024;
        let mut buf = [0u8; BUF_SIZE];
        loop {
            let len = body.read(&mut buf).await?;
            writer.write_all(&buf[..len]).await?;
            if len == 0 {
                break;
            }
        }
        writer.commit().await?;
        // FIXME: provide error instead of unwrapping
        Ok(self.get(&cache_key).await?.unwrap().0)
    }

    async fn delete(&self, cache_key: &str) -> Result<()> {
        Ok(cacache::remove(&self.path, cache_key).await?)
    }
}
