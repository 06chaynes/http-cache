//! The surf middleware implementation for http-cache.
//! ```no_run
//! use http_cache_surf::{Cache, CacheMode, CACacheManager, HttpCache};
//!
//! #[async_std::main]
//! async fn main() -> surf::Result<()> {
//!     let req = surf::get("https://developer.mozilla.org/en-US/docs/Web/HTTP/Caching");
//!     surf::client()
//!         .with(Cache(HttpCache {
//!             mode: CacheMode::Default,
//!             manager: CACacheManager::default(),
//!             options: None,
//!         }))
//!         .send(req)
//!         .await?;
//!     Ok(())
//! }
//! ```
use anyhow::anyhow;
use std::{
    collections::HashMap, convert::TryInto, str::FromStr, time::SystemTime,
};

use http::{header::CACHE_CONTROL, request, request::Parts};
use http_cache::{CacheError, CacheManager, Middleware, Result};
use http_cache_semantics::CachePolicy;
use http_types::{headers::HeaderValue, Method, Version};
use surf::{middleware::Next, Client, Request};
use url::Url;

pub use http_cache::{CacheMode, CacheOptions, HttpCache, HttpResponse};

#[cfg(feature = "manager-cacache")]
pub use http_cache::CACacheManager;

/// Wrapper for [`HttpCache`]
pub struct Cache<T: CacheManager + Send + Sync + 'static>(pub HttpCache<T>);

/// Implements ['Middleware'] for surf
pub(crate) struct SurfMiddleware<'a> {
    pub req: Request,
    pub client: Client,
    pub next: Next<'a>,
}

#[async_trait::async_trait]
impl Middleware for SurfMiddleware<'_> {
    fn is_method_get_head(&self) -> bool {
        self.req.method() == Method::Get || self.req.method() == Method::Head
    }
    fn policy(&self, response: &HttpResponse) -> Result<CachePolicy> {
        Ok(CachePolicy::new(&self.parts()?, &response.parts()?))
    }
    fn policy_with_options(
        &self,
        response: &HttpResponse,
        options: CacheOptions,
    ) -> Result<CachePolicy> {
        Ok(CachePolicy::new_options(
            &self.parts()?,
            &response.parts()?,
            SystemTime::now(),
            options,
        ))
    }
    fn update_headers(&mut self, parts: Parts) -> Result<()> {
        for header in parts.headers.iter() {
            let value = match HeaderValue::from_str(header.1.to_str()?) {
                Ok(v) => v,
                Err(_e) => return Err(CacheError::BadHeader),
            };
            self.req.set_header(header.0.as_str(), value);
        }
        Ok(())
    }
    fn set_no_cache(&mut self) -> Result<()> {
        self.req.insert_header(CACHE_CONTROL.as_str(), "no-cache");
        Ok(())
    }
    fn parts(&self) -> Result<Parts> {
        let mut converted = request::Builder::new()
            .method(self.req.method().as_ref())
            .uri(self.req.url().as_str())
            .body(())?;
        {
            let headers = converted.headers_mut();
            for header in self.req.iter() {
                headers.insert(
                    http::header::HeaderName::from_str(header.0.as_str())?,
                    http::HeaderValue::from_str(header.1.as_str())?,
                );
            }
        }
        Ok(converted.into_parts().0)
    }
    fn url(&self) -> Result<&Url> {
        Ok(self.req.url())
    }
    fn method(&self) -> Result<String> {
        Ok(self.req.method().as_ref().to_string())
    }
    async fn remote_fetch(&mut self) -> Result<HttpResponse> {
        let url = self.req.url().clone();
        let mut res =
            match self.next.run(self.req.clone(), self.client.clone()).await {
                Ok(r) => r,
                Err(e) => return Err(CacheError::General(anyhow!(e))),
            };
        let mut headers = HashMap::new();
        for header in res.iter() {
            headers.insert(
                header.0.as_str().to_owned(),
                header.1.as_str().to_owned(),
            );
        }
        let status = res.status().into();
        let version = res.version().unwrap_or(Version::Http1_1);
        let body: Vec<u8> = match res.body_bytes().await {
            Ok(b) => b,
            Err(e) => return Err(CacheError::General(anyhow!(e))),
        };
        Ok(HttpResponse {
            body,
            headers,
            status,
            url,
            version: version.try_into()?,
        })
    }
}

#[surf::utils::async_trait]
impl<T: CacheManager + 'static + Send + Sync> surf::middleware::Middleware
    for Cache<T>
{
    async fn handle(
        &self,
        req: surf::Request,
        client: surf::Client,
        next: surf::middleware::Next<'_>,
    ) -> std::result::Result<surf::Response, http_types::Error> {
        let middleware = SurfMiddleware { req, client, next };
        let res = self.0.run(middleware).await?;
        let mut converted =
            http_types::Response::new(http_types::StatusCode::Ok);
        for header in &res.headers {
            let val = HeaderValue::from_bytes(header.1.as_bytes().to_vec())?;
            converted.insert_header(header.0.as_str(), val);
        }
        converted.set_status(res.status.try_into()?);
        converted.set_version(Some(res.version.try_into()?));
        converted.set_body(res.body.clone());
        Ok(surf::Response::from(converted))
    }
}

#[cfg(feature = "manager-cacache")]
#[cfg(test)]
mod tests {
    use crate::*;
    use mockito::{mock, Mock};
    use surf::{http::Method, Client, Request, Url};

    fn build_mock_server(
        cache_control_val: &str,
        body: &[u8],
        status: usize,
        expect: usize,
    ) -> Mock {
        mock("GET", "/")
            .with_status(status)
            .with_header("cache-control", cache_control_val)
            .with_body(body)
            .expect(expect)
            .create()
    }

    const GET: &str = "GET";

    const TEST_BODY: &[u8] = b"test";

    #[async_std::test]
    async fn default_mode() -> surf::Result<()> {
        let m = build_mock_server("max-age=86400, public", TEST_BODY, 200, 1);
        let url = format!("{}/", &mockito::server_url());
        let manager = CACacheManager::default();
        let path = manager.path.clone();
        let key = format!("{}:{}", GET, &url);
        let req = Request::new(Method::Get, Url::parse(&url)?);

        // Make sure the record doesn't already exist
        manager.delete(GET, &Url::parse(&url)?).await?;

        // Construct Surf client with cache defaults
        let client = Client::new().with(Cache(HttpCache {
            mode: CacheMode::Default,
            manager: CACacheManager::default(),
            options: None,
        }));

        // Cold pass to load cache
        client.send(req.clone()).await?;

        // Try to load cached object
        let data = cacache::read(&path, &key).await;
        assert!(data.is_ok());

        // Hot pass to make sure the expect response was returned
        let mut res = client.send(req).await?;
        assert_eq!(res.body_bytes().await?, TEST_BODY);
        m.assert();
        manager.clear().await?;
        Ok(())
    }

    #[async_std::test]
    async fn default_mode_with_options() -> surf::Result<()> {
        let m = build_mock_server("max-age=86400, private", TEST_BODY, 200, 1);
        let url = format!("{}/", &mockito::server_url());
        let manager = CACacheManager::default();
        let path = manager.path.clone();
        let key = format!("{}:{}", GET, &url);
        let req = Request::new(Method::Get, Url::parse(&url)?);

        // Make sure the record doesn't already exist
        manager.delete(GET, &Url::parse(&url)?).await?;

        // Construct Surf client with cache options override
        let client = Client::new().with(Cache(HttpCache {
            mode: CacheMode::Default,
            manager: CACacheManager::default(),
            options: Some(CacheOptions { shared: false, ..Default::default() }),
        }));

        // Cold pass to load cache
        client.send(req.clone()).await?;

        // Try to load cached object
        let data = cacache::read(&path, &key).await;
        assert!(data.is_ok());

        // Hot pass to make sure the expect response was returned
        let mut res = client.send(req).await?;
        assert_eq!(res.body_bytes().await?, TEST_BODY);
        m.assert();
        manager.clear().await?;
        Ok(())
    }

    #[async_std::test]
    async fn no_store_mode() -> surf::Result<()> {
        let m = build_mock_server("max-age=86400, public", TEST_BODY, 200, 2);
        let url = format!("{}/", &mockito::server_url());
        let manager = CACacheManager::default();
        let path = manager.path.clone();
        let key = format!("{}:{}", GET, &url);
        let req = Request::new(Method::Get, Url::parse(&url)?);

        // Make sure the record doesn't already exist
        manager.delete(GET, &Url::parse(&url)?).await?;

        // Construct Surf client with cache defaults
        let client = Client::new().with(Cache(HttpCache {
            mode: CacheMode::NoStore,
            manager: CACacheManager::default(),
            options: None,
        }));

        // Remote request but should not cache
        client.send(req.clone()).await?;

        // Try to load cached object
        let data = cacache::read(&path, &key).await;
        assert!(data.is_err());

        // To verify our endpoint receives the request rather than a cache hit
        client.send(req.clone()).await?;
        m.assert();
        manager.clear().await?;
        Ok(())
    }

    #[async_std::test]
    async fn no_cache_mode() -> surf::Result<()> {
        let m = build_mock_server("max-age=86400, public", TEST_BODY, 200, 2);
        let url = format!("{}/", &mockito::server_url());
        let manager = CACacheManager::default();
        let path = manager.path.clone();
        let key = format!("{}:{}", GET, &url);
        let req = Request::new(Method::Get, Url::parse(&url)?);

        // Make sure the record doesn't already exist
        manager.delete(GET, &Url::parse(&url)?).await?;

        // Construct Surf client with cache defaults
        let client = Client::new().with(Cache(HttpCache {
            mode: CacheMode::NoCache,
            manager: CACacheManager::default(),
            options: None,
        }));

        // Remote request and should cache
        client.send(req.clone()).await?;

        // Try to load cached object
        let data = cacache::read(&path, &key).await;
        assert!(data.is_ok());

        // To verify our endpoint receives the request rather than a cache hit
        client.send(req.clone()).await?;
        m.assert();
        manager.clear().await?;
        Ok(())
    }

    #[async_std::test]
    async fn force_cache_mode() -> surf::Result<()> {
        let m = build_mock_server("max-age=86400, public", TEST_BODY, 200, 1);
        let url = format!("{}/", &mockito::server_url());
        let manager = CACacheManager::default();
        let path = manager.path.clone();
        let key = format!("{}:{}", GET, &url);
        let req = Request::new(Method::Get, Url::parse(&url)?);

        // Make sure the record doesn't already exist
        manager.delete(GET, &Url::parse(&url)?).await?;

        // Construct Surf client with cache defaults
        let client = Client::new().with(Cache(HttpCache {
            mode: CacheMode::ForceCache,
            manager: CACacheManager::default(),
            options: None,
        }));

        // Should result in a cache miss and a remote request
        client.send(req.clone()).await?;

        // Try to load cached object
        let data = cacache::read(&path, &key).await;
        assert!(data.is_ok());

        // Should result in a cache hit and no remote request
        client.send(req.clone()).await?;

        // Verify endpoint did receive the request
        m.assert();
        manager.clear().await?;
        Ok(())
    }

    #[cfg(test)]
    mod only_if_cached_mode {
        use super::*;

        #[async_std::test]
        async fn miss() -> surf::Result<()> {
            let m =
                build_mock_server("max-age=86400, public", TEST_BODY, 200, 0);
            let url = format!("{}/", &mockito::server_url());
            let manager = CACacheManager::default();
            let path = manager.path.clone();
            let key = format!("{}:{}", GET, &url);
            let req = Request::new(Method::Get, Url::parse(&url)?);

            // Make sure the record doesn't already exist
            manager.delete(GET, &Url::parse(&url)?).await?;

            // Construct Surf client with cache defaults
            let client = Client::new().with(Cache(HttpCache {
                mode: CacheMode::OnlyIfCached,
                manager: CACacheManager::default(),
                options: None,
            }));

            // Should result in a cache miss and no remote request
            client.send(req.clone()).await?;

            // Try to load cached object
            let data = cacache::read(&path, &key).await;
            assert!(data.is_err());

            // Verify endpoint did not receive the request
            m.assert();
            manager.clear().await?;
            Ok(())
        }

        #[async_std::test]
        async fn hit() -> surf::Result<()> {
            let m =
                build_mock_server("max-age=86400, public", TEST_BODY, 200, 1);
            let url = format!("{}/", &mockito::server_url());
            let manager = CACacheManager::default();
            let path = manager.path.clone();
            let key = format!("{}:{}", GET, &url);
            let req = Request::new(Method::Get, Url::parse(&url)?);

            // Make sure the record doesn't already exist
            manager.delete(GET, &Url::parse(&url)?).await?;

            // Construct Surf client with cache defaults
            let client = Client::new().with(Cache(HttpCache {
                mode: CacheMode::Default,
                manager: CACacheManager::default(),
                options: None,
            }));

            // Cold pass to load the cache
            client.send(req.clone()).await?;

            // Try to load cached object
            let data = cacache::read(&path, &key).await;
            assert!(data.is_ok());

            // Construct Surf client with cache defaults
            let client = Client::new().with(Cache(HttpCache {
                mode: CacheMode::OnlyIfCached,
                manager: CACacheManager::default(),
                options: None,
            }));

            // Should result in a cache hit and no remote request
            let mut res = client.send(req.clone()).await?;

            // Check the body
            assert_eq!(res.body_bytes().await?, TEST_BODY);

            // Verify endpoint received only one request
            m.assert();
            manager.clear().await?;
            Ok(())
        }
    }
}
