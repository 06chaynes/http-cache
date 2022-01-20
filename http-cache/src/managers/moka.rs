use crate::{CacheManager, HttpResponse, Result};

use std::{fmt, sync::Arc};

use http_cache_semantics::CachePolicy;
use moka::future::Cache;
use serde::{Deserialize, Serialize};
use url::Url;

/// Implements [`CacheManager`] with [`moka`](https://github.com/moka-rs/moka) as the backend.
#[derive(Clone)]
pub struct MokaManager {
    /// The instance of `moka::future::Cache` inside an Arc
    pub cache: Cache<String, Arc<Vec<u8>>>,
}

impl fmt::Debug for MokaManager {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        // need to add more data, anything helpful
        f.debug_struct("MokaManager").finish()
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

#[allow(dead_code)]
impl MokaManager {
    /// Clears out the entire cache.
    pub async fn clear(&self) -> Result<()> {
        self.cache.invalidate_all();
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
        Ok(response)
    }

    async fn delete(&self, method: &str, url: &Url) -> Result<()> {
        self.cache.invalidate(&req_key(method, url)).await;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::HttpVersion;
    use anyhow::Result;
    use mockito::mock;
    use reqwest::{Client, Method, Request, Url};

    #[tokio::test]
    async fn can_cache_response() -> Result<()> {
        let m = mock("GET", "/")
            .with_status(200)
            .with_header("cache-control", "max-age=86400, public")
            .with_body("test")
            .create();
        let url = format!("{}/", &mockito::server_url());
        let url_parsed = Url::parse(&url)?;
        let manager = Arc::new(MokaManager::default());

        // We need to fake the request and get the response to build the policy
        let request = Request::new(Method::GET, url_parsed.clone());
        let cloned_req = request.try_clone().unwrap();
        let client = Client::new();
        let response = client.execute(request).await?;
        m.assert();

        // The cache accepts HttpResponse type only
        let http_res = HttpResponse {
            body: b"test".to_vec(),
            headers: Default::default(),
            status: 200,
            url: url_parsed.clone(),
            version: HttpVersion::Http11,
        };

        // Make sure the record doesn't already exist
        manager.delete("GET", &url_parsed).await?;
        let policy = CachePolicy::new(&cloned_req, &response);
        manager.put("GET", &url_parsed, http_res, policy).await?;
        let data = manager.get("GET", &url_parsed).await?;
        let body = match data {
            Some(d) => String::from_utf8(d.0.body)?,
            None => String::new(),
        };
        assert_eq!(&body, "test");
        manager.delete("GET", &url_parsed).await?;
        let data = manager.get("GET", &url_parsed).await?;
        assert!(data.is_none());
        manager.clear().await?;
        Ok(())
    }
}
