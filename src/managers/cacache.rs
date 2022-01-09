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
        let bytes = bincode::serialize(&data).unwrap();
        cacache::write(&self.path, &req_key(method, url), bytes).await?;
        Ok(response)
    }

    async fn delete(&self, method: &str, url: &Url) -> Result<()> {
        Ok(cacache::remove(&self.path, &req_key(method, url)).await?)
    }
}

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use crate::{get_request_parts, get_response_parts};
//     use http_types::{Method, Response, StatusCode};
//     use std::str::FromStr;
//     use surf::{Request, Result};
//
//     #[async_std::test]
//     async fn can_cache_response() -> Result<()> {
//         let url = surf::http::Url::from_str("https://example.com")?;
//         let mut res = Response::new(StatusCode::Ok);
//         res.set_body("test");
//         let mut res = surf::Response::from(res);
//         let req = Request::new(Method::Get, url);
//         let policy = CachePolicy::new(&get_request_parts(&req)?, &get_response_parts(&res)?);
//         let manager = CACacheManager::default();
//         manager.put(&req, &mut res, policy).await?;
//         let data = manager.get(&req).await?;
//         let body = match data {
//             Some(mut d) => d.0.body_string().await?,
//             None => String::new(),
//         };
//         assert_eq!(&body, "test");
//         manager.delete(&req).await?;
//         let data = manager.get(&req).await?;
//         assert!(data.is_none());
//         manager.clear().await?;
//         Ok(())
//     }
// }
