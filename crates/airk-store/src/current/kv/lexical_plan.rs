// SPDX-License-Identifier: Apache-2.0
//! Quota-owned lexical planning. Only captured bytes cross Prepared; source and
//! tokenization work are finished before that durable boundary.
use super::{
    Snapshot,
    directory::{self, Budget, ReadPages, WritePages},
    journal::{Operation, Table},
    lexical::{self, Index, Manifest, ReadLexical, WriteLexical},
    tables,
};
use crate::{
    current::{CurrentScope, MAX_PENDING_MAP_BYTES, MAX_PENDING_MAP_CHANGES},
    quota::{PrivateKv, PrivateTable, Reservation},
};
use airk_core::{Artifact, identity::Sha256Digest};
use anyhow::{Context, Result, ensure};
use std::{cell::Cell, path::Path};

pub struct Plan<'a> {
    base: &'a Snapshot,
    scope: CurrentScope,
    storage: PrivateKv,
    manifest: Manifest,
    failed: Cell<bool>,
    rows: Cell<u64>,
    bytes: Cell<u64>,
}
pub struct Sealed {
    storage: PrivateKv,
    manifest: Manifest,
    rows: u64,
    bytes: u64,
}
fn decode_operation(bytes: &[u8]) -> Result<Option<&[u8]>> {
    ensure!(
        !bytes.is_empty() && bytes.len() <= 65537,
        "invalid lexical staged operation size"
    );
    match bytes[0] {
        0 if bytes.len() == 1 => Ok(None),
        1 => Ok(Some(&bytes[1..])),
        _ => anyhow::bail!("invalid lexical staged operation marker"),
    }
}
impl<'a> Plan<'a> {
    pub fn new(
        base: &'a Snapshot,
        reservation: &Reservation,
        path: impl AsRef<Path>,
        capacity: u64,
    ) -> Result<Self> {
        let head = base.head()?;
        let meta = base.transaction().open_table(tables::META)?;
        let bytes = meta.get(2)?.context("lexical manifest missing")?;
        let manifest = Manifest::decode(bytes.value(), head.lexical_commit, &head.scope)?;
        Ok(Self {
            base,
            scope: head.scope,
            storage: reservation.open_private_kv(path, capacity)?,
            manifest,
            failed: Cell::new(false),
            rows: Cell::new(0),
            bytes: Cell::new(0),
        })
    }
    pub fn replace(
        &mut self,
        path: &str,
        artifact: Option<&Artifact>,
        budget: &mut Budget,
    ) -> Result<()> {
        ensure!(!self.failed.get(), "lexical plan is poisoned");
        let mut profile = super::profile_start();
        let mut next = self.manifest.clone();
        let result = Index::new(&self.scope)?.replace(self, &mut next, path, artifact, budget);
        if result.is_err() {
            self.failed.set(true);
        }
        result?;
        self.manifest = next;
        super::profile_tick("lexical.replace", &mut profile);
        Ok(())
    }
    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }
    fn base_contains(&self, table: PrivateTable, key: &[u8]) -> Result<bool> {
        Ok(match table {
            PrivateTable::LexicalPages => self
                .base
                .transaction()
                .open_table(tables::LEX_DIRECTORY)?
                .get(key)?
                .is_some(),
            PrivateTable::LexicalDocuments => self
                .base
                .transaction()
                .open_table(tables::LEX_DOCUMENT)?
                .get(u64::from_be_bytes(key.try_into()?))?
                .is_some(),
            PrivateTable::LexicalBlocks => self
                .base
                .transaction()
                .open_table(tables::LEX_BLOCK)?
                .get(key)?
                .is_some(),
            _ => anyhow::bail!("invalid lexical staging domain"),
        })
    }
    fn set(&self, table: PrivateTable, key: &[u8], value: Option<&[u8]>) -> Result<()> {
        ensure!(!self.failed.get(), "lexical plan is poisoned");
        let result = (|| {
            ensure!(
                value.is_none_or(|v| v.len() <= 65536),
                "oversized lexical staged value"
            );
            let previous = self.storage.get(table, key)?;
            let previous_bytes = previous
                .as_ref()
                .map_or(0, |v| (key.len() + v.len() + 1) as u64);
            let retain = value.is_some() || self.base_contains(table, key)?;
            let next_bytes = if retain {
                (key.len() + 2 + value.map_or(0, <[u8]>::len)) as u64
            } else {
                0
            };
            let rows = self.rows.get() - u64::from(previous.is_some()) + u64::from(retain);
            let bytes = self.bytes.get() - previous_bytes + next_bytes;
            ensure!(
                rows <= MAX_PENDING_MAP_CHANGES as u64 && bytes <= MAX_PENDING_MAP_BYTES as u64,
                "lexical afterimage budget exceeded"
            );
            if retain {
                let mut encoded = Vec::with_capacity(value.map_or(1, |v| v.len() + 1));
                encoded.push(u8::from(value.is_some()));
                if let Some(value) = value {
                    encoded.extend_from_slice(value);
                }
                self.storage.put(table, key, &encoded)?;
            } else {
                self.storage.remove(table, key)?;
            }
            self.rows.set(rows);
            self.bytes.set(bytes);
            Ok(())
        })();
        if result.is_err() {
            self.failed.set(true);
        }
        result
    }
    pub fn seal(mut self, budget: &mut Budget) -> Result<Sealed> {
        ensure!(!self.failed.get(), "poisoned lexical plan cannot seal");
        let mut profile = super::profile_start();
        self.manifest.canonical_bytes()?;
        Index::new(&self.scope)?.verify(&self, &self.manifest, budget)?;
        super::profile_tick("lexical.seal_verify", &mut profile);
        self.storage.seal()?;
        super::profile_tick("lexical.seal_commit", &mut profile);
        Ok(Sealed {
            storage: self.storage,
            manifest: self.manifest,
            rows: self.rows.get(),
            bytes: self.bytes.get(),
        })
    }
}
impl ReadPages for Plan<'_> {
    fn read_page(&self, digest: Sha256Digest) -> Result<Vec<u8>> {
        if let Some(bytes) = self
            .storage
            .get(PrivateTable::LexicalPages, digest.as_bytes())?
        {
            return Ok(decode_operation(&bytes)?
                .context("lexical page retired in plan")?
                .to_vec());
        }
        self.base.transaction().read_page(digest)
    }
}
impl WritePages for Plan<'_> {
    fn write_page(&self, digest: Sha256Digest, bytes: &[u8]) -> Result<()> {
        self.set(PrivateTable::LexicalPages, digest.as_bytes(), Some(bytes))
    }
    fn retire_page(&self, digest: Sha256Digest) -> Result<()> {
        self.set(PrivateTable::LexicalPages, digest.as_bytes(), None)
    }
}
impl ReadLexical for Plan<'_> {
    fn document(&self, id: u64, budget: &mut Budget) -> Result<Vec<u8>> {
        ensure!(
            budget.bytes >= lexical::MAX_DOCUMENT_BYTES as u64,
            directory::Exhausted("document byte")
        );
        if let Some(encoded) = self
            .storage
            .get(PrivateTable::LexicalDocuments, &id.to_be_bytes())?
        {
            let bytes = decode_operation(&encoded)?.context("lexical document retired in plan")?;
            ensure!(
                budget.bytes >= lexical::MAX_DOCUMENT_BYTES as u64,
                directory::Exhausted("document byte")
            );
            budget.bytes -= bytes.len() as u64;
            return Ok(bytes.to_vec());
        }
        self.base.transaction().document(id, budget)
    }
}
impl WriteLexical for Plan<'_> {
    fn put_document(&self, id: u64, bytes: Option<&[u8]>) -> Result<()> {
        self.set(PrivateTable::LexicalDocuments, &id.to_be_bytes(), bytes)
    }
}
impl super::posting::ReadPostings for Plan<'_> {
    fn read_block(&self, key: &[u8; 40], budget: &mut Budget) -> Result<Vec<u8>> {
        budget.admit_page()?;
        if let Some(encoded) = self.storage.get(PrivateTable::LexicalBlocks, key)? {
            let bytes = decode_operation(&encoded)?.context("posting block retired in plan")?;
            budget.page()?;
            budget.bytes -= bytes.len() as u64;
            return Ok(bytes.to_vec());
        }
        super::posting::ReadPostings::read_block(self.base.transaction(), key, budget)
    }
}
impl super::posting::WritePostings for Plan<'_> {
    fn put_block(&self, key: &[u8; 40], bytes: Option<&[u8]>) -> Result<()> {
        self.set(PrivateTable::LexicalBlocks, key, bytes)
    }
}
impl Sealed {
    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }
    /// Stream in the journal's canonical table/key order, with bounded pages.
    pub fn visit(&self, visitor: &mut dyn FnMut(&Operation) -> Result<()>) -> Result<()> {
        let mut rows = 0u64;
        let mut bytes = 0u64;
        for (role, table) in [
            (PrivateTable::LexicalPages, Table::LexDirectory),
            (PrivateTable::LexicalBlocks, Table::LexBlock),
            (PrivateTable::LexicalDocuments, Table::LexDocument),
        ] {
            let mut after = None;
            loop {
                let page = self.storage.page(role, after.as_deref(), 32)?;
                if page.is_empty() {
                    break;
                }
                for (key, encoded) in page {
                    let value = decode_operation(&encoded)?;
                    rows += 1;
                    bytes += (key.len() + encoded.len() + 1) as u64;
                    ensure!(
                        rows <= self.rows && bytes <= self.bytes,
                        "lexical staging accounting mismatch"
                    );
                    visitor(&Operation {
                        table,
                        key: key.clone(),
                        value: value.map(<[u8]>::to_vec),
                    })?;
                    after = Some(key);
                }
            }
        }
        ensure!(
            rows == self.rows && bytes == self.bytes,
            "lexical staging rows missing"
        );
        visitor(&Operation {
            table: Table::LexicalManifest,
            key: vec![],
            value: Some(self.manifest.canonical_bytes()?),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Database, lexical::QueryWork};
    use super::*;
    use crate::current::{CurrentHead, map};
    use airk_core::{
        Chunk, DocumentLocation, SearchQuery,
        graph_storage::{GraphHead, GraphScope},
        identity::Incarnation,
    };
    use redb::ReadableTable;
    use std::sync::atomic::AtomicBool;
    fn budget() -> Budget {
        Budget {
            pages: 1_000_000,
            bytes: 16 << 30,
            entries: 16_000_000,
        }
    }
    fn artifact(text: &str) -> Artifact {
        Artifact {
            path: "a.rs".into(),
            content_hash: Sha256Digest::from_bytes([1; 32]).to_string(),
            bytes: text.len(),
            chunks: vec![Chunk {
                text: text.into(),
                start_line: 1,
                end_line: 1,
                location: DocumentLocation::Lines { start: 1, end: 1 },
            }],
            language: "rust".into(),
            extraction_method: "fixture".into(),
            parse_status: "ok".into(),
            facts: vec![],
            retained: None,
        }
    }
    fn initialize(db: &Database) -> Result<CurrentHead> {
        let scope = CurrentScope {
            root: "fixture/lexical-plan".into(),
            collection: "lexical".into(),
            incarnation: Incarnation::from_bytes([3; 16]),
        };
        let manifest = Manifest::empty(&scope)?;
        let head = CurrentHead {
            version: 2,
            sequence: 0,
            lexical_sequence: 0,
            artifact_root: map::empty_root(&scope)?,
            lexical_commit: manifest.digest()?,
            graph_head: GraphHead::empty(GraphScope {
                root: scope.root.clone(),
                collection: scope.collection.clone(),
                incarnation: scope.incarnation,
                graph_instance: Incarnation::from_bytes([4; 16]),
                specification: Sha256Digest::from_bytes([5; 32]),
                record_bytes: 32,
            })?,
            scope,
        };
        db.initialize_head(&head)?;
        let tx = db.write()?;
        tx.open_table(tables::META)?
            .insert(2, manifest.canonical_bytes()?.as_slice())?;
        tx.commit()?;
        Ok(head)
    }
    fn verify(tx: &redb::WriteTransaction, head: &CurrentHead) -> Result<()> {
        let meta = tx.open_table(tables::META)?;
        let encoded = meta.get(2)?.context("manifest missing")?;
        let manifest = Manifest::decode(encoded.value(), head.lexical_commit, &head.scope)?;
        drop(encoded);
        drop(meta);
        Index::new(&head.scope)?.verify(tx, &manifest, &mut budget())
    }
    fn count(snapshot: &Snapshot, text: &str) -> Result<usize> {
        let head = snapshot.head()?;
        let meta = snapshot.transaction().open_table(tables::META)?;
        let manifest = Manifest::decode(
            meta.get(2)?.unwrap().value(),
            head.lexical_commit,
            &head.scope,
        )?;
        let cancelled = AtomicBool::new(false);
        Ok(Index::new(&head.scope)?
            .search(
                snapshot.transaction(),
                &manifest,
                &SearchQuery {
                    text: text.into(),
                    path_prefix: None,
                    limit: 10,
                    max_bytes: 65536,
                },
                &mut QueryWork::new(&cancelled),
            )?
            .values
            .len())
    }
    #[test]
    fn sealed_afterimages_publish_without_staging_or_source_and_keep_reads_during_applying()
    -> Result<()> {
        let file = tempfile::NamedTempFile::new()?;
        let db = Database::open(file.reopen()?, 32 << 20, true)?;
        let mut head = initialize(&db)?;
        let root = tempfile::tempdir()?;
        let quota = crate::quota::Quota::open(root.path())?;
        for text in ["first value", "second replacement"] {
            let base = db.snapshot()?;
            let reservation = quota.reserve("lexical", 8 << 20)?;
            let stage = reservation.staging_directory()?.join("lexical.airk");
            let mut plan = Plan::new(&base, &reservation, &stage, 8 << 20)?;
            plan.replace("a.rs", Some(&artifact(text)), &mut budget())?;
            // Repeated updates discard superseded private pages before sealing.
            plan.replace("a.rs", Some(&artifact(text)), &mut budget())?;
            let sealed = plan.seal(&mut budget())?;
            let mut target = head.clone();
            target.sequence += 1;
            target.lexical_sequence = target.sequence;
            target.lexical_commit = sealed.manifest.digest()?;
            let pending = db.prepare_stream(head.token()?, target.clone(), |v| sealed.visit(v))?;
            assert_eq!(count(&db.snapshot()?, "second")?, 0);
            db.mark_applying(&pending)?;
            assert_eq!(count(&db.snapshot()?, "second")?, 0);
            drop(sealed);
            drop(reservation);
            assert!(!stage.exists(), "recovery must not need private staging");
            db.apply(&pending, verify)?;
            assert_eq!(
                count(&db.snapshot()?, text.split_whitespace().next().unwrap())?,
                1
            );
            assert_eq!(base.head()?, head);
            assert_eq!(count(&base, "second")?, 0);
            assert_eq!(db.apply(&pending, verify)?, target.token()?);
            head = target;
        }
        Ok(())
    }
    #[test]
    fn poison_and_captured_corruption_cannot_seal_or_change_the_live_head() -> Result<()> {
        let file = tempfile::NamedTempFile::new()?;
        let db = Database::open(file.reopen()?, 32 << 20, true)?;
        let head = initialize(&db)?;
        let base = db.snapshot()?;
        let root = tempfile::tempdir()?;
        let quota = crate::quota::Quota::open(root.path())?;
        let reservation = quota.reserve("lexical", 16 << 20)?;
        let mut plan = Plan::new(
            &base,
            &reservation,
            reservation.staging_directory()?.join("bad.airk"),
            8 << 20,
        )?;
        let mut exhausted = budget();
        exhausted.pages = 0;
        // An empty index needs no old page to read, so reject an invalid source
        // binding to exercise the same terminal plan state deterministically.
        assert!(
            plan.replace("different.rs", Some(&artifact("first")), &mut exhausted)
                .is_err()
        );
        assert!(plan.seal(&mut budget()).is_err());
        let mut plan = Plan::new(
            &base,
            &reservation,
            reservation.staging_directory()?.join("corrupt.airk"),
            1 << 20,
        )?;
        plan.replace("a.rs", Some(&artifact("first")), &mut budget())?;
        plan.storage
            .transaction()
            .open_table(PrivateTable::LexicalDocuments.definition())?
            .insert(0u64.to_be_bytes().as_slice(), b"\x01{}".as_slice())?;
        assert!(plan.seal(&mut budget()).is_err());
        assert_eq!(db.snapshot()?.head()?, head);
        assert!(db.pending_publication()?.is_none());
        Ok(())
    }
    #[test]
    fn reopen_replays_captured_lexical_pages_from_each_pending_phase() -> Result<()> {
        for applying in [false, true] {
            let file = tempfile::NamedTempFile::new()?;
            let db = Database::open(file.reopen()?, 32 << 20, true)?;
            let head = initialize(&db)?;
            let root = tempfile::tempdir()?;
            let quota = crate::quota::Quota::open(root.path())?;
            let reservation = quota.reserve("lexical", 8 << 20)?;
            let base = db.snapshot()?;
            let mut plan = Plan::new(
                &base,
                &reservation,
                reservation.staging_directory()?.join("reopen.airk"),
                8 << 20,
            )?;
            plan.replace("a.rs", Some(&artifact("captured restart")), &mut budget())?;
            let sealed = plan.seal(&mut budget())?;
            let mut target = head.clone();
            target.sequence += 1;
            target.lexical_sequence = 1;
            target.lexical_commit = sealed.manifest.digest()?;
            let expected = db.prepare_stream(head.token()?, target.clone(), |v| sealed.visit(v))?;
            if applying {
                db.mark_applying(&expected)?;
            }
            drop(sealed);
            drop(reservation);
            drop(base);
            drop(db);
            let db = Database::open(file.reopen()?, 32 << 20, false)?;
            let pending = db.pending_publication()?.unwrap();
            assert_eq!(pending, expected);
            assert_eq!(db.snapshot()?.head()?, head);
            db.mark_applying(&pending)?;
            db.apply(&pending, verify)?;
            assert_eq!(count(&db.snapshot()?, "captured")?, 1);
            assert_eq!(db.snapshot()?.head()?, target);
        }
        Ok(())
    }
}
