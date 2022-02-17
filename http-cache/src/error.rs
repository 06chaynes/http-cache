use miette::Diagnostic;
use thiserror::Error;

/// A `Result` typedef to use with the [`CacheError`] type
pub type Result<T> = std::result::Result<T, CacheError>;

/// A generic “error” for HTTP caches
#[derive(Error, Diagnostic, Debug)]
pub enum CacheError {
    /// A general error used as a catch all for other errors via anyhow
    #[error(transparent)]
    #[diagnostic(code(http_cache::general))]
    General(#[from] anyhow::Error),
    /// Error from http
    #[error(transparent)]
    #[diagnostic(code(http_cache::http))]
    Http(#[from] http::Error),
    /// There was an error parsing the HTTP status code
    #[error(transparent)]
    #[diagnostic(code(http_cache::invalid_status_code))]
    InvalidStatusCode(#[from] http::status::InvalidStatusCode),
    /// There was an error converting the header to a string
    #[error(transparent)]
    #[diagnostic(code(http_cache::header_to_str))]
    HeaderToStr(#[from] http::header::ToStrError),
    /// There was an error parsing the HTTP method
    #[error(transparent)]
    #[diagnostic(code(http_cache::invalid_method))]
    InvalidMethod(#[from] http::method::InvalidMethod),
    /// There was an error parsing the URI
    #[error(transparent)]
    #[diagnostic(code(http_cache::invalid_uri))]
    InvalidUri(#[from] http::uri::InvalidUri),
    /// There was an error parsing the URL
    #[error(transparent)]
    #[diagnostic(code(http_cache::invalid_url))]
    InvalidUrl(#[from] url::ParseError),
    /// There was an error parsing an HTTP header value
    #[error(transparent)]
    #[diagnostic(code(http_cache::invalid_header_value))]
    InvalidHeaderValue(#[from] http::header::InvalidHeaderValue),
    /// There was an error parsing an HTTP header name
    #[error(transparent)]
    #[diagnostic(code(http_cache::invalid_header_name))]
    InvalidHeaderName(#[from] http::header::InvalidHeaderName),
    /// Error from cacache
    #[cfg(feature = "manager-cacache")]
    #[error(transparent)]
    #[diagnostic(code(http_cache::cacache))]
    CaCache(#[from] cacache::Error),
    /// Error from bincode
    #[cfg(any(feature = "manager-cacache", feature = "manager-moka"))]
    #[error(transparent)]
    #[diagnostic(code(http_cache::bincode))]
    Bincode(#[from] Box<bincode::ErrorKind>),
    /// There was an error parsing the HTTP request version
    #[error("Unknown HTTP version")]
    #[diagnostic(code(http_cache::bad_version))]
    BadVersion,
    /// There was an error parsing an HTTP header value
    #[error("Error parsing header value")]
    #[diagnostic(code(http_cache::bad_header))]
    BadHeader,
    /// There was an error parsing the HTTP request
    #[error(
        "Request object is not cloneable. Are you passing a streaming body?"
    )]
    #[diagnostic(code(http_cache::bad_request))]
    BadRequest,
}
