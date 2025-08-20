use std::fmt;

/// Error type for request parsing failure
#[derive(Debug, Default, Copy, Clone)]
pub struct BadRequest;

impl fmt::Display for BadRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad("Request object is not cloneable. Are you passing a streaming body?")
    }
}

impl std::error::Error for BadRequest {}

/// Error type for the `HttpCache` Reqwest implementation.
#[derive(Debug)]
pub enum ReqwestError {
    /// Reqwest client error
    Reqwest(reqwest::Error),
    /// HTTP cache operation failed
    Cache(String),
    /// Request parsing failed
    BadRequest(BadRequest),
    /// Other error
    Other(Box<dyn std::error::Error + Send + Sync>),
}

impl fmt::Display for ReqwestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReqwestError::Reqwest(e) => write!(f, "Reqwest error: {e}"),
            ReqwestError::Cache(msg) => write!(f, "Cache error: {msg}"),
            ReqwestError::BadRequest(e) => write!(f, "Request error: {e}"),
            ReqwestError::Other(e) => write!(f, "Other error: {e}"),
        }
    }
}

impl std::error::Error for ReqwestError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ReqwestError::Reqwest(e) => Some(e),
            ReqwestError::Cache(_) => None,
            ReqwestError::BadRequest(e) => Some(e),
            ReqwestError::Other(e) => Some(e.as_ref()),
        }
    }
}

impl From<reqwest::Error> for ReqwestError {
    fn from(error: reqwest::Error) -> Self {
        ReqwestError::Reqwest(error)
    }
}

impl From<BadRequest> for ReqwestError {
    fn from(error: BadRequest) -> Self {
        ReqwestError::BadRequest(error)
    }
}

impl From<Box<dyn std::error::Error + Send + Sync>> for ReqwestError {
    fn from(error: Box<dyn std::error::Error + Send + Sync>) -> Self {
        ReqwestError::Other(error)
    }
}

#[cfg(feature = "streaming")]
/// Error type for reqwest streaming operations
#[derive(Debug)]
pub enum ReqwestStreamingError {
    /// Reqwest error
    Reqwest(reqwest::Error),
    /// HTTP cache streaming error
    HttpCache(http_cache::StreamingError),
    /// Other error
    Other(Box<dyn std::error::Error + Send + Sync>),
}

#[cfg(feature = "streaming")]
impl fmt::Display for ReqwestStreamingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReqwestStreamingError::Reqwest(e) => {
                write!(f, "Reqwest error: {e}")
            }
            ReqwestStreamingError::HttpCache(e) => {
                write!(f, "HTTP cache streaming error: {e}")
            }
            ReqwestStreamingError::Other(e) => write!(f, "Other error: {e}"),
        }
    }
}

#[cfg(feature = "streaming")]
impl std::error::Error for ReqwestStreamingError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ReqwestStreamingError::Reqwest(e) => Some(e),
            ReqwestStreamingError::HttpCache(e) => Some(e),
            ReqwestStreamingError::Other(e) => Some(&**e),
        }
    }
}

#[cfg(feature = "streaming")]
impl From<reqwest::Error> for ReqwestStreamingError {
    fn from(error: reqwest::Error) -> Self {
        ReqwestStreamingError::Reqwest(error)
    }
}

#[cfg(feature = "streaming")]
impl From<http_cache::StreamingError> for ReqwestStreamingError {
    fn from(error: http_cache::StreamingError) -> Self {
        ReqwestStreamingError::HttpCache(error)
    }
}

#[cfg(feature = "streaming")]
impl From<Box<dyn std::error::Error + Send + Sync>> for ReqwestStreamingError {
    fn from(error: Box<dyn std::error::Error + Send + Sync>) -> Self {
        ReqwestStreamingError::Other(error)
    }
}

#[cfg(feature = "streaming")]
impl From<ReqwestStreamingError> for http_cache::StreamingError {
    fn from(error: ReqwestStreamingError) -> Self {
        http_cache::StreamingError::new(Box::new(error))
    }
}
