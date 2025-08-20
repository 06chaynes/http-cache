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

/// Error type for the `HttpCache` Surf implementation.
#[derive(Debug)]
pub enum SurfError {
    /// There was a Surf client error
    Surf(Box<dyn std::error::Error + Send + Sync>),
    /// HTTP cache operation failed
    Cache(String),
    /// Request parsing failed
    BadRequest(BadRequest),
}

impl fmt::Display for SurfError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SurfError::Surf(e) => write!(f, "Surf error: {e}"),
            SurfError::Cache(msg) => write!(f, "Cache error: {msg}"),
            SurfError::BadRequest(e) => write!(f, "Request error: {e}"),
        }
    }
}

impl std::error::Error for SurfError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            SurfError::Surf(e) => Some(e.as_ref()),
            SurfError::Cache(_) => None,
            SurfError::BadRequest(e) => Some(e),
        }
    }
}

impl From<BadRequest> for SurfError {
    fn from(error: BadRequest) -> Self {
        SurfError::BadRequest(error)
    }
}

impl From<Box<dyn std::error::Error + Send + Sync>> for SurfError {
    fn from(error: Box<dyn std::error::Error + Send + Sync>) -> Self {
        SurfError::Surf(error)
    }
}
