// SPDX-License-Identifier: Apache-2.0
//! One full base verification, then bounded row-group rehashing under the writer
//! gate. A bounded process cache can reuse a physically verified base while
//! database identity/change witnesses remain stable (see cache.rs). Persisted
//! checksums alone never authorize reuse. WITHOUT ROWID tables are scanned because
//! SQLite's update hook does not report their changes.
use super::{ENGINE, LexicalManifest, MAX_INDEX_BYTES, MAX_ROW_BYTES};
use airk_core::identity::Sha256Digest;
use anyhow::{Context, Result, ensure};
use rusqlite::{Connection, Row, types::ValueRef};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Mutex},
};

const MAX_BUCKETS: usize = 1_048_576;
const ROW_TABLES: [&str; 2] = [
    "SELECT id,path,local,stable_id,payload,terms FROM current_lexical_docs NOT INDEXED",
    "SELECT id,block FROM current_fts_data NOT INDEXED",
];
const AUX_TABLES: [&str; 3] = [
    "SELECT type,name,tbl_name,sql FROM sqlite_schema WHERE name IN ('current_lexical_docs','current_lexical_paths','sqlite_autoindex_current_lexical_docs_1','current_fts','current_fts_data','current_fts_idx','current_fts_config') ORDER BY name",
    "SELECT segid,term,pgno FROM current_fts_idx NOT INDEXED ORDER BY segid,term",
    "SELECT k,v FROM current_fts_config NOT INDEXED ORDER BY k",
];

#[derive(Clone)]
struct DigestRow {
    digest: [u8; 32],
    bytes: u64,
}
#[derive(Clone)]
pub(crate) struct Snapshot {
    buckets: BTreeMap<(usize, i64), DigestRow>,
    auxiliary: Vec<DigestRow>,
}

fn add_row(hash: &mut Sha256, bytes: &mut u64, row: &Row<'_>, columns: usize) -> Result<()> {
    hash.update([1]);
    for column in 0..columns {
        match row.get_ref(column)? {
            ValueRef::Null => hash.update([0]),
            ValueRef::Integer(n) => {
                hash.update([1]);
                hash.update(n.to_be_bytes());
                *bytes += 8;
            }
            value @ (ValueRef::Text(_) | ValueRef::Blob(_)) => {
                let (tag, data) = match value {
                    ValueRef::Text(v) => (2, v),
                    ValueRef::Blob(v) => (3, v),
                    _ => unreachable!(),
                };
                ensure!(data.len() <= MAX_ROW_BYTES, "FTS5 row byte bound exceeded");
                hash.update([tag]);
                hash.update((data.len() as u64).to_be_bytes());
                hash.update(data);
                *bytes = bytes
                    .checked_add(data.len() as u64)
                    .context("FTS5 size overflow")?;
            }
            ValueRef::Real(_) => anyhow::bail!("unexpected floating-point FTS5 value"),
        }
        ensure!(*bytes <= MAX_INDEX_BYTES, "FTS5 index byte bound exceeded");
    }
    Ok(())
}
fn finish(hash: Sha256, bytes: u64) -> DigestRow {
    DigestRow {
        digest: hash.finalize().into(),
        bytes,
    }
}

impl Snapshot {
    pub(crate) fn cache_bytes(&self) -> usize {
        self.buckets
            .len()
            .saturating_mul(128)
            .saturating_add(self.auxiliary.len().saturating_mul(64))
            .saturating_add(4096)
    }
    pub(crate) fn capture(connection: &Connection) -> Result<Self> {
        let mut result = Self {
            buckets: BTreeMap::new(),
            auxiliary: Vec::new(),
        };
        for (table, sql) in ROW_TABLES.iter().enumerate() {
            let mut statement = connection.prepare(&format!("{sql} ORDER BY id"))?;
            let columns = statement.column_count();
            let mut rows = statement.query([])?;
            let mut current = None;
            let mut hash = Sha256::new();
            let mut bytes = 0;
            while let Some(row) = rows.next()? {
                let bucket = row.get::<_, i64>(0)? >> 8;
                if current != Some(bucket) {
                    if let Some(old) = current {
                        result.buckets.insert((table, old), finish(hash, bytes));
                    }
                    ensure!(
                        result.buckets.len() < MAX_BUCKETS,
                        "FTS5 seal bucket bound exceeded"
                    );
                    current = Some(bucket);
                    hash = Sha256::new();
                    bytes = 0;
                }
                add_row(&mut hash, &mut bytes, row, columns)?;
            }
            if let Some(bucket) = current {
                result.buckets.insert((table, bucket), finish(hash, bytes));
            }
        }
        result.capture_auxiliary(connection)?;
        Ok(result)
    }
    fn capture_auxiliary(&mut self, connection: &Connection) -> Result<()> {
        self.auxiliary.clear();
        for sql in AUX_TABLES {
            let mut statement = connection.prepare(sql)?;
            let columns = statement.column_count();
            let mut rows = statement.query([])?;
            let mut hash = Sha256::new();
            let mut bytes = 0;
            while let Some(row) = rows.next()? {
                add_row(&mut hash, &mut bytes, row, columns)?;
            }
            self.auxiliary.push(finish(hash, bytes));
        }
        Ok(())
    }
    pub(crate) fn manifest(&self, updates: Sha256Digest) -> Result<LexicalManifest> {
        let mut hash = Sha256::new();
        hash.update(b"airk-fts5-bucket-seal/2");
        let mut bytes = 0u64;
        for ((table, bucket), value) in &self.buckets {
            hash.update([0, *table as u8]);
            hash.update(bucket.to_be_bytes());
            hash.update(value.digest);
            hash.update(value.bytes.to_be_bytes());
            bytes = bytes
                .checked_add(value.bytes)
                .context("FTS5 size overflow")?;
        }
        for (table, value) in self.auxiliary.iter().enumerate() {
            hash.update([1, table as u8]);
            hash.update(value.digest);
            hash.update(value.bytes.to_be_bytes());
            bytes = bytes
                .checked_add(value.bytes)
                .context("FTS5 size overflow")?;
        }
        ensure!(bytes <= MAX_INDEX_BYTES, "FTS5 index byte bound exceeded");
        Ok(LexicalManifest {
            version: 2,
            engine: ENGINE.into(),
            root: Sha256Digest::from_bytes(hash.finalize().into()),
            bytes,
            updates,
        })
    }
    pub(crate) fn apply_tracked<T>(
        &self,
        connection: &Connection,
        apply: impl FnOnce() -> Result<T>,
    ) -> Result<(Self, T)> {
        let touched = Arc::new(Mutex::new((BTreeSet::new(), false)));
        let changes = touched.clone();
        connection.update_hook(Some(
            move |_: rusqlite::hooks::Action, db: &str, table: &str, id: i64| {
                let table = match (db, table) {
                    ("main", "current_lexical_docs") => 0,
                    ("main", "current_fts_data") => 1,
                    _ => return,
                };
                let mut changes = changes.lock().expect("FTS5 change tracker");
                if changes.0.len() == MAX_BUCKETS {
                    changes.1 = true;
                } else {
                    changes.0.insert((table, id >> 8));
                }
            },
        ))?;
        let applied = apply();
        connection.update_hook(None::<fn(rusqlite::hooks::Action, &str, &str, i64)>)?;
        let value = applied?;
        let touched = touched
            .lock()
            .map_err(|_| anyhow::anyhow!("FTS5 change tracker poisoned"))?;
        ensure!(!touched.1, "FTS5 changed bucket bound exceeded");
        let mut next = self.clone();
        for &(table, bucket) in &touched.0 {
            let low = bucket << 8;
            let high = low | 255;
            let mut statement = connection.prepare(&format!(
                "{} WHERE id BETWEEN ?1 AND ?2 ORDER BY id",
                ROW_TABLES[table]
            ))?;
            let columns = statement.column_count();
            let mut rows = statement.query([low, high])?;
            let mut hash = Sha256::new();
            let mut bytes = 0;
            let mut present = false;
            while let Some(row) = rows.next()? {
                present = true;
                add_row(&mut hash, &mut bytes, row, columns)?;
            }
            if present {
                next.buckets.insert((table, bucket), finish(hash, bytes));
            } else {
                next.buckets.remove(&(table, bucket));
            }
        }
        ensure!(
            next.buckets.len() <= MAX_BUCKETS,
            "FTS5 seal bucket bound exceeded"
        );
        next.capture_auxiliary(connection)?;
        Ok((next, value))
    }
}
