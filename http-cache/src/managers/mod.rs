#[cfg(any(feature = "manager-cacache", feature = "manager-cacache-bincode"))]
pub mod cacache;

#[cfg(feature = "manager-foyer")]
pub mod foyer;

#[cfg(feature = "manager-moka")]
pub mod moka;

// Streaming cache managers
#[cfg(feature = "streaming")]
pub mod streaming_cache;
