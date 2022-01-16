//! The reqwest middleware implementation for http-cache.
//! ```no_run
//! use reqwest::Client;
//! use reqwest_middleware::{ClientBuilder, Result};
//! use http_cache_reqwest::{Cache, CacheMode, CACacheManager, HttpCache};
//!
//! #[tokio::main]
//! async fn main() -> Result<()> {
//!     let client = ClientBuilder::new(Client::new())
//!         .with(Cache(HttpCache {
//!             mode: CacheMode::Default,
//!             manager: CACacheManager::default(),
//!             options: None,
//!         }))
//!         .build();
//!     client
//!         .get("https://developer.mozilla.org/en-US/docs/Web/HTTP/Caching")
//!         .send()
//!         .await?;
//!     Ok(())
//! }
//! ```
use anyhow::anyhow;
use std::{
    collections::HashMap,
    convert::{TryFrom, TryInto},
    str::FromStr,
    time::SystemTime,
};

use http::{
    header::{HeaderName, CACHE_CONTROL},
    request::Parts,
    HeaderValue, Method,
};
use http_cache::{CacheError, CacheManager, Middleware, Result};
use http_cache_semantics::CachePolicy;
use reqwest::{Request, Response, ResponseBuilderExt};
use reqwest_middleware::Next;
use task_local_extensions::Extensions;
use url::Url;

pub use http_cache::{CacheMode, CacheOptions, HttpCache, HttpResponse};

#[cfg(feature = "manager-cacache")]
pub use http_cache::CACacheManager;

/// Wrapper for [`HttpCache`]
pub struct Cache<T: CacheManager + Send + Sync + 'static>(pub HttpCache<T>);

/// Implements ['Middleware'] for reqwest
pub(crate) struct ReqwestMiddleware<'a> {
    pub req: Request,
    pub next: Next<'a>,
    pub extensions: &'a mut Extensions,
}

#[async_trait::async_trait]
impl Middleware for ReqwestMiddleware<'_> {
    fn is_method_get_head(&self) -> bool {
        self.req.method() == Method::GET || self.req.method() == Method::HEAD
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
        let headers = parts.headers;
        for header in headers.iter() {
            self.req.headers_mut().insert(header.0.clone(), header.1.clone());
        }
        Ok(())
    }
    fn set_no_cache(&mut self) -> Result<()> {
        self.req
            .headers_mut()
            .insert(CACHE_CONTROL, HeaderValue::from_str("no-cache")?);
        Ok(())
    }
    fn parts(&self) -> Result<Parts> {
        let copied_req = self.req.try_clone().ok_or(CacheError::BadRequest)?;
        let converted = match http::Request::try_from(copied_req) {
            Ok(r) => r,
            Err(e) => return Err(CacheError::General(anyhow!(e))),
        };
        Ok(converted.into_parts().0)
    }
    fn url(&self) -> Result<&Url> {
        Ok(self.req.url())
    }
    fn method(&self) -> Result<String> {
        Ok(self.req.method().as_ref().to_string())
    }
    async fn remote_fetch(&mut self) -> Result<HttpResponse> {
        let copied_req = self.req.try_clone().ok_or(CacheError::BadRequest)?;
        let res = match self.next.clone().run(copied_req, self.extensions).await
        {
            Ok(r) => r,
            Err(e) => return Err(CacheError::General(anyhow!(e))),
        };
        let mut headers = HashMap::new();
        for header in res.headers() {
            headers.insert(
                header.0.as_str().to_owned(),
                header.1.to_str()?.to_owned(),
            );
        }
        let url = res.url().clone();
        let status = res.status().into();
        let version = res.version();
        let body: Vec<u8> = match res.bytes().await {
            Ok(b) => b,
            Err(e) => return Err(CacheError::General(anyhow!(e))),
        }
        .to_vec();
        Ok(HttpResponse {
            body,
            headers,
            status,
            url,
            version: version.try_into()?,
        })
    }
}

// Converts an [`HttpResponse`] to a reqwest [`Response`]
fn convert_response(response: HttpResponse) -> anyhow::Result<Response> {
    let mut ret_res = http::Response::builder()
        .status(response.status)
        .url(response.url)
        .version(response.version.try_into()?)
        .body(response.body)?;
    for header in response.headers {
        ret_res.headers_mut().insert(
            HeaderName::from_str(header.0.clone().as_str())?,
            HeaderValue::from_str(header.1.clone().as_str())?,
        );
    }
    Ok(Response::from(ret_res))
}

#[async_trait::async_trait]
impl<T: CacheManager + 'static + Send + Sync> reqwest_middleware::Middleware
    for Cache<T>
{
    async fn handle(
        &self,
        req: reqwest::Request,
        extensions: &mut task_local_extensions::Extensions,
        next: reqwest_middleware::Next<'_>,
    ) -> std::result::Result<reqwest::Response, reqwest_middleware::Error> {
        let middleware = ReqwestMiddleware { req, next, extensions };
        let res = match self.0.run(middleware).await {
            Ok(r) => r,
            Err(e) => {
                return Err(reqwest_middleware::Error::Middleware(
                    anyhow::anyhow!(e),
                ));
            }
        };
        let converted = convert_response(res)?;
        Ok(converted)
    }
}

#[cfg(feature = "manager-cacache")]
#[cfg(test)]
mod tests {
    use crate::{CACacheManager, Cache, CacheManager, CacheMode, HttpCache};
    use mockito::mock;
    use reqwest::{Client, Url};
    use reqwest_middleware::ClientBuilder;

    #[tokio::test]
    async fn default_mode() -> anyhow::Result<()> {
        let m = mock("GET", "/")
            .with_status(200)
            .with_header("cache-control", "max-age=86400, public")
            .with_body("test")
            .create();
        let url = format!("{}/", &mockito::server_url());
        let manager = CACacheManager::default();
        let path = manager.path.clone();
        let key = format!("GET:{}", &url);

        // Make sure the record doesn't already exist
        manager.delete("GET", &Url::parse(&url)?).await?;

        // Construct reqwest client with cache defaults
        let client = ClientBuilder::new(Client::new())
            .with(Cache(HttpCache {
                mode: CacheMode::Default,
                manager: CACacheManager::default(),
                options: None,
            }))
            .build();

        // Cold pass to load cache
        client.get(url).send().await?;
        m.assert();

        // Try to load cached object
        let data = cacache::read(&path, &key).await;
        assert!(data.is_ok());
        Ok(())
    }
}
