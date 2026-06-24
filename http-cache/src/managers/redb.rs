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

use std::path::Path;
use std::sync::Arc;

use crate::{CacheManager, HttpResponse, Result};

use http_cache_semantics::CachePolicy;
use redb::{Database, ReadableDatabase, TableDefinition};
use serde::{Deserialize, Serialize};

pub(crate) const TABLE: TableDefinition<&str, &[u8]> =
    TableDefinition::new("http_cache_v1");

/// Implements [`CacheManager`] with [`redb`](https://github.com/cberner/redb)
/// as the backend — a pure-Rust, embedded, persistent key/value store.
///
/// All operations are synchronous and require no async runtime, so this
/// manager works under any executor (tokio, smol, Bevy, …) and adds no
/// `tokio` dependency.
#[cfg_attr(docsrs, doc(cfg(feature = "manager-redb")))]
#[derive(Clone)]
pub struct RedbManager {
    /// The shared, embedded redb database.
    db: Arc<Database>,
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
        // Ensure the table exists up front so reads never fail with
        // "table does not exist".
        let write_txn = db.begin_write()?;
        {
            let _table = write_txn.open_table(TABLE)?;
        }
        write_txn.commit()?;
        Ok(Self { db })
    }

    /// Clears out the entire cache.
    pub async fn clear(&self) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        write_txn.delete_table(TABLE)?;
        // Recreate the (now empty) table so subsequent reads still succeed.
        {
            let _table = write_txn.open_table(TABLE)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    /// Reads the raw stored bytes for `cache_key`, if present.
    fn read_entry(&self, cache_key: &str) -> Result<Option<Vec<u8>>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(TABLE)?;
        Ok(table.get(cache_key)?.map(|guard| guard.value().to_vec()))
    }
}

impl CacheManager for RedbManager {
    async fn get(
        &self,
        cache_key: &str,
    ) -> Result<Option<(HttpResponse, CachePolicy)>> {
        // A storage-level read error is treated as a cache miss (degrade to a
        // fresh fetch) rather than a hard error, matching `CACacheManager`.
        let bytes = match self.read_entry(cache_key) {
            Ok(Some(bytes)) => bytes,
            Ok(None) => return Ok(None),
            Err(e) => {
                log::debug!("redb read failed for key '{cache_key}': {e}");
                return Ok(None);
            }
        };
        match postcard::from_bytes::<Store>(&bytes) {
            Ok(store) => Ok(Some((store.response, store.policy))),
            Err(e) => {
                // Treat undecodable entries as a miss rather than an error,
                // matching the other managers.
                log::debug!(
                    "Failed to deserialize cache entry for key '{cache_key}': {e}"
                );
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
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(TABLE)?;
            table.insert(cache_key.as_str(), bytes.as_slice())?;
        }
        write_txn.commit()?;
        Ok(data.response)
    }

    async fn delete(&self, cache_key: &str) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(TABLE)?;
            table.remove(cache_key)?;
        }
        write_txn.commit()?;
        Ok(())
    }
}
