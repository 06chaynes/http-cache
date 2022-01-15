//! The surf middleware implementation, requires the `client-surf` feature.
//!
//! ```no_run
//! use http_cache::{Cache, CacheMode, CACacheManager};
//!
//! #[async_std::main]
//! async fn main() -> surf::Result<()> {
//!     let req = surf::get("https://developer.mozilla.org/en-US/docs/Web/HTTP/Caching");
//!     surf::client()
//!         .with(Cache {
//!             mode: CacheMode::Default,
//!             manager: CACacheManager::default(),
//!             options: None,
//!         })
//!         .send(req)
//!         .await?;
//!     Ok(())
//! }
//! ```
use crate::{
    Cache, CacheError, CacheManager, HttpResponse, HttpVersion, Middleware,
    Result,
};

use anyhow::anyhow;
use std::{
    collections::HashMap,
    convert::{TryFrom, TryInto},
    str::FromStr,
    time::SystemTime,
};

use http::{header::CACHE_CONTROL, request, request::Parts};
use http_cache_semantics::{CacheOptions, CachePolicy};
use http_types::{headers::HeaderValue, Method, Version};
use surf::{middleware::Next, Client, Request, Response};
use url::Url;

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

impl TryFrom<Version> for HttpVersion {
    type Error = CacheError;

    fn try_from(value: Version) -> Result<Self> {
        Ok(match value {
            Version::Http0_9 => HttpVersion::Http09,
            Version::Http1_0 => HttpVersion::Http10,
            Version::Http1_1 => HttpVersion::Http11,
            Version::Http2_0 => HttpVersion::H2,
            Version::Http3_0 => HttpVersion::H3,
            _ => return Err(CacheError::BadVersion),
        })
    }
}

impl From<HttpVersion> for Version {
    fn from(value: HttpVersion) -> Self {
        match value {
            HttpVersion::Http09 => Version::Http0_9,
            HttpVersion::Http10 => Version::Http1_0,
            HttpVersion::Http11 => Version::Http1_1,
            HttpVersion::H2 => Version::Http2_0,
            HttpVersion::H3 => Version::Http3_0,
        }
    }
}

#[surf::utils::async_trait]
impl<T: CacheManager + 'static + Send + Sync> surf::middleware::Middleware
    for Cache<T>
{
    async fn handle(
        &self,
        req: Request,
        client: Client,
        next: Next<'_>,
    ) -> std::result::Result<Response, http_types::Error> {
        let middleware = SurfMiddleware { req, client, next };
        let res = self.run(middleware).await?;

        let mut converted =
            http_types::Response::new(http_types::StatusCode::Ok);
        for header in &res.headers {
            let val = HeaderValue::from_bytes(header.1.as_bytes().to_vec())?;
            converted.insert_header(header.0.as_str(), val);
        }
        converted.set_status(res.status.try_into()?);
        converted.set_version(Some(res.version.try_into()?));
        converted.set_body(res.body.clone());
        Ok(Response::from(converted))
    }
}

#[cfg(test)]
mod tests {
    use crate::{CACacheManager, Cache, CacheManager, CacheMode};
    use mockito::mock;
    use surf::{http::Method, Client, Request, Url};

    #[async_std::test]
    async fn default_mode() -> surf::Result<()> {
        let m = mock("GET", "/")
            .with_status(200)
            .with_header("cache-control", "max-age=86400, public")
            .with_body("test")
            .create();
        let url = format!("{}/", &mockito::server_url());
        let manager = CACacheManager::default();
        let path = manager.path.clone();
        let key = format!("GET:{}", &url);
        let req = Request::new(Method::Get, Url::parse(&url)?);

        // Make sure the record doesn't already exist
        manager.delete("GET", &Url::parse(&url)?).await?;

        // Construct Surf client with cache defaults
        let client = Client::new().with(Cache {
            mode: CacheMode::Default,
            manager: CACacheManager::default(),
            options: None,
        });

        // Cold pass to load cache
        client.send(req.clone()).await?;
        m.assert();

        // Try to load cached object
        let data = cacache::read(&path, &key).await;
        assert!(data.is_ok());
        Ok(())
    }
}
