use thiserror::Error;

/// Generic error type for the `HttpCache` Surf implementation.
#[derive(Error, Debug)]
pub enum Error {
    /// There was a Surf client error
    #[error("Surf error: {0}")]
    Surf(#[from] anyhow::Error),
}
