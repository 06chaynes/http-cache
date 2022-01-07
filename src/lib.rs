mod error;

pub use error::CacheError;

use std::{
    collections::HashMap,
    convert::{TryFrom, TryInto},
    time::SystemTime,
};

use http_cache_semantics::{AfterResponse, BeforeRequest, CachePolicy};
use serde::{Deserialize, Serialize};
use url::Url;

// Represents an HTTP version
#[derive(Debug, Copy, Clone, Deserialize, Serialize)]
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

    fn try_from(value: http::Version) -> Result<Self, CacheError> {
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

    fn try_from(value: http_types::Version) -> Result<Self, CacheError> {
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
#[derive(Debug, Deserialize, Serialize)]
pub struct HttpResponse {
    pub body: Vec<u8>,
    pub headers: HashMap<String, String>,
    pub status: u16,
    pub url: Url,
    pub version: HttpVersion,
}

/// A trait providing methods for storing, reading, and removing cache records.
#[async_trait::async_trait]
pub trait CacheManager {
    /// Attempts to pull a cached response and related policy from cache.
    async fn get(&self, url: &Url) -> Result<Option<(HttpResponse, CachePolicy)>, CacheError>;
    /// Attempts to cache a response and related policy.
    async fn put(
        &self,
        req: &Url,
        res: HttpResponse,
        policy: CachePolicy,
    ) -> Result<HttpResponse, CacheError>;
    /// Attempts to remove a record from cache.
    async fn delete(&self, req: &Url) -> Result<(), CacheError>;
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
