//! Write order and total size of the entries in a redb table, for the
//! managers that limit their size.

use crate::Result;

use redb::{
    ReadableTable, ReadableTableMetadata, Table, TableDefinition,
    WriteTransaction,
};

/// An entry table and its index: `index` maps a key to its sequence and
/// size, `order` a sequence to its key, and `bytes` holds the sum of the
/// indexed sizes.
pub(crate) struct Tables {
    pub(crate) entries: TableDefinition<'static, &'static str, &'static [u8]>,
    pub(crate) index: TableDefinition<'static, &'static str, (u64, u64)>,
    pub(crate) order: TableDefinition<'static, u64, &'static str>,
    pub(crate) bytes: TableDefinition<'static, (), u64>,
}

impl Tables {
    /// Deletes the index and keeps the entries.
    pub(crate) fn delete_index(&self, txn: &WriteTransaction) -> Result<()> {
        txn.delete_table(self.index)?;
        txn.delete_table(self.order)?;
        txn.delete_table(self.bytes)?;
        Ok(())
    }
}

/// The entries of a size-limited cache with their index. All change in one
/// transaction.
pub(crate) struct Limited<'txn> {
    entries: Table<'txn, &'static str, &'static [u8]>,
    index: Table<'txn, &'static str, (u64, u64)>,
    order: Table<'txn, u64, &'static str>,
    bytes: Table<'txn, (), u64>,
}

impl<'txn> Limited<'txn> {
    pub(crate) fn open(
        txn: &'txn WriteTransaction,
        tables: &Tables,
    ) -> Result<Self> {
        Ok(Self {
            entries: txn.open_table(tables.entries)?,
            index: txn.open_table(tables.index)?,
            order: txn.open_table(tables.order)?,
            bytes: txn.open_table(tables.bytes)?,
        })
    }

    fn total(&self) -> Result<u64> {
        Ok(self.bytes.get(())?.map_or(0, |total| total.value()))
    }

    /// Indexes the entries from scratch when the entry and index row counts
    /// differ, as they do after entries were stored or removed without the
    /// index. `size` gives the size of an entry from its value. The write
    /// order is unknown, so the entries are ordered by key.
    pub(crate) fn index(&mut self, size: impl Fn(&[u8]) -> u64) -> Result<()> {
        if self.entries.len()? == self.index.len()? {
            return Ok(());
        }
        self.index.retain(|_, _| false)?;
        self.order.retain(|_, _| false)?;
        let mut total = 0;
        for (seq, entry) in (0..).zip(self.entries.iter()?) {
            let (key, value) = entry?;
            let size = size(value.value());
            self.index.insert(key.value(), (seq, size))?;
            self.order.insert(seq, key.value())?;
            total += size;
        }
        self.bytes.insert((), total)?;
        Ok(())
    }

    /// Removes the entry and index row of `key`, whichever of them exist.
    pub(crate) fn remove(&mut self, key: &str) -> Result<()> {
        self.entries.remove(key)?;
        let row = self.index.remove(key)?.map(|row| row.value());
        if let Some((seq, size)) = row {
            self.order.remove(seq)?;
            self.bytes.insert((), self.total()?.saturating_sub(size))?;
        }
        Ok(())
    }

    /// Stores `value` under `key` as the newest entry, counted as `size`,
    /// replacing any entry already there.
    pub(crate) fn insert(
        &mut self,
        key: &str,
        value: &[u8],
        size: u64,
    ) -> Result<()> {
        self.remove(key)?;
        let seq = self.order.last()?.map_or(0, |(seq, _)| seq.value() + 1);
        self.entries.insert(key, value)?;
        self.index.insert(key, (seq, size))?;
        self.order.insert(seq, key)?;
        self.bytes.insert((), self.total()? + size)?;
        Ok(())
    }

    /// The keys of the entries larger than `max_bytes`.
    pub(crate) fn oversized(&self, max_bytes: u64) -> Result<Vec<String>> {
        let mut oversized = Vec::new();
        // Without an oversized total there is no oversized entry to scan for.
        if self.total()? > max_bytes {
            for row in self.index.iter()? {
                let (key, value) = row?;
                if value.value().1 > max_bytes {
                    oversized.push(key.value().to_string());
                }
            }
        }
        Ok(oversized)
    }

    /// Removes the entry written longest ago and returns its key, if the
    /// indexed entries exceed `max_bytes`.
    pub(crate) fn pop_oldest(
        &mut self,
        max_bytes: u64,
    ) -> Result<Option<String>> {
        if self.total()? <= max_bytes {
            return Ok(None);
        }
        let oldest =
            self.order.pop_first()?.map(|(_, key)| key.value().to_string());
        if let Some(key) = &oldest {
            self.remove(key)?;
        }
        Ok(oldest)
    }
}
