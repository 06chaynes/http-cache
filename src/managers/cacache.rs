use crate::{CacheManager, HttpResponse, Result};

use http_cache_semantics::CachePolicy;
use serde::{Deserialize, Serialize};
use url::Url;

/// Implements [`CacheManager`] with [`cacache`](https://github.com/zkat/cacache-rs) as the backend.
#[derive(Debug, Clone)]
pub struct CACacheManager {
    /// Directory where the cache will be stored.
    pub path: String,
}

impl Default for CACacheManager {
    fn default() -> Self {
        CACacheManager { path: "./http-cacache".into() }
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
        let manager = CACacheManager::default();

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
