use std::fmt;
use thiserror::Error;

/// Error type for request parsing failure
#[derive(Debug, Default, Copy, Clone)]
pub struct BadRequest;

impl fmt::Display for BadRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad("Request object is not cloneable. Are you passing a streaming body?")
    }
}

impl std::error::Error for BadRequest {}

/// Generic error type for the `HttpCache` Surf implementation.
#[derive(Error, Debug)]
pub enum Error {
    /// There was a Surf client error
    #[error("Surf error: {0}")]
    Surf(#[from] anyhow::Error),
}

#[cfg(feature = "streaming")]
/// Error type for surf streaming operations
#[derive(Debug)]
pub enum SurfStreamingError {
    /// Surf error
    Surf(anyhow::Error),
    /// HTTP cache streaming error
    HttpCache(http_cache::StreamingError),
    /// Other error
    Other(Box<dyn std::error::Error + Send + Sync>),
}

#[cfg(feature = "streaming")]
impl fmt::Display for SurfStreamingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SurfStreamingError::Surf(e) => write!(f, "Surf error: {e}"),
            SurfStreamingError::HttpCache(e) => {
                write!(f, "HTTP cache streaming error: {e}")
            }
            SurfStreamingError::Other(e) => write!(f, "Other error: {e}"),
        }
    }
}

#[cfg(feature = "streaming")]
impl std::error::Error for SurfStreamingError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            SurfStreamingError::Surf(e) => e.source(),
            SurfStreamingError::HttpCache(e) => Some(e),
            SurfStreamingError::Other(e) => Some(&**e),
        }
    }
}

#[cfg(feature = "streaming")]
impl From<anyhow::Error> for SurfStreamingError {
    fn from(error: anyhow::Error) -> Self {
        SurfStreamingError::Surf(error)
    }
}

#[cfg(feature = "streaming")]
impl From<http_cache::StreamingError> for SurfStreamingError {
    fn from(error: http_cache::StreamingError) -> Self {
        SurfStreamingError::HttpCache(error)
    }
}

#[cfg(feature = "streaming")]
impl From<Box<dyn std::error::Error + Send + Sync>> for SurfStreamingError {
    fn from(error: Box<dyn std::error::Error + Send + Sync>) -> Self {
        SurfStreamingError::Other(error)
    }
}

#[cfg(feature = "streaming")]
impl From<SurfStreamingError> for http_cache::StreamingError {
    fn from(val: SurfStreamingError) -> Self {
        http_cache::StreamingError::new(Box::new(val))
    }
}
