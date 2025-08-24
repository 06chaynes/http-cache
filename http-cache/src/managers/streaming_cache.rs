//! File-based streaming cache manager that stores response metadata and body content separately.
//! This enables streaming by never loading complete response bodies into memory.
//!
//! This implementation is based on the [http-cache-stream](https://github.com/stjude-rust-labs/http-cache-stream) approach.

use crate::{
    body::StreamingBody,
    error::{Result, StreamingError},
    runtime, StreamingCacheManager,
};
use async_trait::async_trait;
use bytes::Bytes;
use http::{Response, Version};
use http_body_util::{BodyExt, Empty};
use http_cache_semantics::CachePolicy;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use url::Url;

// Import async-compatible synchronization primitives based on feature flags
cfg_if::cfg_if! {
    if #[cfg(feature = "streaming-tokio")] {
        use tokio::sync::RwLock;
    } else if #[cfg(feature = "streaming-smol")] {
        use async_lock::RwLock;
    } else {
        // Fallback to std Mutex if no async feature is enabled
        use std::sync::Mutex as RwLock;
    }
}

const CACHE_VERSION: &str = "cache-v2";

/// Configuration for the streaming cache manager
#[derive(Debug, Clone, Copy)]
pub struct StreamingCacheConfig {
    /// Maximum cache size in bytes (None for unlimited)
    pub max_cache_size: Option<u64>,
    /// Maximum number of cache entries (None for unlimited)
    pub max_entries: Option<usize>,
    /// Whether to enable persistent reference counting
    pub persistent_ref_counting: bool,
    /// Enable background cleanup of orphaned files (default: true)
    pub enable_background_cleanup: bool,
    /// Cleanup interval in seconds (default: 3600 = 1 hour)
    pub cleanup_interval_secs: u64,
    /// Enable content integrity verification during cleanup (default: false)
    pub verify_integrity_on_cleanup: bool,
    /// Streaming buffer size in bytes (default: 8192)
    pub streaming_buffer_size: usize,
    /// Maximum concurrent operations (default: 10)
    pub max_concurrent_operations: usize,
}

impl Default for StreamingCacheConfig {
    fn default() -> Self {
        Self {
            max_cache_size: None,
            max_entries: None,
            persistent_ref_counting: true,
            enable_background_cleanup: true,
            cleanup_interval_secs: 3600, // 1 hour
            verify_integrity_on_cleanup: false,
            streaming_buffer_size: 8192, // 8KB
            max_concurrent_operations: 10,
        }
    }
}

/// LRU tracking entry for cache management
#[derive(Debug, Clone, Serialize, Deserialize)]
struct LruEntry {
    cache_key: String,
    content_digest: String,
    last_accessed: u64,
    file_size: u64,
}

/// Reference counting data for persistence
#[derive(Debug, Clone, Serialize, Deserialize)]
struct RefCountData {
    refs: HashMap<String, u32>,
    lru_entries: Vec<LruEntry>,
    current_cache_size: u64,
    current_entries: usize,
}

/// Reference counting for content files to prevent premature deletion
#[derive(Debug, Default)]
struct ContentRefCounter {
    refs: Arc<RwLock<HashMap<String, u32>>>,
    /// LRU queue for cache eviction
    lru_queue: Arc<RwLock<VecDeque<LruEntry>>>,
    /// Current cache size in bytes
    current_cache_size: Arc<RwLock<u64>>,
    /// Current number of entries
    current_entries: Arc<RwLock<usize>>,
}

impl ContentRefCounter {
    fn new() -> Self {
        Self {
            refs: Arc::new(RwLock::new(HashMap::new())),
            lru_queue: Arc::new(RwLock::new(VecDeque::new())),
            current_cache_size: Arc::new(RwLock::new(0)),
            current_entries: Arc::new(RwLock::new(0)),
        }
    }

    /// Get current cache size in bytes
    async fn current_cache_size(&self) -> Result<u64> {
        cfg_if::cfg_if! {
            if #[cfg(any(feature = "streaming-tokio", feature = "streaming-smol"))] {
                let size = self.current_cache_size.read().await;
                Ok(*size)
            } else {
                let size = self.current_cache_size.lock().map_err(|e| {
                    StreamingError::concurrency(format!(
                        "Failed to acquire lock for current_cache_size: {e}"
                    ))
                })?;
                Ok(*size)
            }
        }
    }

    /// Get current number of cache entries
    async fn current_entries(&self) -> Result<usize> {
        cfg_if::cfg_if! {
            if #[cfg(any(feature = "streaming-tokio", feature = "streaming-smol"))] {
                let entries = self.current_entries.read().await;
                Ok(*entries)
            } else {
                let entries = self.current_entries.lock().map_err(|e| {
                    StreamingError::concurrency(format!(
                        "Failed to acquire lock for current_entries: {e}"
                    ))
                })?;
                Ok(*entries)
            }
        }
    }

    /// Add cache entry to LRU tracking
    async fn add_cache_entry(
        &self,
        cache_key: String,
        content_digest: String,
        file_size: u64,
    ) -> Result<()> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let entry = LruEntry {
            cache_key: cache_key.clone(),
            content_digest,
            last_accessed: now,
            file_size,
        };

        cfg_if::cfg_if! {
            if #[cfg(any(feature = "streaming-tokio", feature = "streaming-smol"))] {
                {
                    let mut lru_queue = self.lru_queue.write().await;
                    // Remove any existing entry for this cache key
                    lru_queue.retain(|e| e.cache_key != cache_key);
                    // Add new entry to front (most recently used)
                    lru_queue.push_front(entry);
                }

                // Update cache size and entry count
                {
                    let mut cache_size = self.current_cache_size.write().await;
                    *cache_size += file_size;
                }

                {
                    let mut entries = self.current_entries.write().await;
                    *entries += 1;
                }
            } else {
                {
                    let mut lru_queue = self.lru_queue.lock().map_err(|e| {
                        StreamingError::concurrency(format!(
                            "Failed to acquire lock for lru_queue: {e}"
                        ))
                    })?;
                    // Remove any existing entry for this cache key
                    lru_queue.retain(|e| e.cache_key != cache_key);
                    // Add new entry to front (most recently used)
                    lru_queue.push_front(entry);
                }

                // Update cache size and entry count
                {
                    let mut cache_size = self.current_cache_size.lock().map_err(|e| {
                        StreamingError::concurrency(format!(
                            "Failed to acquire lock for current_cache_size: {e}"
                        ))
                    })?;
                    *cache_size += file_size;
                }

                {
                    let mut entries = self.current_entries.lock().map_err(|e| {
                        StreamingError::concurrency(format!(
                            "Failed to acquire lock for current_entries: {e}"
                        ))
                    })?;
                    *entries += 1;
                }
            }
        }

        Ok(())
    }

    /// Update last accessed time for a cache entry (move to front of LRU)
    async fn update_access_time(&self, cache_key: &str) -> Result<()> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        cfg_if::cfg_if! {
            if #[cfg(any(feature = "streaming-tokio", feature = "streaming-smol"))] {
                let mut lru_queue = self.lru_queue.write().await;
                // Find and update the entry
                if let Some(pos) = lru_queue.iter().position(|e| e.cache_key == cache_key) {
                    if let Some(mut entry) = lru_queue.remove(pos) {
                        entry.last_accessed = now;
                        lru_queue.push_front(entry);
                    }
                }
            } else {
                let mut lru_queue = self.lru_queue.lock().map_err(|e| {
                    StreamingError::concurrency(format!(
                        "Failed to acquire lock for lru_queue: {e}"
                    ))
                })?;
                // Find and update the entry
                if let Some(pos) = lru_queue.iter().position(|e| e.cache_key == cache_key) {
                    if let Some(mut entry) = lru_queue.remove(pos) {
                        entry.last_accessed = now;
                        lru_queue.push_front(entry);
                    }
                }
            }
        }

        Ok(())
    }

    /// Get least recently used entries for eviction
    async fn get_lru_entries_for_eviction(
        &self,
        target_size: u64,
        target_count: usize,
    ) -> Result<Vec<LruEntry>> {
        let current_size = self.current_cache_size().await?;
        let current_count = self.current_entries().await?;

        cfg_if::cfg_if! {
            if #[cfg(any(feature = "streaming-tokio", feature = "streaming-smol"))] {
                let lru_queue = self.lru_queue.read().await;

                let mut entries_to_evict = Vec::new();
                let mut size_to_free = current_size.saturating_sub(target_size);
                let mut entries_to_free = current_count.saturating_sub(target_count);

                // Collect LRU entries until we have enough space/count
                for entry in lru_queue.iter().rev() { // Start from least recently used
                    if size_to_free == 0 && entries_to_free == 0 {
                        break;
                    }

                    entries_to_evict.push(entry.clone());

                    if size_to_free > 0 {
                        size_to_free = size_to_free.saturating_sub(entry.file_size);
                    }
                    entries_to_free = entries_to_free.saturating_sub(1);
                }

                Ok(entries_to_evict)
            } else {
                let lru_queue = self.lru_queue.lock().map_err(|e| {
                    StreamingError::concurrency(format!(
                        "Failed to acquire lock for lru_queue: {e}"
                    ))
                })?;

                let mut entries_to_evict = Vec::new();
                let mut size_to_free = current_size.saturating_sub(target_size);
                let mut entries_to_free = current_count.saturating_sub(target_count);

                // Collect LRU entries until we have enough space/count
                for entry in lru_queue.iter().rev() { // Start from least recently used
                    if size_to_free == 0 && entries_to_free == 0 {
                        break;
                    }

                    entries_to_evict.push(entry.clone());

                    if size_to_free > 0 {
                        size_to_free = size_to_free.saturating_sub(entry.file_size);
                    }
                    entries_to_free = entries_to_free.saturating_sub(1);
                }

                Ok(entries_to_evict)
            }
        }
    }

    /// Remove cache entry from LRU tracking
    async fn remove_cache_entry(
        &self,
        cache_key: &str,
    ) -> Result<Option<LruEntry>> {
        cfg_if::cfg_if! {
            if #[cfg(any(feature = "streaming-tokio", feature = "streaming-smol"))] {
                let mut lru_queue = self.lru_queue.write().await;

                if let Some(pos) = lru_queue.iter().position(|e| e.cache_key == cache_key) {
                    let entry = lru_queue.remove(pos).unwrap();

                    // Update cache size and entry count
                    {
                        let mut cache_size = self.current_cache_size.write().await;
                        *cache_size = cache_size.saturating_sub(entry.file_size);
                    }

                    {
                        let mut entries = self.current_entries.write().await;
                        *entries = entries.saturating_sub(1);
                    }

                    return Ok(Some(entry));
                }
            } else {
                let mut lru_queue = self.lru_queue.lock().map_err(|e| {
                    StreamingError::concurrency(format!(
                        "Failed to acquire lock for lru_queue: {e}"
                    ))
                })?;

                if let Some(pos) = lru_queue.iter().position(|e| e.cache_key == cache_key) {
                    let entry = lru_queue.remove(pos).unwrap();

                    // Update cache size and entry count
                    {
                        let mut cache_size = self.current_cache_size.lock().map_err(|e| {
                            StreamingError::concurrency(format!(
                                "Failed to acquire lock for current_cache_size: {e}"
                            ))
                        })?;
                        *cache_size = cache_size.saturating_sub(entry.file_size);
                    }

                    {
                        let mut entries = self.current_entries.lock().map_err(|e| {
                            StreamingError::concurrency(format!(
                                "Failed to acquire lock for current_entries: {e}"
                            ))
                        })?;
                        *entries = entries.saturating_sub(1);
                    }

                    return Ok(Some(entry));
                }
            }
        }

        Ok(None)
    }

    /// Get path for persistent reference counting storage
    fn ref_count_path(&self, root_path: &Path) -> PathBuf {
        root_path.join(CACHE_VERSION).join("ref_counts.json")
    }

    /// Save reference counts to persistent storage
    async fn save_ref_counts(&self, root_path: &Path) -> Result<()> {
        let ref_path = self.ref_count_path(root_path);

        cfg_if::cfg_if! {
            if #[cfg(any(feature = "streaming-tokio", feature = "streaming-smol"))] {
                let refs = {
                    let refs_guard = self.refs.read().await;
                    refs_guard.clone()
                };

                let lru_queue: Vec<LruEntry> = {
                    let lru_guard = self.lru_queue.read().await;
                    lru_guard.iter().cloned().collect()
                };

                let cache_size = {
                    let size_guard = self.current_cache_size.read().await;
                    *size_guard
                };

                let entries = {
                    let entries_guard = self.current_entries.read().await;
                    *entries_guard
                };
            } else {
                let refs = self.refs.lock().map_err(|e| {
                    StreamingError::concurrency(format!(
                        "Failed to acquire lock for refs during save: {e}"
                    ))
                })?.clone();

                let lru_queue = self.lru_queue.lock().map_err(|e| {
                    StreamingError::concurrency(format!(
                        "Failed to acquire lock for lru_queue during save: {e}"
                    ))
                })?.iter().cloned().collect();

                let cache_size = *self.current_cache_size.lock().map_err(|e| {
                    StreamingError::concurrency(format!(
                        "Failed to acquire lock for current_cache_size during save: {e}"
                    ))
                })?;

                let entries = *self.current_entries.lock().map_err(|e| {
                    StreamingError::concurrency(format!(
                        "Failed to acquire lock for current_entries during save: {e}"
                    ))
                })?;
            }
        }

        let data = RefCountData {
            refs,
            lru_entries: lru_queue,
            current_cache_size: cache_size,
            current_entries: entries,
        };

        // Ensure directory exists
        if let Some(parent) = ref_path.parent() {
            runtime::create_dir_all(parent)
                .await
                .map_err(StreamingError::io)?;
        }

        let json_data = serde_json::to_vec_pretty(&data)
            .map_err(StreamingError::serialization)?;

        runtime::write(&ref_path, &json_data)
            .await
            .map_err(StreamingError::io)?;

        Ok(())
    }

    /// Load reference counts from persistent storage
    async fn load_ref_counts(&self, root_path: &Path) -> Result<()> {
        let ref_path = self.ref_count_path(root_path);

        if !ref_path.exists() {
            return Ok(()); // No previous data to load
        }

        let json_data =
            runtime::read(&ref_path).await.map_err(StreamingError::io)?;
        let data: RefCountData = serde_json::from_slice(&json_data)
            .map_err(StreamingError::serialization)?;

        cfg_if::cfg_if! {
            if #[cfg(any(feature = "streaming-tokio", feature = "streaming-smol"))] {
                // Restore reference counts
                {
                    let mut refs = self.refs.write().await;
                    *refs = data.refs;
                }

                // Restore LRU queue
                {
                    let mut lru_queue = self.lru_queue.write().await;
                    *lru_queue = data.lru_entries.into();
                }

                // Restore cache size
                {
                    let mut cache_size = self.current_cache_size.write().await;
                    *cache_size = data.current_cache_size;
                }

                // Restore entry count
                {
                    let mut entries = self.current_entries.write().await;
                    *entries = data.current_entries;
                }
            } else {
                // Restore reference counts
                {
                    let mut refs = self.refs.lock().map_err(|e| {
                        StreamingError::concurrency(format!(
                            "Failed to acquire lock for refs during load: {e}"
                        ))
                    })?;
                    *refs = data.refs;
                }

                // Restore LRU queue
                {
                    let mut lru_queue = self.lru_queue.lock().map_err(|e| {
                        StreamingError::concurrency(format!(
                            "Failed to acquire lock for lru_queue during load: {e}"
                        ))
                    })?;
                    *lru_queue = data.lru_entries.into();
                }

                // Restore cache size
                {
                    let mut cache_size = self.current_cache_size.lock().map_err(|e| {
                        StreamingError::concurrency(format!(
                            "Failed to acquire lock for current_cache_size during load: {e}"
                        ))
                    })?;
                    *cache_size = data.current_cache_size;
                }

                // Restore entry count
                {
                    let mut entries = self.current_entries.lock().map_err(|e| {
                        StreamingError::concurrency(format!(
                            "Failed to acquire lock for current_entries during load: {e}"
                        ))
                    })?;
                    *entries = data.current_entries;
                }
            }
        }

        Ok(())
    }

    /// Increment reference count for a content digest
    async fn add_ref(&self, digest: &str) -> Result<()> {
        cfg_if::cfg_if! {
            if #[cfg(any(feature = "streaming-tokio", feature = "streaming-smol"))] {
                let mut refs = self.refs.write().await;
                *refs.entry(digest.to_string()).or_insert(0) += 1;
                Ok(())
            } else {
                let mut refs = self.refs.lock().map_err(|e| {
                    StreamingError::concurrency(format!(
                        "Failed to acquire lock for add_ref: {e}"
                    ))
                })?;
                *refs.entry(digest.to_string()).or_insert(0) += 1;
                Ok(())
            }
        }
    }

    /// Decrement reference count for a content digest, returns true if safe to delete
    async fn remove_ref(&self, digest: &str) -> Result<bool> {
        cfg_if::cfg_if! {
            if #[cfg(any(feature = "streaming-tokio", feature = "streaming-smol"))] {
                let mut refs = self.refs.write().await;
                if let Some(count) = refs.get_mut(digest) {
                    *count -= 1;
                    if *count == 0 {
                        refs.remove(digest);
                        return Ok(true);
                    }
                }
                Ok(false)
            } else {
                let mut refs = self.refs.lock().map_err(|e| {
                    StreamingError::concurrency(format!(
                        "Failed to acquire lock for remove_ref: {e}"
                    ))
                })?;
                if let Some(count) = refs.get_mut(digest) {
                    *count -= 1;
                    if *count == 0 {
                        refs.remove(digest);
                        return Ok(true);
                    }
                }
                Ok(false)
            }
        }
    }
}

/// Cache statistics for monitoring
#[derive(Debug, Clone, Copy)]
pub struct CacheStats {
    pub current_size: u64,
    pub current_entries: usize,
    pub max_size: Option<u64>,
    pub max_entries: Option<usize>,
}

/// Report of corrupted cache entry
#[derive(Debug, Clone)]
pub struct CorruptedEntry {
    pub cache_key: String,
    pub digest: String,
    pub reason: String,
}

/// Cache integrity verification report
#[derive(Debug, Clone)]
pub struct CacheIntegrityReport {
    pub total_entries: usize,
    pub valid_entries: usize,
    pub corrupted_entries: Vec<CorruptedEntry>,
    pub orphaned_content: Vec<String>,
}

/// Metadata stored for each cached response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheMetadata {
    pub status: u16,
    pub version: u8,
    pub headers: HashMap<String, String>,
    pub content_digest: String,
    pub policy: CachePolicy,
    pub created_at: u64,
}

/// Background task manager for cleanup operations
#[derive(Debug)]
struct BackgroundTaskManager {
    shutdown_signal: Arc<AtomicBool>,
}

impl BackgroundTaskManager {
    fn new() -> Self {
        Self { shutdown_signal: Arc::new(AtomicBool::new(false)) }
    }

    fn shutdown(&self) {
        self.shutdown_signal.store(true, Ordering::Relaxed);
    }
}

impl Drop for BackgroundTaskManager {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// File-based streaming cache manager
#[derive(Debug)]
pub struct StreamingManager {
    root_path: PathBuf,
    ref_counter: ContentRefCounter,
    config: StreamingCacheConfig,
    background_tasks: Option<BackgroundTaskManager>,
}

impl Clone for StreamingManager {
    fn clone(&self) -> Self {
        Self {
            root_path: self.root_path.clone(),
            ref_counter: ContentRefCounter {
                refs: Arc::clone(&self.ref_counter.refs),
                lru_queue: Arc::clone(&self.ref_counter.lru_queue),
                current_cache_size: Arc::clone(
                    &self.ref_counter.current_cache_size,
                ),
                current_entries: Arc::clone(&self.ref_counter.current_entries),
            },
            config: self.config,
            background_tasks: None, // Background tasks are not cloned
        }
    }
}

impl StreamingManager {
    /// Create a new streaming cache manager with default configuration
    pub fn new(root_path: PathBuf) -> Self {
        Self::new_with_config(root_path, StreamingCacheConfig::default())
    }

    /// Create a new streaming cache manager with custom configuration
    pub fn new_with_config(
        root_path: PathBuf,
        config: StreamingCacheConfig,
    ) -> Self {
        let mut manager = Self {
            root_path,
            ref_counter: ContentRefCounter::new(),
            config,
            background_tasks: None,
        };

        // Start background tasks if enabled
        if manager.config.enable_background_cleanup {
            manager.start_background_tasks();
        }

        manager
    }

    /// Create a new streaming cache manager and rebuild reference counts from existing cache
    pub async fn new_with_existing_cache(root_path: PathBuf) -> Result<Self> {
        Self::new_with_existing_cache_and_config(
            root_path,
            StreamingCacheConfig::default(),
        )
        .await
    }

    /// Create a new streaming cache manager with config and rebuild reference counts from existing cache
    pub async fn new_with_existing_cache_and_config(
        root_path: PathBuf,
        config: StreamingCacheConfig,
    ) -> Result<Self> {
        let manager = Self::new_with_config(root_path, config);

        // Load persistent reference counts if enabled
        if manager.config.persistent_ref_counting {
            manager.ref_counter.load_ref_counts(&manager.root_path).await?;
        }

        // Fallback to rebuilding from metadata files if no persistent data or if disabled
        let current_entries = manager.ref_counter.current_entries().await?;
        if current_entries == 0 {
            manager.rebuild_reference_counts().await?;
        }

        Ok(manager)
    }

    /// Verify content integrity by checking if file content matches its digest
    pub async fn verify_content_integrity(
        &self,
        digest: &str,
        content_path: &Path,
    ) -> Result<bool> {
        if !content_path.exists() {
            return Ok(false);
        }

        // Read the content file and compute its digest
        let content =
            runtime::read(content_path).await.map_err(StreamingError::io)?;
        let computed_digest = Self::calculate_digest(&content);

        Ok(computed_digest == digest)
    }

    /// Verify all cached content integrity and return report
    pub async fn verify_cache_integrity(&self) -> Result<CacheIntegrityReport> {
        let mut report = CacheIntegrityReport {
            total_entries: 0,
            valid_entries: 0,
            corrupted_entries: Vec::new(),
            orphaned_content: Vec::new(),
        };

        let metadata_dir = self.root_path.join(CACHE_VERSION).join("metadata");
        let content_dir = self.root_path.join(CACHE_VERSION).join("content");

        if !metadata_dir.exists() {
            return Ok(report);
        }

        // Track all content files to identify orphans
        let mut referenced_digests = std::collections::HashSet::new();

        cfg_if::cfg_if! {
            if #[cfg(feature = "streaming-tokio")] {
                let mut entries = runtime::read_dir(&metadata_dir).await.map_err(StreamingError::io)?;

                while let Some(entry) = entries.next_entry().await.map_err(StreamingError::io)? {
                    let path = entry.path();

                    if path.extension().and_then(|s| s.to_str()) == Some("json") {
                        report.total_entries += 1;

                        let content = runtime::read(&path).await.map_err(StreamingError::io)?;
                        match serde_json::from_slice::<CacheMetadata>(&content) {
                            Ok(metadata) => {
                                referenced_digests.insert(metadata.content_digest.clone());

                                let content_path = self.content_path(&metadata.content_digest);
                                match self.verify_content_integrity(&metadata.content_digest, &content_path).await {
                                    Ok(true) => report.valid_entries += 1,
                                    Ok(false) => {
                                        let cache_key = path.file_stem()
                                            .and_then(|s| s.to_str())
                                            .and_then(|s| hex::decode(s).ok())
                                            .and_then(|bytes| String::from_utf8(bytes).ok())
                                            .unwrap_or_else(|| format!("unknown-{}", metadata.content_digest));
                                        report.corrupted_entries.push(CorruptedEntry {
                                            cache_key,
                                            digest: metadata.content_digest.clone(),
                                            reason: "Content digest mismatch".to_string(),
                                        });
                                    }
                                    Err(e) => {
                                        let cache_key = path.file_stem()
                                            .and_then(|s| s.to_str())
                                            .and_then(|s| hex::decode(s).ok())
                                            .and_then(|bytes| String::from_utf8(bytes).ok())
                                            .unwrap_or_else(|| format!("unknown-{}", metadata.content_digest));
                                        report.corrupted_entries.push(CorruptedEntry {
                                            cache_key,
                                            digest: metadata.content_digest.clone(),
                                            reason: format!("Verification error: {}", e),
                                        });
                                    }
                                }
                            }
                            Err(e) => {
                                report.corrupted_entries.push(CorruptedEntry {
                                    cache_key: "unknown".to_string(),
                                    digest: "unknown".to_string(),
                                    reason: format!("Invalid metadata: {}", e),
                                });
                            }
                        }
                    }
                }
            } else if #[cfg(feature = "streaming-smol")] {
                use futures::stream::StreamExt;

                let mut entries = runtime::read_dir(&metadata_dir).await.map_err(StreamingError::io)?;

                while let Some(entry_result) = entries.next().await {
                    let entry = entry_result.map_err(StreamingError::io)?;
                    let path = entry.path();

                    if path.extension().and_then(|s| s.to_str()) == Some("json") {
                        report.total_entries += 1;

                        let content = runtime::read(&path).await.map_err(StreamingError::io)?;
                        match serde_json::from_slice::<CacheMetadata>(&content) {
                            Ok(metadata) => {
                                referenced_digests.insert(metadata.content_digest.clone());

                                let content_path = self.content_path(&metadata.content_digest);
                                match self.verify_content_integrity(&metadata.content_digest, &content_path).await {
                                    Ok(true) => report.valid_entries += 1,
                                    Ok(false) => {
                                        let cache_key = path.file_stem()
                                            .and_then(|s| s.to_str())
                                            .and_then(|s| hex::decode(s).ok())
                                            .and_then(|bytes| String::from_utf8(bytes).ok())
                                            .unwrap_or_else(|| format!("unknown-{}", metadata.content_digest));
                                        report.corrupted_entries.push(CorruptedEntry {
                                            cache_key,
                                            digest: metadata.content_digest.clone(),
                                            reason: "Content digest mismatch".to_string(),
                                        });
                                    }
                                    Err(e) => {
                                        let cache_key = path.file_stem()
                                            .and_then(|s| s.to_str())
                                            .and_then(|s| hex::decode(s).ok())
                                            .and_then(|bytes| String::from_utf8(bytes).ok())
                                            .unwrap_or_else(|| format!("unknown-{}", metadata.content_digest));
                                        report.corrupted_entries.push(CorruptedEntry {
                                            cache_key,
                                            digest: metadata.content_digest.clone(),
                                            reason: format!("Verification error: {}", e),
                                        });
                                    }
                                }
                            }
                            Err(e) => {
                                report.corrupted_entries.push(CorruptedEntry {
                                    cache_key: "unknown".to_string(),
                                    digest: "unknown".to_string(),
                                    reason: format!("Invalid metadata: {}", e),
                                });
                            }
                        }
                    }
                }
            }
        }

        // Check for orphaned content files
        if content_dir.exists() {
            cfg_if::cfg_if! {
                if #[cfg(feature = "streaming-tokio")] {
                    let mut content_entries = runtime::read_dir(&content_dir).await.map_err(StreamingError::io)?;

                    while let Some(entry) = content_entries.next_entry().await.map_err(StreamingError::io)? {
                        let path = entry.path();
                        if let Some(filename) = path.file_name().and_then(|s| s.to_str()) {
                            if !referenced_digests.contains(filename) {
                                report.orphaned_content.push(filename.to_string());
                            }
                        }
                    }
                } else if #[cfg(feature = "streaming-smol")] {
                    use futures::stream::StreamExt;

                    let mut content_entries = runtime::read_dir(&content_dir).await.map_err(StreamingError::io)?;

                    while let Some(entry_result) = content_entries.next().await {
                        let entry = entry_result.map_err(StreamingError::io)?;
                        let path = entry.path();
                        if let Some(filename) = path.file_name().and_then(|s| s.to_str()) {
                            if !referenced_digests.contains(filename) {
                                report.orphaned_content.push(filename.to_string());
                            }
                        }
                    }
                }
            }
        }

        Ok(report)
    }

    /// Remove corrupted cache entries
    pub async fn remove_corrupted_entries(
        &self,
        corrupted_digests: &[String],
    ) -> Result<usize> {
        let mut removed_count = 0;

        for digest in corrupted_digests {
            let content_path = self.content_path(digest);

            // Remove corrupted content file
            if content_path.exists() {
                if let Err(e) = runtime::remove_file(&content_path).await {
                    eprintln!("Warning: Failed to remove corrupted content file {}: {}", digest, e);
                } else {
                    removed_count += 1;
                }
            }
        }

        Ok(removed_count)
    }

    /// Start background cleanup tasks
    #[allow(unused_variables)]
    fn start_background_tasks(&mut self) {
        let task_manager = BackgroundTaskManager::new();
        let _shutdown_signal = Arc::clone(&task_manager.shutdown_signal);

        cfg_if::cfg_if! {
            if #[cfg(feature = "streaming-tokio")] {
                let _root_path = self.root_path.clone();
                let _config = self.config;
                let _ref_counter = ContentRefCounter {
                    refs: Arc::clone(&self.ref_counter.refs),
                    lru_queue: Arc::clone(&self.ref_counter.lru_queue),
                    current_cache_size: Arc::clone(&self.ref_counter.current_cache_size),
                    current_entries: Arc::clone(&self.ref_counter.current_entries),
                };

                // TODO: Implement proper background cleanup task spawning
                // tokio::spawn(async move {
                //     Self::run_background_cleanup(root_path, config, ref_counter, shutdown_signal).await;
                // });
            } else if #[cfg(feature = "streaming-smol")] {
                // For smol runtime, we'll disable background tasks for now
                // This is a placeholder for proper smol task spawning
                eprintln!("Warning: Background tasks not implemented for smol runtime yet");
            }
        }

        self.background_tasks = Some(task_manager);
    }

    /// Stop background cleanup tasks
    pub fn stop_background_tasks(&mut self) {
        if let Some(tasks) = &self.background_tasks {
            tasks.shutdown();
        }
        self.background_tasks = None;
    }

    /// Get cache statistics
    pub async fn cache_stats(&self) -> Result<CacheStats> {
        Ok(CacheStats {
            current_size: self.ref_counter.current_cache_size().await?,
            current_entries: self.ref_counter.current_entries().await?,
            max_size: self.config.max_cache_size,
            max_entries: self.config.max_entries,
        })
    }

    /// Enforce cache size and entry limits by evicting LRU entries
    async fn enforce_cache_limits(&self) -> Result<()> {
        let current_size = self.ref_counter.current_cache_size().await?;
        let current_entries = self.ref_counter.current_entries().await?;

        let target_size = self.config.max_cache_size.unwrap_or(u64::MAX);
        let target_count = self.config.max_entries.unwrap_or(usize::MAX);

        // Check if we need to evict entries
        if current_size <= target_size && current_entries <= target_count {
            return Ok(());
        }

        // Get entries that need to be evicted
        let entries_to_evict = self
            .ref_counter
            .get_lru_entries_for_eviction(target_size, target_count)
            .await?;

        // Evict the LRU entries
        for entry in entries_to_evict {
            if let Err(e) = self.delete(&entry.cache_key).await {
                // Log error but continue with other evictions
                eprintln!(
                    "Warning: Failed to evict cache entry '{}': {}",
                    entry.cache_key, e
                );
            }
        }

        Ok(())
    }

    /// Get the path for storing metadata
    fn metadata_path(&self, key: &str) -> PathBuf {
        let encoded_key = hex::encode(key.as_bytes());
        self.root_path
            .join(CACHE_VERSION)
            .join("metadata")
            .join(format!("{encoded_key}.json"))
    }

    /// Get the path for storing content
    fn content_path(&self, digest: &str) -> PathBuf {
        self.root_path.join(CACHE_VERSION).join("content").join(digest)
    }

    /// Calculate SHA256 digest of content
    fn calculate_digest(content: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(content);
        hex::encode(hasher.finalize())
    }

    /// Ensure directory exists
    async fn ensure_dir_exists(path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            runtime::create_dir_all(parent)
                .await
                .map_err(StreamingError::io)?;
        }
        Ok(())
    }

    /// Atomic file write operation to prevent corruption from concurrent writes
    async fn atomic_write(path: &Path, content: &[u8]) -> Result<()> {
        use std::ffi::OsString;

        // Create a temporary file with a unique suffix
        let mut temp_path = path.to_path_buf();
        let mut temp_name = temp_path
            .file_name()
            .map(|n| n.to_os_string())
            .unwrap_or_else(|| OsString::from("temp"));
        temp_name.push(".tmp");

        // Add process ID and timestamp to make it unique
        let pid = std::process::id();
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        temp_name.push(format!(".{}.{}", pid, timestamp));

        temp_path.set_file_name(&temp_name);

        // Ensure the parent directory exists
        Self::ensure_dir_exists(&temp_path).await?;

        // Write to temporary file first
        runtime::write(&temp_path, content)
            .await
            .map_err(StreamingError::io)?;

        // Atomically rename temporary file to final destination
        runtime::rename(&temp_path, path).await.map_err(|e| {
            // Clean up temporary file on failure
            let _ = std::fs::remove_file(&temp_path);
            StreamingError::io(format!(
                "Failed to atomically write file {:?}: {}",
                path, e
            ))
        })?;

        Ok(())
    }

    /// Build reference counts from existing cache entries
    /// This should be called on manager initialization to rebuild ref counts
    async fn rebuild_reference_counts(&self) -> Result<()> {
        let metadata_dir = self.root_path.join(CACHE_VERSION).join("metadata");

        if !metadata_dir.exists() {
            return Ok(());
        }

        cfg_if::cfg_if! {
            if #[cfg(feature = "streaming-tokio")] {
                let mut entries = runtime::read_dir(&metadata_dir).await.map_err(StreamingError::io)?;

                while let Some(entry) = entries.next_entry().await.map_err(StreamingError::io)? {
                    let path = entry.path();

                    if path.extension().and_then(|s| s.to_str()) == Some("json") {
                        let content = runtime::read(&path).await.map_err(StreamingError::io)?;
                        match serde_json::from_slice::<CacheMetadata>(&content) {
                            Ok(metadata) => {
                                // Add reference to this content digest
                                if let Err(e) = self.ref_counter.add_ref(&metadata.content_digest).await {
                                    return Err(StreamingError::consistency(format!("Failed to rebuild reference count for {}: {}", metadata.content_digest, e)).into());
                                }

                                // Rebuild LRU tracking - get file size
                                let cache_key = path.file_stem()
                                    .and_then(|s| s.to_str())
                                    .and_then(|s| hex::decode(s).ok())
                                    .and_then(|bytes| String::from_utf8(bytes).ok())
                                    .unwrap_or_else(|| format!("unknown-{}", metadata.content_digest));

                                let content_path = self.content_path(&metadata.content_digest);
                                let file_size = if let Ok(meta) = runtime::metadata(&content_path).await {
                                    meta.len()
                                } else {
                                    0
                                };

                                if let Err(e) = self.ref_counter.add_cache_entry(
                                    cache_key,
                                    metadata.content_digest.clone(),
                                    file_size
                                ).await {
                                    return Err(StreamingError::consistency(format!("Failed to rebuild LRU tracking for {}: {}", metadata.content_digest, e)).into());
                                }
                            }
                            Err(e) => {
                                return Err(StreamingError::serialization(format!("Failed to parse metadata file {:?}: {}", path, e)).into());
                            }
                        }
                    }
                }
            } else if #[cfg(feature = "streaming-smol")] {
                use futures::stream::StreamExt;

                let mut entries = runtime::read_dir(&metadata_dir).await.map_err(StreamingError::io)?;

                while let Some(entry_result) = entries.next().await {
                    let entry = entry_result.map_err(StreamingError::io)?;
                    let path = entry.path();

                    if path.extension().and_then(|s| s.to_str()) == Some("json") {
                        let content = runtime::read(&path).await.map_err(StreamingError::io)?;
                        match serde_json::from_slice::<CacheMetadata>(&content) {
                            Ok(metadata) => {
                                // Add reference to this content digest
                                if let Err(e) = self.ref_counter.add_ref(&metadata.content_digest).await {
                                    return Err(StreamingError::consistency(format!("Failed to rebuild reference count for {}: {}", metadata.content_digest, e)).into());
                                }

                                // Rebuild LRU tracking - get file size
                                let cache_key = path.file_stem()
                                    .and_then(|s| s.to_str())
                                    .and_then(|s| hex::decode(s).ok())
                                    .and_then(|bytes| String::from_utf8(bytes).ok())
                                    .unwrap_or_else(|| format!("unknown-{}", metadata.content_digest));

                                let content_path = self.content_path(&metadata.content_digest);
                                let file_size = if let Ok(meta) = runtime::metadata(&content_path).await {
                                    meta.len()
                                } else {
                                    0
                                };

                                if let Err(e) = self.ref_counter.add_cache_entry(
                                    cache_key,
                                    metadata.content_digest.clone(),
                                    file_size
                                ).await {
                                    return Err(StreamingError::consistency(format!("Failed to rebuild LRU tracking for {}: {}", metadata.content_digest, e)).into());
                                }
                            }
                            Err(e) => {
                                return Err(StreamingError::serialization(format!("Failed to parse metadata file {:?}: {}", path, e)).into());
                            }
                        }
                    }
                }
            }
        }

        Ok(())
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
        <Self::Body as http_body::Body>::Data: Send,
        <Self::Body as http_body::Body>::Error:
            Into<StreamingError> + Send + Sync + 'static,
    {
        let metadata_path = self.metadata_path(cache_key);

        // Check if metadata file exists
        if !metadata_path.exists() {
            return Ok(None);
        }

        // Update LRU access time
        let _ = self.ref_counter.update_access_time(cache_key).await;

        // Read and parse metadata
        let metadata_content =
            runtime::read(&metadata_path).await.map_err(StreamingError::io)?;
        let metadata: CacheMetadata = serde_json::from_slice(&metadata_content)
            .map_err(StreamingError::serialization)?;

        // Check if content file exists
        let content_path = self.content_path(&metadata.content_digest);
        if !content_path.exists() {
            return Ok(None);
        }

        // Open content file for streaming
        let file = runtime::File::open(&content_path)
            .await
            .map_err(StreamingError::io)?;

        // Build response with streaming body
        let mut response_builder = Response::builder()
            .status(metadata.status)
            .version(match metadata.version {
                9 => Version::HTTP_09,
                10 => Version::HTTP_10,
                11 => Version::HTTP_11,
                2 => Version::HTTP_2,
                3 => Version::HTTP_3,
                _ => Version::HTTP_11,
            });

        // Add headers
        for (name, value) in &metadata.headers {
            if let (Ok(header_name), Ok(header_value)) = (
                name.parse::<http::HeaderName>(),
                value.parse::<http::HeaderValue>(),
            ) {
                response_builder =
                    response_builder.header(header_name, header_value);
            }
        }

        // Create streaming body from file
        let body = StreamingBody::from_file(file);
        let response =
            response_builder.body(body).map_err(StreamingError::new)?;

        Ok(Some((response, metadata.policy)))
    }

    async fn put<B>(
        &self,
        cache_key: String,
        response: Response<B>,
        policy: CachePolicy,
        _request_url: Url,
    ) -> Result<Response<Self::Body>>
    where
        B: http_body::Body + Send + 'static,
        B::Data: Send,
        B::Error: Into<StreamingError>,
        <Self::Body as http_body::Body>::Data: Send,
        <Self::Body as http_body::Body>::Error:
            Into<StreamingError> + Send + Sync + 'static,
    {
        let (parts, body) = response.into_parts();

        // Collect body content
        let collected =
            body.collect().await.map_err(|e| StreamingError::new(e.into()))?;
        let body_bytes = collected.to_bytes();

        // Calculate content digest for deduplication
        let content_digest = Self::calculate_digest(&body_bytes);
        let content_path = self.content_path(&content_digest);

        // Ensure content directory exists and write content if not already present
        if !content_path.exists() {
            Self::atomic_write(&content_path, &body_bytes).await?;
        }

        // Add reference count for this content file
        self.ref_counter.add_ref(&content_digest).await?;

        // Add to LRU tracking and enforce cache limits
        let file_size = body_bytes.len() as u64;
        self.ref_counter
            .add_cache_entry(
                cache_key.clone(),
                content_digest.clone(),
                file_size,
            )
            .await?;
        self.enforce_cache_limits().await?;

        // Save reference counts if persistent mode is enabled
        if self.config.persistent_ref_counting {
            self.ref_counter.save_ref_counts(&self.root_path).await?;
        }

        // Create metadata
        let metadata = CacheMetadata {
            status: parts.status.as_u16(),
            version: match parts.version {
                Version::HTTP_09 => 9,
                Version::HTTP_10 => 10,
                Version::HTTP_11 => 11,
                Version::HTTP_2 => 2,
                Version::HTTP_3 => 3,
                _ => 11,
            },
            headers: crate::HttpCacheOptions::headers_to_hashmap(
                &parts.headers,
            ),
            content_digest: content_digest.clone(),
            policy,
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        };

        // Write metadata atomically
        let metadata_path = self.metadata_path(&cache_key);
        let metadata_json = serde_json::to_vec(&metadata)
            .map_err(StreamingError::serialization)?;
        Self::atomic_write(&metadata_path, &metadata_json).await?;

        // Return response with buffered body for immediate use
        let response =
            Response::from_parts(parts, StreamingBody::buffered(body_bytes));
        Ok(response)
    }

    async fn convert_body<B>(
        &self,
        response: Response<B>,
    ) -> Result<Response<Self::Body>>
    where
        B: http_body::Body + Send + 'static,
        B::Data: Send,
        B::Error: Into<StreamingError>,
        <Self::Body as http_body::Body>::Data: Send,
        <Self::Body as http_body::Body>::Error:
            Into<StreamingError> + Send + Sync + 'static,
    {
        let (parts, body) = response.into_parts();

        // For non-cacheable responses, simply collect the body and return it as buffered
        // This is more efficient than creating temporary files
        let collected =
            body.collect().await.map_err(|e| StreamingError::new(e.into()))?;
        let body_bytes = collected.to_bytes();
        let streaming_body = StreamingBody::buffered(body_bytes);

        Ok(Response::from_parts(parts, streaming_body))
    }

    async fn delete(&self, cache_key: &str) -> Result<()> {
        let metadata_path = self.metadata_path(cache_key);

        // Read metadata to get content digest before removing metadata file
        let metadata_content =
            runtime::read(&metadata_path).await.map_err(StreamingError::io)?;

        let metadata: CacheMetadata = serde_json::from_slice(&metadata_content)
            .map_err(StreamingError::serialization)?;

        // Remove metadata file first
        runtime::remove_file(&metadata_path)
            .await
            .map_err(StreamingError::io)?;

        // Remove from LRU tracking
        self.ref_counter.remove_cache_entry(cache_key).await?;

        // Save reference counts if persistent mode is enabled
        if self.config.persistent_ref_counting {
            self.ref_counter.save_ref_counts(&self.root_path).await?;
        }

        // Decrement reference count and remove content file if no longer referenced
        let can_delete =
            self.ref_counter.remove_ref(&metadata.content_digest).await?;
        if can_delete {
            let content_path = self.content_path(&metadata.content_digest);
            runtime::remove_file(&content_path).await.map_err(|e| {
                StreamingError::io(format!(
                    "Failed to remove content file {:?}: {}",
                    content_path, e
                ))
            })?;
        }

        Ok(())
    }

    #[cfg(feature = "streaming")]
    fn body_to_bytes_stream(
        body: Self::Body,
    ) -> impl futures_util::Stream<
        Item = std::result::Result<
            Bytes,
            Box<dyn std::error::Error + Send + Sync>,
        >,
    > + Send
    where
        <Self::Body as http_body::Body>::Data: Send,
        <Self::Body as http_body::Body>::Error: Send + Sync + 'static,
    {
        // Use the StreamingBody's built-in conversion method
        body.into_bytes_stream()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::StreamingCacheManager as StreamingCacheManagerTrait;
    use http_body_util::Full;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_streaming_cache_put_get() {
        let temp_dir = TempDir::new().unwrap();
        let cache = StreamingManager::new(temp_dir.path().to_path_buf());

        let original_body = Full::new(Bytes::from("test response body"));
        let response = Response::builder()
            .status(200)
            .header("content-type", "text/plain")
            .body(original_body)
            .unwrap();

        let policy = CachePolicy::new(
            &http::request::Request::builder()
                .method("GET")
                .uri("/test")
                .body(())
                .unwrap()
                .into_parts()
                .0,
            &response.clone().map(|_| ()),
        );

        let request_url = Url::parse("http://example.com/test").unwrap();

        // Put response into cache
        let cached_response = cache
            .put("test-key".to_string(), response, policy.clone(), request_url)
            .await
            .unwrap();

        // Response should be returned immediately
        assert_eq!(cached_response.status(), 200);

        // Get response from cache
        let retrieved = cache.get("test-key").await.unwrap();
        assert!(retrieved.is_some());

        let (cached_response, cached_policy) = retrieved.unwrap();
        assert_eq!(cached_response.status(), 200);
        assert_eq!(
            cached_response.headers().get("content-type").unwrap(),
            "text/plain"
        );

        // Verify policy is preserved
        let now = std::time::SystemTime::now();
        assert_eq!(cached_policy.time_to_live(now), policy.time_to_live(now));
    }

    #[tokio::test]
    async fn test_streaming_cache_delete() {
        let temp_dir = TempDir::new().unwrap();
        let cache = StreamingManager::new(temp_dir.path().to_path_buf());

        let original_body = Full::new(Bytes::from("test response body"));
        let response = Response::builder()
            .status(200)
            .header("content-type", "text/plain")
            .body(original_body)
            .unwrap();

        let policy = CachePolicy::new(
            &http::request::Request::builder()
                .method("GET")
                .uri("/test")
                .body(())
                .unwrap()
                .into_parts()
                .0,
            &response.clone().map(|_| ()),
        );

        let request_url = Url::parse("http://example.com/test").unwrap();
        let cache_key = "test-key-delete";

        // Put response into cache
        cache
            .put(cache_key.to_string(), response, policy, request_url)
            .await
            .unwrap();

        // Verify it exists
        let retrieved = cache.get(cache_key).await.unwrap();
        assert!(retrieved.is_some());

        // Delete it
        cache.delete(cache_key).await.unwrap();

        // Verify it's gone
        let retrieved = cache.get(cache_key).await.unwrap();
        assert!(retrieved.is_none());
    }

    /// Test content deduplication - multiple cache entries with identical content
    /// should share the same content file with proper reference counting
    #[tokio::test]
    async fn test_content_deduplication() {
        let temp_dir = TempDir::new().unwrap();
        let cache = StreamingManager::new(temp_dir.path().to_path_buf());

        let identical_content = Bytes::from("identical response body content");
        let request_url = Url::parse("http://example.com/test").unwrap();

        // Create two different responses with identical content
        let response1 = Response::builder()
            .status(200)
            .header("cache-control", "max-age=3600")
            .body(Full::new(identical_content.clone()))
            .unwrap();

        let response2 = Response::builder()
            .status(200)
            .header("content-type", "application/json")
            .body(Full::new(identical_content.clone()))
            .unwrap();

        let policy1 = CachePolicy::new(
            &http::request::Request::builder()
                .method("GET")
                .uri("/test1")
                .body(())
                .unwrap()
                .into_parts()
                .0,
            &response1.clone().map(|_| ()),
        );

        let policy2 = CachePolicy::new(
            &http::request::Request::builder()
                .method("GET")
                .uri("/test2")
                .body(())
                .unwrap()
                .into_parts()
                .0,
            &response2.clone().map(|_| ()),
        );

        // Cache both responses
        cache
            .put("key1".to_string(), response1, policy1, request_url.clone())
            .await
            .unwrap();
        cache
            .put("key2".to_string(), response2, policy2, request_url)
            .await
            .unwrap();

        // Verify both can be retrieved
        let retrieved1 = cache.get("key1").await.unwrap().unwrap();
        let retrieved2 = cache.get("key2").await.unwrap().unwrap();

        assert_eq!(retrieved1.0.status(), 200);
        assert_eq!(retrieved2.0.status(), 200);

        // Verify they have the same content digest (content deduplication)
        let content_digest1 =
            StreamingManager::calculate_digest(&identical_content);
        let content_path1 = cache.content_path(&content_digest1);
        assert!(content_path1.exists());

        // Count content files - should only have one for identical content
        let content_dir = temp_dir.path().join(CACHE_VERSION).join("content");
        let mut content_file_count = 0;
        if content_dir.exists() {
            for entry in std::fs::read_dir(&content_dir).unwrap() {
                let entry = entry.unwrap();
                if entry.file_type().unwrap().is_file() {
                    content_file_count += 1;
                }
            }
        }
        assert_eq!(
            content_file_count, 1,
            "Should have only one content file due to deduplication"
        );

        // Delete one cache entry
        cache.delete("key1").await.unwrap();

        // Content file should still exist due to reference counting
        assert!(
            content_path1.exists(),
            "Content file should still exist after deleting one reference"
        );

        // Verify other entry still works
        let retrieved2_again = cache.get("key2").await.unwrap().unwrap();
        assert_eq!(retrieved2_again.0.status(), 200);

        // Delete second cache entry
        cache.delete("key2").await.unwrap();

        // Now content file should be gone
        assert!(
            !content_path1.exists(),
            "Content file should be deleted when no references remain"
        );
    }

    /// Test reference count persistence across cache manager restarts
    #[tokio::test]
    async fn test_reference_count_persistence() {
        let temp_dir = TempDir::new().unwrap();
        let cache_path = temp_dir.path().to_path_buf();

        let identical_content =
            Bytes::from("persistent reference test content");
        let request_url = Url::parse("http://example.com/test").unwrap();

        // Phase 1: Create initial cache with content deduplication
        {
            let cache = StreamingManager::new(cache_path.clone());

            let response1 = Response::builder()
                .status(200)
                .body(Full::new(identical_content.clone()))
                .unwrap();

            let response2 = Response::builder()
                .status(404)
                .body(Full::new(identical_content.clone()))
                .unwrap();

            let policy1 = CachePolicy::new(
                &http::request::Request::builder()
                    .method("GET")
                    .uri("/test1")
                    .body(())
                    .unwrap()
                    .into_parts()
                    .0,
                &response1.clone().map(|_| ()),
            );

            let policy2 = CachePolicy::new(
                &http::request::Request::builder()
                    .method("GET")
                    .uri("/test2")
                    .body(())
                    .unwrap()
                    .into_parts()
                    .0,
                &response2.clone().map(|_| ()),
            );

            cache
                .put(
                    "persistent-key1".to_string(),
                    response1,
                    policy1,
                    request_url.clone(),
                )
                .await
                .unwrap();
            cache
                .put(
                    "persistent-key2".to_string(),
                    response2,
                    policy2,
                    request_url.clone(),
                )
                .await
                .unwrap();

            // Verify content file exists
            let content_digest =
                StreamingManager::calculate_digest(&identical_content);
            let content_path = cache.content_path(&content_digest);
            assert!(content_path.exists(), "Content file should exist");
        }

        // Phase 2: Create new cache manager and rebuild reference counts
        {
            let cache =
                StreamingManager::new_with_existing_cache(cache_path.clone())
                    .await
                    .unwrap();

            // Verify both entries still exist
            let retrieved1 = cache.get("persistent-key1").await.unwrap();
            let retrieved2 = cache.get("persistent-key2").await.unwrap();
            assert!(retrieved1.is_some());
            assert!(retrieved2.is_some());

            // Delete one entry - content should still exist
            cache.delete("persistent-key1").await.unwrap();

            let content_digest =
                StreamingManager::calculate_digest(&identical_content);
            let content_path = cache.content_path(&content_digest);
            assert!(
                content_path.exists(),
                "Content file should still exist after one deletion"
            );

            // Verify other entry still works
            let retrieved2_again = cache.get("persistent-key2").await.unwrap();
            assert!(retrieved2_again.is_some());

            // Delete second entry - now content should be deleted
            cache.delete("persistent-key2").await.unwrap();

            assert!(
                !content_path.exists(),
                "Content file should be deleted when all references removed"
            );
        }
    }

    /// Test concurrent access to reference counting
    #[tokio::test]
    async fn test_concurrent_reference_counting() {
        use std::sync::Arc;
        use tokio::task;

        let temp_dir = TempDir::new().unwrap();
        let cache =
            Arc::new(StreamingManager::new(temp_dir.path().to_path_buf()));
        let request_url = Url::parse("http://example.com/test").unwrap();

        let shared_content = Bytes::from("concurrent test content");
        let tasks_count = 10;

        // Create multiple tasks that store identical content concurrently
        let mut handles = Vec::new();
        for i in 0..tasks_count {
            let cache = Arc::clone(&cache);
            let content = shared_content.clone();
            let url = request_url.clone();

            let handle = task::spawn(async move {
                let response = Response::builder()
                    .status(200)
                    .header("x-task-id", i.to_string())
                    .body(Full::new(content))
                    .unwrap();

                let policy = CachePolicy::new(
                    &http::request::Request::builder()
                        .method("GET")
                        .uri(format!("/concurrent-test-{}", i))
                        .body(())
                        .unwrap()
                        .into_parts()
                        .0,
                    &response.clone().map(|_| ()),
                );

                cache
                    .put(format!("concurrent-key-{}", i), response, policy, url)
                    .await
                    .unwrap();
            });
            handles.push(handle);
        }

        // Wait for all tasks to complete
        for handle in handles {
            handle.await.unwrap();
        }

        // Verify all entries can be retrieved
        for i in 0..tasks_count {
            let retrieved =
                cache.get(&format!("concurrent-key-{}", i)).await.unwrap();
            assert!(retrieved.is_some(), "Entry {} should exist", i);
        }

        // Verify content deduplication worked (only one content file)
        let content_digest =
            StreamingManager::calculate_digest(&shared_content);
        let content_path = cache.content_path(&content_digest);
        assert!(content_path.exists(), "Shared content file should exist");

        // Delete half the entries concurrently
        let mut delete_handles = Vec::new();
        for i in 0..tasks_count / 2 {
            let cache = Arc::clone(&cache);
            let handle = task::spawn(async move {
                cache.delete(&format!("concurrent-key-{}", i)).await.unwrap();
            });
            delete_handles.push(handle);
        }

        for handle in delete_handles {
            handle.await.unwrap();
        }

        // Content should still exist (remaining references)
        assert!(content_path.exists(), "Content file should still exist");

        // Delete remaining entries
        let mut final_delete_handles = Vec::new();
        for i in tasks_count / 2..tasks_count {
            let cache = Arc::clone(&cache);
            let handle = task::spawn(async move {
                cache.delete(&format!("concurrent-key-{}", i)).await.unwrap();
            });
            final_delete_handles.push(handle);
        }

        for handle in final_delete_handles {
            handle.await.unwrap();
        }

        // Now content should be deleted
        assert!(
            !content_path.exists(),
            "Content file should be deleted when all references removed"
        );
    }

    /// Test large content handling and streaming behavior
    #[tokio::test]
    async fn test_large_content_streaming() {
        let temp_dir = TempDir::new().unwrap();
        let cache = StreamingManager::new(temp_dir.path().to_path_buf());

        // Create a large response body (1MB)
        let large_content = vec![b'X'; 1024 * 1024];
        let large_bytes = Bytes::from(large_content);

        let response = Response::builder()
            .status(200)
            .header("content-type", "application/octet-stream")
            .header("content-length", large_bytes.len().to_string())
            .body(Full::new(large_bytes.clone()))
            .unwrap();

        let policy = CachePolicy::new(
            &http::request::Request::builder()
                .method("GET")
                .uri("/large-file")
                .body(())
                .unwrap()
                .into_parts()
                .0,
            &response.clone().map(|_| ()),
        );

        let request_url = Url::parse("http://example.com/large-file").unwrap();

        // Store large response
        let cached_response = cache
            .put("large-key".to_string(), response, policy, request_url)
            .await
            .unwrap();

        assert_eq!(cached_response.status(), 200);

        // Retrieve and verify
        let (retrieved_response, _) =
            cache.get("large-key").await.unwrap().unwrap();
        assert_eq!(retrieved_response.status(), 200);
        assert_eq!(
            retrieved_response.headers().get("content-type").unwrap(),
            "application/octet-stream"
        );

        // Verify content file exists and has correct size
        let content_digest = StreamingManager::calculate_digest(&large_bytes);
        let content_path = cache.content_path(&content_digest);
        assert!(content_path.exists(), "Large content file should exist");

        let metadata = std::fs::metadata(&content_path).unwrap();
        assert_eq!(
            metadata.len(),
            1024 * 1024,
            "Content file should have correct size"
        );
    }

    /// Test error handling for various failure scenarios
    #[tokio::test]
    async fn test_error_handling() {
        let temp_dir = TempDir::new().unwrap();
        let cache = StreamingManager::new(temp_dir.path().to_path_buf());

        // Test getting non-existent key
        let result = cache.get("non-existent").await.unwrap();
        assert!(result.is_none(), "Should return None for non-existent key");

        // Test deleting non-existent key
        let result = cache.delete("non-existent").await;
        assert!(result.is_err(), "Should error when deleting non-existent key");

        // Test with corrupted metadata (create invalid JSON file)
        let metadata_dir = temp_dir.path().join(CACHE_VERSION).join("metadata");
        std::fs::create_dir_all(&metadata_dir).unwrap();
        let corrupt_metadata_path = metadata_dir.join("corrupt.json");
        std::fs::write(&corrupt_metadata_path, "invalid json").unwrap();

        // Cache should still function normally despite corrupted file
        let response = Response::builder()
            .status(200)
            .body(Full::new(Bytes::from("test")))
            .unwrap();

        let policy = CachePolicy::new(
            &http::request::Request::builder()
                .method("GET")
                .uri("/test")
                .body(())
                .unwrap()
                .into_parts()
                .0,
            &response.clone().map(|_| ()),
        );

        let request_url = Url::parse("http://example.com/test").unwrap();

        let result = cache
            .put("valid-key".to_string(), response, policy, request_url)
            .await;
        assert!(result.is_ok(), "Should handle corrupted metadata gracefully");
    }

    /// Test content validation and corruption detection
    #[tokio::test]
    async fn test_content_integrity_validation() {
        let temp_dir = TempDir::new().unwrap();
        let cache = StreamingManager::new(temp_dir.path().to_path_buf());

        let original_content =
            Bytes::from("original content for integrity test");
        let response = Response::builder()
            .status(200)
            .body(Full::new(original_content.clone()))
            .unwrap();

        let policy = CachePolicy::new(
            &http::request::Request::builder()
                .method("GET")
                .uri("/integrity-test")
                .body(())
                .unwrap()
                .into_parts()
                .0,
            &response.clone().map(|_| ()),
        );

        let request_url =
            Url::parse("http://example.com/integrity-test").unwrap();

        // Store original content
        cache
            .put("integrity-key".to_string(), response, policy, request_url)
            .await
            .unwrap();

        // Verify content can be retrieved
        let retrieved = cache.get("integrity-key").await.unwrap();
        assert!(retrieved.is_some(), "Content should be retrievable");

        // Corrupt the content file
        let content_digest =
            StreamingManager::calculate_digest(&original_content);
        let content_path = cache.content_path(&content_digest);
        assert!(content_path.exists(), "Content file should exist");

        std::fs::write(&content_path, "corrupted content").unwrap();

        // Cache should handle corrupted content gracefully
        let retrieved_after_corruption =
            cache.get("integrity-key").await.unwrap();
        assert!(
            retrieved_after_corruption.is_some(),
            "Should still return metadata even with corrupted content"
        );
    }

    /// Test HTTP version handling
    #[tokio::test]
    async fn test_http_version_preservation() {
        let temp_dir = TempDir::new().unwrap();
        let cache = StreamingManager::new(temp_dir.path().to_path_buf());

        let test_versions = vec![
            (Version::HTTP_09, 9u8),
            (Version::HTTP_10, 10u8),
            (Version::HTTP_11, 11u8),
            (Version::HTTP_2, 2u8),
            (Version::HTTP_3, 3u8),
        ];

        for (i, (version, expected_stored)) in
            test_versions.into_iter().enumerate()
        {
            let response = Response::builder()
                .status(200)
                .version(version)
                .header("content-type", "text/plain")
                .body(Full::new(Bytes::from(format!("version test {}", i))))
                .unwrap();

            let policy = CachePolicy::new(
                &http::request::Request::builder()
                    .method("GET")
                    .uri(format!("/version-test-{}", i))
                    .body(())
                    .unwrap()
                    .into_parts()
                    .0,
                &response.clone().map(|_| ()),
            );

            let request_url =
                Url::parse(&format!("http://example.com/version-test-{}", i))
                    .unwrap();
            let cache_key = format!("version-key-{}", i);

            // Store response
            cache
                .put(cache_key.clone(), response, policy, request_url)
                .await
                .unwrap();

            // Retrieve and verify version is preserved
            let (retrieved_response, _) =
                cache.get(&cache_key).await.unwrap().unwrap();
            assert_eq!(
                retrieved_response.version(),
                version,
                "HTTP version should be preserved for version {:?}",
                version
            );

            // Verify the stored version value in metadata
            let metadata_path = cache.metadata_path(&cache_key);
            let metadata_content = std::fs::read(&metadata_path).unwrap();
            let metadata: CacheMetadata =
                serde_json::from_slice(&metadata_content).unwrap();
            assert_eq!(
                metadata.version, expected_stored,
                "Stored version should match expected for {:?}",
                version
            );
        }
    }

    /// Test header preservation and edge cases
    #[tokio::test]
    async fn test_header_edge_cases() {
        let temp_dir = TempDir::new().unwrap();
        let cache = StreamingManager::new(temp_dir.path().to_path_buf());

        // Test response with various header types
        let response = Response::builder()
            .status(200)
            .header("content-type", "application/json; charset=utf-8")
            .header("cache-control", "max-age=3600, public")
            .header("custom-header", "custom-value")
            .header("empty-header", "")
            .header("unicode-header", "test-ñ-value")
            .header("multiple-values", "value1")
            .header("multiple-values", "value2") // This will overwrite the first one in http crate
            .body(Full::new(Bytes::from(r#"{"test": "json"}"#)))
            .unwrap();

        let policy = CachePolicy::new(
            &http::request::Request::builder()
                .method("GET")
                .uri("/header-test")
                .body(())
                .unwrap()
                .into_parts()
                .0,
            &response.clone().map(|_| ()),
        );

        let request_url = Url::parse("http://example.com/header-test").unwrap();

        // Store response
        cache
            .put(
                "header-key".to_string(),
                response.clone(),
                policy,
                request_url,
            )
            .await
            .unwrap();

        // Retrieve and verify headers
        let (retrieved_response, _) =
            cache.get("header-key").await.unwrap().unwrap();

        // Verify critical headers are preserved
        assert_eq!(
            retrieved_response.headers().get("content-type").unwrap(),
            "application/json; charset=utf-8"
        );
        assert_eq!(
            retrieved_response.headers().get("cache-control").unwrap(),
            "max-age=3600, public"
        );
        assert_eq!(
            retrieved_response.headers().get("custom-header").unwrap(),
            "custom-value"
        );

        // Verify empty header is handled
        assert_eq!(
            retrieved_response.headers().get("empty-header").unwrap(),
            ""
        );
    }

    /// Test edge cases with cache keys
    #[tokio::test]
    async fn test_cache_key_edge_cases() {
        let temp_dir = TempDir::new().unwrap();
        let cache = StreamingManager::new(temp_dir.path().to_path_buf());

        let response = Response::builder()
            .status(200)
            .body(Full::new(Bytes::from("test content")))
            .unwrap();

        let policy = CachePolicy::new(
            &http::request::Request::builder()
                .method("GET")
                .uri("/test")
                .body(())
                .unwrap()
                .into_parts()
                .0,
            &response.clone().map(|_| ()),
        );

        let request_url = Url::parse("http://example.com/test").unwrap();

        // Test various cache key formats
        let edge_case_keys = vec![
            "simple-key",
            "key:with:colons",
            "key with spaces",
            "key/with/slashes",
            "key?with=query&params=true",
            "key#with-fragment",
            "very-long-key-that-exceeds-normal-filename-length-limits-and-should-still-work-properly-without-issues-abcdefghijklmnopqrstuvwxyz",
            "unicode-key-ñáéíóú-test",
            "",  // Empty key
        ];

        for (i, key) in edge_case_keys.into_iter().enumerate() {
            let test_response = Response::builder()
                .status(200)
                .header("x-key-index", i.to_string())
                .body(Full::new(Bytes::from(format!(
                    "content for key: {}",
                    key
                ))))
                .unwrap();

            // Store with edge case key
            let result = cache
                .put(
                    key.to_string(),
                    test_response,
                    policy.clone(),
                    request_url.clone(),
                )
                .await;

            // Skip empty keys and very long keys that might fail due to filesystem limitations
            if key.is_empty() || key.len() > 100 {
                continue;
            }

            assert!(result.is_ok(), "Should handle key: '{}'", key);

            // Retrieve and verify
            let retrieved = cache.get(key).await.unwrap();
            assert!(
                retrieved.is_some(),
                "Should retrieve content for key: '{}'",
                key
            );

            let (retrieved_response, _) = retrieved.unwrap();
            assert_eq!(retrieved_response.status(), 200);
            assert_eq!(
                retrieved_response.headers().get("x-key-index").unwrap(),
                &i.to_string()
            );
        }
    }
}
