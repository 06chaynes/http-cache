use std::fmt;

/// Generic error type for the `HttpCache` middleware.
pub type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// A `Result` typedef to use with the [`BoxError`] type
pub type Result<T> = std::result::Result<T, BoxError>;

/// Error type for unknown http versions
#[derive(Debug, Default, Copy, Clone)]
pub struct BadVersion;

impl fmt::Display for BadVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad("Unknown HTTP version")
    }
}

impl std::error::Error for BadVersion {}

/// Error type for bad header values
#[derive(Debug, Default, Copy, Clone)]
pub struct BadHeader;

impl fmt::Display for BadHeader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad("Error parsing header value")
    }
}

impl std::error::Error for BadHeader {}
