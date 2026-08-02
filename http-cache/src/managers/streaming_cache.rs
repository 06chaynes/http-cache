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
//! regardless of response size. Writes (`put`) spool each frame straight to a
//! tmp file and flush before pulling the next, so at most one frame is in RAM.
//! `max_body_size` is a decline, not an error: an over-cap response is not
//! cached but is still served in full. On success `put` returns the `File`
//! variant, served from the committed on-disk body.
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
    sync::{Arc, Weak},
};

use crate::{
    body::StreamingBody,
    error::{Result, StreamingError},
    HttpHeaders, StreamingCacheManager, Url,
};
use bytes::{Buf, Bytes};
use http::{Response, Version};
use http_body::Body;
use http_body_util::{combinators::UnsyncBoxBody, BodyExt};
use http_cache_semantics::CachePolicy;
use moka::future::Cache;
use rand::RngExt;
use redb::{Database, ReadableDatabase, TableDefinition};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use tokio::sync::RwLock;

use crate::CachedUserMetadata;

/// Default maximum body size for cached responses (100MB).
///
/// Responses larger than this are not cached — not an error: caching is
/// declined and the body streams through to the caller uncached. Configure
/// with [`StreamingManager::with_max_body_size`].
pub const DEFAULT_MAX_BODY_SIZE: u64 = 100 * 1024 * 1024;

/// Size of the 16-byte nonce header prepended to every body file on disk.
const NONCE_LEN: usize = 16;

/// The concrete body type used by [`StreamingManager`]. Non-cacheable
/// responses (see [`StreamingCacheManager::convert_body`]) are passed
/// through as `Streaming` bodies boxed via `boxed_unsync` — `UnsyncBoxBody`
/// only requires `Send + 'static`, not `Sync`, matching the bound on
/// `convert_body`'s generic `B`.
type ManagerBody = StreamingBody<UnsyncBoxBody<Bytes, StreamingError>>;

/// Number of striped per-key lock shards. Bounded, so no lock-map cleanup is
/// needed; shard index is derived from the body-hash prefix.
const KEY_LOCK_SHARDS: usize = 64;

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
    /// blake3 hash of the body bytes; verified while streaming on read.
    checksum: [u8; 32],
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
    /// Striped per-key locks serializing put/get-validate/delete for a key.
    /// redb's file lock makes cross-process access impossible, so
    /// intra-process striping is sufficient.
    key_locks: Arc<Vec<RwLock<()>>>,
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
    /// * `max_body_size` - Maximum body size in bytes (responses larger than this are not
    ///   cached — caching is declined and the body still streams through to the caller)
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
        // Hydration is lazy: get() falls through to redb on a moka miss and
        // self-heals poisoned rows there.
        let metadata: Cache<String, CacheMetadata> =
            Cache::builder().max_capacity(capacity).build();

        let key_locks =
            Arc::new((0..KEY_LOCK_SHARDS).map(|_| RwLock::new(())).collect());

        Ok(Self {
            cache_dir,
            body_dir,
            tmp_dir,
            db,
            metadata,
            max_body_size,
            key_locks,
        })
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
    /// Note: this is not the total number of persisted entries. Hydration
    /// is lazy, so this reads 0 after a restart until keys are accessed,
    /// and once `capacity` is exceeded, cold entries remain on disk
    /// (reachable via `get`) but are not counted here.
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
        recreate_dir(&self.body_dir).await.map_err(|e| {
            crate::HttpCacheError::cache(format!(
                "Failed to recreate body directory: {e}"
            ))
        })?;
        recreate_dir(&self.tmp_dir).await.map_err(|e| {
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
        redb_remove_row(self.db.clone(), cache_key.to_string()).await;
    }

    /// Drop an entry from all three tiers (moka, redb, body file). Used by
    /// the read-path self-heal branches and by delete.
    async fn self_heal(&self, cache_key: &str, body_path: &Path) {
        self.metadata.invalidate(cache_key).await;
        self.redb_remove(cache_key).await;
        let _ = tokio::fs::remove_file(body_path).await;
    }

    /// Return the striped shard lock for the given body hash: get() takes
    /// it shared, put/delete/heal take it exclusive. Callers must not
    /// acquire a second shard lock while already holding one.
    fn key_lock(&self, body_hash: &str) -> &RwLock<()> {
        &self.key_locks[shard_index(body_hash)]
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
        &self,
        cache_key: &str,
        body_hash: &str,
        body_path: &Path,
        metadata: &CacheMetadata,
        file: tokio::fs::File,
    ) -> Result<Response<ManagerBody>> {
        let mut response_builder = Response::builder()
            .status(metadata.status)
            .version(version_from_u8(metadata.version));
        for (name, value) in metadata.headers.iter() {
            response_builder =
                response_builder.header(name.as_str(), value.as_str());
        }
        let heal = CorruptHeal {
            db: Arc::downgrade(&self.db),
            metadata: self.metadata.clone(),
            key_locks: self.key_locks.clone(),
            key: cache_key.to_string(),
            body_hash: body_hash.to_string(),
            body_path: body_path.to_path_buf(),
            nonce: metadata.nonce,
        };
        let body = StreamingBody::from_file_verified(
            file,
            metadata.body_size,
            metadata.checksum,
            move || {
                tokio::spawn(heal.run());
            },
        );
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
        response
            .extensions_mut()
            .insert(crate::CacheEntryToken(metadata.nonce.to_vec()));
        Ok(response)
    }
}

/// Write to the spool. `tokio::fs::File` defers write errors to the next
/// operation and `sync_all` discards them, so this flush is required.
async fn spool_write<W: tokio::io::AsyncWrite + Unpin>(
    writer: &mut W,
    data: &[u8],
) -> std::io::Result<()> {
    writer.write_all(data).await?;
    writer.flush().await
}

/// Unlinks the spool tmp file on drop unless `defuse()` was called
/// (defused = the file was renamed into its final location). In structs that
/// also own the spool `File`, declare the guard AFTER the file field so the
/// handle closes first (Windows unlink-while-open).
struct TmpGuard {
    path: PathBuf,
    defused: bool,
}

impl TmpGuard {
    fn new(path: PathBuf) -> Self {
        Self { path, defused: false }
    }
    fn defuse(&mut self) {
        self.defused = true;
    }
}

impl Drop for TmpGuard {
    fn drop(&mut self) {
        if !self.defused {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

/// Single agreed Content-Length, if one exists. Multiple disagreeing or
/// unparseable values are treated as unknown length (RFC 9110 §8.6) so the
/// running spool counter stays authoritative.
fn parse_content_length(headers: &http::HeaderMap) -> Option<u64> {
    let mut iter = headers.get_all(http::header::CONTENT_LENGTH).iter();
    let first = iter.next()?.to_str().ok()?.trim().parse::<u64>().ok()?;
    for v in iter {
        if v.to_str().ok()?.trim().parse::<u64>().ok()? != first {
            return None;
        }
    }
    Some(first)
}

/// Hop-by-hop headers must not be replayed from cache (RFC 9111 §3.1):
/// the stored body is already transfer-decoded, so replaying e.g.
/// `transfer-encoding: chunked` would be a lie.
const HOP_BY_HOP: &[&str] = &[
    "connection",
    "keep-alive",
    "proxy-authenticate",
    "proxy-authorization",
    "proxy-connection",
    "te",
    "trailer",
    "transfer-encoding",
    "upgrade",
];

/// Header set persisted into `CacheMetadata`: hop-by-hop stripped —
/// including any field names nominated by `Connection` header values
/// (RFC 9111 §3.1) — multi-valued preserved, non-UTF-8 values skipped
/// (unchanged policy).
fn stored_headers(headers: &http::HeaderMap) -> HttpHeaders {
    // Field names listed inside Connection values are hop-by-hop too.
    let nominated: Vec<String> = headers
        .get_all(http::header::CONNECTION)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(','))
        .map(|t| t.trim().to_ascii_lowercase())
        .filter(|t| !t.is_empty())
        .collect();

    let mut out = HttpHeaders::new();
    for (name, value) in headers.iter() {
        // HeaderName::as_str() is always lower case.
        let name = name.as_str();
        if HOP_BY_HOP.contains(&name) || nominated.iter().any(|n| n == name) {
            continue;
        }
        if let Ok(value_str) = value.to_str() {
            out.append(name.to_string(), value_str.to_string());
        }
    }
    out
}

fn shard_index(body_hash: &str) -> usize {
    usize::from_str_radix(&body_hash[..2], 16).unwrap_or(0) % KEY_LOCK_SHARDS
}

/// Remove a metadata row. Errors are swallowed; used by self-heal paths.
async fn redb_remove_row(db: Arc<Database>, key: String) {
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

/// Read a row's nonce; any failure reads as `None`. Used by the deferred
/// corrupt-heal identity check.
async fn redb_read_nonce(
    db: Arc<Database>,
    key: String,
) -> Option<[u8; NONCE_LEN]> {
    tokio::task::spawn_blocking(move || {
        let read_txn = db.begin_read().ok()?;
        let table = read_txn.open_table(METADATA_TABLE).ok()?;
        let guard = table.get(key.as_str()).ok()??;
        postcard::from_bytes::<CacheMetadata>(guard.value())
            .ok()
            .map(|m| m.nonce)
    })
    .await
    .ok()
    .flatten()
}

/// Deferred heal for a corrupt streamed body. Holds a weak database handle
/// so an in-flight body does not keep the redb file lock alive, and the
/// nonce observed at read time so it never deletes an entry written after
/// the corrupt read.
struct CorruptHeal {
    db: Weak<Database>,
    metadata: Cache<String, CacheMetadata>,
    key_locks: Arc<Vec<RwLock<()>>>,
    key: String,
    body_hash: String,
    body_path: PathBuf,
    nonce: [u8; NONCE_LEN],
}

impl CorruptHeal {
    async fn run(self) {
        let Some(db) = self.db.upgrade() else {
            return;
        };
        let _guard = self.key_locks[shard_index(&self.body_hash)].write().await;
        let current = match self.metadata.get(&self.key).await {
            Some(m) => Some(m.nonce),
            None => redb_read_nonce(db.clone(), self.key.clone()).await,
        };
        if current != Some(self.nonce) {
            return;
        }
        self.metadata.invalidate(&self.key).await;
        redb_remove_row(db, self.key).await;
        let _ = tokio::fs::remove_file(&self.body_path).await;
    }
}

/// Remove and recreate `dir`, tolerating the `AlreadyExists` that
/// `create_dir_all` can return when it races a concurrent removal.
async fn recreate_dir(dir: &Path) -> std::io::Result<()> {
    let _ = tokio::fs::remove_dir_all(dir).await;
    match tokio::fs::create_dir_all(dir).await {
        Err(e) if e.kind() != std::io::ErrorKind::AlreadyExists => Err(e),
        _ => Ok(()),
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

pin_project_lite::pin_project! {
    /// Pass-through stream that owns the spool TmpGuard, tying the tmp
    /// file's lifetime to the caller's body. Field order: `inner` (which
    /// owns the File inside its StreamingBody) before `guard`, so the
    /// handle closes before the unlink attempt.
    struct GuardedStream<S> {
        #[pin]
        inner: S,
        guard: TmpGuard,
    }
}

impl<S: futures_util::Stream> futures_util::Stream for GuardedStream<S> {
    type Item = S::Item;
    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        self.project().inner.poll_next(cx)
    }
}

/// Serve an uncached response assembled from a partially- or fully-spooled
/// tmp file plus whatever of the upstream body remains. Used when put()
/// declines mid-flight: the caller still receives every byte, nothing is
/// cached, and the tmp file is unlinked when the body is dropped.
///
/// `file`'s cursor position on entry is irrelevant: the stream seeks past
/// the nonce before reading. `written` is the count of flush-confirmed
/// spooled body bytes — the size-enforced File body reads exactly that
/// many, so partial trailing writes are ignored.
fn serve_uncached_spooled(
    parts: http::response::Parts,
    file: tokio::fs::File,
    guard: TmpGuard,
    written: u64,
    pending: Option<Bytes>,
    rest: Option<UnsyncBoxBody<Bytes, StreamingError>>,
) -> Result<Response<ManagerBody>> {
    use futures_util::{StreamExt, TryStreamExt};
    use http_body_util::{BodyStream, StreamBody};

    // The seek is deferred into the stream so this fn stays sync.
    let prefix = futures_util::stream::once(async move {
        let mut file = file;
        file.seek(std::io::SeekFrom::Start(NONCE_LEN as u64))
            .await
            .map_err(|e| StreamingError::new(Box::new(e)))?;
        Ok::<_, StreamingError>(BodyStream::new(StreamingBody::<
            UnsyncBoxBody<Bytes, StreamingError>,
        >::from_file_with_size(
            file, written
        )))
    })
    .try_flatten();

    let pending_stream = futures_util::stream::iter(
        pending.into_iter().map(|b| Ok(http_body::Frame::data(b))),
    );

    let rest_stream = rest
        .map(BodyStream::new)
        .map(StreamExt::left_stream)
        .unwrap_or_else(|| futures_util::stream::empty().right_stream());

    let chained = prefix.chain(pending_stream).chain(rest_stream);
    let body = StreamBody::new(GuardedStream { inner: chained, guard });
    // Extensions must survive every decline path: reqwest reads the final
    // URL back out of them.
    Ok(Response::from_parts(
        parts,
        StreamingBody::streaming(body.boxed_unsync()),
    ))
}

impl StreamingCacheManager for StreamingManager {
    type Body = ManagerBody;

    async fn get(
        &self,
        cache_key: &str,
    ) -> Result<Option<(Response<Self::Body>, CachePolicy)>>
    where
        <Self::Body as Body>::Data: Send,
        <Self::Body as Body>::Error:
            Into<StreamingError> + Send + Sync + 'static,
    {
        let body_hash = body_hash_for(cache_key);
        let body_path = body_path_for(&self.body_dir, &body_hash);
        let _guard = self.key_lock(&body_hash).read().await;

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

        // Open the body file. NotFound self-heals as a miss.
        let mut file = match tokio::fs::File::open(&body_path).await {
            Ok(f) => f,
            Err(ref e) if e.kind() == std::io::ErrorKind::NotFound => {
                self.self_heal(cache_key, &body_path).await;
                return Ok(None);
            }
            Err(e) => {
                // Other open errors (permissions, transient I/O) read as a
                // miss without deleting the entry.
                log::debug!(
                    "body file open failed for {cache_key}; treating as \
                     miss: {e}"
                );
                return Ok(None);
            }
        };

        // Length check: file must be exactly nonce header + body_size.
        let file_len = match file.metadata().await {
            Ok(m) => m.len(),
            Err(e) => {
                log::debug!(
                    "body file stat failed for {cache_key}; self-healing: {e}"
                );
                self.self_heal(cache_key, &body_path).await;
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
            self.self_heal(cache_key, &body_path).await;
            return Ok(None);
        }

        // Nonce check: read the 16-byte header and compare to metadata.
        let mut nonce_buf = [0u8; NONCE_LEN];
        if let Err(e) = file.read_exact(&mut nonce_buf).await {
            log::debug!(
                "body-file nonce read failed for {cache_key}; self-healing: {e}"
            );
            drop(file);
            self.self_heal(cache_key, &body_path).await;
            return Ok(None);
        }
        if nonce_buf != metadata.nonce {
            log::debug!(
                "nonce mismatch for {cache_key}; self-healing (overwrite-crash \
                 window or tampering)"
            );
            drop(file);
            self.self_heal(cache_key, &body_path).await;
            return Ok(None);
        }

        // File cursor is now at offset NONCE_LEN; the body streams exactly
        // body_size bytes from here, verified against the stored checksum.
        let response = self.build_response_from_parts(
            cache_key, &body_hash, &body_path, &metadata, file,
        )?;
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

        // Normalize the incoming body once: Data -> Bytes, Error ->
        // StreamingError, boxed so decline paths can hand it back out.
        let mut inner: UnsyncBoxBody<Bytes, StreamingError> = body
            .map_frame(|frame| {
                frame.map_data(|mut d| d.copy_to_bytes(d.remaining()))
            })
            .map_err(Into::into)
            .boxed_unsync();

        // HEAD responses carry the entity's Content-Length with an empty
        // body (RFC 9110 §8.6): exempt them from the CL-based size and
        // completeness checks below.
        let is_head = parts
            .extensions
            .get::<crate::CachedRequestMethod>()
            .is_some_and(|m| m.0 == http::Method::HEAD);

        // Decline before the stream is touched.
        let content_length = parse_content_length(&parts.headers);
        if self.max_body_size == 0
            || (!is_head
                && content_length.is_some_and(|cl| cl > self.max_body_size))
        {
            return Ok(Response::from_parts(
                parts,
                StreamingBody::streaming(inner),
            ));
        }

        // Spool setup. Failures here degrade to pass-through; the stream has
        // not been polled yet, so nothing is lost.
        let body_hash = body_hash_for(&cache_key);
        let tmp_suffix: u64 = rand::rng().random();
        let tmp_path =
            self.tmp_dir.join(format!("{body_hash}.{tmp_suffix:016x}.tmp"));
        let final_dir = self.body_dir.join(&body_hash[0..2]);
        if let Err(e) = tokio::fs::create_dir_all(&final_dir).await {
            log::debug!(
                "put: create body subdir failed; serving uncached: {e}"
            );
            return Ok(Response::from_parts(
                parts,
                StreamingBody::streaming(inner),
            ));
        }
        let final_path = final_dir.join(format!("{body_hash}.bin"));
        let nonce: [u8; NONCE_LEN] = rand::rng().random();

        // read+write: the same handle serves the response after commit.
        let mut file = match tokio::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&tmp_path)
            .await
        {
            Ok(f) => f,
            Err(e) => {
                log::debug!("put: open tmp failed; serving uncached: {e}");
                return Ok(Response::from_parts(
                    parts,
                    StreamingBody::streaming(inner),
                ));
            }
        };
        // Construct the guard only after `create_new` succeeded: on EEXIST
        // the tmp file belongs to another in-flight put, not to us.
        let mut guard = TmpGuard::new(tmp_path.clone());
        if let Err(e) = spool_write(&mut file, &nonce).await {
            log::debug!("put: nonce write failed; serving uncached: {e}");
            drop(file); // close before the guard unlinks
            return Ok(Response::from_parts(
                parts,
                StreamingBody::streaming(inner),
            ));
        }

        // Spool loop: exactly one frame in flight. The size check precedes
        // the write so the tmp file never exceeds max_body_size, and
        // `written` counts flush-confirmed bytes (see `spool_write`).
        let mut hasher = blake3::Hasher::new();
        let mut written: u64 = 0;
        loop {
            match inner.frame().await {
                None => break,
                Some(Err(e)) => {
                    // Upstream failed: fail the request; the guard unlinks
                    // the tmp file.
                    drop(file);
                    return Err(Box::new(e));
                }
                Some(Ok(frame)) => {
                    let Ok(data) = frame.into_data() else {
                        // Trailer frame: the cache format has no trailers.
                        continue;
                    };
                    if written + data.len() as u64 > self.max_body_size {
                        // Unknown-length overflow: decline, keep serving.
                        return serve_uncached_spooled(
                            parts,
                            file,
                            guard,
                            written,
                            Some(data),
                            Some(inner),
                        );
                    }
                    if let Err(e) = spool_write(&mut file, &data).await {
                        log::debug!(
                            "put: spool write failed; serving uncached: {e}"
                        );
                        return serve_uncached_spooled(
                            parts,
                            file,
                            guard,
                            written,
                            Some(data),
                            Some(inner),
                        );
                    }
                    hasher.update(&data);
                    written += data.len() as u64;
                }
            }
        }

        // RFC 9111 §3.3: never store a response we know is incomplete.
        if !is_head {
            if let Some(cl) = content_length {
                if cl != written {
                    log::debug!(
                        "put: content-length {cl} != received {written}; \
                         serving uncached (incomplete response)"
                    );
                    return serve_uncached_spooled(
                        parts, file, guard, written, None, None,
                    );
                }
            }
        }
        if let Err(e) = file.sync_all().await {
            log::debug!("put: fsync failed; serving uncached: {e}");
            return serve_uncached_spooled(
                parts, file, guard, written, None, None,
            );
        }

        let metadata = CacheMetadata {
            status: parts.status.as_u16(),
            version: version_to_u8(parts.version),
            headers: stored_headers(&parts.headers),
            body_size: written,
            nonce,
            checksum: *hasher.finalize().as_bytes(),
            policy,
            user_metadata,
        };
        let checksum = metadata.checksum;

        // Serialization failure is a cache-side failure: decline, don't
        // fail the request.
        let serialized = match postcard::to_allocvec(&metadata) {
            Ok(s) => s,
            Err(e) => {
                log::debug!(
                    "put: metadata serialization failed; serving uncached: {e}"
                );
                return serve_uncached_spooled(
                    parts, file, guard, written, None, None,
                );
            }
        };

        // Publish under the shard write lock: rename -> redb commit -> moka.
        {
            let _guard_lock = self.key_lock(&body_hash).write().await;
            if let Err(e) = tokio::fs::rename(&tmp_path, &final_path).await {
                log::debug!("put: rename failed; serving uncached: {e}");
                return serve_uncached_spooled(
                    parts, file, guard, written, None, None,
                );
            }
            guard.defuse(); // tmp path no longer exists

            let db = self.db.clone();
            let key_for_redb = cache_key.clone();
            let commit_result: Result<()> =
                match tokio::task::spawn_blocking(move || -> Result<()> {
                    let write_txn = db.begin_write().map_err(|e| {
                        crate::HttpCacheError::cache(format!(
                            "redb begin_write (put) failed: {e}"
                        ))
                    })?;
                    {
                        let mut table = write_txn
                            .open_table(METADATA_TABLE)
                            .map_err(|e| {
                                crate::HttpCacheError::cache(format!(
                                    "redb open_table (put) failed: {e}"
                                ))
                            })?;
                        table
                            .insert(
                                key_for_redb.as_str(),
                                serialized.as_slice(),
                            )
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
                    Ok(inner_result) => inner_result,
                    Err(e) => Err(Box::new(crate::HttpCacheError::cache(
                        format!("put join failed: {e}"),
                    ))),
                };
            if let Err(e) = commit_result {
                // Roll back the rename (unlink final) so we don't leak an
                // orphaned body, then keep serving from the open handle —
                // the inode stays alive until the handle drops (Rust's std
                // opens files with FILE_SHARE_DELETE on Windows, so the
                // unlink is legal there too).
                log::debug!("put: redb commit failed; serving uncached: {e}");
                let _ = tokio::fs::remove_file(&final_path).await;
                return serve_uncached_spooled(
                    parts, file, guard, written, None, None,
                );
            }
            self.metadata.insert(cache_key.clone(), metadata).await;
        }

        // Serve from the same handle we just wrote: no reopen race with a
        // concurrent put. Parts stay exactly as received; only the stored
        // metadata got the hop-by-hop strip.
        if let Err(e) =
            file.seek(std::io::SeekFrom::Start(NONCE_LEN as u64)).await
        {
            // Entry is committed and valid; only our serving handle is
            // broken. Fall back to a fresh get().
            log::debug!("put: post-commit seek failed: {e}");
            drop(file);
            if let Some((resp, _)) = self.get(&cache_key).await? {
                let (_, body) = resp.into_parts();
                return Ok(Response::from_parts(parts, body));
            }
            return Err(crate::HttpCacheError::cache(format!(
                "put: entry vanished after commit: {e}"
            ))
            .into());
        }
        let heal = CorruptHeal {
            db: Arc::downgrade(&self.db),
            metadata: self.metadata.clone(),
            key_locks: self.key_locks.clone(),
            key: cache_key,
            body_hash,
            body_path: final_path,
            nonce,
        };
        let body = StreamingBody::from_file_verified(
            file,
            written,
            checksum,
            move || {
                tokio::spawn(heal.run());
            },
        );
        Ok(Response::from_parts(parts, body))
    }

    async fn update_metadata(
        &self,
        cache_key: &str,
        headers: &http::HeaderMap,
        policy: CachePolicy,
        user_metadata: Option<Vec<u8>>,
        token: Option<&crate::CacheEntryToken>,
    ) -> Result<bool> {
        let body_hash = body_hash_for(cache_key);
        let _guard = self.key_lock(&body_hash).write().await;

        // redb is authoritative; moka may lag.
        let Some(mut metadata) = self.redb_get(cache_key).await? else {
            // Entry vanished (evicted/healed). Drop any stale moka row.
            self.metadata.invalidate(cache_key).await;
            return Ok(false);
        };

        // Identity check: refuse to staple this revision's headers onto a
        // concurrently-stored replacement entry's body.
        if let Some(t) = token {
            if t.0.as_slice() != metadata.nonce {
                return Ok(false);
            }
        }

        metadata.headers = stored_headers(headers);
        metadata.policy = policy;
        metadata.user_metadata = user_metadata;
        // nonce/checksum/body_size stay as-is, which also keeps CorruptHeal
        // correct for a corrupt read already in flight.

        let serialized = postcard::to_allocvec(&metadata).map_err(|e| {
            crate::HttpCacheError::cache(format!(
                "Failed to serialize metadata: {e}"
            ))
        })?;
        let db = self.db.clone();
        let key_for_redb = cache_key.to_string();
        tokio::task::spawn_blocking(move || -> Result<()> {
            let write_txn = db.begin_write().map_err(|e| {
                crate::HttpCacheError::cache(format!(
                    "redb begin_write (update_metadata) failed: {e}"
                ))
            })?;
            {
                let mut table =
                    write_txn.open_table(METADATA_TABLE).map_err(|e| {
                        crate::HttpCacheError::cache(format!(
                            "redb open_table (update_metadata) failed: {e}"
                        ))
                    })?;
                table
                    .insert(key_for_redb.as_str(), serialized.as_slice())
                    .map_err(|e| {
                        crate::HttpCacheError::cache(format!(
                            "redb insert (update_metadata) failed: {e}"
                        ))
                    })?;
            }
            write_txn.commit().map_err(|e| {
                crate::HttpCacheError::cache(format!(
                    "redb commit (update_metadata) failed: {e}"
                ))
            })?;
            Ok(())
        })
        .await
        .map_err(|e| {
            crate::HttpCacheError::cache(format!(
                "update_metadata join failed: {e}"
            ))
        })??;

        self.metadata.insert(cache_key.to_string(), metadata).await;
        Ok(true)
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
        // Non-cacheable responses pass through without buffering.
        Ok(response.map(|body| {
            StreamingBody::streaming(
                body.map_frame(|frame| {
                    frame.map_data(|mut d| d.copy_to_bytes(d.remaining()))
                })
                .map_err(Into::into)
                .boxed_unsync(),
            )
        }))
    }

    async fn delete(&self, cache_key: &str) -> Result<()> {
        let body_hash = body_hash_for(cache_key);
        let body_path = body_path_for(&self.body_dir, &body_hash);
        let _guard = self.key_lock(&body_hash).write().await;
        self.self_heal(cache_key, &body_path).await;
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

    async fn read_body_bytes(resp: Response<ManagerBody>) -> Bytes {
        resp.into_body().collect().await.unwrap().to_bytes()
    }

    #[tokio::test]
    async fn test_convert_body_passes_through_without_buffering() {
        let manager = StreamingManager::with_temp_dir(10).await.unwrap();
        let resp = response_with_body(Bytes::from("pass-through"));
        let converted = manager.convert_body(resp).await.unwrap();
        assert!(
            matches!(converted.body(), StreamingBody::Streaming { .. }),
            "non-cacheable responses must not be buffered"
        );
        let b = converted.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(b, "pass-through");
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
    async fn test_recreate_dir_tolerates_already_exists() {
        // `create_dir_all` returns `AlreadyExists` when the path exists as a
        // non-directory — the same error a concurrent removal produces mid
        // `clear()`. `recreate_dir` must treat it as success.
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("collide");
        std::fs::write(&path, b"x").unwrap();
        recreate_dir(&path).await.unwrap();
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

    /// update_metadata must rewrite headers/policy without touching the body
    /// file (no re-read into RAM, no rewrite: bytes stay identical).
    #[tokio::test]
    async fn test_update_metadata_leaves_body_file_untouched() {
        let dir = TempDir::new().unwrap();
        let manager =
            StreamingManager::new(dir.path().to_path_buf(), 100).await.unwrap();
        let body = Full::new(Bytes::from_static(b"immutable body"));
        let response = Response::builder()
            .status(StatusCode::OK)
            .header("x-old", "1")
            .body(body)
            .unwrap();
        let key = "GET:https://example.com/reval".to_string();
        let _ = manager
            .put(key.clone(), response, sample_policy(), test_url(), None)
            .await
            .unwrap();

        let body_hash = body_hash_for(&key);
        let body_path = body_path_for(&manager.body_dir, &body_hash);
        let before = std::fs::read(&body_path).unwrap();

        // Fetch the entry token the way the orchestrator would.
        let (resp, _) = manager.get(&key).await.unwrap().unwrap();
        let token = resp
            .extensions()
            .get::<crate::CacheEntryToken>()
            .cloned()
            .expect("get() must attach a CacheEntryToken");

        let mut new_headers = http::HeaderMap::new();
        new_headers.insert("x-new", "2".parse().unwrap());
        let updated = manager
            .update_metadata(
                &key,
                &new_headers,
                sample_policy(),
                None,
                Some(&token),
            )
            .await
            .unwrap();
        assert!(updated);

        let after = std::fs::read(&body_path).unwrap();
        assert_eq!(before, after, "body file must be byte-identical");

        let (resp, _) = manager.get(&key).await.unwrap().unwrap();
        assert!(resp.headers().get("x-new").is_some());
        assert!(
            resp.headers().get("x-old").is_none(),
            "header set is replaced, not merged (the orchestrator does the merge)"
        );
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(&bytes[..], b"immutable body");
    }

    #[tokio::test]
    async fn test_update_metadata_missing_entry_returns_false() {
        let dir = TempDir::new().unwrap();
        let manager =
            StreamingManager::new(dir.path().to_path_buf(), 100).await.unwrap();
        let updated = manager
            .update_metadata(
                "GET:https://example.com/absent",
                &http::HeaderMap::new(),
                sample_policy(),
                None,
                None,
            )
            .await
            .unwrap();
        assert!(!updated);
    }

    /// A stale token (entry was concurrently replaced) must refuse the update
    /// rather than stapling old headers onto the new entry's body.
    #[tokio::test]
    async fn test_update_metadata_stale_token_returns_false() {
        let dir = TempDir::new().unwrap();
        let manager =
            StreamingManager::new(dir.path().to_path_buf(), 100).await.unwrap();
        let key = "GET:https://example.com/race".to_string();
        let mk_body =
            |s: &'static str| Full::new(Bytes::from_static(s.as_bytes()));
        let response = Response::builder()
            .status(StatusCode::OK)
            .header("x-version", "v1")
            .body(mk_body("v1"))
            .unwrap();
        let _ = manager
            .put(key.clone(), response, sample_policy(), test_url(), None)
            .await
            .unwrap();
        let (resp, _) = manager.get(&key).await.unwrap().unwrap();
        let v1_token =
            resp.extensions().get::<crate::CacheEntryToken>().cloned().unwrap();

        // Concurrent replacement: v2 lands (new nonce).
        let response = Response::builder()
            .status(StatusCode::OK)
            .header("x-version", "v2")
            .body(mk_body("v2"))
            .unwrap();
        let _ = manager
            .put(key.clone(), response, sample_policy(), test_url(), None)
            .await
            .unwrap();

        let updated = manager
            .update_metadata(
                &key,
                &http::HeaderMap::new(),
                sample_policy(),
                None,
                Some(&v1_token),
            )
            .await
            .unwrap();
        assert!(!updated, "stale token must refuse the metadata update");

        // The refused update must not have mutated v2's stored entry: it
        // still carries v2's own headers and body, not v1's and not the
        // (empty) headers passed to the refused `update_metadata` call.
        let (resp, _) = manager.get(&key).await.unwrap().unwrap();
        assert_eq!(
            resp.headers().get("x-version").unwrap(),
            "v2",
            "refused update must leave v2's stored headers unmodified"
        );
        let bytes = read_body_bytes(resp).await;
        assert_eq!(
            &bytes[..],
            b"v2",
            "refused update must leave v2's stored body unmodified"
        );
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

        // Reopen: the poisoned row reads as a miss.
        let manager = StreamingManager::new(path, 100).await.unwrap();
        assert!(manager.get("bad").await.unwrap().is_none());
        // The lazy self-heal in redb_get removed the poisoned row.
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

        // With per-key locking, concurrent puts serialize: the entry must
        // survive and contain one of the two committed bodies.
        let (resp, _) = manager
            .get("k")
            .await
            .unwrap()
            .expect("entry must survive concurrent puts");
        let body = read_body_bytes(resp).await;
        assert!(body == "aaa" || body == "bbb", "got {body:?}");

        // With per-key locking, exactly one body file remains under the
        // key's prefix.
        let prefix_dir = manager.body_dir.join(&body_hash_for("k")[0..2]);
        let mut count = 0usize;
        if prefix_dir.exists() {
            let mut rd = tokio::fs::read_dir(&prefix_dir).await.unwrap();
            while rd.next_entry().await.unwrap().is_some() {
                count += 1;
            }
        }
        assert_eq!(count, 1, "expected exactly one body file, got {count}");
    }

    #[tokio::test]
    async fn test_concurrent_get_put_no_entry_loss() {
        let manager =
            Arc::new(StreamingManager::with_temp_dir(100).await.unwrap());
        manager
            .put(
                "k".into(),
                response_with_body(Bytes::from("seed")),
                sample_policy(),
                test_url(),
                None,
            )
            .await
            .unwrap();
        let putter = {
            let m = manager.clone();
            tokio::spawn(async move {
                for i in 0..50u32 {
                    m.put(
                        "k".into(),
                        response_with_body(Bytes::from(format!("body-{i}"))),
                        sample_policy(),
                        test_url(),
                        None,
                    )
                    .await
                    .unwrap();
                }
            })
        };
        let getter = {
            let m = manager.clone();
            tokio::spawn(async move {
                for _ in 0..200 {
                    let (resp, _) = m
                        .get("k")
                        .await
                        .unwrap()
                        .expect("entry lost during concurrent get/put");
                    let b = read_body_bytes(resp).await;
                    assert!(
                        b == "seed" || b.starts_with(b"body-"),
                        "torn body {b:?}"
                    );
                }
            })
        };
        putter.await.unwrap();
        getter.await.unwrap();
        assert!(manager.get("k").await.unwrap().is_some());
    }

    #[tokio::test]
    async fn test_max_body_size_declines() {
        let tmp = TempDir::new().unwrap();
        let manager = StreamingManager::with_max_body_size(
            tmp.path().to_path_buf(),
            100,
            10,
        )
        .await
        .unwrap();

        let returned = manager
            .put(
                "k".into(),
                response_with_body(Bytes::from("this body exceeds the limit")),
                sample_policy(),
                test_url(),
                None,
            )
            .await
            .unwrap();

        // Response succeeds with the full body — decline, not an error.
        let bytes = returned.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(&bytes[..], b"this body exceeds the limit");

        // No redb row, no body file: nothing cached.
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
    async fn test_corrupted_body_detected_and_healed() {
        let manager =
            Arc::new(StreamingManager::with_temp_dir(100).await.unwrap());
        manager
            .put(
                "k".into(),
                response_with_body(Bytes::from("hello corruption test")),
                sample_policy(),
                test_url(),
                None,
            )
            .await
            .unwrap();

        // Flip one body byte on disk without changing length or nonce.
        let path = body_path_for(&manager.body_dir, &body_hash_for("k"));
        let mut contents = tokio::fs::read(&path).await.unwrap();
        let idx = contents.len() - 1;
        contents[idx] ^= 0xFF;
        tokio::fs::write(&path, &contents).await.unwrap();

        // The stream must error rather than yield corrupt bytes silently.
        let (resp, _) = manager.get("k").await.unwrap().unwrap();
        let collected = resp.into_body().collect().await;
        assert!(collected.is_err(), "corrupt body must fail the stream");

        // Self-heal runs asynchronously; poll until the entry is gone.
        // yield_now instead of sleep: tokio's "time" feature is not part of
        // the streaming feature gate.
        for _ in 0..1000 {
            if manager.get("k").await.unwrap().is_none() {
                return;
            }
            tokio::task::yield_now().await;
        }
        panic!("corrupt entry was not self-healed");
    }

    #[tokio::test]
    async fn test_corrupt_heal_spares_fresh_entry() {
        let manager =
            Arc::new(StreamingManager::with_temp_dir(100).await.unwrap());
        manager
            .put(
                "k".into(),
                response_with_body(Bytes::from("stale entry body")),
                sample_policy(),
                test_url(),
                None,
            )
            .await
            .unwrap();

        // Corrupt the body, open a stream against it, then replace the
        // entry before the stream detects the corruption.
        let path = body_path_for(&manager.body_dir, &body_hash_for("k"));
        let mut contents = tokio::fs::read(&path).await.unwrap();
        let idx = contents.len() - 1;
        contents[idx] ^= 0xFF;
        tokio::fs::write(&path, &contents).await.unwrap();

        let (resp, _) = manager.get("k").await.unwrap().unwrap();
        manager
            .put(
                "k".into(),
                response_with_body(Bytes::from("fresh entry body")),
                sample_policy(),
                test_url(),
                None,
            )
            .await
            .unwrap();

        // The corrupt stream errors, but the heal it fires must not touch
        // the entry written after the corrupt read.
        assert!(resp.into_body().collect().await.is_err());
        for _ in 0..1000 {
            tokio::task::yield_now().await;
        }
        let (resp, _) = manager
            .get("k")
            .await
            .unwrap()
            .expect("fresh entry must survive the deferred heal");
        assert_eq!(read_body_bytes(resp).await, "fresh entry body");
    }

    #[tokio::test]
    async fn test_body_does_not_hold_redb_lock() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().to_path_buf();
        let manager = StreamingManager::new(path.clone(), 100).await.unwrap();
        manager
            .put(
                "k".into(),
                response_with_body(Bytes::from("body outlives manager")),
                sample_policy(),
                test_url(),
                None,
            )
            .await
            .unwrap();
        let (resp, _) = manager.get("k").await.unwrap().unwrap();
        drop(manager);

        // The still-alive body must not keep the redb file lock, so a new
        // manager can open the same directory.
        let manager = StreamingManager::new(path, 100).await.unwrap();
        assert_eq!(read_body_bytes(resp).await, "body outlives manager");
        assert!(manager.get("k").await.unwrap().is_some());
    }

    /// put() must return a disk-backed File body (not Buffered), and the entry
    /// must be durable+visible before put() returns.
    #[tokio::test]
    async fn test_put_returns_file_variant_and_commits_before_return() {
        let dir = TempDir::new().unwrap();
        let manager =
            StreamingManager::new(dir.path().to_path_buf(), 100).await.unwrap();
        let body = Full::new(Bytes::from_static(b"hello streaming world"));
        let response = Response::builder()
            .status(StatusCode::OK)
            .header("content-type", "text/plain")
            .body(body)
            .unwrap();
        let returned = manager
            .put(
                "GET:https://example.com/test".to_string(),
                response,
                sample_policy(),
                test_url(),
                None,
            )
            .await
            .unwrap();

        // Pins the current visible-at-return behavior; the documented
        // contract only promises visible at-or-after full body consumption.
        let got = manager.get("GET:https://example.com/test").await.unwrap();
        assert!(got.is_some(), "entry must be visible when put() returns");

        // Returned body is File-backed, not an in-RAM copy.
        let body = returned.into_body();
        assert!(
            matches!(body, StreamingBody::File { .. }),
            "put must return the File variant, got {body:?}"
        );
        let bytes = body.collect().await.unwrap().to_bytes();
        assert_eq!(&bytes[..], b"hello streaming world");

        // And the cached copy replays identically.
        let (cached, _policy) = got.unwrap();
        let cached_bytes =
            cached.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(&cached_bytes[..], b"hello streaming world");
    }

    /// Structural bounded-RAM proof: every previously-yielded frame must already
    /// be on disk (in tmp/) before the next frame is pulled. A collect()-based
    /// put leaves tmp/ empty until EOF; the spool writes+flushes as it reads.
    #[tokio::test]
    async fn test_put_spools_frames_to_disk_incrementally() {
        use std::pin::Pin;
        use std::task::{Context, Poll};

        const FRAME: usize = 64 * 1024;
        const NFRAMES: u64 = 8;

        /// Yields NFRAMES x 64KB frames; on each poll after the first, asserts
        /// the spool dir already holds at least (frames_yielded - 1) * FRAME
        /// body bytes.
        struct AssertSpooled {
            tmp_dir: PathBuf,
            yielded: u64,
        }
        impl Body for AssertSpooled {
            type Data = Bytes;
            type Error = StreamingError;
            fn poll_frame(
                mut self: Pin<&mut Self>,
                _cx: &mut Context<'_>,
            ) -> Poll<
                Option<
                    std::result::Result<
                        http_body::Frame<Bytes>,
                        StreamingError,
                    >,
                >,
            > {
                if self.yielded > 0 {
                    // Size via an opened handle, not DirEntry::metadata:
                    // Windows serves a stale directory-entry size for a file
                    // being written, but a by-handle query is current.
                    let spooled: u64 = std::fs::read_dir(&self.tmp_dir)
                        .unwrap()
                        .filter_map(|e| e.ok())
                        .filter_map(|e| std::fs::File::open(e.path()).ok())
                        .filter_map(|f| f.metadata().ok())
                        .map(|m| m.len())
                        .sum();
                    let expected =
                        NONCE_LEN as u64 + (self.yielded - 1) * FRAME as u64;
                    assert!(
                        spooled >= expected,
                        "frame {} pulled but only {spooled} bytes spooled \
                         (expected >= {expected}); put() is buffering",
                        self.yielded
                    );
                }
                if self.yielded == NFRAMES {
                    return Poll::Ready(None);
                }
                self.yielded += 1;
                Poll::Ready(Some(Ok(http_body::Frame::data(Bytes::from(
                    vec![0xAB; FRAME],
                )))))
            }
        }

        let dir = TempDir::new().unwrap();
        let manager =
            StreamingManager::new(dir.path().to_path_buf(), 100).await.unwrap();
        let body =
            AssertSpooled { tmp_dir: dir.path().join("tmp"), yielded: 0 };
        let response =
            Response::builder().status(StatusCode::OK).body(body).unwrap();
        let returned = manager
            .put(
                "GET:https://example.com/incremental".to_string(),
                response,
                sample_policy(),
                test_url(),
                None,
            )
            .await
            .unwrap();
        let bytes = returned.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(bytes.len() as u64, NFRAMES * FRAME as u64);
    }

    /// One data frame, then end-of-stream. Used by the decline tests.
    struct OneFrameBody {
        frame: &'static [u8],
        sent: bool,
    }
    impl Body for OneFrameBody {
        type Data = Bytes;
        type Error = StreamingError;
        fn poll_frame(
            mut self: std::pin::Pin<&mut Self>,
            _cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<
            Option<
                std::result::Result<http_body::Frame<Bytes>, StreamingError>,
            >,
        > {
            if self.sent {
                std::task::Poll::Ready(None)
            } else {
                self.sent = true;
                std::task::Poll::Ready(Some(Ok(http_body::Frame::data(
                    Bytes::from_static(self.frame),
                ))))
            }
        }
    }

    /// Content-Length over the cap: put() must decline up front — no spool
    /// file, nothing cached — and pass the body through for the caller.
    #[tokio::test]
    async fn test_put_declines_oversized_content_length_up_front() {
        let dir = TempDir::new().unwrap();
        let manager = StreamingManager::with_max_body_size(
            dir.path().to_path_buf(),
            100,
            1024, // 1KiB cap
        )
        .await
        .unwrap();

        let response = Response::builder()
            .status(StatusCode::OK)
            .header("content-length", "1048576") // claims 1MiB
            .body(OneFrameBody { frame: b"served-after-decline", sent: false })
            .unwrap();
        let returned = manager
            .put(
                "GET:https://example.com/big".to_string(),
                response,
                sample_policy(),
                test_url(),
                None,
            )
            .await
            .unwrap();

        // Nothing cached, no spool file ever created.
        assert!(manager
            .get("GET:https://example.com/big")
            .await
            .unwrap()
            .is_none());
        let tmp_entries = std::fs::read_dir(dir.path().join("tmp"))
            .map(|rd| rd.count())
            .unwrap_or(0);
        assert_eq!(tmp_entries, 0, "decline must not touch the spool dir");

        // The caller still gets the body.
        let bytes = returned.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(&bytes[..], b"served-after-decline");
    }

    /// HEAD responses carry the entity's Content-Length with an empty body
    /// (RFC 9110 §8.6) — they must still be cached, not declined by the
    /// size/completeness checks. Regression guard for the CL checks above.
    #[tokio::test]
    async fn test_put_head_response_with_entity_content_length_is_cached() {
        use http_body_util::Empty;
        let dir = TempDir::new().unwrap();
        let manager = StreamingManager::with_max_body_size(
            dir.path().to_path_buf(),
            100,
            1024,
        )
        .await
        .unwrap();
        let mut response = Response::builder()
            .status(StatusCode::OK)
            .header("content-length", "1048576") // entity size, > cap; body empty
            .body(Empty::<Bytes>::new())
            .unwrap();
        response
            .extensions_mut()
            .insert(crate::CachedRequestMethod(http::Method::HEAD));
        let _ = manager
            .put(
                "HEAD:https://example.com/test".to_string(),
                response,
                sample_policy(),
                test_url(),
                None,
            )
            .await
            .unwrap();
        assert!(
            manager
                .get("HEAD:https://example.com/test")
                .await
                .unwrap()
                .is_some(),
            "HEAD response must be cached despite entity Content-Length"
        );
    }

    /// Unknown-length body overflowing the cap mid-stream: caller receives the
    /// complete body, nothing is cached, no request error (this case used to
    /// fail the request).
    #[tokio::test]
    async fn test_put_unknown_length_overflow_serves_full_body_uncached() {
        use http_body_util::StreamBody;
        let dir = TempDir::new().unwrap();
        let manager = StreamingManager::with_max_body_size(
            dir.path().to_path_buf(),
            100,
            1024,
        )
        .await
        .unwrap();

        // 4 x 512B frames = 2KiB total, no content-length -> overflows 1KiB cap
        // at frame 3.
        let frames = (0..4).map(|i| {
            Ok::<_, StreamingError>(http_body::Frame::data(Bytes::from(
                vec![i as u8; 512],
            )))
        });
        let body = StreamBody::new(futures_util::stream::iter(frames));
        let response =
            Response::builder().status(StatusCode::OK).body(body).unwrap();
        let returned = manager
            .put(
                "GET:https://example.com/overflow".to_string(),
                response,
                sample_policy(),
                test_url(),
                None,
            )
            .await
            .unwrap();

        let bytes = returned.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(bytes.len(), 2048, "caller must receive every byte");
        assert_eq!(&bytes[..512], &[0u8; 512][..]);
        assert_eq!(&bytes[1536..], &[3u8; 512][..]);

        assert!(
            manager
                .get("GET:https://example.com/overflow")
                .await
                .unwrap()
                .is_none(),
            "overflowing entry must not be cached"
        );
        // Guard cleanup: tmp dir empty after body drop.
        let tmp_entries =
            std::fs::read_dir(dir.path().join("tmp")).unwrap().count();
        assert_eq!(tmp_entries, 0);
    }

    /// Exact boundary: body of exactly max_body_size IS cached.
    #[tokio::test]
    async fn test_put_exactly_max_body_size_is_cached() {
        let dir = TempDir::new().unwrap();
        let manager = StreamingManager::with_max_body_size(
            dir.path().to_path_buf(),
            100,
            1024,
        )
        .await
        .unwrap();
        let body = Full::new(Bytes::from(vec![7u8; 1024]));
        let response =
            Response::builder().status(StatusCode::OK).body(body).unwrap();
        let _ = manager
            .put(
                "GET:https://example.com/exact".to_string(),
                response,
                sample_policy(),
                test_url(),
                None,
            )
            .await
            .unwrap();
        assert!(manager
            .get("GET:https://example.com/exact")
            .await
            .unwrap()
            .is_some());
    }

    /// Content-Length lie (RFC 9111 §3.3): upstream declared 100 bytes but sent
    /// 5 -> serve the 5 bytes, do NOT cache a known-incomplete response.
    #[tokio::test]
    async fn test_put_content_length_mismatch_declines_commit() {
        let dir = TempDir::new().unwrap();
        let manager =
            StreamingManager::new(dir.path().to_path_buf(), 100).await.unwrap();
        let body = Full::new(Bytes::from_static(b"short"));
        let response = Response::builder()
            .status(StatusCode::OK)
            .header("content-length", "100")
            .body(body)
            .unwrap();
        let returned = manager
            .put(
                "GET:https://example.com/truncated".to_string(),
                response,
                sample_policy(),
                test_url(),
                None,
            )
            .await
            .unwrap();
        let bytes = returned.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(&bytes[..], b"short");
        assert!(manager
            .get("GET:https://example.com/truncated")
            .await
            .unwrap()
            .is_none());
    }

    /// Upstream body error mid-stream: put() errors (unchanged contract) and
    /// the spool tmp file is cleaned up.
    #[tokio::test]
    async fn test_put_upstream_error_fails_and_cleans_tmp() {
        use http_body_util::StreamBody;
        let dir = TempDir::new().unwrap();
        let manager =
            StreamingManager::new(dir.path().to_path_buf(), 100).await.unwrap();
        let frames = vec![
            Ok(http_body::Frame::data(Bytes::from_static(b"good"))),
            Err(StreamingError::new(Box::new(std::io::Error::other(
                "upstream reset",
            )))),
        ];
        let body = StreamBody::new(futures_util::stream::iter(frames));
        let response =
            Response::builder().status(StatusCode::OK).body(body).unwrap();
        let result = manager
            .put(
                "GET:https://example.com/reset".to_string(),
                response,
                sample_policy(),
                test_url(),
                None,
            )
            .await;
        assert!(result.is_err(), "upstream error must propagate");
        assert!(manager
            .get("GET:https://example.com/reset")
            .await
            .unwrap()
            .is_none());
        let tmp_entries =
            std::fs::read_dir(dir.path().join("tmp")).unwrap().count();
        assert_eq!(tmp_entries, 0, "tmp must be cleaned on upstream error");
    }

    /// Empty body round-trips.
    #[tokio::test]
    async fn test_put_empty_body_round_trips() {
        use http_body_util::Empty;
        let dir = TempDir::new().unwrap();
        let manager =
            StreamingManager::new(dir.path().to_path_buf(), 100).await.unwrap();
        let body = Empty::<Bytes>::new();
        let response = Response::builder()
            .status(StatusCode::NO_CONTENT)
            .body(body)
            .unwrap();
        let returned = manager
            .put(
                "GET:https://example.com/empty".to_string(),
                response,
                sample_policy(),
                test_url(),
                None,
            )
            .await
            .unwrap();
        let bytes = returned.into_body().collect().await.unwrap().to_bytes();
        assert!(bytes.is_empty());
        let (cached, _) = manager
            .get("GET:https://example.com/empty")
            .await
            .unwrap()
            .unwrap();
        let cached_bytes =
            cached.into_body().collect().await.unwrap().to_bytes();
        assert!(cached_bytes.is_empty());
    }

    /// Response extensions must survive every put() return path.
    #[tokio::test]
    async fn test_put_preserves_extensions_on_success_and_decline() {
        #[derive(Clone, PartialEq, Debug)]
        struct Marker(u32);

        let dir = TempDir::new().unwrap();
        let manager = StreamingManager::with_max_body_size(
            dir.path().to_path_buf(),
            100,
            1024,
        )
        .await
        .unwrap();

        // Success path.
        let mut response = Response::builder()
            .status(StatusCode::OK)
            .body(Full::new(Bytes::from_static(b"ok")))
            .unwrap();
        response.extensions_mut().insert(Marker(1));
        let returned = manager
            .put(
                "GET:https://example.com/ext1".to_string(),
                response,
                sample_policy(),
                test_url(),
                None,
            )
            .await
            .unwrap();
        assert_eq!(returned.extensions().get::<Marker>(), Some(&Marker(1)));

        // Decline path (content-length over cap).
        let mut response = Response::builder()
            .status(StatusCode::OK)
            .header("content-length", "1048576")
            .body(Full::new(Bytes::from(vec![0u8; 16])))
            .unwrap();
        response.extensions_mut().insert(Marker(2));
        let returned = manager
            .put(
                "GET:https://example.com/ext2".to_string(),
                response,
                sample_policy(),
                test_url(),
                None,
            )
            .await
            .unwrap();
        assert_eq!(returned.extensions().get::<Marker>(), Some(&Marker(2)));
    }

    /// max_body_size == 0 degenerate: always decline, always pass through.
    #[tokio::test]
    async fn test_put_zero_max_body_size_always_declines() {
        let dir = TempDir::new().unwrap();
        let manager = StreamingManager::with_max_body_size(
            dir.path().to_path_buf(),
            100,
            0,
        )
        .await
        .unwrap();
        let response = Response::builder()
            .status(StatusCode::OK)
            .body(Full::new(Bytes::from_static(b"never cached")))
            .unwrap();
        let returned = manager
            .put(
                "GET:https://example.com/zero".to_string(),
                response,
                sample_policy(),
                test_url(),
                None,
            )
            .await
            .unwrap();
        let bytes = returned.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(&bytes[..], b"never cached");
        assert!(manager
            .get("GET:https://example.com/zero")
            .await
            .unwrap()
            .is_none());
    }

    /// Pass-through decline is true streaming: put() returns without consuming
    /// the stalled upstream, and the first frame reaches the caller even though
    /// upstream never finishes. The success path spools to EOF before
    /// serving, so this property is specific to the decline path.
    #[tokio::test]
    async fn test_put_decline_path_streams_first_frame_before_eof() {
        use std::pin::Pin;
        use std::task::{Context, Poll};

        struct FirstFrameThenForeverPending {
            sent: bool,
        }
        impl Body for FirstFrameThenForeverPending {
            type Data = Bytes;
            type Error = StreamingError;
            fn poll_frame(
                mut self: Pin<&mut Self>,
                _cx: &mut Context<'_>,
            ) -> Poll<
                Option<
                    std::result::Result<
                        http_body::Frame<Bytes>,
                        StreamingError,
                    >,
                >,
            > {
                if self.sent {
                    Poll::Pending // upstream stalls forever
                } else {
                    self.sent = true;
                    Poll::Ready(Some(Ok(http_body::Frame::data(
                        Bytes::from_static(b"first"),
                    ))))
                }
            }
        }

        let dir = TempDir::new().unwrap();
        let manager = StreamingManager::with_max_body_size(
            dir.path().to_path_buf(),
            100,
            1024,
        )
        .await
        .unwrap();
        let response = Response::builder()
            .status(StatusCode::OK)
            .header("content-length", "1048576") // -> decline path
            .body(FirstFrameThenForeverPending { sent: false })
            .unwrap();
        // Wrapped in a timeout so a regression (put() consuming the body)
        // fails deterministically instead of hanging the suite.
        let returned = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            manager.put(
                "GET:https://example.com/ttfb".to_string(),
                response,
                sample_policy(),
                test_url(),
                None,
            ),
        )
        .await
        .expect("put() must return without consuming the stalled upstream")
        .unwrap();

        let mut body = returned.into_body();
        let first = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            body.frame(),
        )
        .await
        .expect("first frame must arrive without waiting for upstream EOF")
        .unwrap()
        .unwrap();
        assert_eq!(first.into_data().unwrap(), Bytes::from_static(b"first"));
    }

    /// Without the flush a write counts as spooled before it is
    /// confirmed, and `put` can commit an entry shorter than its metadata says.
    #[tokio::test]
    async fn test_spool_write_surfaces_deferred_write_error() {
        use std::pin::Pin;
        use std::task::{Context, Poll};
        use tokio::io::AsyncWrite;

        struct DeferredErrorWriter;

        impl AsyncWrite for DeferredErrorWriter {
            fn poll_write(
                self: Pin<&mut Self>,
                _cx: &mut Context<'_>,
                buf: &[u8],
            ) -> Poll<std::io::Result<usize>> {
                Poll::Ready(Ok(buf.len()))
            }
            fn poll_flush(
                self: Pin<&mut Self>,
                _cx: &mut Context<'_>,
            ) -> Poll<std::io::Result<()>> {
                Poll::Ready(Err(std::io::Error::other("disk full")))
            }
            fn poll_shutdown(
                self: Pin<&mut Self>,
                _cx: &mut Context<'_>,
            ) -> Poll<std::io::Result<()>> {
                Poll::Ready(Ok(()))
            }
        }

        let err = spool_write(&mut DeferredErrorWriter, b"frame")
            .await
            .expect_err("a flush-time failure must be reported");
        assert_eq!(err.to_string(), "disk full");
    }

    #[test]
    fn test_tmp_guard_unlinks_unless_defused() {
        let dir = TempDir::new().unwrap();
        let armed = dir.path().join("armed.tmp");
        let defused = dir.path().join("defused.tmp");
        std::fs::write(&armed, b"x").unwrap();
        std::fs::write(&defused, b"x").unwrap();
        {
            let _g = TmpGuard::new(armed.clone());
        }
        {
            let mut g = TmpGuard::new(defused.clone());
            g.defuse();
        }
        assert!(!armed.exists(), "armed guard must unlink on drop");
        assert!(defused.exists(), "defused guard must leave the file");
    }

    #[test]
    fn test_parse_content_length() {
        let mut h = http::HeaderMap::new();
        assert_eq!(parse_content_length(&h), None, "absent -> None");
        h.insert(http::header::CONTENT_LENGTH, "1234".parse().unwrap());
        assert_eq!(parse_content_length(&h), Some(1234));
        h.append(http::header::CONTENT_LENGTH, "1234".parse().unwrap());
        assert_eq!(parse_content_length(&h), Some(1234), "agreeing dupes ok");
        h.append(http::header::CONTENT_LENGTH, "999".parse().unwrap());
        assert_eq!(parse_content_length(&h), None, "disagreeing dupes -> None");
        let mut bad = http::HeaderMap::new();
        bad.insert(http::header::CONTENT_LENGTH, "12x4".parse().unwrap());
        assert_eq!(parse_content_length(&bad), None, "unparseable -> None");
    }

    /// Collect every stored value for a name (HttpHeaders has no get_all).
    fn header_values(h: &HttpHeaders, name: &str) -> Vec<String> {
        h.iter()
            .filter(|(k, _)| k.as_str() == name)
            .map(|(_, v)| v.clone())
            .collect()
    }

    #[test]
    fn test_stored_headers_strips_hop_by_hop_keeps_multi_valued() {
        let mut h = http::HeaderMap::new();
        h.insert("transfer-encoding", "chunked".parse().unwrap());
        h.insert("connection", "keep-alive, x-tracing-id".parse().unwrap());
        h.insert("x-tracing-id", "abc".parse().unwrap()); // nominated by Connection
        h.insert("proxy-connection", "keep-alive".parse().unwrap());
        h.insert("te", "trailers".parse().unwrap());
        h.insert("content-type", "text/plain".parse().unwrap());
        h.append("set-cookie", "a=1".parse().unwrap());
        h.append("set-cookie", "b=2".parse().unwrap());
        let stored = stored_headers(&h);
        assert!(header_values(&stored, "transfer-encoding").is_empty());
        assert!(header_values(&stored, "connection").is_empty());
        assert!(header_values(&stored, "proxy-connection").is_empty());
        assert!(header_values(&stored, "te").is_empty());
        assert!(
            header_values(&stored, "x-tracing-id").is_empty(),
            "Connection-nominated fields must be stripped (RFC 9111 §3.1)"
        );
        assert_eq!(header_values(&stored, "content-type"), vec!["text/plain"]);
        assert_eq!(header_values(&stored, "set-cookie"), vec!["a=1", "b=2"]);
    }

    #[tokio::test]
    async fn test_serve_uncached_spooled_chains_prefix_pending_rest_and_unlinks(
    ) {
        let dir = TempDir::new().unwrap();
        let tmp = dir.path().join("spool.tmp");

        // Simulate a spool: nonce header + 8 confirmed bytes, then a partial
        // trailing junk byte (as if a chunk write died halfway).
        let mut f = tokio::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&tmp)
            .await
            .unwrap();
        f.write_all(&[0u8; NONCE_LEN]).await.unwrap();
        f.write_all(b"prefix12").await.unwrap();
        f.write_all(b"J").await.unwrap(); // junk past `written`, must be ignored
        f.flush().await.unwrap();

        let guard = TmpGuard::new(tmp.clone());
        let pending = Some(Bytes::from_static(b"PENDING!"));
        let rest: Option<UnsyncBoxBody<Bytes, StreamingError>> = Some(
            Full::new(Bytes::from_static(b"rest-of-upstream"))
                .map_err(|never| match never {})
                .boxed_unsync(),
        );

        let parts =
            Response::builder().status(200).body(()).unwrap().into_parts().0;
        let resp =
            serve_uncached_spooled(parts, f, guard, 8, pending, rest).unwrap();

        let collected = resp.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(&collected[..], b"prefix12PENDING!rest-of-upstream");
        // Body fully consumed and dropped -> guard dropped -> tmp unlinked.
        assert!(!tmp.exists(), "tmp file must be unlinked after body drop");
    }

    #[tokio::test]
    async fn test_serve_uncached_spooled_prefix_only() {
        use http_body_util::BodyExt;
        let dir = TempDir::new().unwrap();
        let tmp = dir.path().join("spool2.tmp");
        let mut f = tokio::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&tmp)
            .await
            .unwrap();
        f.write_all(&[0u8; NONCE_LEN]).await.unwrap();
        f.write_all(b"only-prefix").await.unwrap();
        f.flush().await.unwrap();

        let guard = TmpGuard::new(tmp.clone());
        let parts =
            Response::builder().status(200).body(()).unwrap().into_parts().0;
        let resp =
            serve_uncached_spooled(parts, f, guard, 11, None, None).unwrap();
        let collected = resp.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(&collected[..], b"only-prefix");
        assert!(!tmp.exists());
    }
}
