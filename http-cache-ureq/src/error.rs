use std::fmt;

/// Errors that can occur when using the ureq cache
#[derive(Debug)]
pub enum UreqError {
    /// Cache-related errors
    Cache(String),
    /// HTTP conversion errors
    Http(String),
}

impl fmt::Display for UreqError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UreqError::Cache(msg) => write!(f, "Cache error: {}", msg),
            UreqError::Http(msg) => write!(f, "HTTP error: {}", msg),
        }
    }
}

impl std::error::Error for UreqError {}

/// Error for bad requests that can't be cached
#[derive(Debug, Clone, Copy, Default)]
pub struct BadRequest;

impl fmt::Display for BadRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Bad request: unable to clone or process request")
    }
}

impl std::error::Error for BadRequest {}
