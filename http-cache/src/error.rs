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
    kind: StreamingErrorKind,
}

/// Different kinds of streaming errors for better error handling
#[derive(Debug, Clone, Copy)]
pub enum StreamingErrorKind {
    /// I/O error (file operations, network)
    Io,
    /// Serialization/deserialization error
    Serialization,
    /// Lock contention or synchronization error
    Concurrency,
    /// Cache consistency error
    Consistency,
    /// Temporary file management error
    TempFile,
    /// Content addressing error (SHA256, file paths)
    ContentAddressing,
    /// Generic streaming error
    Other,
}

impl StreamingError {
    /// Create a new streaming error from any error type
    pub fn new<E: Into<BoxError>>(error: E) -> Self {
        Self { inner: error.into(), kind: StreamingErrorKind::Other }
    }

    /// Create a streaming error with a specific kind
    pub fn with_kind<E: Into<BoxError>>(
        error: E,
        kind: StreamingErrorKind,
    ) -> Self {
        Self { inner: error.into(), kind }
    }

    /// Create an I/O error
    pub fn io<E: Into<BoxError>>(error: E) -> Self {
        Self::with_kind(error, StreamingErrorKind::Io)
    }

    /// Create a serialization error
    pub fn serialization<E: Into<BoxError>>(error: E) -> Self {
        Self::with_kind(error, StreamingErrorKind::Serialization)
    }

    /// Create a concurrency error
    pub fn concurrency<E: Into<BoxError>>(error: E) -> Self {
        Self::with_kind(error, StreamingErrorKind::Concurrency)
    }

    /// Create a consistency error
    pub fn consistency<E: Into<BoxError>>(error: E) -> Self {
        Self::with_kind(error, StreamingErrorKind::Consistency)
    }

    /// Create a temp file error
    pub fn temp_file<E: Into<BoxError>>(error: E) -> Self {
        Self::with_kind(error, StreamingErrorKind::TempFile)
    }

    /// Create a content addressing error
    pub fn content_addressing<E: Into<BoxError>>(error: E) -> Self {
        Self::with_kind(error, StreamingErrorKind::ContentAddressing)
    }

    /// Get the error kind
    pub fn kind(&self) -> &StreamingErrorKind {
        &self.kind
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

impl From<std::io::Error> for StreamingError {
    fn from(error: std::io::Error) -> Self {
        Self::new(error)
    }
}
