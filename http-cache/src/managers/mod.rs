#[cfg(feature = "manager-cacache")]
pub mod cacache;

#[cfg(feature = "manager-moka")]
pub mod moka;

// Streaming cache managers
#[cfg(feature = "streaming")]
pub mod streaming_cache;
