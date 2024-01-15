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
//! The reqwest middleware implementation for http-cache.
//! ```no_run
//! use reqwest::Client;
//! use reqwest_middleware::{ClientBuilder, Result};
//! use http_cache_reqwest::{Cache, CacheMode, CACacheManager, HttpCache, HttpCacheOptions};
//!
//! #[tokio::main]
//! async fn main() -> Result<()> {
//!     let client = ClientBuilder::new(Client::new())
//!         .with(Cache(HttpCache {
//!             mode: CacheMode::Default,
//!             manager: CACacheManager::default(),
//!             options: HttpCacheOptions::default(),
//!         }))
//!         .build();
//!     client
//!         .get("https://developer.mozilla.org/en-US/docs/Web/HTTP/Caching")
//!         .send()
//!         .await?;
//!     Ok(())
//! }
//! ```
//!
//! ## Overriding the cache mode
//!
//! The cache mode can be overridden on a per-request basis by making use of the
//! `reqwest-middleware` extensions system.
//!
//! ```no_run
//! client.get("https://developer.mozilla.org/en-US/docs/Web/HTTP/Caching")
//!     .with_extension(CacheMode::OnlyIfCached)
//!     .send()
//!     .await?;
//! ```
mod error;

use anyhow::anyhow;

pub use error::BadRequest;

use std::{
    collections::HashMap,
    convert::{TryFrom, TryInto},
    str::FromStr,
    time::SystemTime,
};

pub use http::request::Parts;
use http::{
    header::{HeaderName, CACHE_CONTROL},
    HeaderValue, Method,
};
use http_cache::{
    BoxError, HitOrMiss, Middleware, Result, XCACHE, XCACHELOOKUP,
};
use http_cache_semantics::CachePolicy;
use reqwest::{Request, Response, ResponseBuilderExt};
use reqwest_middleware::{Error, Next};
use task_local_extensions::Extensions;
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

/// Implements ['Middleware'] for reqwest
pub(crate) struct ReqwestMiddleware<'a> {
    pub req: Request,
    pub next: Next<'a>,
    pub extensions: &'a mut Extensions,
}

fn clone_req(request: &Request) -> std::result::Result<Request, Error> {
    match request.try_clone() {
        Some(r) => Ok(r),
        None => Err(Error::Middleware(anyhow!(BadRequest))),
    }
}

#[async_trait::async_trait]
impl Middleware for ReqwestMiddleware<'_> {
    fn overridden_cache_mode(&self) -> Option<CacheMode> {
        self.extensions.get().cloned()
    }
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
    fn update_headers(&mut self, parts: &Parts) -> Result<()> {
        for header in parts.headers.iter() {
            self.req.headers_mut().insert(header.0.clone(), header.1.clone());
        }
        Ok(())
    }
    fn force_no_cache(&mut self) -> Result<()> {
        self.req
            .headers_mut()
            .insert(CACHE_CONTROL, HeaderValue::from_str("no-cache")?);
        Ok(())
    }
    fn parts(&self) -> Result<Parts> {
        let copied_req = clone_req(&self.req)?;
        let converted = match http::Request::try_from(copied_req) {
            Ok(r) => r,
            Err(e) => return Err(Box::new(e)),
        };
        Ok(converted.into_parts().0)
    }
    fn url(&self) -> Result<Url> {
        Ok(self.req.url().clone())
    }
    fn method(&self) -> Result<String> {
        Ok(self.req.method().as_ref().to_string())
    }
    async fn remote_fetch(&mut self) -> Result<HttpResponse> {
        let copied_req = clone_req(&self.req)?;
        let res = match self.next.clone().run(copied_req, self.extensions).await
        {
            Ok(r) => r,
            Err(e) => return Err(Box::new(e)),
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
            Err(e) => return Err(Box::new(e)),
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
        .version(response.version.into())
        .body(response.body)?;
    for header in response.headers {
        ret_res.headers_mut().insert(
            HeaderName::from_str(header.0.clone().as_str())?,
            HeaderValue::from_str(header.1.clone().as_str())?,
        );
    }
    Ok(Response::from(ret_res))
}

fn bad_header(e: reqwest::header::InvalidHeaderValue) -> Error {
    Error::Middleware(anyhow!(e))
}

fn from_box_error(e: BoxError) -> Error {
    Error::Middleware(anyhow!(e))
}

#[async_trait::async_trait]
impl<T: CacheManager> reqwest_middleware::Middleware for Cache<T> {
    async fn handle(
        &self,
        req: Request,
        extensions: &mut Extensions,
        next: Next<'_>,
    ) -> std::result::Result<Response, Error> {
        let mut middleware = ReqwestMiddleware { req, next, extensions };
        if self
            .0
            .can_cache_request(&middleware)
            .map_err(|e| Error::Middleware(anyhow!(e)))?
        {
            let res = self.0.run(middleware).await.map_err(from_box_error)?;
            let converted = convert_response(res)?;
            Ok(converted)
        } else {
            self.0
                .run_no_cache(&mut middleware)
                .await
                .map_err(from_box_error)?;
            let mut res = middleware
                .next
                .run(middleware.req, middleware.extensions)
                .await?;

            let miss =
                HeaderValue::from_str(HitOrMiss::MISS.to_string().as_ref())
                    .map_err(bad_header)?;
            res.headers_mut().insert(XCACHE, miss.clone());
            res.headers_mut().insert(XCACHELOOKUP, miss);
            Ok(res)
        }
    }
}

#[cfg(test)]
mod test;
