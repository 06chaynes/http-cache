#![forbid(unsafe_code, future_incompatible)]
#![deny(
    missing_docs,
    missing_debug_implementations,
    missing_copy_implementations,
    nonstandard_style,
    unused_qualifications,
    unused_import_braces,
    unused_extern_crates,
    trivial_casts,
    trivial_numeric_casts
)]
#![cfg_attr(docsrs, feature(doc_cfg))]
//! The surf middleware implementation for http-cache.
//! ```no_run
//! use http_cache_surf::{Cache, CacheMode, CACacheManager, HttpCache, HttpCacheOptions};
//!
//! #[async_std::main]
//! async fn main() -> surf::Result<()> {
//!     let req = surf::get("https://developer.mozilla.org/en-US/docs/Web/HTTP/Caching");
//!     surf::client()
//!         .with(Cache(HttpCache {
//!             mode: CacheMode::Default,
//!             manager: CACacheManager::default(),
//!             options: HttpCacheOptions::default(),
//!         }))
//!         .send(req)
//!         .await?;
//!     Ok(())
//! }
//! ```
mod error;

use anyhow::anyhow;
use std::{
    collections::HashMap, convert::TryInto, str::FromStr, time::SystemTime,
};

pub use http::request::Parts;
use http::{header::CACHE_CONTROL, request};
use http_cache::{
    BadHeader, BoxError, HitOrMiss, Middleware, Result, XCACHE, XCACHELOOKUP,
};
use http_cache_semantics::CachePolicy;
use http_types::{headers::HeaderValue, Method, Response, StatusCode, Version};
use surf::{middleware::Next, Client, Request};
use url::Url;

pub use http_cache::{
    CacheManager, CacheMode, CacheOptions, HttpCache, HttpCacheOptions,
    HttpResponse,
};

#[cfg(feature = "manager-cacache")]
#[cfg_attr(docsrs, doc(cfg(feature = "manager-cacache")))]
pub use http_cache::CACacheManager;

#[cfg(feature = "manager-moka")]
#[cfg_attr(docsrs, doc(cfg(feature = "manager-moka")))]
pub use http_cache::{MokaCache, MokaCacheBuilder, MokaManager};

/// Wrapper for [`HttpCache`]
#[derive(Debug)]
pub struct Cache<T: CacheManager>(pub HttpCache<T>);

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
    fn update_headers(&mut self, parts: &Parts) -> Result<()> {
        for header in parts.headers.iter() {
            let value = match HeaderValue::from_str(header.1.to_str()?) {
                Ok(v) => v,
                Err(_e) => return Err(Box::new(BadHeader)),
            };
            self.req.set_header(header.0.as_str(), value);
        }
        Ok(())
    }
    fn force_no_cache(&mut self) -> Result<()> {
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
    fn url(&self) -> Result<Url> {
        Ok(self.req.url().clone())
    }
    fn method(&self) -> Result<String> {
        Ok(self.req.method().as_ref().to_string())
    }
    async fn remote_fetch(&mut self) -> Result<HttpResponse> {
        let url = self.req.url().clone();
        let mut res =
            self.next.run(self.req.clone(), self.client.clone()).await?;
        let mut headers = HashMap::new();
        for header in res.iter() {
            headers.insert(
                header.0.as_str().to_owned(),
                header.1.as_str().to_owned(),
            );
        }
        let status = res.status().into();
        let version = res.version().unwrap_or(Version::Http1_1);
        let body: Vec<u8> = res.body_bytes().await?;
        Ok(HttpResponse {
            body,
            headers,
            status,
            url,
            version: version.try_into()?,
        })
    }
}

fn to_http_types_error(e: BoxError) -> http_types::Error {
    http_types::Error::from(anyhow!(e))
}

#[surf::utils::async_trait]
impl<T: CacheManager> surf::middleware::Middleware for Cache<T> {
    async fn handle(
        &self,
        req: Request,
        client: Client,
        next: Next<'_>,
    ) -> std::result::Result<surf::Response, http_types::Error> {
        let mut middleware = SurfMiddleware { req, client, next };
        if self.0.can_cache_request(&middleware) {
            let res =
                self.0.run(middleware).await.map_err(to_http_types_error)?;
            let mut converted = Response::new(StatusCode::Ok);
            for header in &res.headers {
                let val =
                    HeaderValue::from_bytes(header.1.as_bytes().to_vec())?;
                converted.insert_header(header.0.as_str(), val);
            }
            converted.set_status(res.status.try_into()?);
            converted.set_version(Some(res.version.try_into()?));
            converted.set_body(res.body);
            Ok(surf::Response::from(converted))
        } else {
            self.0
                .run_no_cache(&mut middleware)
                .await
                .map_err(to_http_types_error)?;
            let mut res =
                middleware.next.run(middleware.req, middleware.client).await?;
            let miss = HitOrMiss::MISS.to_string();
            res.append_header(XCACHE, miss.clone());
            res.append_header(XCACHELOOKUP, miss);
            Ok(res)
        }
    }
}

#[cfg(test)]
mod test;
