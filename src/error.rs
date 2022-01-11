#![allow(missing_docs)]
use miette::Diagnostic;
use thiserror::Error;

/// A generic “error” for HTTP caches
#[derive(Error, Diagnostic, Debug)]
pub enum CacheError {
    #[error(transparent)]
    #[diagnostic(code(http_cache::general))]
    General(#[from] anyhow::Error),
    #[error(transparent)]
    #[diagnostic(code(http_cache::io_error))]
    IoError(#[from] std::io::Error),
    #[error(transparent)]
    #[diagnostic(code(http_cache::invalid_status_code))]
    InvalidStatusCode(#[from] http::status::InvalidStatusCode),
    #[error(transparent)]
    #[diagnostic(code(http_cache::header_to_str_error))]
    HeaderToStrError(#[from] http::header::ToStrError),
    #[error(transparent)]
    #[diagnostic(code(http_cache::invalid_method))]
    InvalidMethod(#[from] http::method::InvalidMethod),
    #[error(transparent)]
    #[diagnostic(code(http_cache::invalid_uri))]
    InvalidUri(#[from] http::uri::InvalidUri),
    #[error(transparent)]
    #[diagnostic(code(http_cache::invalid_header_value))]
    InvalidHeaderValue(#[from] http::header::InvalidHeaderValue),
    #[error(transparent)]
    #[diagnostic(code(http_cache::invalid_header_name))]
    InvalidHeaderName(#[from] http::header::InvalidHeaderName),
    #[cfg(feature = "client-reqwest")]
    #[error(transparent)]
    #[diagnostic(code(http_cache::reqwest_error))]
    ReqwestError(#[from] reqwest::Error),
    #[cfg(feature = "client-reqwest")]
    #[error(transparent)]
    #[diagnostic(code(http_cache::reqwest_middleware_error))]
    ReqwestMiddlewareError(#[from] reqwest_middleware::Error),
    #[cfg(feature = "manager-cacache")]
    #[error(transparent)]
    #[diagnostic(code(http_cache::cacache_error))]
    CaCacheError(#[from] cacache::Error),
    #[cfg(feature = "manager-cacache")]
    #[error(transparent)]
    #[diagnostic(code(http_cache::bincode_error))]
    BincodeError(#[from] Box<bincode::ErrorKind>),
    #[error("Unknown HTTP version")]
    #[diagnostic(code(http_cache::bad_version))]
    BadVersion,
    #[error("Error parsing header value")]
    #[diagnostic(code(http_cache::bad_header))]
    BadHeader,
    #[error(
        "Request object is not cloneable. Are you passing a streaming body?"
    )]
    #[diagnostic(code(http_cache::bad_request))]
    BadRequest,
}
