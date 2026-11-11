//! HTTP cache manager backed by an embedded [`redb`](https://github.com/cberner/redb)
//! key/value database.
//!
//! Unlike [`CACacheManager`](crate::CACacheManager), this manager performs all
//! of its I/O **synchronously** and depends on **no async runtime** — `redb`
//! itself only depends on `libc`. That makes it a good fit for smol-based
//! clients (such as `http-cache-ureq`) and for embedding runtimes like Bevy,
//! where there is no tokio reactor available: it pulls in no `tokio`
//! dependency and requires no reactor at runtime.
//!
//! The cache is stored in a single database file. Only one instance may have
//! the file open at a time — `redb` takes an exclusive file lock — so wrap the
//! manager in `Arc`/a `static` and share it rather than constructing a second
//! instance for the same path.
//!
//! Stores are batched for durability: entries are committed without an
//! fsync and flushed to disk every 64 writes (configurable via
//! [`RedbManager::from_database_with_flush_interval`]) and on drop, so a
//! crash can lose approximately the most recent 64 stores. Deletes commit
//! durably, so an invalidated entry cannot come back after a crash. When
//! sharing a [`Database`] via [`RedbManager::from_database`], do not drop
//! the last manager clone while holding an open `WriteTransaction` on that
//! database — the drop-time flush must acquire the writer lock.
//!
//! The cache grows without bound unless it is created with
//! [`RedbManager::from_database_with_max_bytes`], which removes the entries
//! written longest ago once the stored entries exceed a byte limit.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::managers::limit::{Limited, Tables};
use crate::{CacheManager, HttpResponse, Result};

use http_cache_semantics::CachePolicy;
use redb::{Database, Durability, ReadableDatabase, TableDefinition};
use serde::{Deserialize, Serialize};

pub(crate) const TABLE: TableDefinition<&str, &[u8]> =
    TableDefinition::new("http_cache_v1");

/// The entry table with the index a size limit keeps of its entries.
pub(crate) const TABLES: Tables = Tables {
    entries: TABLE,
    index: TableDefinition::new("http_cache_v1_index"),
    order: TableDefinition::new("http_cache_v1_order"),
    bytes: TableDefinition::new("http_cache_v1_bytes"),
};

/// Implements [`CacheManager`] with [`redb`](https://github.com/cberner/redb)
/// as the backend — a pure-Rust, embedded, persistent key/value store.
///
/// All operations are synchronous and require no async runtime, so this
/// manager works under any executor (tokio, smol, Bevy, …) and adds no
/// `tokio` dependency.
#[cfg_attr(docsrs, doc(cfg(feature = "manager-redb")))]
#[derive(Clone)]
pub struct RedbManager {
    flush: Arc<FlushState>,
    max_bytes: Option<u64>,
}

impl std::fmt::Debug for RedbManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RedbManager").finish_non_exhaustive()
    }
}

// Store format (postcard). `manager-redb` always enables `postcard`, and this
// is a new backend with no legacy on-disk data, so only postcard is supported.
#[derive(Debug, Deserialize, Serialize)]
struct Store {
    response: HttpResponse,
    policy: CachePolicy,
}

/// Stores are committed with `Durability::None` and made durable by an empty
/// `Immediate` commit every `interval` writes and when the last manager
/// clone drops.
const DEFAULT_FLUSH_INTERVAL: u64 = 64;

struct FlushState {
    db: Arc<Database>,
    unflushed: AtomicU64,
    interval: u64,
}

impl FlushState {
    fn flush(&self) -> Result<()> {
        // An empty Immediate commit persists all prior None-durability
        // commits.
        let write_txn = self.db.begin_write()?;
        write_txn.commit()?;
        Ok(())
    }

    fn record_write(&self) {
        if self.unflushed.fetch_add(1, Ordering::Relaxed) + 1 >= self.interval {
            // Claim the whole batch so writers queued behind a flush in
            // flight don't each fsync. The write's own commit already
            // succeeded; on failure the count is restored and the drop
            // guard retries — though redb latches I/O errors, so retries
            // fail until the database is reopened.
            let n = self.unflushed.swap(0, Ordering::Relaxed);
            if n == 0 {
                return;
            }
            if let Err(e) = self.flush() {
                self.unflushed.fetch_add(n, Ordering::Relaxed);
                log::warn!("redb interval flush failed: {e}");
            }
        }
    }
}

impl Drop for FlushState {
    fn drop(&mut self) {
        if self.unflushed.load(Ordering::Relaxed) > 0
            && let Err(e) = self.flush()
        {
            log::warn!("redb flush on drop failed: {e}");
        }
    }
}

/// Removes the entries written longest ago until the indexed entries fit in
/// `max_bytes`.
fn evict(limited: &mut Limited, max_bytes: u64) -> Result<()> {
    while limited.pop_oldest(max_bytes)?.is_some() {}
    Ok(())
}

impl RedbManager {
    /// Creates a new [`RedbManager`], creating or opening the redb database
    /// file at `path`.
    ///
    /// Returns an error if the database cannot be opened (for example if
    /// another instance already holds the exclusive file lock).
    pub fn new(path: impl AsRef<Path>) -> Result<Self> {
        let db = Database::create(path)?;
        Self::from_database(Arc::new(db))
    }

    /// Wraps an already-open redb [`Database`], allowing callers to configure
    /// the database (cache size, etc.) before handing it to the manager.
    pub fn from_database(db: Arc<Database>) -> Result<Self> {
        Self::from_database_with_flush_interval(db, DEFAULT_FLUSH_INTERVAL)
    }

    /// Like [`from_database`](Self::from_database), but flushes stores to
    /// disk every `flush_interval` writes instead of the default 64. An
    /// interval of 1 flushes after every store.
    pub fn from_database_with_flush_interval(
        db: Arc<Database>,
        flush_interval: u64,
    ) -> Result<Self> {
        Self::open(db, flush_interval, None)
    }

    /// Wraps an already-open redb [`Database`], flushing every
    /// `flush_interval` writes and keeping the stored entries within
    /// `max_bytes`. When a store takes the cache over the limit, the entries
    /// written longest ago are removed in the same transaction. A response
    /// that is larger than `max_bytes` on its own is not cached, and is
    /// removed if it is already in the database.
    ///
    /// The limit counts the serialized entries, not the database file, which
    /// is larger and does not shrink right away when entries are removed.
    /// Entries already in the database count towards the limit. Do not open
    /// one database with and without a limit at the same time, or let a
    /// version of this crate without size limits write to it in between:
    /// entries stored that way are not counted or removed until the manager
    /// is opened again.
    pub fn from_database_with_max_bytes(
        db: Arc<Database>,
        flush_interval: u64,
        max_bytes: u64,
    ) -> Result<Self> {
        Self::open(db, flush_interval, Some(max_bytes))
    }

    fn open(
        db: Arc<Database>,
        flush_interval: u64,
        max_bytes: Option<u64>,
    ) -> Result<Self> {
        // Ensure the table exists up front so reads never fail with
        // "table does not exist".
        let write_txn = db.begin_write()?;
        match max_bytes {
            Some(max_bytes) => {
                let mut limited = Limited::open(&write_txn, &TABLES)?;
                limited.index(|entry| entry.len() as u64)?;
                // An entry `put` would not have stored goes first, so that
                // evicting by age does not take the older entries with it.
                for key in limited.oversized(max_bytes)? {
                    limited.remove(&key)?;
                }
                evict(&mut limited, max_bytes)?;
            }
            // Entries stored without a limit are not indexed, so an index
            // left by an earlier limited manager would go stale.
            None => {
                write_txn.open_table(TABLE)?;
                TABLES.delete_index(&write_txn)?;
            }
        }
        write_txn.commit()?;
        Ok(Self {
            flush: Arc::new(FlushState {
                db,
                unflushed: AtomicU64::new(0),
                interval: flush_interval.max(1),
            }),
            max_bytes,
        })
    }

    /// Clears out the entire cache.
    pub async fn clear(&self) -> Result<()> {
        let write_txn = self.flush.db.begin_write()?;
        write_txn.delete_table(TABLE)?;
        TABLES.delete_index(&write_txn)?;
        // Recreate the (now empty) table so subsequent reads still succeed.
        {
            let _table = write_txn.open_table(TABLE)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    /// Reads and decodes the stored entry for `cache_key`, if present.
    /// Deserializes from the borrowed value while the read transaction is
    /// alive to avoid copying the entry out of redb first.
    fn read_entry(&self, cache_key: &str) -> Result<Option<Store>> {
        let read_txn = self.flush.db.begin_read()?;
        let table = read_txn.open_table(TABLE)?;
        match table.get(cache_key)? {
            Some(guard) => {
                match postcard::from_bytes::<Store>(guard.value()) {
                    Ok(store) => Ok(Some(store)),
                    Err(e) => {
                        // Treat undecodable entries as a miss rather than an
                        // error, matching the other managers.
                        log::debug!(
                            "Failed to deserialize cache entry for key \
                             '{cache_key}': {e}"
                        );
                        Ok(None)
                    }
                }
            }
            None => Ok(None),
        }
    }
}

impl CacheManager for RedbManager {
    async fn get(
        &self,
        cache_key: &str,
    ) -> Result<Option<(HttpResponse, CachePolicy)>> {
        // A storage-level read error is treated as a cache miss (degrade to a
        // fresh fetch) rather than a hard error, matching `CACacheManager`.
        match self.read_entry(cache_key) {
            Ok(Some(store)) => Ok(Some((store.response, store.policy))),
            Ok(None) => Ok(None),
            Err(e) => {
                log::debug!("redb read failed for key '{cache_key}': {e}");
                Ok(None)
            }
        }
    }

    async fn put(
        &self,
        cache_key: String,
        response: HttpResponse,
        policy: CachePolicy,
    ) -> Result<HttpResponse> {
        let data = Store { response, policy };
        let bytes = postcard::to_allocvec(&data)?;
        let mut write_txn = self.flush.db.begin_write()?;
        write_txn.set_durability(Durability::None)?;
        match self.max_bytes {
            Some(max_bytes) => {
                let mut limited = Limited::open(&write_txn, &TABLES)?;
                let size = bytes.len() as u64;
                // An entry over the limit is not stored, and must not leave
                // an older response in its place.
                if size > max_bytes {
                    limited.remove(&cache_key)?;
                } else {
                    limited.insert(&cache_key, &bytes, size)?;
                    evict(&mut limited, max_bytes)?;
                }
            }
            None => {
                let mut table = write_txn.open_table(TABLE)?;
                table.insert(cache_key.as_str(), bytes.as_slice())?;
            }
        }
        write_txn.commit()?;
        self.flush.record_write();
        Ok(data.response)
    }

    async fn delete(&self, cache_key: &str) -> Result<()> {
        // Deletes commit durably: an invalidation rolled back by a crash
        // would resurrect an entry the origin already replaced.
        let write_txn = self.flush.db.begin_write()?;
        if self.max_bytes.is_some() {
            Limited::open(&write_txn, &TABLES)?.remove(cache_key)?;
        } else {
            let mut table = write_txn.open_table(TABLE)?;
            table.remove(cache_key)?;
        }
        write_txn.commit()?;
        Ok(())
    }
}
