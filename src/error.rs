use miette::Diagnostic;
use thiserror::Error;

#[derive(Error, Diagnostic, Debug)]
pub enum CacheError {
    #[error(transparent)]
    #[diagnostic(code(http_cache::io_error))]
    IoError(#[from] std::io::Error),

    #[error("Unknown HTTP version")]
    #[diagnostic(code(http_cache::bad_version))]
    BadVersion,
}
