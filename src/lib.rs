//! A caching middleware that follows HTTP caching rules, thanks to
//! [`http-cache-semantics`](https://github.com/kornelski/rusty-http-cache-semantics).
//! By default, it uses [`cacache`](https://github.com/zkat/cacache-rs) as the backend cache manager.
//!
//! ## Supported Clients
//!
//! - **Surf** **Should likely be registered after any middleware modifying the request*
//! - **Reqwest** **Uses [reqwest-middleware](https://github.com/TrueLayer/reqwest-middleware) for middleware support*
//!
//! ## Examples
//!
//! ### Surf (requires feature: `client-surf`)
//!
//! ```ignore
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
//!
//! ### Reqwest (requires feature: `client-reqwest`)
//!
//! ```ignore
//! use reqwest::Client;
//! use reqwest_middleware::{ClientBuilder, Result};
//! use http_cache::{Cache, CacheMode, CACacheManager};
//!
//! #[tokio::main]
//! async fn main() -> Result<()> {
//!     let client = ClientBuilder::new(Client::new())
//!         .with(Cache {
//!             mode: CacheMode::Default,
//!             manager: CACacheManager::default(),
//!             options: None,
//!         })
//!         .build();
//!     client
//!         .get("https://developer.mozilla.org/en-US/docs/Web/HTTP/Caching")
//!         .send()
//!         .await?;
//!     Ok(())
//! }
//! ```
//!
//! ## Features
//!
//! The following features are available. By default `manager-cacache` is enabled.
//!
//! - `manager-cacache` (default): use [cacache](https://github.com/zkat/cacache-rs),
//! a high-performance disk cache, for the manager backend.
//! - `client-surf` (disabled): enables [surf](https://github.com/http-rs/surf) client support.
//! - `client-reqwest` (disabled): enables [reqwest](https://github.com/seanmonstar/reqwest) client support.
#![forbid(unsafe_code, future_incompatible)]
#![deny(
    missing_docs,
    missing_debug_implementations,
    missing_copy_implementations,
    nonstandard_style,
    unused_qualifications,
    unused_import_braces,
    unused_extern_crates,
    rustdoc::missing_doc_code_examples,
    trivial_casts,
    trivial_numeric_casts
)]

use std::time::SystemTime;

use http::StatusCode;
use http_cache_semantics::{AfterResponse, BeforeRequest, CachePolicy};

pub use http_cache_types::{
    CacheError, CacheManager, CacheMode, HttpResponse, HttpVersion, Middleware,
    Result,
};

#[cfg(feature = "manager-cacache")]
pub use http_cache_manager_cacache::CACacheManager;

/// Options struct provided by
/// [`http-cache-semantics`](https://github.com/kornelski/rusty-http-cache-semantics).
pub use http_cache_semantics::CacheOptions;

/// Caches requests according to http spec.
#[derive(Debug, Clone)]
pub struct Cache<T: CacheManager + Send + Sync + 'static> {
    /// Determines the manager behavior.
    pub mode: CacheMode,
    /// Manager instance that implements the [`CacheManager`] trait.
    /// By default, a manager implementation with [`cacache`](https://github.com/zkat/cacache-rs)
    /// as the backend has been provided, see [`CACacheManager`].
    pub manager: T,
    /// Override the default cache options.
    pub options: Option<CacheOptions>,
}

#[allow(dead_code)]
impl<T: CacheManager + Send + Sync + 'static> Cache<T> {
    pub(crate) async fn run(
        &self,
        mut middleware: impl Middleware,
    ) -> Result<HttpResponse> {
        let is_cacheable = middleware.is_method_get_head()
            && self.mode != CacheMode::NoStore
            && self.mode != CacheMode::Reload;
        if !is_cacheable {
            return middleware.remote_fetch().await;
        }
        if let Some(store) =
            self.manager.get(&middleware.method()?, middleware.url()?).await?
        {
            let (mut res, policy) = store;
            let res_url = res.url.clone();
            if let Some(warning_code) = res.warning_code() {
                // https://tools.ietf.org/html/rfc7234#section-4.3.4
                //
                // If a stored response is selected for update, the cache MUST:
                //
                // * delete any Warning header fields in the stored response with
                //   warn-code 1xx (see Section 5.5);
                //
                // * retain any Warning header fields in the stored response with
                //   warn-code 2xx;
                //
                #[allow(clippy::manual_range_contains)]
                if warning_code >= 100 && warning_code < 200 {
                    res.remove_warning();
                }
            }

            match self.mode {
                CacheMode::Default => {
                    self.conditional_fetch(middleware, res, policy).await
                }
                CacheMode::NoCache => {
                    middleware.set_no_cache()?;
                    self.conditional_fetch(middleware, res, policy).await
                }
                CacheMode::ForceCache | CacheMode::OnlyIfCached => {
                    //   112 Disconnected operation
                    // SHOULD be included if the cache is intentionally disconnected from
                    // the rest of the network for a period of time.
                    // (https://tools.ietf.org/html/rfc2616#section-14.46)
                    res.add_warning(res_url, 112, "Disconnected operation");
                    Ok(res)
                }
                _ => self.remote_fetch(&mut middleware).await,
            }
        } else {
            match self.mode {
                CacheMode::OnlyIfCached => {
                    // ENOTCACHED
                    return Ok(HttpResponse {
                        body: b"GatewayTimeout".to_vec(),
                        headers: Default::default(),
                        status: 504,
                        url: middleware.url()?.clone(),
                        version: HttpVersion::Http11,
                    });
                }
                _ => self.remote_fetch(&mut middleware).await,
            }
        }
    }

    async fn remote_fetch(
        &self,
        middleware: &mut impl Middleware,
    ) -> Result<HttpResponse> {
        let res = middleware.remote_fetch().await?;
        let policy = match self.options {
            Some(options) => middleware.policy_with_options(&res, options)?,
            None => middleware.policy(&res)?,
        };
        let is_cacheable = middleware.is_method_get_head()
            && self.mode != CacheMode::NoStore
            && self.mode != CacheMode::Reload
            && res.status == 200
            && policy.is_storable();
        if is_cacheable {
            Ok(self
                .manager
                .put(&middleware.method()?, middleware.url()?, res, policy)
                .await?)
        } else if !middleware.is_method_get_head() {
            self.manager
                .delete(&middleware.method()?, middleware.url()?)
                .await?;
            Ok(res)
        } else {
            Ok(res)
        }
    }

    async fn conditional_fetch(
        &self,
        mut middleware: impl Middleware,
        mut cached_res: HttpResponse,
        mut policy: CachePolicy,
    ) -> Result<HttpResponse> {
        let before_req =
            policy.before_request(&middleware.parts()?, SystemTime::now());
        match before_req {
            BeforeRequest::Fresh(parts) => {
                cached_res.update_headers(parts)?;
                return Ok(cached_res);
            }
            BeforeRequest::Stale { request: parts, matches } => {
                if matches {
                    middleware.update_headers(parts)?;
                }
            }
        }
        let req_url = middleware.url()?.clone();
        match middleware.remote_fetch().await {
            Ok(cond_res) => {
                let status = StatusCode::from_u16(cond_res.status)?;
                if status.is_server_error() && cached_res.must_revalidate() {
                    //   111 Revalidation failed
                    //   MUST be included if a cache returns a stale response
                    //   because an attempt to revalidate the response failed,
                    //   due to an inability to reach the server.
                    // (https://tools.ietf.org/html/rfc2616#section-14.46)
                    cached_res.add_warning(
                        req_url.clone(),
                        111,
                        "Revalidation failed",
                    );
                    Ok(cached_res)
                } else if cond_res.status == 304 {
                    let after_res = policy.after_response(
                        &middleware.parts()?,
                        &cond_res.parts()?,
                        SystemTime::now(),
                    );
                    match after_res {
                        AfterResponse::Modified(new_policy, parts)
                        | AfterResponse::NotModified(new_policy, parts) => {
                            policy = new_policy;
                            cached_res.update_headers(parts)?;
                        }
                    }
                    let res = self
                        .manager
                        .put(
                            &middleware.method()?,
                            &req_url,
                            cached_res,
                            policy,
                        )
                        .await?;
                    Ok(res)
                } else {
                    Ok(cached_res)
                }
            }
            Err(e) => {
                if cached_res.must_revalidate() {
                    Err(e)
                } else {
                    //   111 Revalidation failed
                    //   MUST be included if a cache returns a stale response
                    //   because an attempt to revalidate the response failed,
                    //   due to an inability to reach the server.
                    // (https://tools.ietf.org/html/rfc2616#section-14.46)
                    cached_res.add_warning(req_url, 111, "Revalidation failed");
                    Ok(cached_res)
                }
            }
        }
    }
}

#[cfg(feature = "client-reqwest")]
mod reqwest {
    use crate::{Cache, CacheManager};
    use http_cache_middleware_reqwest::{convert_response, ReqwestMiddleware};
    #[async_trait::async_trait]
    impl<T: CacheManager + 'static + Send + Sync> reqwest_middleware::Middleware
        for Cache<T>
    {
        async fn handle(
            &self,
            req: reqwest::Request,
            extensions: &mut task_local_extensions::Extensions,
            next: reqwest_middleware::Next<'_>,
        ) -> Result<reqwest::Response, reqwest_middleware::Error> {
            let middleware = ReqwestMiddleware { req, next, extensions };
            let res = match self.run(middleware).await {
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

    #[cfg(test)]
    mod tests {
        use crate::{CACacheManager, Cache, CacheManager, CacheMode};
        use mockito::mock;
        use reqwest::{Client, Url};
        use reqwest_middleware::ClientBuilder;

        #[tokio::test]
        async fn reqwest_default_mode() -> anyhow::Result<()> {
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
                .with(Cache {
                    mode: CacheMode::Default,
                    manager: CACacheManager::default(),
                    options: None,
                })
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
}

#[cfg(feature = "client-surf")]
mod surf {
    use crate::{Cache, CacheManager};
    use http_cache_middleware_surf::SurfMiddleware;
    use http_types::headers::HeaderValue;
    use std::convert::TryInto;
    #[surf::utils::async_trait]
    impl<T: CacheManager + 'static + Send + Sync> surf::middleware::Middleware
        for Cache<T>
    {
        async fn handle(
            &self,
            req: surf::Request,
            client: surf::Client,
            next: surf::middleware::Next<'_>,
        ) -> Result<surf::Response, http_types::Error> {
            let middleware = SurfMiddleware { req, client, next };
            let res = self.run(middleware).await?;

            let mut converted =
                http_types::Response::new(http_types::StatusCode::Ok);
            for header in &res.headers {
                let val =
                    HeaderValue::from_bytes(header.1.as_bytes().to_vec())?;
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
}

#[cfg(feature = "manager-cacache")]
#[cfg(test)]
mod tests {
    use crate::{HttpResponse, HttpVersion};
    use anyhow::Result;
    use http_cache_manager_cacache::CACacheManager;
    use http_cache_semantics::CachePolicy;
    use http_cache_types::CacheManager;
    use mockito::mock;
    use reqwest::{Client, Method, Request, Url};

    #[tokio::test]
    async fn cacache_can_cache_response() -> Result<()> {
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
