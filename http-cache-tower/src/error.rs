use http_cache;
use std::fmt;

/// Errors that can occur during HTTP caching operations
#[derive(Debug)]
pub enum TowerError {
    /// Cache operation failed
    CacheError(String),
    /// Body collection failed
    BodyError(Box<dyn std::error::Error + Send + Sync>),
    /// HTTP processing error
    HttpError(Box<dyn std::error::Error + Send + Sync>),
}

impl fmt::Display for TowerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TowerError::CacheError(msg) => write!(f, "Cache error: {msg}"),
            TowerError::BodyError(e) => {
                write!(f, "Body processing error: {e}")
            }
            TowerError::HttpError(e) => write!(f, "HTTP error: {e}"),
        }
    }
}

impl std::error::Error for TowerError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            TowerError::CacheError(_) => None,
            TowerError::BodyError(e) => Some(e.as_ref()),
            TowerError::HttpError(e) => Some(e.as_ref()),
        }
    }
}

impl From<http_cache::BoxError> for TowerError {
    fn from(error: http_cache::BoxError) -> Self {
        TowerError::HttpError(error)
    }
}

#[cfg(feature = "streaming")]
/// Errors that can occur during streaming HTTP cache operations
#[derive(Debug)]
pub enum TowerStreamingError {
    /// Tower-specific error
    Tower(Box<dyn std::error::Error + Send + Sync>),
    /// HTTP cache streaming error
    HttpCache(http_cache::StreamingError),
    /// Other error
    Other(Box<dyn std::error::Error + Send + Sync>),
}

#[cfg(feature = "streaming")]
impl fmt::Display for TowerStreamingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TowerStreamingError::Tower(e) => write!(f, "Tower error: {e}"),
            TowerStreamingError::HttpCache(e) => {
                write!(f, "HTTP cache streaming error: {e}")
            }
            TowerStreamingError::Other(e) => write!(f, "Other error: {e}"),
        }
    }
}

#[cfg(feature = "streaming")]
impl std::error::Error for TowerStreamingError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            TowerStreamingError::Tower(e) => Some(&**e),
            TowerStreamingError::HttpCache(e) => Some(e),
            TowerStreamingError::Other(e) => Some(&**e),
        }
    }
}

#[cfg(feature = "streaming")]
impl From<Box<dyn std::error::Error + Send + Sync>> for TowerStreamingError {
    fn from(error: Box<dyn std::error::Error + Send + Sync>) -> Self {
        TowerStreamingError::Tower(error)
    }
}

#[cfg(feature = "streaming")]
impl From<http_cache::StreamingError> for TowerStreamingError {
    fn from(error: http_cache::StreamingError) -> Self {
        TowerStreamingError::HttpCache(error)
    }
}

#[cfg(feature = "streaming")]
impl From<TowerStreamingError> for http_cache::StreamingError {
    fn from(val: TowerStreamingError) -> Self {
        match val {
            TowerStreamingError::HttpCache(e) => e,
            TowerStreamingError::Tower(e) => http_cache::StreamingError::new(e),
            TowerStreamingError::Other(e) => http_cache::StreamingError::new(e),
        }
    }
}
