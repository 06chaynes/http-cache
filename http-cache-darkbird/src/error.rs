use thiserror::Error;

/// Generic error type for the `HttpCache` Darkbird implementation.
#[derive(Error, Debug)]
pub enum Error {
    #[error("Darkbird put error: {0}")]
    Put(String),
    #[error("Darkbird delete error: {0}")]
    Delete(String),
}
