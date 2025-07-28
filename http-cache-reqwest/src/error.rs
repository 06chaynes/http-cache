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
