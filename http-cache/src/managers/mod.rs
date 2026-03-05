#[cfg(any(feature = "manager-cacache", feature = "manager-cacache-bincode"))]
pub mod cacache;

#[cfg(feature = "manager-foyer")]
pub mod foyer;

#[cfg(any(feature = "manager-moka", feature = "manager-moka-bincode"))]
pub mod moka;

// Streaming cache managers
#[cfg(feature = "streaming")]
pub mod streaming_cache;
