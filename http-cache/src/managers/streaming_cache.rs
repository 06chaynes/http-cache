//! Streaming cache manager with persistent disk-based storage.
//!
//! This module provides [`StreamingManager`], a streaming cache implementation that
//! combines [redb](https://docs.rs/redb) for metadata + raw [`tokio::fs`] files for
//! bodies, fronted by [moka](https://docs.rs/moka) as an in-memory hot cache.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │  moka::Cache<String, CacheMetadata>  (in-memory hot cache)      │
//! │  - Pure RAM; eviction does NOT touch disk                       │
//! └─────────────────────────────────────────────────────────────────┘
//!                           │ (slow-path miss)
//!                           ▼
//! ┌─────────────────────────────────────────────────────────────────┐
//! │  redb B-tree at $cache_dir/metadata.redb                        │
//! │  - key → postcard-serialized CacheMetadata                      │
//! │  - ACID txns, exclusive file lock on open                       │
//! └─────────────────────────────────────────────────────────────────┘
//!                           │ (body_hash lookup)
//!                           ▼
//! ┌─────────────────────────────────────────────────────────────────┐
//! │  Body files at $cache_dir/bodies/<prefix>/<body_hash>.bin       │
//! │  - body_hash = blake3(cache_key) hex, sharded by first 2 chars  │
//! │  - File layout: [16-byte nonce][body bytes]                     │
//! │  - Streamed via tokio::fs::File in 64KB chunks                  │
//! └─────────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Single-instance invariant
//!
//! Exactly one [`StreamingManager`] may operate on a given `cache_dir` at a time.
//! Enforced by redb's exclusive file lock on `metadata.redb` — a second
//! construction at the same path fails while the first is alive. This is reliable
//! on local filesystems (flock/LockFileEx); best-effort on networked or overlay
//! filesystems (NFS, some container overlays) — do not share a cache directory
//! across hosts.
//!
//! # Crash safety
//!
//! Every `put` writes the body to a unique tmp file under `tmp/`, `fsync`s,
//! atomically renames to the final body path, then commits the redb metadata
//! transaction. Each body file is prefixed with a 16-byte random nonce also
//! stored in metadata; `get` validates both the file length and nonce before
//! streaming. A crash in any ordering is either self-healed on next read or
//! leaves recoverable state (orphan tmp files swept on startup).
//!
//! # Memory efficiency
//!
//! On cache hit, only ~64KB is held in memory at a time (the streaming buffer),
//! regardless of response size. Writes (put) still buffer the full body in RAM
//! — use `max_body_size` to cap per-entry cost.
//!
//! # Example
//!
//! ```rust,ignore
//! use http_cache::StreamingManager;
//! use std::path::PathBuf;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let manager = StreamingManager::new(PathBuf::from("./cache"), 10_000).await?;
//! # Ok(())
//! # }
//! ```

use std::{
    fmt,
    path::{Path, PathBuf},
    sync::Arc,
};

use crate::{
    body::StreamingBody,
    error::{Result, StreamingError, StreamingErrorKind},
    HttpHeaders, StreamingCacheManager, Url,
};
use bytes::Bytes;
use http::{Response, Version};
use http_body::Body;
use http_body_util::{BodyExt, Empty};
use http_cache_semantics::CachePolicy;
use moka::future::Cache;
use rand::RngExt;
use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::CachedUserMetadata;

/// Default maximum body size for cached responses (100MB).
///
/// Responses larger than this will be rejected during caching to prevent
/// memory exhaustion. Configure with [`StreamingManager::with_max_body_size`].
pub const DEFAULT_MAX_BODY_SIZE: u64 = 100 * 1024 * 1024;

/// Size of the 16-byte nonce header prepended to every body file on disk.
const NONCE_LEN: usize = 16;

/// redb table holding `cache_key -> postcard(CacheMetadata)` mappings.
const METADATA_TABLE: TableDefinition<&str, &[u8]> =
    TableDefinition::new("http_streaming_metadata_v1");

/// Metadata stored alongside each cached response.
///
/// Kept small — held in moka as the hot cache and persisted in redb as the
/// durable index. The actual body bytes live in a separate file on disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CacheMetadata {
    /// HTTP status code
    status: u16,
    /// HTTP version encoded as u8
    version: u8,
    /// HTTP response headers
    headers: HttpHeaders,
    /// Size of the body in bytes (not including the 16-byte nonce header
    /// prepended to the on-disk file)
    body_size: u64,
    /// 16-byte random nonce written to the body-file header; used to detect
    /// overwrite-crash corruption where the file contents changed but the
    /// metadata transaction did not land.
    nonce: [u8; NONCE_LEN],
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

/// Compute the deterministic on-disk body hash for a cache key.
fn body_hash_for(key: &str) -> String {
    blake3::hash(key.as_bytes()).to_hex().to_string()
}

/// Compute the full body file path for a given body hash.
fn body_path_for(body_dir: &Path, body_hash: &str) -> PathBuf {
    body_dir.join(&body_hash[0..2]).join(format!("{body_hash}.bin"))
}

/// Streaming cache manager backed by redb (metadata) + `tokio::fs` (bodies).
///
/// This implementation provides:
///
/// - **Persistence across restarts**: metadata lives in an on-disk redb database,
///   not just in-memory moka — cached entries survive process restarts.
/// - **True streaming reads**: Cached responses are streamed from disk in 64KB
///   chunks, not loaded fully into memory.
/// - **Single-instance enforcement**: redb's file lock prevents multiple
///   [`StreamingManager`]s from operating on the same cache_dir concurrently.
/// - **Crash-safe writes**: atomic rename + 16-byte nonce header detect
///   overwrite-crash corruption; orphan tmp files are swept on startup.
/// - **Body size limits**: Configurable max body size to prevent memory
///   exhaustion.
///
/// # Only one instance per cache directory
///
/// Only one [`StreamingManager`] may point at a given `cache_dir` at a time
/// (enforced by redb's internal file lock on `metadata.redb`). Cloning an
/// existing `StreamingManager` is fine — construction via [`new`], [`with_max_body_size`],
/// or [`with_temp_dir`] against a directory already in use will fail. This
/// guarantee is reliable on local filesystems; on NFS or container overlay
/// filesystems it is best-effort. Do not share a cache directory across hosts.
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
    /// Root cache directory; contains `metadata.redb`, `bodies/`, and `tmp/`.
    cache_dir: PathBuf,
    /// Body files subdirectory: `cache_dir/bodies/<prefix>/<body_hash>.bin`.
    body_dir: PathBuf,
    /// Staging directory for in-flight body writes.
    tmp_dir: PathBuf,
    /// redb database storing key → metadata mappings.
    db: Arc<Database>,
    /// Metadata hot cache (pure RAM; evictions do NOT touch disk).
    metadata: Cache<String, CacheMetadata>,
    /// Maximum body size for cached responses.
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
    /// # Single-instance invariant
    ///
    /// Only one [`StreamingManager`] may operate on a given `cache_dir` at a
    /// time. Construction fails if another instance in any process currently
    /// holds the `metadata.redb` file lock.
    ///
    /// # Arguments
    ///
    /// * `cache_dir` - Directory to store cached response bodies and metadata
    /// * `capacity` - Maximum number of metadata entries in the in-memory hot cache
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
    /// See [`StreamingManager::new`] for details on the single-instance
    /// invariant.
    ///
    /// # Arguments
    ///
    /// * `cache_dir` - Directory to store cached response bodies and metadata
    /// * `capacity` - Maximum number of metadata entries in the in-memory hot cache
    /// * `max_body_size` - Maximum body size in bytes (responses larger than this are rejected)
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use http_cache::StreamingManager;
    /// use std::path::PathBuf;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
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
        // Ensure cache directory and subdirectories exist
        tokio::fs::create_dir_all(&cache_dir).await.map_err(|e| {
            crate::HttpCacheError::cache(format!(
                "Failed to create cache directory: {e}"
            ))
        })?;
        let body_dir = cache_dir.join("bodies");
        let tmp_dir = cache_dir.join("tmp");
        tokio::fs::create_dir_all(&body_dir).await.map_err(|e| {
            crate::HttpCacheError::cache(format!(
                "Failed to create body directory: {e}"
            ))
        })?;
        tokio::fs::create_dir_all(&tmp_dir).await.map_err(|e| {
            crate::HttpCacheError::cache(format!(
                "Failed to create tmp directory: {e}"
            ))
        })?;

        // Open redb — acquires an exclusive file lock on metadata.redb.
        // Must happen before the tmp sweep so we know we're the only instance.
        let db_path = cache_dir.join("metadata.redb");
        let db = tokio::task::spawn_blocking(move || Database::create(db_path))
            .await
            .map_err(|e| {
                crate::HttpCacheError::cache(format!(
                    "redb open join failed: {e}"
                ))
            })?
            .map_err(|e| {
                crate::HttpCacheError::cache(format!(
                    "Failed to open redb database (another StreamingManager \
                     instance may be active against this cache_dir): {e}"
                ))
            })?;
        let db = Arc::new(db);

        // Ensure the table exists so subsequent read transactions won't fail
        // with "table does not exist" on a brand-new database.
        {
            let db_init = db.clone();
            tokio::task::spawn_blocking(move || -> Result<()> {
                let write_txn = db_init.begin_write().map_err(|e| {
                    crate::HttpCacheError::cache(format!(
                        "redb begin_write failed during init: {e}"
                    ))
                })?;
                {
                    let _table =
                        write_txn.open_table(METADATA_TABLE).map_err(|e| {
                            crate::HttpCacheError::cache(format!(
                                "redb open_table failed during init: {e}"
                            ))
                        })?;
                }
                write_txn.commit().map_err(|e| {
                    crate::HttpCacheError::cache(format!(
                        "redb commit failed during init: {e}"
                    ))
                })?;
                Ok(())
            })
            .await
            .map_err(|e| {
                crate::HttpCacheError::cache(format!(
                    "redb init join failed: {e}"
                ))
            })??;
        }

        // Sweep tmp/: delete any stale files from prior crashed puts. Safe
        // because we hold the redb lock — no other instance can be writing
        // tmp files right now.
        sweep_tmp_dir(&tmp_dir).await;

        // Build moka without any eviction listener — evictions drop RAM only.
        let metadata: Cache<String, CacheMetadata> =
            Cache::builder().max_capacity(capacity).build();

        // Lazily hydrate moka from redb up to `capacity` entries. Use a
        // spawn_blocking worker for the sync redb iteration, then do the
        // async inserts back on this task.
        type StartupScan = (Vec<(String, CacheMetadata)>, Vec<String>);
        let db_for_scan = db.clone();
        let cap_usize = capacity as usize;
        let (entries, bad_keys) =
            tokio::task::spawn_blocking(move || -> Result<StartupScan> {
                let read_txn = db_for_scan.begin_read().map_err(|e| {
                    crate::HttpCacheError::cache(format!(
                        "redb begin_read failed during startup: {e}"
                    ))
                })?;
                let table =
                    read_txn.open_table(METADATA_TABLE).map_err(|e| {
                        crate::HttpCacheError::cache(format!(
                            "redb open_table failed during startup: {e}"
                        ))
                    })?;
                let mut entries: Vec<(String, CacheMetadata)> = Vec::new();
                let mut bad_keys: Vec<String> = Vec::new();
                for row in table.iter().map_err(|e| {
                    crate::HttpCacheError::cache(format!(
                        "redb iter failed: {e}"
                    ))
                })? {
                    let (k_guard, v_guard) = match row {
                        Ok(pair) => pair,
                        Err(e) => {
                            log::debug!(
                                "Skipping corrupt redb row during startup: \
                                 {e}"
                            );
                            continue;
                        }
                    };
                    let k = k_guard.value().to_string();
                    let v = v_guard.value();
                    match postcard::from_bytes::<CacheMetadata>(v) {
                        Ok(m) => {
                            if entries.len() < cap_usize {
                                entries.push((k, m));
                            }
                        }
                        Err(e) => {
                            log::debug!(
                                "Skipping poisoned metadata for key {k}: {e}"
                            );
                            bad_keys.push(k);
                        }
                    }
                }
                Ok((entries, bad_keys))
            })
            .await
            .map_err(|e| {
                crate::HttpCacheError::cache(format!(
                    "startup scan join failed: {e}"
                ))
            })??;

        // Remove any poisoned rows in a single follow-up write txn.
        if !bad_keys.is_empty() {
            let db_cleanup = db.clone();
            tokio::task::spawn_blocking(move || -> Result<()> {
                let write_txn = db_cleanup.begin_write().map_err(|e| {
                    crate::HttpCacheError::cache(format!(
                        "redb begin_write (poisoned cleanup) failed: {e}"
                    ))
                })?;
                {
                    let mut table =
                        write_txn.open_table(METADATA_TABLE).map_err(|e| {
                            crate::HttpCacheError::cache(format!(
                                "redb open_table (poisoned cleanup) failed: \
                                 {e}"
                            ))
                        })?;
                    for k in &bad_keys {
                        let _ = table.remove(k.as_str());
                    }
                }
                write_txn.commit().map_err(|e| {
                    crate::HttpCacheError::cache(format!(
                        "redb commit (poisoned cleanup) failed: {e}"
                    ))
                })?;
                Ok(())
            })
            .await
            .map_err(|e| {
                crate::HttpCacheError::cache(format!(
                    "poisoned cleanup join failed: {e}"
                ))
            })??;
        }

        for (k, m) in entries {
            metadata.insert(k, m).await;
        }

        Ok(Self { cache_dir, body_dir, tmp_dir, db, metadata, max_body_size })
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

    /// Returns the current number of entries in the **in-memory hot cache**.
    ///
    /// Note: this is not necessarily the total number of persisted entries.
    /// When the cache has fewer entries than the configured `capacity`, this
    /// equals the total. Once capacity is exceeded, cold entries remain on
    /// disk (reachable via `get`) but are not counted here.
    #[must_use]
    pub fn entry_count(&self) -> u64 {
        self.metadata.entry_count()
    }

    /// Returns the maximum body size for cached responses.
    #[must_use]
    pub fn max_body_size(&self) -> u64 {
        self.max_body_size
    }

    /// Clears all entries from the cache — moka, redb, and on-disk bodies.
    pub async fn clear(&self) -> Result<()> {
        // Drop the redb table; it will be recreated lazily on next open_table.
        let db = self.db.clone();
        tokio::task::spawn_blocking(move || -> Result<()> {
            let write_txn = db.begin_write().map_err(|e| {
                crate::HttpCacheError::cache(format!(
                    "redb begin_write (clear) failed: {e}"
                ))
            })?;
            write_txn.delete_table(METADATA_TABLE).map_err(|e| {
                crate::HttpCacheError::cache(format!(
                    "redb delete_table failed: {e}"
                ))
            })?;
            // Recreate the table so subsequent reads don't fail with
            // "table does not exist".
            {
                let _table =
                    write_txn.open_table(METADATA_TABLE).map_err(|e| {
                        crate::HttpCacheError::cache(format!(
                            "redb open_table (clear recreate) failed: {e}"
                        ))
                    })?;
            }
            write_txn.commit().map_err(|e| {
                crate::HttpCacheError::cache(format!(
                    "redb commit (clear) failed: {e}"
                ))
            })?;
            Ok(())
        })
        .await
        .map_err(|e| {
            crate::HttpCacheError::cache(format!("clear join failed: {e}"))
        })??;

        // Wipe body files and tmp files.
        let _ = tokio::fs::remove_dir_all(&self.body_dir).await;
        tokio::fs::create_dir_all(&self.body_dir).await.map_err(|e| {
            crate::HttpCacheError::cache(format!(
                "Failed to recreate body directory: {e}"
            ))
        })?;
        let _ = tokio::fs::remove_dir_all(&self.tmp_dir).await;
        tokio::fs::create_dir_all(&self.tmp_dir).await.map_err(|e| {
            crate::HttpCacheError::cache(format!(
                "Failed to recreate tmp directory: {e}"
            ))
        })?;

        // Invalidate moka last so in-flight gets don't resurrect metadata
        // pointing at now-deleted files.
        self.metadata.invalidate_all();
        self.metadata.run_pending_tasks().await;

        Ok(())
    }

    /// Runs pending maintenance tasks (eviction, etc).
    ///
    /// This is called automatically but can be invoked manually
    /// to force immediate cleanup.
    pub async fn run_pending_tasks(&self) {
        self.metadata.run_pending_tasks().await;
    }

    /// Remove a key from redb. Swallows errors at debug level; used by
    /// self-heal paths.
    async fn redb_remove(&self, cache_key: &str) {
        let db = self.db.clone();
        let key = cache_key.to_string();
        let _ = tokio::task::spawn_blocking(move || -> Result<()> {
            let write_txn = db.begin_write().map_err(|e| {
                crate::HttpCacheError::cache(format!(
                    "redb begin_write (remove) failed: {e}"
                ))
            })?;
            {
                let mut table =
                    write_txn.open_table(METADATA_TABLE).map_err(|e| {
                        crate::HttpCacheError::cache(format!(
                            "redb open_table (remove) failed: {e}"
                        ))
                    })?;
                let _ = table.remove(key.as_str());
            }
            write_txn.commit().map_err(|e| {
                crate::HttpCacheError::cache(format!(
                    "redb commit (remove) failed: {e}"
                ))
            })?;
            Ok(())
        })
        .await;
    }

    /// Read a metadata row from redb by key.
    async fn redb_get(&self, cache_key: &str) -> Result<Option<CacheMetadata>> {
        let db = self.db.clone();
        let key = cache_key.to_string();
        let bytes =
            tokio::task::spawn_blocking(move || -> Result<Option<Vec<u8>>> {
                let read_txn = db.begin_read().map_err(|e| {
                    crate::HttpCacheError::cache(format!(
                        "redb begin_read failed: {e}"
                    ))
                })?;
                let table =
                    read_txn.open_table(METADATA_TABLE).map_err(|e| {
                        crate::HttpCacheError::cache(format!(
                            "redb open_table failed: {e}"
                        ))
                    })?;
                match table.get(key.as_str()).map_err(|e| {
                    crate::HttpCacheError::cache(format!(
                        "redb get failed: {e}"
                    ))
                })? {
                    Some(g) => Ok(Some(g.value().to_vec())),
                    None => Ok(None),
                }
            })
            .await
            .map_err(|e| {
                crate::HttpCacheError::cache(format!(
                    "redb_get join failed: {e}"
                ))
            })??;

        match bytes {
            None => Ok(None),
            Some(b) => match postcard::from_bytes::<CacheMetadata>(&b) {
                Ok(m) => Ok(Some(m)),
                Err(e) => {
                    log::debug!(
                        "Poisoned metadata for key {cache_key}; removing: {e}"
                    );
                    self.redb_remove(cache_key).await;
                    Ok(None)
                }
            },
        }
    }

    /// Build a `Response<StreamingBody>` from an opened body file and
    /// validated metadata. File cursor must already be past the 16-byte nonce
    /// header.
    fn build_response_from_parts(
        metadata: &CacheMetadata,
        file: tokio::fs::File,
    ) -> Result<Response<StreamingBody<Empty<Bytes>>>> {
        let mut response_builder = Response::builder()
            .status(metadata.status)
            .version(version_from_u8(metadata.version));
        for (name, value) in metadata.headers.iter() {
            response_builder =
                response_builder.header(name.as_str(), value.as_str());
        }
        let body = StreamingBody::from_file_with_size(file, metadata.body_size);
        let mut response = response_builder.body(body).map_err(|e| {
            crate::HttpCacheError::cache(format!(
                "Failed to build response: {e}"
            ))
        })?;
        // Preserve user metadata via extensions for the orchestrator to pick
        // up on 304 re-cache.
        response
            .extensions_mut()
            .insert(CachedUserMetadata(metadata.user_metadata.clone()));
        Ok(response)
    }
}

/// Delete every entry in `tmp_dir`. Called on startup after we hold the redb
/// lock, so no concurrent put is in-flight.
async fn sweep_tmp_dir(tmp_dir: &Path) {
    let mut rd = match tokio::fs::read_dir(tmp_dir).await {
        Ok(rd) => rd,
        Err(e) => {
            log::debug!("tmp sweep: read_dir failed: {e}");
            return;
        }
    };
    let mut removed = 0usize;
    loop {
        match rd.next_entry().await {
            Ok(Some(entry)) => {
                let p = entry.path();
                if let Err(e) = tokio::fs::remove_file(&p).await {
                    log::debug!(
                        "tmp sweep: remove_file {} failed: {e}",
                        p.display()
                    );
                } else {
                    removed += 1;
                }
            }
            Ok(None) => break,
            Err(e) => {
                log::debug!("tmp sweep: next_entry failed: {e}");
                break;
            }
        }
    }
    if removed > 0 {
        log::debug!("tmp sweep removed {removed} stale file(s)");
    }
}

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
        // Resolve metadata: moka hit first, fall through to redb on miss.
        let metadata = match self.metadata.get(cache_key).await {
            Some(m) => m,
            None => match self.redb_get(cache_key).await? {
                Some(m) => {
                    self.metadata
                        .insert(cache_key.to_string(), m.clone())
                        .await;
                    m
                }
                None => return Ok(None),
            },
        };

        let body_hash = body_hash_for(cache_key);
        let body_path = body_path_for(&self.body_dir, &body_hash);

        // Open the body file. NotFound self-heals as a miss.
        let mut file = match tokio::fs::File::open(&body_path).await {
            Ok(f) => f,
            Err(ref e) if e.kind() == std::io::ErrorKind::NotFound => {
                self.metadata.invalidate(cache_key).await;
                self.redb_remove(cache_key).await;
                return Ok(None);
            }
            Err(e) => {
                return Err(Box::new(crate::HttpCacheError::cache(format!(
                    "Failed to open cached body file: {e}"
                ))));
            }
        };

        // Length check: file must be exactly nonce header + body_size.
        let file_len = match file.metadata().await {
            Ok(m) => m.len(),
            Err(e) => {
                log::debug!(
                    "body file stat failed for {cache_key}; self-healing: {e}"
                );
                self.metadata.invalidate(cache_key).await;
                self.redb_remove(cache_key).await;
                let _ = tokio::fs::remove_file(&body_path).await;
                return Ok(None);
            }
        };
        if file_len != NONCE_LEN as u64 + metadata.body_size {
            log::debug!(
                "body-size mismatch for {cache_key} (file={file_len}, \
                 expected={}); self-healing",
                NONCE_LEN as u64 + metadata.body_size
            );
            drop(file);
            self.metadata.invalidate(cache_key).await;
            self.redb_remove(cache_key).await;
            let _ = tokio::fs::remove_file(&body_path).await;
            return Ok(None);
        }

        // Nonce check: read the 16-byte header and compare to metadata.
        let mut nonce_buf = [0u8; NONCE_LEN];
        if let Err(e) = file.read_exact(&mut nonce_buf).await {
            log::debug!(
                "body-file nonce read failed for {cache_key}; self-healing: {e}"
            );
            drop(file);
            self.metadata.invalidate(cache_key).await;
            self.redb_remove(cache_key).await;
            let _ = tokio::fs::remove_file(&body_path).await;
            return Ok(None);
        }
        if nonce_buf != metadata.nonce {
            log::debug!(
                "nonce mismatch for {cache_key}; self-healing (overwrite-crash \
                 window or tampering)"
            );
            drop(file);
            self.metadata.invalidate(cache_key).await;
            self.redb_remove(cache_key).await;
            let _ = tokio::fs::remove_file(&body_path).await;
            return Ok(None);
        }

        // File cursor is now at offset NONCE_LEN; StreamingBody streams from
        // here to EOF, and the length check guarantees EOF = body_size bytes
        // later.
        let response = Self::build_response_from_parts(&metadata, file)?;
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

        // Collect body — the put path is fully buffered (reads stream).
        let body_bytes = body
            .collect()
            .await
            .map_err(|e| StreamingError::new(e.into()))?
            .to_bytes();

        // Enforce body-size limit before writing anything to disk.
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
        // Crash-detection nonce: written at the head of the body file AND
        // stored in metadata. Any mismatch on read means the file was
        // overwritten by a put whose redb commit did not land (or by
        // external tampering).
        let nonce: [u8; NONCE_LEN] = rand::rng().random();

        // Build metadata.
        let mut headers = HttpHeaders::new();
        for (name, value) in parts.headers.iter() {
            if let Ok(value_str) = value.to_str() {
                headers
                    .append(name.as_str().to_string(), value_str.to_string());
            }
        }
        let metadata = CacheMetadata {
            status: parts.status.as_u16(),
            version: version_to_u8(parts.version),
            headers,
            body_size,
            nonce,
            policy,
            user_metadata,
        };

        // Write to tmp, sync, rename to final.
        let body_hash = body_hash_for(&cache_key);
        let tmp_suffix: u64 = rand::rng().random();
        let tmp_path =
            self.tmp_dir.join(format!("{body_hash}.{tmp_suffix:016x}.tmp"));
        let final_dir = self.body_dir.join(&body_hash[0..2]);
        tokio::fs::create_dir_all(&final_dir).await.map_err(|e| {
            crate::HttpCacheError::cache(format!(
                "Failed to create body subdir: {e}"
            ))
        })?;
        let final_path = final_dir.join(format!("{body_hash}.bin"));

        {
            let mut f =
                tokio::fs::File::create(&tmp_path).await.map_err(|e| {
                    crate::HttpCacheError::cache(format!(
                        "Failed to open tmp body file: {e}"
                    ))
                })?;
            f.write_all(&nonce).await.map_err(|e| {
                crate::HttpCacheError::cache(format!(
                    "Failed to write nonce header: {e}"
                ))
            })?;
            f.write_all(&body_bytes).await.map_err(|e| {
                crate::HttpCacheError::cache(format!(
                    "Failed to write body bytes: {e}"
                ))
            })?;
            f.sync_all().await.map_err(|e| {
                crate::HttpCacheError::cache(format!(
                    "Failed to fsync body file: {e}"
                ))
            })?;
        }
        tokio::fs::rename(&tmp_path, &final_path).await.map_err(|e| {
            crate::HttpCacheError::cache(format!(
                "Failed to rename tmp to final body file: {e}"
            ))
        })?;

        // Commit metadata to redb. If the commit fails AFTER the body file
        // has been renamed into place, roll back the rename by unlinking the
        // body file so we don't leak disk space on an orphaned body.
        let db = self.db.clone();
        let key_for_redb = cache_key.clone();
        let serialized = postcard::to_allocvec(&metadata).map_err(|e| {
            crate::HttpCacheError::cache(format!(
                "Failed to serialize metadata: {e}"
            ))
        })?;
        let commit_result: Result<()> =
            match tokio::task::spawn_blocking(move || -> Result<()> {
                let write_txn = db.begin_write().map_err(|e| {
                    crate::HttpCacheError::cache(format!(
                        "redb begin_write (put) failed: {e}"
                    ))
                })?;
                {
                    let mut table =
                        write_txn.open_table(METADATA_TABLE).map_err(|e| {
                            crate::HttpCacheError::cache(format!(
                                "redb open_table (put) failed: {e}"
                            ))
                        })?;
                    table
                        .insert(key_for_redb.as_str(), serialized.as_slice())
                        .map_err(|e| {
                            crate::HttpCacheError::cache(format!(
                                "redb insert (put) failed: {e}"
                            ))
                        })?;
                }
                write_txn.commit().map_err(|e| {
                    crate::HttpCacheError::cache(format!(
                        "redb commit (put) failed: {e}"
                    ))
                })?;
                Ok(())
            })
            .await
            {
                Ok(inner) => inner,
                Err(e) => Err(Box::new(crate::HttpCacheError::cache(format!(
                    "put join failed: {e}"
                )))),
            };
        if let Err(e) = commit_result {
            // Roll back the body file to avoid a disk leak. Best-effort.
            let _ = tokio::fs::remove_file(&final_path).await;
            return Err(e);
        }

        // Populate moka.
        self.metadata.insert(cache_key, metadata).await;

        // Return response with Buffered body (bytes are already in RAM —
        // matches the pre-refactor contract that puts return in-memory bodies).
        let mut response_builder =
            Response::builder().status(parts.status).version(parts.version);
        for (name, value) in parts.headers.iter() {
            response_builder = response_builder.header(name, value);
        }
        let return_body = StreamingBody::buffered(body_bytes);
        let mut return_response =
            response_builder.body(return_body).map_err(|e| {
                crate::HttpCacheError::cache(format!(
                    "Failed to build response: {e}"
                ))
            })?;
        *return_response.extensions_mut() = parts.extensions;

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
        let mut response =
            response_builder.body(streaming_body).map_err(|e| {
                crate::HttpCacheError::cache(format!(
                    "Failed to build response: {e}"
                ))
            })?;

        // Preserve extensions from the original response
        *response.extensions_mut() = parts.extensions;

        Ok(response)
    }

    async fn delete(&self, cache_key: &str) -> Result<()> {
        // Drop metadata first (both moka and redb).
        self.metadata.invalidate(cache_key).await;
        self.redb_remove(cache_key).await;

        // Unlink the body file (idempotent: NotFound is fine).
        let body_hash = body_hash_for(cache_key);
        let body_path = body_path_for(&self.body_dir, &body_hash);
        let _ = tokio::fs::remove_file(&body_path).await;

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
    use tempfile::TempDir;

    fn sample_policy() -> CachePolicy {
        CachePolicy::new(
            &http::Request::builder()
                .uri("https://example.com/test")
                .body(())
                .unwrap(),
            &Response::builder()
                .status(200)
                .header("cache-control", "max-age=3600")
                .body(())
                .unwrap(),
        )
    }

    fn test_url() -> Url {
        "https://example.com/test".parse().unwrap()
    }

    fn response_with_body(bytes: Bytes) -> Response<Full<Bytes>> {
        Response::builder()
            .status(StatusCode::OK)
            .header("content-type", "text/plain")
            .body(Full::new(bytes))
            .unwrap()
    }

    async fn read_body_bytes(
        resp: Response<StreamingBody<Empty<Bytes>>>,
    ) -> Bytes {
        resp.into_body().collect().await.unwrap().to_bytes()
    }

    #[tokio::test]
    async fn test_streaming_manager_basic() {
        let manager = StreamingManager::with_temp_dir(100).await.unwrap();
        let response = response_with_body(Bytes::from("Hello, World!"));

        let _stored = manager
            .put("test-key".into(), response, sample_policy(), test_url(), None)
            .await
            .unwrap();

        let (resp, _policy) = manager.get("test-key").await.unwrap().unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(read_body_bytes(resp).await, "Hello, World!");
    }

    #[tokio::test]
    async fn test_streaming_manager_delete() {
        let manager = StreamingManager::with_temp_dir(100).await.unwrap();
        let response = response_with_body(Bytes::from("test"));
        manager
            .put(
                "delete-test".into(),
                response,
                sample_policy(),
                test_url(),
                None,
            )
            .await
            .unwrap();
        assert!(manager.get("delete-test").await.unwrap().is_some());
        manager.delete("delete-test").await.unwrap();
        assert!(manager.get("delete-test").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_same_body_different_keys_both_readable() {
        let manager = StreamingManager::with_temp_dir(100).await.unwrap();
        let body = Bytes::from("Duplicate content");
        for key in ["key1", "key2"] {
            let response = response_with_body(body.clone());
            manager
                .put(key.into(), response, sample_policy(), test_url(), None)
                .await
                .unwrap();
        }
        for key in ["key1", "key2"] {
            let (resp, _) = manager.get(key).await.unwrap().unwrap();
            assert_eq!(read_body_bytes(resp).await, "Duplicate content");
        }
    }

    #[tokio::test]
    async fn test_persistence_across_restart() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().to_path_buf();

        {
            let manager =
                StreamingManager::new(path.clone(), 100).await.unwrap();
            for (k, body) in [("a", "body-a"), ("b", "body-b"), ("c", "body-c")]
            {
                manager
                    .put(
                        k.into(),
                        response_with_body(Bytes::copy_from_slice(
                            body.as_bytes(),
                        )),
                        sample_policy(),
                        test_url(),
                        None,
                    )
                    .await
                    .unwrap();
            }
            drop(manager);
        }

        let manager = StreamingManager::new(path.clone(), 100).await.unwrap();
        for (k, body) in [("a", "body-a"), ("b", "body-b"), ("c", "body-c")] {
            let (resp, _) = manager.get(k).await.unwrap().unwrap();
            assert_eq!(resp.status(), StatusCode::OK);
            assert_eq!(read_body_bytes(resp).await, body);
        }
    }

    #[tokio::test]
    async fn test_persistence_preserves_policy_and_user_metadata() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().to_path_buf();

        let user_meta = vec![1u8, 2, 3, 4, 5];
        let policy = sample_policy();

        {
            let manager =
                StreamingManager::new(path.clone(), 100).await.unwrap();
            manager
                .put(
                    "k".into(),
                    response_with_body(Bytes::from("body")),
                    policy.clone(),
                    test_url(),
                    Some(user_meta.clone()),
                )
                .await
                .unwrap();
            drop(manager);
        }

        let manager = StreamingManager::new(path, 100).await.unwrap();
        let (resp, restored_policy) = manager.get("k").await.unwrap().unwrap();

        // user_metadata available via response extension
        let got = resp.extensions().get::<CachedUserMetadata>().unwrap();
        assert_eq!(got.0.as_ref().unwrap(), &user_meta);

        // policy round-trips (same responses to a clone request stay fresh).
        // Compare at a shared instant so microsecond drift between two
        // SystemTime::now() calls doesn't make the assertion flaky.
        let now = std::time::SystemTime::now();
        assert_eq!(restored_policy.time_to_live(now), policy.time_to_live(now));
    }

    #[tokio::test]
    async fn test_delete_persists_across_restart() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().to_path_buf();
        {
            let manager =
                StreamingManager::new(path.clone(), 100).await.unwrap();
            manager
                .put(
                    "k".into(),
                    response_with_body(Bytes::from("body")),
                    sample_policy(),
                    test_url(),
                    None,
                )
                .await
                .unwrap();
            manager.delete("k").await.unwrap();
        }
        let manager = StreamingManager::new(path, 100).await.unwrap();
        assert!(manager.get("k").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_overwrite_replaces_body() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().to_path_buf();
        let manager = StreamingManager::new(path.clone(), 100).await.unwrap();

        manager
            .put(
                "k".into(),
                response_with_body(Bytes::from("first")),
                sample_policy(),
                test_url(),
                None,
            )
            .await
            .unwrap();
        manager
            .put(
                "k".into(),
                response_with_body(Bytes::from("second-body")),
                sample_policy(),
                test_url(),
                None,
            )
            .await
            .unwrap();

        let (resp, _) = manager.get("k").await.unwrap().unwrap();
        assert_eq!(read_body_bytes(resp).await, "second-body");

        drop(manager);
        let manager = StreamingManager::new(path, 100).await.unwrap();
        let (resp, _) = manager.get("k").await.unwrap().unwrap();
        assert_eq!(read_body_bytes(resp).await, "second-body");
    }

    #[tokio::test]
    async fn test_overwrite_does_not_leak_prior_content() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().to_path_buf();
        let manager = StreamingManager::new(path, 100).await.unwrap();

        manager
            .put(
                "k".into(),
                response_with_body(Bytes::from("first")),
                sample_policy(),
                test_url(),
                None,
            )
            .await
            .unwrap();
        manager
            .put(
                "k".into(),
                response_with_body(Bytes::from("second")),
                sample_policy(),
                test_url(),
                None,
            )
            .await
            .unwrap();

        let body_hash = body_hash_for("k");
        let prefix_dir = manager.body_dir.join(&body_hash[0..2]);
        let mut rd = tokio::fs::read_dir(&prefix_dir).await.unwrap();
        let mut count = 0usize;
        while let Some(entry) = rd.next_entry().await.unwrap() {
            if entry.path().extension().map(|s| s == "bin").unwrap_or(false) {
                count += 1;
            }
        }
        assert_eq!(count, 1, "expected exactly one body file for the key");
    }

    #[tokio::test]
    async fn test_delete_removes_body_file() {
        let manager = StreamingManager::with_temp_dir(100).await.unwrap();
        manager
            .put(
                "k".into(),
                response_with_body(Bytes::from("body")),
                sample_policy(),
                test_url(),
                None,
            )
            .await
            .unwrap();
        manager.delete("k").await.unwrap();

        let body_hash = body_hash_for("k");
        let body_path = body_path_for(&manager.body_dir, &body_hash);
        assert!(!body_path.exists());
    }

    #[tokio::test]
    async fn test_missing_body_self_heals_fast_path() {
        let manager = StreamingManager::with_temp_dir(100).await.unwrap();
        manager
            .put(
                "k".into(),
                response_with_body(Bytes::from("body")),
                sample_policy(),
                test_url(),
                None,
            )
            .await
            .unwrap();

        // moka still holds the metadata; unlink the body file out-of-band.
        let body_path = body_path_for(&manager.body_dir, &body_hash_for("k"));
        tokio::fs::remove_file(&body_path).await.unwrap();

        assert!(manager.get("k").await.unwrap().is_none());

        // Should also be removed from redb so a fresh manager at the same
        // path doesn't find it either.
        assert!(manager.redb_get("k").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_missing_body_self_heals_slow_path() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().to_path_buf();
        let manager = StreamingManager::new(path, 100).await.unwrap();
        manager
            .put(
                "k".into(),
                response_with_body(Bytes::from("body")),
                sample_policy(),
                test_url(),
                None,
            )
            .await
            .unwrap();

        // Force a moka miss so get goes via the slow (redb) path.
        manager.metadata.invalidate("k").await;
        manager.metadata.run_pending_tasks().await;

        let body_path = body_path_for(&manager.body_dir, &body_hash_for("k"));
        tokio::fs::remove_file(&body_path).await.unwrap();

        assert!(manager.get("k").await.unwrap().is_none());
        assert!(manager.redb_get("k").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_corrupt_metadata_entry_is_skipped_and_removed() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().to_path_buf();

        // Create manager so directories + db exist; insert a good entry
        // plus a poisoned one directly.
        {
            let manager =
                StreamingManager::new(path.clone(), 100).await.unwrap();
            manager
                .put(
                    "good".into(),
                    response_with_body(Bytes::from("ok")),
                    sample_policy(),
                    test_url(),
                    None,
                )
                .await
                .unwrap();

            // Poisoned row: raw bytes that won't deserialize as CacheMetadata.
            let db = manager.db.clone();
            tokio::task::spawn_blocking(move || {
                let write_txn = db.begin_write().unwrap();
                {
                    let mut table =
                        write_txn.open_table(METADATA_TABLE).unwrap();
                    table.insert("bad", &vec![0xFFu8; 8][..]).unwrap();
                }
                write_txn.commit().unwrap();
            })
            .await
            .unwrap();
        }

        // Reopen: startup should tolerate the poisoned row.
        let manager = StreamingManager::new(path, 100).await.unwrap();
        assert!(manager.get("bad").await.unwrap().is_none());
        // The startup cleanup should have removed the poisoned row.
        assert!(manager.redb_get("bad").await.unwrap().is_none());
        // Good row still loadable.
        let (resp, _) = manager.get("good").await.unwrap().unwrap();
        assert_eq!(read_body_bytes(resp).await, "ok");
    }

    #[tokio::test]
    async fn test_startup_sweeps_tmp_dir() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().to_path_buf();

        // First construction: ensures the layout exists.
        {
            let _manager =
                StreamingManager::new(path.clone(), 100).await.unwrap();
        }

        // Write a stale tmp file.
        let tmp_dir = path.join("tmp");
        tokio::fs::write(tmp_dir.join("stale.tmp"), b"stale").await.unwrap();

        // Reopen — sweep should drop the stale file.
        let _manager = StreamingManager::new(path, 100).await.unwrap();
        let mut rd = tokio::fs::read_dir(&tmp_dir).await.unwrap();
        assert!(rd.next_entry().await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_lazy_load_on_capacity_overflow() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().to_path_buf();

        {
            let manager = StreamingManager::new(path.clone(), 2).await.unwrap();
            for i in 0..5 {
                manager
                    .put(
                        format!("k{i}"),
                        response_with_body(Bytes::from(format!("body{i}"))),
                        sample_policy(),
                        test_url(),
                        None,
                    )
                    .await
                    .unwrap();
                // Let moka's TinyLFU settle between puts without requiring
                // tokio's `time` feature (not in the streaming feature gate).
                manager.metadata.run_pending_tasks().await;
            }
        }

        // Restart: moka reloads up to capacity (2).
        let manager = StreamingManager::new(path, 2).await.unwrap();
        manager.metadata.run_pending_tasks().await;
        assert!(manager.entry_count() <= 2);

        // All 5 entries must still be reachable (some via slow path).
        for i in 0..5 {
            let (resp, _) =
                manager.get(&format!("k{i}")).await.unwrap().unwrap();
            let body = read_body_bytes(resp).await;
            assert_eq!(body, format!("body{i}"));
        }
    }

    #[tokio::test]
    async fn test_concurrent_put_different_keys() {
        let manager =
            Arc::new(StreamingManager::with_temp_dir(100).await.unwrap());
        let mut tasks = Vec::new();
        for i in 0..4 {
            let m = manager.clone();
            tasks.push(tokio::spawn(async move {
                m.put(
                    format!("k{i}"),
                    response_with_body(Bytes::from(format!("body{i}"))),
                    sample_policy(),
                    test_url(),
                    None,
                )
                .await
                .unwrap();
            }));
        }
        for t in tasks {
            t.await.unwrap();
        }
        for i in 0..4 {
            let (resp, _) =
                manager.get(&format!("k{i}")).await.unwrap().unwrap();
            assert_eq!(read_body_bytes(resp).await, format!("body{i}"));
        }
    }

    #[tokio::test]
    async fn test_concurrent_put_same_key() {
        let manager =
            Arc::new(StreamingManager::with_temp_dir(100).await.unwrap());
        let m1 = manager.clone();
        let m2 = manager.clone();
        let t1 = tokio::spawn(async move {
            m1.put(
                "k".into(),
                response_with_body(Bytes::from("aaa")),
                sample_policy(),
                test_url(),
                None,
            )
            .await
            .unwrap();
        });
        let t2 = tokio::spawn(async move {
            m2.put(
                "k".into(),
                response_with_body(Bytes::from("bbb")),
                sample_policy(),
                test_url(),
                None,
            )
            .await
            .unwrap();
        });
        t1.await.unwrap();
        t2.await.unwrap();

        // Concurrent puts to the same key can interleave such that the body
        // file and the committed metadata.nonce end up describing different
        // writes. When that happens, `get` correctly self-heals to `None`
        // rather than returning corrupt data. Either outcome is acceptable:
        //   - `Some(body)` where body is one of the two values committed
        //   - `None` (self-healed due to nonce/length mismatch)
        // The invariants we care about: no panic, no corruption returned,
        // and at most one body file left on disk for the key.
        if let Some((resp, _)) = manager.get("k").await.unwrap() {
            let body = read_body_bytes(resp).await;
            assert!(body == "aaa" || body == "bbb", "got {body:?}");
        }

        // At most one body file under the key's prefix (self-heal may have
        // removed it; the non-racy path leaves exactly one).
        let prefix_dir = manager.body_dir.join(&body_hash_for("k")[0..2]);
        let mut count = 0usize;
        if prefix_dir.exists() {
            let mut rd = tokio::fs::read_dir(&prefix_dir).await.unwrap();
            while rd.next_entry().await.unwrap().is_some() {
                count += 1;
            }
        }
        assert!(count <= 1, "expected at most one body file, got {count}");
    }

    #[tokio::test]
    async fn test_max_body_size_rejection() {
        let tmp = TempDir::new().unwrap();
        let manager = StreamingManager::with_max_body_size(
            tmp.path().to_path_buf(),
            100,
            10,
        )
        .await
        .unwrap();

        let err = manager
            .put(
                "k".into(),
                response_with_body(Bytes::from("this body exceeds the limit")),
                sample_policy(),
                test_url(),
                None,
            )
            .await
            .unwrap_err();
        assert!(err.to_string().to_lowercase().contains("exceeds"));

        // No redb row, no body file.
        assert!(manager.redb_get("k").await.unwrap().is_none());
        let body_path = body_path_for(&manager.body_dir, &body_hash_for("k"));
        assert!(!body_path.exists());
    }

    #[tokio::test]
    async fn test_clear_wipes_everything() {
        let manager = StreamingManager::with_temp_dir(100).await.unwrap();
        for i in 0..3 {
            manager
                .put(
                    format!("k{i}"),
                    response_with_body(Bytes::from(format!("body{i}"))),
                    sample_policy(),
                    test_url(),
                    None,
                )
                .await
                .unwrap();
        }
        manager.clear().await.unwrap();
        manager.run_pending_tasks().await;

        for i in 0..3 {
            assert!(manager.get(&format!("k{i}")).await.unwrap().is_none());
        }
        // Body directory has no .bin files.
        let mut rd = tokio::fs::read_dir(&manager.body_dir).await.unwrap();
        while let Some(entry) = rd.next_entry().await.unwrap() {
            if entry.file_type().await.unwrap().is_dir() {
                let mut inner =
                    tokio::fs::read_dir(entry.path()).await.unwrap();
                while let Some(e2) = inner.next_entry().await.unwrap() {
                    if e2
                        .path()
                        .extension()
                        .map(|s| s == "bin")
                        .unwrap_or(false)
                    {
                        panic!(
                            "unexpected body file after clear: {:?}",
                            e2.path()
                        );
                    }
                }
            }
        }
    }

    #[tokio::test]
    async fn test_streaming_body_is_backed_by_file() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().to_path_buf();
        {
            let manager =
                StreamingManager::new(path.clone(), 100).await.unwrap();
            let big = Bytes::from(vec![0u8; 1024 * 1024]);
            manager
                .put(
                    "big".into(),
                    response_with_body(big),
                    sample_policy(),
                    test_url(),
                    None,
                )
                .await
                .unwrap();
        }

        // Re-open and verify the returned body is the File variant (streaming).
        let manager = StreamingManager::new(path, 100).await.unwrap();
        let (resp, _) = manager.get("big").await.unwrap().unwrap();
        match resp.into_body() {
            StreamingBody::File { size, .. } => {
                assert_eq!(size, 1024 * 1024);
            }
            other => {
                panic!("expected StreamingBody::File, got {other:?}");
            }
        }
    }

    #[tokio::test]
    async fn test_body_size_mismatch_self_heals() {
        let manager = StreamingManager::with_temp_dir(100).await.unwrap();
        manager
            .put(
                "k".into(),
                response_with_body(Bytes::from("abcdef")),
                sample_policy(),
                test_url(),
                None,
            )
            .await
            .unwrap();

        // Overwrite body file with a different length (breaks length check).
        let body_path = body_path_for(&manager.body_dir, &body_hash_for("k"));
        tokio::fs::write(&body_path, vec![0xAAu8; 100]).await.unwrap();

        assert!(manager.get("k").await.unwrap().is_none());
        assert!(manager.redb_get("k").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_nonce_mismatch_self_heals() {
        let manager = StreamingManager::with_temp_dir(100).await.unwrap();
        manager
            .put(
                "k".into(),
                response_with_body(Bytes::from("abcdef")),
                sample_policy(),
                test_url(),
                None,
            )
            .await
            .unwrap();

        // Overwrite with identical length but different nonce + garbage body.
        let body_path = body_path_for(&manager.body_dir, &body_hash_for("k"));
        let mut fake = vec![0x11u8; NONCE_LEN];
        fake.extend_from_slice(b"abcdef");
        tokio::fs::write(&body_path, &fake).await.unwrap();

        assert!(manager.get("k").await.unwrap().is_none());
        assert!(manager.redb_get("k").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_second_instance_fails_construction() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().to_path_buf();

        let first = StreamingManager::new(path.clone(), 100).await.unwrap();
        let second = StreamingManager::new(path.clone(), 100).await;
        assert!(
            second.is_err(),
            "second construction must fail while first is alive"
        );
        drop(first);

        // After dropping the first, another instance can be constructed.
        let _third = StreamingManager::new(path, 100).await.unwrap();
    }

    #[tokio::test]
    async fn test_in_memory_variant_still_delegates_to_temp_dir() {
        #[allow(deprecated)]
        let manager = StreamingManager::in_memory(10).await.unwrap();
        // Temp-dir subdirectory should exist on disk.
        assert!(manager.cache_dir().exists());
        assert!(manager.body_dir.exists());
        assert!(manager.tmp_dir.exists());
    }

    #[tokio::test]
    async fn test_put_returns_buffered_variant() {
        let manager = StreamingManager::with_temp_dir(100).await.unwrap();
        let resp = manager
            .put(
                "k".into(),
                response_with_body(Bytes::from("body")),
                sample_policy(),
                test_url(),
                None,
            )
            .await
            .unwrap();
        match resp.into_body() {
            StreamingBody::Buffered { .. } => {}
            other => panic!("expected Buffered variant, got {other:?}"),
        }
    }
}
