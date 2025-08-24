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

/// Error type for request parsing failure
#[derive(Debug, Default, Copy, Clone)]
pub struct BadRequest;

impl fmt::Display for BadRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad("Request object is not cloneable. Are you passing a streaming body?")
    }
}

impl std::error::Error for BadRequest {}

/// Unified error type for HTTP cache operations that works across all client libraries.
///
/// This enum consolidates error handling patterns from all http-cache client crates
/// (reqwest, surf, tower, ureq) while providing a clean, extensible interface.
///
/// # Examples
///
/// ```rust
/// use http_cache::{HttpCacheError, BadRequest};
///
/// // Cache operation errors
/// let cache_err = HttpCacheError::cache("Failed to read cache entry");
///
/// // Request parsing errors
/// let request_err = HttpCacheError::from(BadRequest);
///
/// // HTTP processing errors
/// let http_err = HttpCacheError::http("Invalid header format");
///
/// // Body processing errors  
/// let body_err = HttpCacheError::body("Failed to collect request body");
/// ```
#[derive(Debug)]
pub enum HttpCacheError {
    /// HTTP client error (reqwest, surf, etc.)
    Client(BoxError),
    /// HTTP cache operation failed
    Cache(String),
    /// Request parsing failed (e.g., non-cloneable request)
    BadRequest(BadRequest),
    /// HTTP processing error (header parsing, version handling, etc.)
    Http(BoxError),
    /// Body processing error (collection, streaming, etc.)
    Body(BoxError),
    /// Streaming operation error (with detailed error kind)
    Streaming(StreamingError),
    /// Other generic error
    Other(BoxError),
}

impl HttpCacheError {
    /// Create a cache operation error
    ///
    /// # Examples
    ///
    /// ```rust
    /// use http_cache::HttpCacheError;
    ///
    /// let err = HttpCacheError::cache("Cache entry not found");
    /// ```
    pub fn cache<S: Into<String>>(message: S) -> Self {
        Self::Cache(message.into())
    }

    /// Create an HTTP processing error
    ///
    /// # Examples
    ///
    /// ```rust
    /// use http_cache::HttpCacheError;
    ///
    /// let err = HttpCacheError::http("Invalid header format");
    /// ```
    pub fn http<E: Into<BoxError>>(error: E) -> Self {
        Self::Http(error.into())
    }

    /// Create a body processing error
    ///
    /// # Examples
    ///
    /// ```rust
    /// use http_cache::HttpCacheError;
    ///
    /// let err = HttpCacheError::body("Failed to collect request body");
    /// ```
    pub fn body<E: Into<BoxError>>(error: E) -> Self {
        Self::Body(error.into())
    }

    /// Create a client error
    ///
    /// # Examples
    ///
    /// ```rust
    /// use http_cache::HttpCacheError;
    ///
    /// let err = HttpCacheError::client("Network timeout");
    /// ```
    pub fn client<E: Into<BoxError>>(error: E) -> Self {
        Self::Client(error.into())
    }

    /// Create a generic error
    ///
    /// # Examples
    ///
    /// ```rust
    /// use http_cache::HttpCacheError;
    ///
    /// let err = HttpCacheError::other("Unexpected error occurred");
    /// ```
    pub fn other<E: Into<BoxError>>(error: E) -> Self {
        Self::Other(error.into())
    }

    /// Returns true if this error is related to cache operations
    pub fn is_cache_error(&self) -> bool {
        matches!(self, Self::Cache(_))
    }

    /// Returns true if this error is related to client operations
    pub fn is_client_error(&self) -> bool {
        matches!(self, Self::Client(_))
    }

    /// Returns true if this error is related to streaming operations
    pub fn is_streaming_error(&self) -> bool {
        matches!(self, Self::Streaming(_))
    }

    /// Returns true if this error is a bad request
    pub fn is_bad_request(&self) -> bool {
        matches!(self, Self::BadRequest(_))
    }
}

impl fmt::Display for HttpCacheError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Client(e) => write!(f, "HTTP client error: {e}"),
            Self::Cache(msg) => write!(f, "Cache error: {msg}"),
            Self::BadRequest(e) => write!(f, "Request error: {e}"),
            Self::Http(e) => write!(f, "HTTP error: {e}"),
            Self::Body(e) => write!(f, "Body processing error: {e}"),
            Self::Streaming(e) => write!(f, "Streaming error: {e}"),
            Self::Other(e) => write!(f, "Other error: {e}"),
        }
    }
}

impl std::error::Error for HttpCacheError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Client(e) => Some(e.as_ref()),
            Self::Cache(_) => None,
            Self::BadRequest(e) => Some(e),
            Self::Http(e) => Some(e.as_ref()),
            Self::Body(e) => Some(e.as_ref()),
            Self::Streaming(e) => Some(e),
            Self::Other(e) => Some(e.as_ref()),
        }
    }
}

// Comprehensive From implementations for common error types

impl From<BadRequest> for HttpCacheError {
    fn from(error: BadRequest) -> Self {
        Self::BadRequest(error)
    }
}

impl From<BadHeader> for HttpCacheError {
    fn from(error: BadHeader) -> Self {
        Self::Http(Box::new(error))
    }
}

impl From<BadVersion> for HttpCacheError {
    fn from(error: BadVersion) -> Self {
        Self::Http(Box::new(error))
    }
}

impl From<StreamingError> for HttpCacheError {
    fn from(error: StreamingError) -> Self {
        Self::Streaming(error)
    }
}

impl From<BoxError> for HttpCacheError {
    fn from(error: BoxError) -> Self {
        Self::Other(error)
    }
}

impl From<std::io::Error> for HttpCacheError {
    fn from(error: std::io::Error) -> Self {
        Self::Other(Box::new(error))
    }
}

impl From<http::Error> for HttpCacheError {
    fn from(error: http::Error) -> Self {
        Self::Http(Box::new(error))
    }
}

impl From<http::header::InvalidHeaderValue> for HttpCacheError {
    fn from(error: http::header::InvalidHeaderValue) -> Self {
        Self::Http(Box::new(error))
    }
}

impl From<http::header::InvalidHeaderName> for HttpCacheError {
    fn from(error: http::header::InvalidHeaderName) -> Self {
        Self::Http(Box::new(error))
    }
}

impl From<http::uri::InvalidUri> for HttpCacheError {
    fn from(error: http::uri::InvalidUri) -> Self {
        Self::Http(Box::new(error))
    }
}

impl From<http::method::InvalidMethod> for HttpCacheError {
    fn from(error: http::method::InvalidMethod) -> Self {
        Self::Http(Box::new(error))
    }
}

impl From<http::status::InvalidStatusCode> for HttpCacheError {
    fn from(error: http::status::InvalidStatusCode) -> Self {
        Self::Http(Box::new(error))
    }
}

impl From<url::ParseError> for HttpCacheError {
    fn from(error: url::ParseError) -> Self {
        Self::Http(Box::new(error))
    }
}

// Note: Client-specific error conversions (reqwest, surf, ureq, etc.)
// are implemented in their respective http-cache-* crates to avoid
// feature dependencies in the core http-cache crate.

// Type alias for results using the unified error type
/// A `Result` type alias for HTTP cache operations using [`HttpCacheError`]
pub type HttpCacheResult<T> = std::result::Result<T, HttpCacheError>;

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
    /// Client library error (e.g., reqwest, surf)
    Client,
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

    /// Create a client error
    pub fn client<E: Into<BoxError>>(error: E) -> Self {
        Self::with_kind(error, StreamingErrorKind::Client)
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

impl From<HttpCacheError> for StreamingError {
    fn from(error: HttpCacheError) -> Self {
        match error {
            HttpCacheError::Streaming(streaming_err) => streaming_err,
            _ => Self::new(Box::new(error)),
        }
    }
}

/// Streaming error type specifically for client-specific streaming operations
///
/// This type provides a more granular error classification for streaming operations
/// while being compatible with the unified HttpCacheError system.
///
/// # Examples
///
/// ```rust
/// use http_cache::{ClientStreamingError, HttpCacheError};
///
/// // Create a streaming error with specific client context
/// let streaming_err = ClientStreamingError::client("reqwest", "Network timeout during streaming");
/// let cache_err: HttpCacheError = streaming_err.into();
/// ```
#[derive(Debug)]
pub enum ClientStreamingError {
    /// Client-specific streaming error with context
    Client {
        /// The name of the client library (e.g., "reqwest", "tower")
        client: String,
        /// The underlying client error
        error: BoxError,
    },
    /// HTTP cache streaming error (delegated to StreamingError)
    HttpCache(StreamingError),
    /// Other streaming error
    Other(BoxError),
}

impl ClientStreamingError {
    /// Create a client-specific streaming error
    ///
    /// # Examples
    ///
    /// ```rust
    /// use http_cache::ClientStreamingError;
    ///
    /// let err = ClientStreamingError::client("reqwest", "Connection timeout");
    /// ```
    pub fn client<C, E>(client: C, error: E) -> Self
    where
        C: Into<String>,
        E: Into<BoxError>,
    {
        Self::Client { client: client.into(), error: error.into() }
    }

    /// Create an HTTP cache streaming error
    ///
    /// # Examples
    ///
    /// ```rust
    /// use http_cache::{ClientStreamingError, StreamingError};
    ///
    /// let streaming_err = StreamingError::io("File read failed");
    /// let err = ClientStreamingError::http_cache(streaming_err);
    /// ```
    pub fn http_cache(error: StreamingError) -> Self {
        Self::HttpCache(error)
    }

    /// Create a generic streaming error
    ///
    /// # Examples
    ///
    /// ```rust
    /// use http_cache::ClientStreamingError;
    ///
    /// let err = ClientStreamingError::other("Unexpected streaming error");
    /// ```
    pub fn other<E: Into<BoxError>>(error: E) -> Self {
        Self::Other(error.into())
    }
}

impl fmt::Display for ClientStreamingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Client { client, error } => {
                write!(f, "{} streaming error: {}", client, error)
            }
            Self::HttpCache(e) => {
                write!(f, "HTTP cache streaming error: {}", e)
            }
            Self::Other(e) => write!(f, "Streaming error: {}", e),
        }
    }
}

impl std::error::Error for ClientStreamingError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Client { error, .. } => Some(error.as_ref()),
            Self::HttpCache(e) => Some(e),
            Self::Other(e) => Some(e.as_ref()),
        }
    }
}

impl From<StreamingError> for ClientStreamingError {
    fn from(error: StreamingError) -> Self {
        Self::HttpCache(error)
    }
}

impl From<BoxError> for ClientStreamingError {
    fn from(error: BoxError) -> Self {
        Self::Other(error)
    }
}

impl From<ClientStreamingError> for HttpCacheError {
    fn from(error: ClientStreamingError) -> Self {
        match error {
            ClientStreamingError::HttpCache(streaming_err) => {
                Self::Streaming(streaming_err)
            }
            ClientStreamingError::Client { error, .. } => Self::Client(error),
            ClientStreamingError::Other(error) => Self::Other(error),
        }
    }
}

impl From<ClientStreamingError> for StreamingError {
    fn from(error: ClientStreamingError) -> Self {
        match error {
            ClientStreamingError::HttpCache(streaming_err) => streaming_err,
            ClientStreamingError::Client { client, error } => {
                // Preserve client context by wrapping in a descriptive error
                let client_error =
                    format!("Client '{}' error: {}", client, error);
                Self::client(client_error)
            }
            ClientStreamingError::Other(error) => Self::new(error),
        }
    }
}
