//! Streaming cache manager with disk-based storage and TinyLFU eviction.
//!
//! This module provides [`StreamingManager`], a streaming cache implementation that
//! combines [cacache](https://docs.rs/cacache) for disk storage with
//! [moka](https://docs.rs/moka) for metadata tracking and eviction.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │  moka::Cache<String, CacheMetadata>  (in-memory)               │
//! │  - Tracks: key → {content_hash, policy, headers, size}         │
//! │  - TinyLFU eviction (better hit rates than LRU)                │
//! │  - Eviction listener triggers cacache cleanup                  │
//! └─────────────────────────────────────────────────────────────────┘
//!                           │
//!                           ▼
//! ┌─────────────────────────────────────────────────────────────────┐
//! │  cacache (disk)                                                 │
//! │  - Content-addressed storage (automatic deduplication)         │
//! │  - Streaming reads via AsyncRead (64KB chunks)                 │
//! │  - Integrity verification built-in                              │
//! └─────────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Memory Efficiency
//!
//! On cache hit, only ~64KB is held in memory at a time (the streaming buffer),
//! regardless of response size. A 10MB cached response uses ~64KB of memory
//! during streaming, not 10MB.
//!
//! # Important: Write-Path Buffering
//!
//! While cached responses are streamed on **read** (GET), the **write** path (PUT)
//! requires buffering the entire response body in memory. This is necessary to:
//! - Compute the content hash for cacache's content-addressed storage
//! - Enable content deduplication
//!
//! For very large responses, use the `max_body_size` configuration to prevent OOM.
//! Memory usage during PUT is O(response_size), not O(buffer_size).
//!
//! # Example
//!
//! ```rust,ignore
//! use http_cache::StreamingManager;
//! use std::path::PathBuf;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Create a streaming cache with 10,000 entry capacity
//! let manager = StreamingManager::new(PathBuf::from("./cache"), 10_000).await?;
//!
//! // Or use the temp directory variant for testing
//! let test_manager = StreamingManager::with_temp_dir(1000).await?;
//! # Ok(())
//! # }
//! ```

use std::{
    fmt,
    path::{Path, PathBuf},
};

use crate::{
    body::StreamingBody,
    error::{Result, StreamingError, StreamingErrorKind},
    HttpHeaders, StreamingCacheManager, Url,
};
use async_trait::async_trait;
use bytes::Bytes;
use http::{Response, Version};
use http_body::Body;
use http_body_util::{BodyExt, Empty};
use http_cache_semantics::CachePolicy;
use moka::future::Cache;
use rand::Rng;
use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;

/// Default maximum body size for cached responses (100MB).
///
/// Responses larger than this will be rejected during caching to prevent
/// memory exhaustion. Configure with [`StreamingManager::with_max_body_size`].
pub const DEFAULT_MAX_BODY_SIZE: u64 = 100 * 1024 * 1024;

/// Size of the bounded channel for eviction cleanup tasks.
/// If the channel fills up, cleanup will happen via periodic GC instead.
const EVICTION_CHANNEL_SIZE: usize = 100;

/// Metadata stored in moka for each cache entry.
///
/// This is kept small to minimize memory usage - the actual body
/// is stored in cacache and streamed on demand.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CacheMetadata {
    /// HTTP status code
    status: u16,
    /// HTTP version encoded as u8
    version: u8,
    /// HTTP response headers
    headers: HttpHeaders,
    /// Content integrity hash from cacache (for content-addressed lookup)
    integrity: String,
    /// Size of the body in bytes (for cache size tracking)
    body_size: u64,
    /// Cache policy for revalidation decisions
    policy: CachePolicy,
    /// Optional user-provided metadata
    #[serde(default)]
    user_metadata: Option<Vec<u8>>,
}

/// Convert HTTP version to u8 for compact storage.
fn version_to_u8(version: Version) -> u8 {
    match version {
        Version::HTTP_09 => 9,
        Version::HTTP_10 => 10,
        Version::HTTP_11 => 11,
        Version::HTTP_2 => 2,
        Version::HTTP_3 => 3,
        _ => 11, // Default to HTTP/1.1 for unknown versions
    }
}

/// Convert u8 back to HTTP version.
fn version_from_u8(v: u8) -> Version {
    match v {
        9 => Version::HTTP_09,
        10 => Version::HTTP_10,
        11 => Version::HTTP_11,
        2 => Version::HTTP_2,
        3 => Version::HTTP_3,
        _ => Version::HTTP_11,
    }
}

/// Streaming cache manager combining cacache storage with moka eviction.
///
/// This implementation provides:
///
/// - **True streaming**: Cached responses are streamed from disk in 64KB chunks,
///   not loaded fully into memory
/// - **TinyLFU eviction**: Better hit rates than simple LRU by filtering out
///   one-hit wonders
/// - **Content deduplication**: Automatic via cacache's content-addressed storage
/// - **Integrity verification**: Cached data is verified on read
/// - **Body size limits**: Configurable max body size to prevent memory exhaustion
///
/// # Example
///
/// ```rust,ignore
/// use http_cache::StreamingManager;
/// use std::path::PathBuf;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let manager = StreamingManager::new(PathBuf::from("./cache"), 10_000).await?;
/// # Ok(())
/// # }
/// ```
#[cfg_attr(docsrs, doc(cfg(feature = "streaming")))]
#[derive(Clone)]
pub struct StreamingManager {
    /// Cache directory for cacache storage
    cache_dir: PathBuf,
    /// Metadata cache with TinyLFU eviction
    metadata: Cache<String, CacheMetadata>,
    /// Maximum body size for cached responses
    max_body_size: u64,
}

impl fmt::Debug for StreamingManager {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StreamingManager")
            .field("cache_dir", &self.cache_dir)
            .field("entry_count", &self.metadata.entry_count())
            .field("max_body_size", &self.max_body_size)
            .finish()
    }
}

impl StreamingManager {
    /// Creates a new [`StreamingManager`] with disk-backed storage.
    ///
    /// Uses the default maximum body size of 100MB. For custom limits,
    /// use [`StreamingManager::with_max_body_size`].
    ///
    /// # Arguments
    ///
    /// * `cache_dir` - Directory to store cached response bodies
    /// * `capacity` - Maximum number of entries in the cache
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use http_cache::StreamingManager;
    /// use std::path::PathBuf;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let manager = StreamingManager::new(PathBuf::from("./cache"), 10_000).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn new(cache_dir: PathBuf, capacity: u64) -> Result<Self> {
        Self::with_max_body_size(cache_dir, capacity, DEFAULT_MAX_BODY_SIZE)
            .await
    }

    /// Creates a new [`StreamingManager`] with a custom maximum body size.
    ///
    /// # Arguments
    ///
    /// * `cache_dir` - Directory to store cached response bodies
    /// * `capacity` - Maximum number of entries in the cache
    /// * `max_body_size` - Maximum body size in bytes (responses larger than this are rejected)
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use http_cache::StreamingManager;
    /// use std::path::PathBuf;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// // Create cache with 50MB max body size
    /// let manager = StreamingManager::with_max_body_size(
    ///     PathBuf::from("./cache"),
    ///     10_000,
    ///     50 * 1024 * 1024,
    /// ).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn with_max_body_size(
        cache_dir: PathBuf,
        capacity: u64,
        max_body_size: u64,
    ) -> Result<Self> {
        // Ensure cache directory exists
        tokio::fs::create_dir_all(&cache_dir).await.map_err(|e| {
            crate::HttpCacheError::cache(format!(
                "Failed to create cache directory: {e}"
            ))
        })?;

        // Create bounded channel for eviction cleanup (backpressure)
        let (cleanup_tx, cleanup_rx) = mpsc::channel(EVICTION_CHANNEL_SIZE);

        // Spawn cleanup worker
        let cache_dir_worker = cache_dir.clone();
        tokio::spawn(async move {
            Self::cleanup_worker(cleanup_rx, cache_dir_worker).await;
        });

        // Build moka cache with eviction listener to clean up cacache entries
        // The eviction listener owns the sender, keeping the channel open
        let metadata: Cache<String, CacheMetadata> = Cache::builder()
            .max_capacity(capacity)
            .eviction_listener(move |_key, value: CacheMetadata, _cause| {
                // Send cleanup task to bounded channel (non-blocking)
                // If channel is full, cleanup will happen via periodic GC
                if let Err(e) = cleanup_tx.try_send(value.integrity.clone()) {
                    log::debug!(
                        "Eviction cleanup channel full, deferring cleanup: {e}"
                    );
                }
            })
            .build();

        Ok(Self { cache_dir, metadata, max_body_size })
    }

    /// Background worker that processes eviction cleanup tasks.
    ///
    /// Runs in a dedicated task and processes integrity hashes from the
    /// bounded channel, removing the corresponding cache content.
    async fn cleanup_worker(
        mut rx: mpsc::Receiver<String>,
        cache_dir: PathBuf,
    ) {
        while let Some(integrity_str) = rx.recv().await {
            match integrity_str.parse::<cacache::Integrity>() {
                Ok(integrity) => {
                    if let Err(e) =
                        cacache::remove_hash(&cache_dir, &integrity).await
                    {
                        log::warn!(
                            "Failed to remove evicted cache content: {e}"
                        );
                    }
                }
                Err(e) => {
                    log::warn!(
                        "Failed to parse integrity hash for cleanup: {e}"
                    );
                }
            }
        }
    }

    /// Creates a new [`StreamingManager`] using a temporary directory.
    ///
    /// **Note:** Despite the historical name, this still uses disk storage
    /// in a temp directory. Only metadata is kept in memory; response bodies
    /// are stored on disk and streamed.
    ///
    /// Use [`StreamingManager::new`] with a persistent directory for production deployments.
    ///
    /// # Arguments
    ///
    /// * `capacity` - Maximum number of entries in the cache
    #[deprecated(
        since = "1.1.0",
        note = "renamed to with_temp_dir() for clarity"
    )]
    pub async fn in_memory(capacity: u64) -> Result<Self> {
        Self::with_temp_dir(capacity).await
    }

    /// Creates a new [`StreamingManager`] using a temporary directory.
    ///
    /// This is useful for testing or when persistence is not needed.
    /// The cache directory is created in the system's temporary directory
    /// with a unique name including process ID and random component for security.
    ///
    /// **Note:** This still uses disk storage in a temp directory.
    /// Only metadata is kept in memory; response bodies are stored on disk and streamed.
    ///
    /// # Arguments
    ///
    /// * `capacity` - Maximum number of entries in the cache
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use http_cache::StreamingManager;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let manager = StreamingManager::with_temp_dir(1000).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn with_temp_dir(capacity: u64) -> Result<Self> {
        let random_suffix: u32 = rand::rng().random();
        let temp_dir = std::env::temp_dir().join(format!(
            "http-cache-streaming-{}-{:08x}",
            std::process::id(),
            random_suffix
        ));
        Self::new(temp_dir, capacity).await
    }

    /// Returns the cache directory path.
    #[must_use]
    pub fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }

    /// Returns the current number of entries in the cache.
    #[must_use]
    pub fn entry_count(&self) -> u64 {
        self.metadata.entry_count()
    }

    /// Returns the maximum body size for cached responses.
    #[must_use]
    pub fn max_body_size(&self) -> u64 {
        self.max_body_size
    }

    /// Clears all entries from the cache.
    pub async fn clear(&self) -> Result<()> {
        self.metadata.invalidate_all();
        self.metadata.run_pending_tasks().await;

        // Clear cacache directory
        cacache::clear(&self.cache_dir).await.map_err(|e| {
            crate::HttpCacheError::cache(format!("Failed to clear cache: {e}"))
        })?;

        Ok(())
    }

    /// Runs pending maintenance tasks (eviction, etc).
    ///
    /// This is called automatically but can be invoked manually
    /// to force immediate cleanup.
    pub async fn run_pending_tasks(&self) {
        self.metadata.run_pending_tasks().await;
    }
}

#[async_trait]
impl StreamingCacheManager for StreamingManager {
    type Body = StreamingBody<Empty<Bytes>>;

    async fn get(
        &self,
        cache_key: &str,
    ) -> Result<Option<(Response<Self::Body>, CachePolicy)>>
    where
        <Self::Body as Body>::Data: Send,
        <Self::Body as Body>::Error:
            Into<StreamingError> + Send + Sync + 'static,
    {
        // Look up metadata in moka
        let metadata = match self.metadata.get(cache_key).await {
            Some(m) => m,
            None => return Ok(None),
        };

        // Open streaming reader from cacache using content hash
        let reader = match cacache::Reader::open_hash(
            &self.cache_dir,
            metadata.integrity.parse().map_err(|e| {
                crate::HttpCacheError::cache(format!(
                    "Invalid integrity hash: {e}"
                ))
            })?,
        )
        .await
        {
            Ok(r) => r,
            Err(cacache::Error::EntryNotFound(_, _)) => {
                // Content was deleted but metadata exists - remove stale metadata
                self.metadata.invalidate(cache_key).await;
                return Ok(None);
            }
            Err(e) => {
                return Err(Box::new(crate::HttpCacheError::cache(format!(
                    "Failed to open cached content: {e}"
                ))));
            }
        };

        // Build response with streaming body
        let mut response_builder = Response::builder()
            .status(metadata.status)
            .version(version_from_u8(metadata.version));

        for (name, value) in metadata.headers.iter() {
            response_builder =
                response_builder.header(name.as_str(), value.as_str());
        }

        // Create streaming body from cacache reader with size hint
        let body =
            StreamingBody::from_reader_with_size(reader, metadata.body_size);

        let response = response_builder.body(body).map_err(|e| {
            crate::HttpCacheError::cache(format!(
                "Failed to build response: {e}"
            ))
        })?;

        Ok(Some((response, metadata.policy)))
    }

    async fn put<B>(
        &self,
        cache_key: String,
        response: Response<B>,
        policy: CachePolicy,
        _request_url: Url,
        user_metadata: Option<Vec<u8>>,
    ) -> Result<Response<Self::Body>>
    where
        B: Body + Send + 'static,
        B::Data: Send,
        B::Error: Into<StreamingError>,
        <Self::Body as Body>::Data: Send,
        <Self::Body as Body>::Error:
            Into<StreamingError> + Send + Sync + 'static,
    {
        let (parts, body) = response.into_parts();

        // Collect body - this is necessary to compute the content hash
        // and store in cacache. The body is still streamed on GET.
        let body_bytes = body
            .collect()
            .await
            .map_err(|e| StreamingError::new(e.into()))?
            .to_bytes();

        // Check body size limit to prevent caching responses that are too large
        if body_bytes.len() as u64 > self.max_body_size {
            return Err(Box::new(StreamingError::with_kind(
                format!(
                    "Response body size ({} bytes) exceeds maximum size ({} bytes)",
                    body_bytes.len(),
                    self.max_body_size
                ),
                StreamingErrorKind::Other,
            )));
        }

        let body_size = body_bytes.len() as u64;

        // Write to cacache and get integrity hash (content-addressed)
        let mut writer = cacache::WriteOpts::new()
            .open_hash(&self.cache_dir)
            .await
            .map_err(|e| {
                crate::HttpCacheError::cache(format!(
                    "Failed to open cache writer: {e}"
                ))
            })?;

        writer.write_all(&body_bytes).await.map_err(|e| {
            crate::HttpCacheError::cache(format!(
                "Failed to write to cache: {e}"
            ))
        })?;

        let integrity = writer.commit().await.map_err(|e| {
            crate::HttpCacheError::cache(format!(
                "Failed to commit cache entry: {e}"
            ))
        })?;

        // Convert headers to HttpHeaders
        let mut headers = HttpHeaders::new();
        for (name, value) in parts.headers.iter() {
            if let Ok(value_str) = value.to_str() {
                headers
                    .append(name.as_str().to_string(), value_str.to_string());
            }
        }

        // Create metadata entry
        let metadata = CacheMetadata {
            status: parts.status.as_u16(),
            version: version_to_u8(parts.version),
            headers,
            integrity: integrity.to_string(),
            body_size,
            policy,
            user_metadata,
        };

        // Store metadata in moka
        self.metadata.insert(cache_key, metadata).await;

        // Build return response with buffered body (already in memory)
        let mut response_builder =
            Response::builder().status(parts.status).version(parts.version);

        for (name, value) in parts.headers.iter() {
            response_builder = response_builder.header(name, value);
        }

        let return_body = StreamingBody::buffered(body_bytes);
        let return_response =
            response_builder.body(return_body).map_err(|e| {
                crate::HttpCacheError::cache(format!(
                    "Failed to build response: {e}"
                ))
            })?;

        Ok(return_response)
    }

    async fn convert_body<B>(
        &self,
        response: Response<B>,
    ) -> Result<Response<Self::Body>>
    where
        B: Body + Send + 'static,
        B::Data: Send,
        B::Error: Into<StreamingError>,
        <Self::Body as Body>::Data: Send,
        <Self::Body as Body>::Error:
            Into<StreamingError> + Send + Sync + 'static,
    {
        let (parts, body) = response.into_parts();

        // Collect body into bytes
        let body_bytes = body
            .collect()
            .await
            .map_err(|e| StreamingError::new(e.into()))?
            .to_bytes();

        // Build response with buffered body
        let mut response_builder =
            Response::builder().status(parts.status).version(parts.version);

        for (name, value) in parts.headers.iter() {
            response_builder = response_builder.header(name, value);
        }

        let streaming_body = StreamingBody::buffered(body_bytes);
        let response = response_builder.body(streaming_body).map_err(|e| {
            crate::HttpCacheError::cache(format!(
                "Failed to build response: {e}"
            ))
        })?;

        Ok(response)
    }

    async fn delete(&self, cache_key: &str) -> Result<()> {
        // Get metadata to find content hash
        if let Some(metadata) = self.metadata.get(cache_key).await {
            // Remove from metadata cache
            self.metadata.invalidate(cache_key).await;

            // Remove content from cacache
            match metadata.integrity.parse::<cacache::Integrity>() {
                Ok(integrity) => {
                    if let Err(e) =
                        cacache::remove_hash(&self.cache_dir, &integrity).await
                    {
                        log::warn!(
                            "Failed to remove cache content for key {cache_key}: {e}"
                        );
                    }
                }
                Err(e) => {
                    log::warn!(
                        "Invalid integrity hash in metadata for key {cache_key}: {e}"
                    );
                }
            }
        }

        Ok(())
    }

    fn empty_body(&self) -> Self::Body {
        StreamingBody::buffered(Bytes::new())
    }

    fn body_to_bytes_stream(
        body: Self::Body,
    ) -> impl futures_util::Stream<
        Item = std::result::Result<
            Bytes,
            Box<dyn std::error::Error + Send + Sync>,
        >,
    > + Send
    where
        <Self::Body as Body>::Data: Send,
        <Self::Body as Body>::Error: Send + Sync + 'static,
    {
        body.into_bytes_stream()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use http::StatusCode;
    use http_body_util::Full;

    #[tokio::test]
    async fn test_streaming_manager_basic() {
        let manager = StreamingManager::with_temp_dir(100)
            .await
            .expect("Failed to create manager");

        // Create a test response
        let response = Response::builder()
            .status(StatusCode::OK)
            .header("content-type", "text/plain")
            .body(Full::new(Bytes::from("Hello, World!")))
            .unwrap();

        let policy = CachePolicy::new(
            &http::Request::builder()
                .uri("https://example.com/test")
                .body(())
                .unwrap(),
            &Response::builder()
                .status(200)
                .header("cache-control", "max-age=3600")
                .body(())
                .unwrap(),
        );

        // Put into cache
        let cache_key = "test-key".to_string();
        let url: Url = "https://example.com/test".parse().unwrap();

        let _stored = manager
            .put(cache_key.clone(), response, policy.clone(), url, None)
            .await
            .expect("Failed to put");

        // Get from cache
        let result = manager.get(&cache_key).await.expect("Failed to get");
        assert!(result.is_some());

        let (response, _policy) = result.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // Read body
        let body_bytes = response
            .into_body()
            .collect()
            .await
            .expect("Failed to collect body")
            .to_bytes();
        assert_eq!(body_bytes, "Hello, World!");
    }

    #[tokio::test]
    async fn test_streaming_manager_delete() {
        let manager = StreamingManager::with_temp_dir(100)
            .await
            .expect("Failed to create manager");

        let response = Response::builder()
            .status(StatusCode::OK)
            .body(Full::new(Bytes::from("test")))
            .unwrap();

        let policy = CachePolicy::new(
            &http::Request::builder()
                .uri("https://example.com/test")
                .body(())
                .unwrap(),
            &Response::builder()
                .status(200)
                .header("cache-control", "max-age=3600")
                .body(())
                .unwrap(),
        );

        let cache_key = "delete-test".to_string();
        let url: Url = "https://example.com/test".parse().unwrap();

        manager
            .put(cache_key.clone(), response, policy, url, None)
            .await
            .expect("Failed to put");

        // Verify it exists
        assert!(manager.get(&cache_key).await.unwrap().is_some());

        // Delete it
        manager.delete(&cache_key).await.expect("Failed to delete");

        // Verify it's gone
        assert!(manager.get(&cache_key).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_streaming_manager_content_dedup() {
        let manager = StreamingManager::with_temp_dir(100)
            .await
            .expect("Failed to create manager");

        let body_content = Bytes::from("Duplicate content");

        let policy = CachePolicy::new(
            &http::Request::builder()
                .uri("https://example.com/test")
                .body(())
                .unwrap(),
            &Response::builder()
                .status(200)
                .header("cache-control", "max-age=3600")
                .body(())
                .unwrap(),
        );

        // Store same content under two different keys
        for key in ["key1", "key2"] {
            let response = Response::builder()
                .status(StatusCode::OK)
                .body(Full::new(body_content.clone()))
                .unwrap();

            let url: Url = "https://example.com/test".parse().unwrap();
            manager
                .put(key.to_string(), response, policy.clone(), url, None)
                .await
                .expect("Failed to put");
        }

        // Both should return the same content
        for key in ["key1", "key2"] {
            let result = manager.get(key).await.expect("Failed to get");
            assert!(result.is_some());

            let (response, _) = result.unwrap();
            let body = response.into_body().collect().await.unwrap().to_bytes();
            assert_eq!(body, "Duplicate content");
        }
    }
}
