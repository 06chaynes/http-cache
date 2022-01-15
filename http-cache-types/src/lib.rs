mod error;

use std::{
    collections::HashMap, convert::TryFrom, str::FromStr, time::SystemTime,
};

use http::{header::CACHE_CONTROL, request, response};
use http_cache_semantics::{CacheOptions, CachePolicy};
use serde::{Deserialize, Serialize};
use url::Url;

pub use error::{CacheError, Result};

/// Represents an HTTP version
#[derive(Debug, Copy, Clone, Deserialize, Serialize)]
#[non_exhaustive]
pub enum HttpVersion {
    /// HTTP Version 0.9
    #[serde(rename = "HTTP/0.9")]
    Http09,
    /// HTTP Version 1.0
    #[serde(rename = "HTTP/1.0")]
    Http10,
    /// HTTP Version 1.1
    #[serde(rename = "HTTP/1.1")]
    Http11,
    /// HTTP Version 2.0
    #[serde(rename = "HTTP/2.0")]
    H2,
    /// HTTP Version 3.0
    #[serde(rename = "HTTP/3.0")]
    H3,
}

/// A basic generic type that represents an HTTP response
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HttpResponse {
    /// HTTP response body
    pub body: Vec<u8>,
    /// HTTP response headers
    pub headers: HashMap<String, String>,
    /// HTTP response status code
    pub status: u16,
    /// HTTP response url
    pub url: Url,
    /// HTTP response version
    pub version: HttpVersion,
}

impl HttpResponse {
    /// Returns http::response::Parts
    pub fn parts(&self) -> Result<response::Parts> {
        let mut converted =
            response::Builder::new().status(self.status).body(())?;
        {
            let headers = converted.headers_mut();
            for header in self.headers.iter() {
                headers.insert(
                    http::header::HeaderName::from_str(header.0.as_str())?,
                    http::HeaderValue::from_str(header.1.as_str())?,
                );
            }
        }
        Ok(converted.into_parts().0)
    }

    /// Returns the status code of the warning header if present
    pub fn warning_code(&self) -> Option<usize> {
        self.headers.get("Warning").and_then(|hdr| {
            hdr.as_str().chars().take(3).collect::<String>().parse().ok()
        })
    }

    /// Adds a warning header to a response
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

    /// Removes a warning header from a response
    pub fn remove_warning(&mut self) {
        self.headers.remove("Warning");
    }

    /// Update the headers from http::response::Parts
    pub fn update_headers(&mut self, parts: response::Parts) -> Result<()> {
        for header in parts.headers.iter() {
            self.headers.insert(
                header.0.as_str().to_string(),
                header.1.to_str()?.to_string(),
            );
        }
        Ok(())
    }

    /// Checks if the Cache-Control header contains the must-revalidate directive
    pub fn must_revalidate(&self) -> bool {
        if let Some(val) = self.headers.get(CACHE_CONTROL.as_str()) {
            val.as_str().to_lowercase().contains("must-revalidate")
        } else {
            false
        }
    }
}

/// A trait providing methods for storing, reading, and removing cache records.
#[async_trait::async_trait]
pub trait CacheManager {
    /// Attempts to pull a cached response and related policy from cache.
    async fn get(
        &self,
        method: &str,
        url: &Url,
    ) -> Result<Option<(HttpResponse, CachePolicy)>>;
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

// Describes the functionality required for interfacing with HTTP client middleware
#[async_trait::async_trait]
pub trait Middleware {
    fn is_method_get_head(&self) -> bool;
    fn policy(&self, response: &HttpResponse) -> Result<CachePolicy>;
    fn policy_with_options(
        &self,
        response: &HttpResponse,
        options: CacheOptions,
    ) -> Result<CachePolicy>;
    fn update_headers(&mut self, parts: request::Parts) -> Result<()>;
    fn set_no_cache(&mut self) -> Result<()>;
    fn parts(&self) -> Result<request::Parts>;
    fn url(&self) -> Result<&Url>;
    fn method(&self) -> Result<String>;
    async fn remote_fetch(&mut self) -> Result<HttpResponse>;
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
    /// it returns a network error.
    OnlyIfCached,
}

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
