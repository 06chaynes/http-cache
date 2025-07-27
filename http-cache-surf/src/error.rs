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
