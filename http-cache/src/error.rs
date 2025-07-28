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

/// Error type for streaming operations
#[derive(Debug)]
pub struct StreamingError {
    inner: BoxError,
}

impl StreamingError {
    /// Create a new streaming error from any error type
    pub fn new<E: Into<BoxError>>(error: E) -> Self {
        Self { inner: error.into() }
    }
}

impl fmt::Display for StreamingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Streaming error: {}", self.inner)
    }
}

impl std::error::Error for StreamingError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&*self.inner)
    }
}

impl From<BoxError> for StreamingError {
    fn from(error: BoxError) -> Self {
        Self::new(error)
    }
}

impl From<std::convert::Infallible> for StreamingError {
    fn from(never: std::convert::Infallible) -> Self {
        match never {}
    }
}
