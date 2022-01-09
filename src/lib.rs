mod error;
mod managers;

pub use error::CacheError;

#[cfg(feature = "manager-cacache")]
pub use managers::cacache::CACacheManager;

use http::{header::CACHE_CONTROL, request::Parts};
use std::str::FromStr;
use std::{
    collections::HashMap,
    convert::{TryFrom, TryInto},
    fmt,
    time::SystemTime,
};

use http_cache_semantics::{AfterResponse, BeforeRequest, CachePolicy};
use serde::{Deserialize, Serialize};
use url::Url;

pub type Result<T> = std::result::Result<T, CacheError>;

// Represents an HTTP version
#[derive(Debug, Copy, Clone, Deserialize, Serialize)]
#[non_exhaustive]
pub enum HttpVersion {
    #[serde(rename = "HTTP/0.9")]
    Http09,
    #[serde(rename = "HTTP/1.0")]
    Http10,
    #[serde(rename = "HTTP/1.1")]
    Http11,
    #[serde(rename = "HTTP/2.0")]
    H2,
    #[serde(rename = "HTTP/3.0")]
    H3,
}

#[cfg(feature = "client-reqwest")]
impl TryFrom<http::Version> for HttpVersion {
    type Error = CacheError;

    fn try_from(value: http::Version) -> Result<Self> {
        Ok(match value {
            http::Version::HTTP_09 => HttpVersion::Http09,
            http::Version::HTTP_10 => HttpVersion::Http10,
            http::Version::HTTP_11 => HttpVersion::Http11,
            http::Version::HTTP_2 => HttpVersion::H2,
            http::Version::HTTP_3 => HttpVersion::H3,
            _ => return Err(CacheError::BadVersion),
        })
    }
}

#[cfg(feature = "client-reqwest")]
impl From<HttpVersion> for http::Version {
    fn from(value: HttpVersion) -> Self {
        match value {
            HttpVersion::Http09 => http::Version::HTTP_09,
            HttpVersion::Http10 => http::Version::HTTP_10,
            HttpVersion::Http11 => http::Version::HTTP_11,
            HttpVersion::H2 => http::Version::HTTP_2,
            HttpVersion::H3 => http::Version::HTTP_3,
        }
    }
}

#[cfg(feature = "client-surf")]
impl TryFrom<http_types::Version> for HttpVersion {
    type Error = CacheError;

    fn try_from(value: http_types::Version) -> Result<Self> {
        Ok(match value {
            http_types::Version::Http0_9 => HttpVersion::Http09,
            http_types::Version::Http1_0 => HttpVersion::Http10,
            http_types::Version::Http1_1 => HttpVersion::Http11,
            http_types::Version::Http2_0 => HttpVersion::H2,
            http_types::Version::Http3_0 => HttpVersion::H3,
            _ => return Err(CacheError::BadVersion),
        })
    }
}

#[cfg(feature = "client-surf")]
impl From<HttpVersion> for http_types::Version {
    fn from(value: HttpVersion) -> Self {
        match value {
            HttpVersion::Http09 => http_types::Version::Http0_9,
            HttpVersion::Http10 => http_types::Version::Http1_0,
            HttpVersion::Http11 => http_types::Version::Http1_1,
            HttpVersion::H2 => http_types::Version::Http2_0,
            HttpVersion::H3 => http_types::Version::Http3_0,
        }
    }
}

/// A basic generic type that represents an HTTP response
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HttpResponse {
    pub body: Vec<u8>,
    pub headers: HashMap<String, String>,
    pub status: u16,
    pub url: Url,
    pub version: HttpVersion,
}

impl HttpResponse {
    pub fn get_parts(&self) -> Result<http::response::Parts> {
        let mut headers = http::HeaderMap::new();
        for header in self.headers.iter() {
            headers.insert(
                http::header::HeaderName::from_str(header.0.as_str())?,
                http::HeaderValue::from_str(header.1.as_str())?,
            );
        }
        let status = http::StatusCode::from_u16(self.status)?;
        let mut converted = http::response::Response::new(());
        converted.headers_mut().clone_from(&headers);
        converted.status_mut().clone_from(&status);
        let parts = converted.into_parts();
        Ok(parts.0)
    }

    pub fn get_warning_code(&self) -> Option<usize> {
        self.headers.get("Warning").and_then(|hdr| {
            hdr.as_str()
                .chars()
                .take(3)
                .collect::<String>()
                .parse()
                .ok()
        })
    }

    pub fn add_warning(&mut self, url: Url, code: usize, message: &str) {
        // Warning    = "Warning" ":" 1#warning-value
        // warning-value = warn-code SP warn-agent SP warn-text [SP warn-date]
        // warn-code  = 3DIGIT
        // warn-agent = ( host [ ":" port ] ) | pseudonym
        //                 ; the name or pseudonym of the server adding
        //                 ; the Warning header, for use in debugging
        // warn-text  = quoted-string
        // warn-date  = <"> HTTP-date <">
        // (https://tools.ietf.org/html/rfc2616#section-14.46)
        self.headers.insert(
            "Warning".to_string(),
            format!(
                "{} {} {:?} \"{}\"",
                code,
                url.host().expect("Invalid URL"),
                message,
                httpdate::fmt_http_date(SystemTime::now())
            ),
        );
    }

    pub fn remove_warning(&mut self) {
        self.headers.remove("Warning");
    }

    pub fn update_headers_from_parts(&mut self, parts: http::response::Parts) -> Result<()> {
        for header in parts.headers.iter() {
            self.headers.insert(
                header.0.as_str().to_string(),
                header.1.to_str()?.to_string(),
            );
        }
        Ok(())
    }
    pub fn must_revalidate(&self) -> bool {
        if let Some(val) = self.headers.get(CACHE_CONTROL.as_str()) {
            val.as_str().to_lowercase().contains("must-revalidate")
        } else {
            false
        }
    }
}

#[async_trait::async_trait]
pub(crate) trait Middleware {
    fn is_method_get_head(&self) -> bool;
    fn new_policy(&self, response: &HttpResponse) -> Result<CachePolicy>;
    fn update_request_headers(&mut self, parts: http::request::Parts) -> Result<()>;
    fn set_no_cache(&mut self) -> Result<()>;
    fn get_request_parts(&self) -> Result<http::request::Parts>;
    fn before_request(&self, policy: &CachePolicy) -> Result<BeforeRequest>;
    fn after_response(
        &self,
        policy: &CachePolicy,
        response: &HttpResponse,
    ) -> Result<AfterResponse>;
    fn url(&self) -> Result<&Url>;
    fn method(&self) -> Result<String>;
    async fn remote_fetch(&self) -> Result<HttpResponse>;
}

#[cfg(feature = "client-surf")]
struct SurfMiddleware<'a> {
    req: surf::Request,
    client: surf::Client,
    next: surf::middleware::Next<'a>,
}

#[cfg(feature = "client-surf")]
#[async_trait::async_trait]
impl Middleware for SurfMiddleware<'_> {
    fn is_method_get_head(&self) -> bool {
        self.req.method() == http_types::Method::Get
            || self.req.method() == http_types::Method::Head
    }
    fn new_policy(&self, response: &HttpResponse) -> Result<CachePolicy> {
        Ok(CachePolicy::new(
            &self.get_request_parts()?,
            &response.get_parts()?,
        ))
    }
    fn update_request_headers(&mut self, parts: Parts) -> Result<()> {
        for header in parts.headers.iter() {
            let value = match http_types::headers::HeaderValue::from_str(header.1.to_str()?) {
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
    fn get_request_parts(&self) -> Result<Parts> {
        let mut headers = http::HeaderMap::new();
        for header in self.req.iter() {
            headers.insert(
                http::header::HeaderName::from_str(header.0.as_str())?,
                http::HeaderValue::from_str(header.1.as_str())?,
            );
        }
        let uri = http::Uri::from_str(self.req.url().as_str())?;
        let method = http::Method::from_str(self.req.method().as_ref())?;
        let mut converted = http::request::Request::new(());
        converted.headers_mut().clone_from(&headers);
        converted.uri_mut().clone_from(&uri);
        converted.method_mut().clone_from(&method);
        let parts = converted.into_parts();
        Ok(parts.0)
    }
    fn before_request(&self, policy: &CachePolicy) -> Result<BeforeRequest> {
        Ok(policy.before_request(&self.get_request_parts()?, SystemTime::now()))
    }
    fn after_response(
        &self,
        policy: &CachePolicy,
        response: &HttpResponse,
    ) -> Result<AfterResponse> {
        Ok(policy.after_response(
            &self.get_request_parts()?,
            &response.get_parts()?,
            SystemTime::now(),
        ))
    }
    fn url(&self) -> Result<&Url> {
        Ok(self.req.url())
    }
    fn method(&self) -> Result<String> {
        Ok(self.req.method().as_ref().to_string())
    }
    async fn remote_fetch(&self) -> Result<HttpResponse> {
        let url = self.req.url().clone();
        let mut res = self
            .next
            .run(self.req.clone(), self.client.clone())
            .await
            .unwrap();
        let mut headers = HashMap::new();
        for header in res.iter() {
            headers.insert(header.0.as_str().to_owned(), header.1.as_str().to_owned());
        }
        let status = res.status().into();
        let version = res.version().unwrap_or(http_types::Version::Http1_1);
        let body: Vec<u8> = res.body_bytes().await.unwrap();
        Ok(HttpResponse {
            body,
            headers,
            status,
            url,
            version: version.try_into()?,
        })
    }
}

#[cfg(feature = "client-reqwest")]
struct ReqwestMiddleware<'a> {
    req: reqwest::Request,
    next: reqwest_middleware::Next<'a>,
    ext: task_local_extensions::Extensions,
}

#[cfg(feature = "client-reqwest")]
#[async_trait::async_trait]
impl Middleware for ReqwestMiddleware<'_> {
    fn is_method_get_head(&self) -> bool {
        self.req.method() == http::Method::GET || self.req.method() == http::Method::HEAD
    }
    fn new_policy(&self, response: &HttpResponse) -> Result<CachePolicy> {
        todo!()
    }
    fn update_request_headers(&mut self, parts: Parts) -> Result<()> {
        todo!()
    }
    fn set_no_cache(&mut self) -> Result<()> {
        todo!()
    }
    fn get_request_parts(&self) -> Result<Parts> {
        todo!()
    }
    fn before_request(&self, policy: &CachePolicy) -> Result<BeforeRequest> {
        todo!()
    }
    fn after_response(
        &self,
        policy: &CachePolicy,
        response: &HttpResponse,
    ) -> Result<AfterResponse> {
        todo!()
    }
    fn url(&self) -> Result<&Url> {
        todo!()
    }
    fn method(&self) -> Result<&str> {
        todo!()
    }
    async fn remote_fetch(&self) -> Result<HttpResponse> {
        todo!()
    }
}

/// A trait providing methods for storing, reading, and removing cache records.
#[async_trait::async_trait]
pub trait CacheManager {
    /// Attempts to pull a cached response and related policy from cache.
    async fn get(&self, method: &str, url: &Url) -> Result<Option<(HttpResponse, CachePolicy)>>;
    /// Attempts to cache a response and related policy.
    async fn put(
        &self,
        method: &str,
        url: &Url,
        res: HttpResponse,
        policy: CachePolicy,
    ) -> Result<HttpResponse>;
    /// Attempts to remove a record from cache.
    async fn delete(&self, method: &str, url: &Url) -> Result<()>;
}

/// Similar to [make-fetch-happen cache options](https://github.com/npm/make-fetch-happen#--optscache).
/// Passed in when the [`Cache`] struct is being built.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheMode {
    /// Will inspect the HTTP cache on the way to the network.
    /// If there is a fresh response it will be used.
    /// If there is a stale response a conditional request will be created,
    /// and a normal request otherwise.
    /// It then updates the HTTP cache with the response.
    /// If the revalidation request fails (for example, on a 500 or if you're offline),
    /// the stale response will be returned.
    Default,
    /// Behaves as if there is no HTTP cache at all.
    NoStore,
    /// Behaves as if there is no HTTP cache on the way to the network.
    /// Ergo, it creates a normal request and updates the HTTP cache with the response.
    Reload,
    /// Creates a conditional request if there is a response in the HTTP cache
    /// and a normal request otherwise. It then updates the HTTP cache with the response.
    NoCache,
    /// Uses any response in the HTTP cache matching the request,
    /// not paying attention to staleness. If there was no response,
    /// it creates a normal request and updates the HTTP cache with the response.
    ForceCache,
    /// Uses any response in the HTTP cache matching the request,
    /// not paying attention to staleness. If there was no response,
    /// it returns a network error. (Can only be used when request’s mode is "same-origin".
    /// Any cached redirects will be followed assuming request’s redirect mode is "follow"
    /// and the redirects do not violate request’s mode.)
    OnlyIfCached,
}

/// Caches requests according to http spec
#[derive(Debug, Clone)]
pub struct Cache<T: CacheManager + Send + Sync + 'static> {
    /// Determines the manager behavior
    pub mode: CacheMode,
    /// Manager instance that implements the CacheManager trait
    pub cache_manager: T,
}

impl<T: CacheManager + Send + Sync + 'static> Cache<T> {
    pub(crate) async fn run(&self, mut middleware: impl Middleware) -> Result<HttpResponse> {
        let is_cacheable = middleware.is_method_get_head()
            && self.mode != CacheMode::NoStore
            && self.mode != CacheMode::Reload;
        if !is_cacheable {
            return middleware.remote_fetch().await;
        }
        if let Some(store) = self
            .cache_manager
            .get(&middleware.method()?, middleware.url()?)
            .await?
        {
            let (mut res, policy) = store;
            let res_url = res.url.clone();
            if let Some(warning_code) = res.get_warning_code() {
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
                CacheMode::Default => self.conditional_fetch(middleware, res, policy).await,
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
                    return Ok(res);
                }
                _ => self.remote_fetch(middleware).await,
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
                _ => self.remote_fetch(middleware).await,
            }
        }
    }

    async fn remote_fetch(&self, middleware: impl Middleware) -> Result<HttpResponse> {
        let res = middleware.remote_fetch().await?;
        let policy = middleware.new_policy(&res)?;
        let is_cacheable = middleware.is_method_get_head()
            && self.mode != CacheMode::NoStore
            && self.mode != CacheMode::Reload
            && res.status == 200
            && policy.is_storable();
        if is_cacheable {
            Ok(self
                .cache_manager
                .put(&middleware.method()?, middleware.url()?, res, policy)
                .await?)
        } else if !middleware.is_method_get_head() {
            self.cache_manager
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
        let before_req = middleware.before_request(&policy)?;
        match before_req {
            BeforeRequest::Fresh(parts) => {
                cached_res.update_headers_from_parts(parts)?;
                return Ok(cached_res);
            }
            BeforeRequest::Stale {
                request: parts,
                matches,
            } => {
                if matches {
                    middleware.update_request_headers(parts)?;
                }
            }
        }
        let res_url = middleware.url()?.clone();
        match middleware.remote_fetch().await {
            Ok(cond_res) => {
                let status = http::StatusCode::from_u16(cond_res.status)?;
                if status.is_server_error() && cached_res.must_revalidate() {
                    //   111 Revalidation failed
                    //   MUST be included if a cache returns a stale response
                    //   because an attempt to revalidate the response failed,
                    //   due to an inability to reach the server.
                    // (https://tools.ietf.org/html/rfc2616#section-14.46)
                    cached_res.add_warning(res_url.clone(), 111, "Revalidation failed");
                    Ok(cached_res)
                } else if cond_res.status == 304 {
                    let after_res = middleware.after_response(&policy, &cond_res)?;
                    match after_res {
                        AfterResponse::Modified(new_policy, parts) => {
                            policy = new_policy;
                            cached_res.update_headers_from_parts(parts)?;
                        }
                        AfterResponse::NotModified(new_policy, parts) => {
                            policy = new_policy;
                            cached_res.update_headers_from_parts(parts)?;
                        }
                    }
                    let res = self
                        .cache_manager
                        .put(&middleware.method()?, &res_url, cached_res, policy)
                        .await?;
                    Ok(res)
                } else {
                    Ok(cached_res)
                }
            }
            Err(e) => {
                if cached_res.must_revalidate() {
                    return Err(e);
                } else {
                    //   111 Revalidation failed
                    //   MUST be included if a cache returns a stale response
                    //   because an attempt to revalidate the response failed,
                    //   due to an inability to reach the server.
                    // (https://tools.ietf.org/html/rfc2616#section-14.46)
                    cached_res.add_warning(res_url.clone(), 111, "Revalidation failed");
                    Ok(cached_res)
                }
            }
        }
    }
}

#[cfg(feature = "client-surf")]
#[surf::utils::async_trait]
impl<T: CacheManager + 'static + Send + Sync> surf::middleware::Middleware for Cache<T> {
    async fn handle(
        &self,
        req: surf::Request,
        client: surf::Client,
        next: surf::middleware::Next<'_>,
    ) -> std::result::Result<surf::Response, http_types::Error> {
        let middleware = SurfMiddleware { req, client, next };
        let res = self.run(middleware).await?;
        let mut converted = http_types::Response::new(http_types::StatusCode::Ok);
        for header in &res.headers {
            let val =
                http_types::headers::HeaderValue::from_bytes(header.1.as_bytes().to_vec()).unwrap();
            converted.insert_header(header.0.as_str(), val);
        }
        converted.set_status(res.status.try_into()?);
        converted.set_version(Some(res.version.try_into()?));
        converted.set_body(res.body.clone());
        Ok(surf::Response::from(converted))
    }
}
