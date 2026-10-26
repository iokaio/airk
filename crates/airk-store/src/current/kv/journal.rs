// SPDX-License-Identifier: Apache-2.0
//! Durable afterimages and intent transitions; callers retain object/grant ownership.
use super::{Database, Snapshot, tables};
use crate::current::{CurrentHead, CurrentToken, MAX_PENDING_MAP_BYTES, MAX_PENDING_MAP_CHANGES};
use airk_core::identity::{Sha256Digest, framed_digest};
use anyhow::{Context, Result, ensure};
use redb::{ReadableTable, ReadableTableMetadata};
use serde::{Deserialize, Serialize};

const HEAD: u8 = 0;
const PHASE: u8 = 1;
const DESCRIPTOR: u8 = 4;
const DESCRIPTOR_HASH: u8 = 5;
const READY: &[u8] = b"ready";
const PREPARED: &[u8] = b"prepared";
const APPLYING: &[u8] = b"applying";

/// Closed table domain prevents journal rows from mutating their own descriptor/head.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Table {
    MapBucket = 0,
    MapNode = 1,
    GraphRecord = 2,
    GraphNode = 3,
    LexTerm = 4,
    LexDirectory = 5,
    LexBlock = 6,
    LexDocument = 7,
    LexPath = 8,
    LexForward = 9,
    LexicalManifest = 10,
    CapturedInput = 11,
}
impl TryFrom<u8> for Table {
    type Error = anyhow::Error;
    fn try_from(value: u8) -> Result<Self> {
        Ok(match value {
            0 => Self::MapBucket,
            1 => Self::MapNode,
            2 => Self::GraphRecord,
            3 => Self::GraphNode,
            4 => Self::LexTerm,
            5 => Self::LexDirectory,
            6 => Self::LexBlock,
            7 => Self::LexDocument,
            8 => Self::LexPath,
            9 => Self::LexForward,
            10 => Self::LexicalManifest,
            11 => Self::CapturedInput,
            _ => anyhow::bail!("unknown redb pending table"),
        })
    }
}
pub struct Operation {
    pub table: Table,
    pub key: Vec<u8>,
    pub value: Option<Vec<u8>>,
}
impl Operation {
    fn encoded_key(&self) -> Result<Vec<u8>> {
        ensure!(self.key.len() <= 8192, "pending key exceeds bound");
        let mut key = vec![self.table as u8];
        key.extend_from_slice(&self.key);
        Ok(key)
    }
    fn encoded_value(&self) -> Result<Vec<u8>> {
        ensure!(
            self.value.as_ref().is_none_or(|v| v.len() <= 512 << 10),
            "pending afterimage exceeds bound"
        );
        let mut result = vec![u8::from(self.value.is_some())];
        if let Some(value) = &self.value {
            result.extend_from_slice(value);
        }
        Ok(result)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Prepared {
    pub version: u32,
    pub expected: CurrentToken,
    pub target: CurrentHead,
    pub operations: u64,
    pub bytes: u64,
    pub digest: Sha256Digest,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intent: Option<crate::current::PendingTransaction>,
}

fn value(transaction: &redb::WriteTransaction, key: u8) -> Result<Vec<u8>> {
    let table = transaction.open_table(tables::META)?;
    let value = table.get(key)?.context("redb journal metadata missing")?;
    let maximum = match key {
        HEAD => crate::current::MAX_HEAD_BYTES,
        PHASE => 8,
        DESCRIPTOR => 512 << 10,
        DESCRIPTOR_HASH => 32,
        _ => anyhow::bail!("unsupported journal metadata slot"),
    };
    ensure!(
        value.value().len() <= maximum,
        "oversized redb journal metadata"
    );
    Ok(value.value().to_vec())
}
fn current(transaction: &redb::WriteTransaction) -> Result<CurrentHead> {
    let bytes = value(transaction, HEAD)?;
    ensure!(
        bytes.len() <= crate::current::MAX_HEAD_BYTES,
        "oversized redb head"
    );
    let head: CurrentHead = serde_json::from_slice(&bytes)?;
    ensure!(head.canonical_bytes()? == bytes, "noncanonical redb head");
    Ok(head)
}
fn pending(transaction: &redb::WriteTransaction) -> Result<Prepared> {
    let bytes = value(transaction, DESCRIPTOR)?;
    ensure!(
        bytes.len() <= 512 << 10,
        "oversized redb pending descriptor"
    );
    let descriptor: Prepared = serde_json::from_slice(&bytes)?;
    ensure!(
        value(transaction, DESCRIPTOR_HASH)?.as_slice() == identity(&descriptor)?.as_bytes(),
        "redb pending descriptor digest mismatch"
    );
    descriptor.target.validate()?;
    ensure!(
        descriptor.version == 1 && serde_json::to_vec(&descriptor)? == bytes,
        "invalid redb pending descriptor"
    );
    ensure!(
        descriptor.expected == current(transaction)?.token()?,
        "redb pending base mismatch"
    );
    if let Some(intent) = &descriptor.intent {
        intent.bytes()?;
        ensure!(
            intent.expected == descriptor.expected && intent.target == descriptor.target,
            "redb native intention differs from publication"
        );
    }
    Ok(descriptor)
}
fn operations_digest(table: &redb::Table<'_, &[u8], &[u8]>) -> Result<(u64, u64, Sha256Digest)> {
    use sha2::{Digest, Sha256};
    let mut hash = Sha256::new();
    hash.update(b"airk-kv-afterimages/1");
    let mut count = 0u64;
    let mut bytes = 0u64;
    for row in table.iter()? {
        let (key, value) = row?;
        let key = key.value();
        let value = value.value();
        ensure!(
            !key.is_empty()
                && key.len() <= 8193
                && !value.is_empty()
                && value.len() <= (512 << 10) + 1,
            "invalid pending row bounds"
        );
        Table::try_from(key[0])?;
        ensure!(
            value[0] == 1 || value == [0],
            "invalid pending put/delete marker"
        );
        count += 1;
        bytes = bytes
            .checked_add((key.len() + value.len()) as u64)
            .context("pending bytes overflow")?;
        ensure!(
            count <= MAX_PENDING_MAP_CHANGES as u64 && bytes <= MAX_PENDING_MAP_BYTES as u64,
            "pending journal budget exceeded"
        );
        hash.update((key.len() as u64).to_be_bytes());
        hash.update(key);
        hash.update((value.len() as u64).to_be_bytes());
        hash.update(value);
    }
    Ok((
        count,
        bytes,
        Sha256Digest::from_bytes(hash.finalize().into()),
    ))
}
pub(crate) fn captured_inputs(
    table: &impl ReadableTable<&'static [u8], &'static [u8]>,
    intent: Option<&crate::current::PendingTransaction>,
    visitor: &mut dyn FnMut(&crate::current::CapturedInput) -> Result<()>,
) -> Result<()> {
    let start = [Table::CapturedInput as u8];
    let end = [Table::CapturedInput as u8 + 1];
    let mut digest = crate::current::journal::InputDigest::new();
    for row in table.range::<&[u8]>(start.as_slice()..end.as_slice())? {
        let (key, value) = row?;
        let key = key.value();
        let value = value.value();
        ensure!(
            (2..=8193).contains(&key.len())
                && !value.is_empty()
                && value[0] == 1
                && value.len() <= crate::current::MAX_INPUT_METADATA_BYTES + 1,
            "invalid captured input row"
        );
        let input: crate::current::CapturedInput = serde_json::from_slice(&value[1..])?;
        ensure!(
            input.artifact.path.as_bytes() == &key[1..] && input.bytes()? == value[1..],
            "captured input identity/encoding mismatch"
        );
        digest.add(&input, &value[1..])?;
        visitor(&input)?;
    }
    if let Some(intent) = intent {
        ensure!(
            digest.count == intent.inputs_count
                && digest.bytes == intent.metadata_bytes
                && digest.objects == intent.object_bytes
                && digest.finish() == intent.inputs_digest,
            "captured input counts/digest mismatch"
        );
    } else {
        ensure!(
            digest.count == 0,
            "captured inputs require a native intention"
        );
    }
    Ok(())
}
fn verify_operations(transaction: &redb::WriteTransaction, expected: &Prepared) -> Result<()> {
    let actual = operations_digest(&transaction.open_table(tables::PENDING)?)?;
    ensure!(
        actual == (expected.operations, expected.bytes, expected.digest),
        "pending afterimages digest/count mismatch"
    );
    captured_inputs(
        &transaction.open_table(tables::PENDING)?,
        expected.intent.as_ref(),
        &mut |_| Ok(()),
    )?;
    Ok(())
}

impl Snapshot {
    pub fn phase(&self) -> Result<crate::current::TransactionPhase> {
        let meta = self.transaction().open_table(tables::META)?;
        let phase = meta.get(PHASE)?.context("redb phase missing")?;
        Ok(match phase.value() {
            READY => crate::current::TransactionPhase::Ready,
            PREPARED => crate::current::TransactionPhase::Prepared,
            APPLYING => crate::current::TransactionPhase::Applying,
            _ => anyhow::bail!("invalid redb transaction phase"),
        })
    }
    pub fn head(&self) -> Result<CurrentHead> {
        let meta = self.transaction().open_table(tables::META)?;
        let bytes = meta.get(HEAD)?.context("redb head missing")?;
        ensure!(
            bytes.value().len() <= crate::current::MAX_HEAD_BYTES,
            "oversized redb head"
        );
        let head: CurrentHead = serde_json::from_slice(bytes.value())?;
        ensure!(
            head.canonical_bytes()? == bytes.value(),
            "invalid redb head"
        );
        Ok(head)
    }
}
impl Database {
    pub fn initialize_head(&self, head: &CurrentHead) -> Result<()> {
        ensure!(head.sequence == 0, "redb initial sequence must be zero");
        let transaction = self.write()?;
        {
            let mut meta = transaction.open_table(tables::META)?;
            ensure!(meta.get(HEAD)?.is_none(), "redb head already initialized");
            meta.insert(HEAD, head.canonical_bytes()?.as_slice())?;
            meta.insert(PHASE, READY)?;
        }
        transaction.commit()?;
        Ok(())
    }
    pub fn prepare(
        &self,
        expected: CurrentToken,
        target: CurrentHead,
        operations: &[Operation],
    ) -> Result<Prepared> {
        self.prepare_stream(expected, target, |visitor| {
            for operation in operations {
                visitor(operation)?;
            }
            Ok(())
        })
    }
    /// Consume an ordered bounded stream without materializing the publication
    /// in RAM. A source/visitor error aborts this entire preparation transaction.
    pub fn prepare_stream(
        &self,
        expected: CurrentToken,
        target: CurrentHead,
        source: impl FnOnce(&mut dyn FnMut(&Operation) -> Result<()>) -> Result<()>,
    ) -> Result<Prepared> {
        self.prepare_intent_stream(expected, target, None, source)
    }
    pub fn prepare_intent_stream(
        &self,
        expected: CurrentToken,
        target: CurrentHead,
        intent: Option<crate::current::PendingTransaction>,
        source: impl FnOnce(&mut dyn FnMut(&Operation) -> Result<()>) -> Result<()>,
    ) -> Result<Prepared> {
        target.validate()?;
        let transaction = self.write()?;
        let base = current(&transaction)?;
        if let Some(intent) = &intent {
            intent.bytes()?;
            ensure!(
                intent.expected == expected
                    && intent.target == target
                    && intent.graph.base == base.graph_head,
                "native intention components differ from publication"
            );
        }
        ensure!(
            value(&transaction, PHASE)? == READY && base.token()? == expected,
            "redb prepare base expired or already pending"
        );
        ensure!(
            target.scope == base.scope
                && target.sequence == base.sequence.checked_add(1).context("sequence overflow")?,
            "redb target does not extend base"
        );
        let mut rows = transaction.open_table(tables::PENDING)?;
        ensure!(rows.is_empty()?, "orphan pending rows require recovery");
        let mut previous: Option<Vec<u8>> = None;
        let mut encoded_bytes = 0usize;
        let mut operations = 0usize;
        source(&mut |operation| {
            ensure!(
                operations < MAX_PENDING_MAP_CHANGES,
                "too many pending operations"
            );
            operations += 1;
            let key = operation.encoded_key()?;
            let value = operation.encoded_value()?;
            ensure!(
                previous.as_ref().is_none_or(|p| p < &key),
                "pending operations must be ordered and unique"
            );
            encoded_bytes = encoded_bytes
                .checked_add(key.len() + value.len())
                .context("pending bytes overflow")?;
            ensure!(
                encoded_bytes <= MAX_PENDING_MAP_BYTES,
                "pending operation byte budget exceeded"
            );
            rows.insert(key.as_slice(), value.as_slice())?;
            previous = Some(key);
            Ok(())
        })?;
        let (count, bytes, digest) = operations_digest(&rows)?;
        captured_inputs(&rows, intent.as_ref(), &mut |_| Ok(()))?;
        let descriptor = Prepared {
            version: 1,
            expected,
            target,
            operations: count,
            bytes,
            digest,
            intent,
        };
        drop(rows);
        {
            let mut meta = transaction.open_table(tables::META)?;
            meta.insert(DESCRIPTOR, serde_json::to_vec(&descriptor)?.as_slice())?;
            meta.insert(
                DESCRIPTOR_HASH,
                identity(&descriptor)?.as_bytes().as_slice(),
            )?;
            meta.insert(PHASE, PREPARED)?;
        }
        transaction.commit()?;
        Ok(descriptor)
    }
    pub fn visit_captured(
        &self,
        expected: &Prepared,
        visitor: &mut dyn FnMut(&crate::current::CapturedInput) -> Result<()>,
    ) -> Result<()> {
        let pending = self
            .pending_publication()?
            .context("captured publication missing")?;
        ensure!(pending == *expected, "captured publication changed");
        let snapshot = self.snapshot()?;
        ensure!(
            snapshot.head()?.token()? == expected.expected,
            "captured publication base expired"
        );
        captured_inputs(
            &snapshot.transaction().open_table(tables::PENDING)?,
            expected.intent.as_ref(),
            visitor,
        )
    }
    pub fn pending_publication(&self) -> Result<Option<Prepared>> {
        let transaction = self.write()?;
        let phase = value(&transaction, PHASE)?;
        if phase == READY {
            ensure!(
                transaction.open_table(tables::PENDING)?.is_empty()?
                    && transaction
                        .open_table(tables::META)?
                        .get(DESCRIPTOR)?
                        .is_none()
                    && transaction
                        .open_table(tables::META)?
                        .get(DESCRIPTOR_HASH)?
                        .is_none(),
                "ready redb state retains pending journal"
            );
            Ok(None)
        } else {
            ensure!(
                phase == PREPARED || phase == APPLYING,
                "invalid redb publication phase"
            );
            let descriptor = pending(&transaction)?;
            verify_operations(&transaction, &descriptor)?;
            Ok(Some(descriptor))
        }
    }
    pub fn mark_applying(&self, expected: &Prepared) -> Result<()> {
        let transaction = self.write()?;
        let phase = value(&transaction, PHASE)?;
        ensure!(
            (phase == PREPARED || phase == APPLYING) && pending(&transaction)? == *expected,
            "redb prepared intention changed"
        );
        verify_operations(&transaction, expected)?;
        transaction
            .open_table(tables::META)?
            .insert(PHASE, APPLYING)?;
        transaction.commit()?;
        Ok(())
    }
    /// Verification runs against installed afterimages before the atomic head commit.
    /// An error aborts all installations; external grant cleanup follows reconciliation.
    pub fn apply(
        &self,
        expected: &Prepared,
        verify: impl FnOnce(&redb::WriteTransaction, &CurrentHead) -> Result<()>,
    ) -> Result<CurrentToken> {
        let transaction = self.write()?;
        if value(&transaction, PHASE)? == READY && current(&transaction)? == expected.target {
            ensure!(
                transaction.open_table(tables::PENDING)?.is_empty()?
                    && transaction
                        .open_table(tables::META)?
                        .get(DESCRIPTOR)?
                        .is_none()
                    && transaction
                        .open_table(tables::META)?
                        .get(DESCRIPTOR_HASH)?
                        .is_none(),
                "completed publication retains pending rows"
            );
            return expected.target.token();
        }
        ensure!(
            value(&transaction, PHASE)? == APPLYING && pending(&transaction)? == *expected,
            "redb publication is not applying this intention"
        );
        stage_application(&transaction, expected, verify)?;
        transaction.commit()?;
        expected.target.token()
    }

    /// Exercise the exact captured installation while the old Prepared head
    /// remains committed. Sample allocator high-water before restoring exact
    /// roots; this is admission/verification, never a second publication.
    pub(crate) fn probe_application(
        &self,
        expected: &Prepared,
        verify: impl FnOnce(&redb::WriteTransaction, &CurrentHead) -> Result<()>,
        sample: impl FnOnce() -> Result<u64>,
    ) -> Result<u64> {
        let mut transaction = self.write()?;
        let savepoint = transaction.ephemeral_savepoint()?;
        ensure!(
            value(&transaction, PHASE)? == PREPARED
                && pending(&transaction)? == *expected
                && current(&transaction)?.token()? == expected.expected,
            "replay admission base changed"
        );
        stage_application(&transaction, expected, verify)?;
        let allocated = transaction.stats()?.allocated_pages();
        // Fund actual pages for separate marker/commit/recovery metadata too.
        // The pinned allocator stores eight-byte page numbers; this allows
        // multiple copies/tree overhead plus bounded head/descriptor pages.
        let padding = allocated
            .checked_mul(64)
            .and_then(|bytes| bytes.checked_add(2 << 20))
            .context("replay padding overflow")?;
        {
            let mut table = transaction.open_table(redb::TableDefinition::<u64, &[u8]>::new(
                "replay_admission_padding",
            ))?;
            // Leave space for the leaf/key header so a padding record fits
            // one 1MiB allocator block instead of rounding up to 2MiB.
            const CHUNK: usize = (1 << 20) - 4096;
            let bytes = vec![0u8; CHUNK];
            for index in 0..padding.div_ceil(CHUNK as u64) {
                table.insert(index, bytes.as_slice())?;
            }
        }
        let high_water = sample()?;
        // Restore exact Prepared roots, then persist allocator bookkeeping. Abort
        // rolls growth back and cannot establish the same retained capacity.
        transaction.restore_savepoint(&savepoint)?;
        drop(savepoint);
        transaction.commit()?;
        Ok(high_water)
    }

    /// The owner invokes this only for an explicitly canceled, unapplied intention.
    /// Applying is deliberately not abortable and must settle through recovery.
    pub fn abort_prepared(&self, expected: &Prepared) -> Result<()> {
        let transaction = self.write()?;
        ensure!(
            value(&transaction, PHASE)? == PREPARED && pending(&transaction)? == *expected,
            "only this prepared intention may be aborted"
        );
        verify_operations(&transaction, expected)?;
        transaction
            .open_table(tables::PENDING)?
            .retain(|_, _| false)?;
        {
            let mut meta = transaction.open_table(tables::META)?;
            meta.remove(DESCRIPTOR)?;
            meta.remove(DESCRIPTOR_HASH)?;
            meta.insert(PHASE, READY)?;
        }
        transaction.commit()?;
        Ok(())
    }
}
fn install(
    transaction: &redb::WriteTransaction,
    table: Table,
    key: &[u8],
    value: Option<&[u8]>,
) -> Result<()> {
    macro_rules! put {
        ($definition:expr, $key:expr) => {{
            let mut rows = transaction.open_table($definition)?;
            if let Some(value) = value {
                rows.insert($key, value)?;
            } else {
                rows.remove($key)?;
            }
        }};
    }
    match table {
        Table::MapBucket => put!(tables::MAP_BUCKET, u16::from_be_bytes(key.try_into()?)),
        Table::MapNode => put!(tables::MAP_NODE, u32::from_be_bytes(key.try_into()?)),
        Table::GraphRecord => put!(tables::GRAPH_RECORD, u32::from_be_bytes(key.try_into()?)),
        Table::GraphNode => put!(tables::GRAPH_NODE, u64::from_be_bytes(key.try_into()?)),
        Table::LexTerm => put!(tables::LEX_TERM, key),
        Table::LexDirectory => put!(tables::LEX_DIRECTORY, key),
        Table::LexBlock => put!(tables::LEX_BLOCK, key),
        Table::LexDocument => put!(tables::LEX_DOCUMENT, u64::from_be_bytes(key.try_into()?)),
        Table::LexPath => put!(tables::LEX_PATH, key),
        Table::LexForward => put!(tables::LEX_FORWARD, key),
        Table::LexicalManifest => {
            ensure!(
                key.is_empty() && value.is_some_and(|v| !v.is_empty() && v.len() <= 1 << 20),
                "invalid lexical manifest operation"
            );
            put!(tables::META, 2);
        }
        Table::CapturedInput => {
            let bytes = value.context("captured inputs cannot be tombstones")?;
            ensure!(
                bytes.len() <= crate::current::MAX_INPUT_METADATA_BYTES,
                "oversized captured input"
            );
            let input: crate::current::CapturedInput = serde_json::from_slice(bytes)?;
            ensure!(
                input.artifact.path.as_bytes() == key && input.bytes()? == bytes,
                "captured input identity/encoding mismatch"
            );
            // Replay authority only. The owner verifies retained objects before
            // the same atomic head commit; no source is read during recovery.
        }
    }
    Ok(())
}

/// Stable identity for receipts outside the database transaction.
pub fn identity(pending: &Prepared) -> Result<Sha256Digest> {
    Ok(framed_digest(
        b"airk-kv-prepared/1",
        &[&serde_json::to_vec(pending)?],
    ))
}

fn stage_application(
    transaction: &redb::WriteTransaction,
    expected: &Prepared,
    verify: impl FnOnce(&redb::WriteTransaction, &CurrentHead) -> Result<()>,
) -> Result<()> {
    verify_operations(transaction, expected)?;
    {
        let rows = transaction.open_table(tables::PENDING)?;
        for row in rows.iter()? {
            let (key, value) = row?;
            let key = key.value();
            let value = value.value();
            install(
                transaction,
                Table::try_from(key[0])?,
                &key[1..],
                (value[0] == 1).then_some(&value[1..]),
            )?;
        }
    }
    verify(transaction, &expected.target)?;
    {
        let mut meta = transaction.open_table(tables::META)?;
        meta.insert(HEAD, expected.target.canonical_bytes()?.as_slice())?;
        meta.insert(PHASE, READY)?;
        meta.remove(DESCRIPTOR)?;
        meta.remove(DESCRIPTOR_HASH)?;
    }
    transaction
        .open_table(tables::PENDING)?
        .retain(|_, _| false)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::current::{CurrentScope, map};
    use airk_core::{
        graph_storage::{GraphHead, GraphScope},
        identity::Incarnation,
    };
    use std::io::{BufRead, Write};

    fn head() -> Result<CurrentHead> {
        let scope = CurrentScope {
            root: "original-kv-fixture".into(),
            collection: "kv-fixture".into(),
            incarnation: Incarnation::from_bytes([7; 16]),
        };
        Ok(CurrentHead {
            version: 2,
            sequence: 0,
            lexical_sequence: 0,
            artifact_root: map::empty_root(&scope)?,
            lexical_commit: Sha256Digest::from_bytes([8; 32]),
            graph_head: GraphHead::empty(GraphScope {
                root: scope.root.clone(),
                collection: scope.collection.clone(),
                incarnation: scope.incarnation,
                graph_instance: Incarnation::from_bytes([9; 16]),
                specification: Sha256Digest::from_bytes([10; 32]),
                record_bytes: 32,
            })?,
            scope,
        })
    }
    fn target() -> Result<CurrentHead> {
        let mut head = head()?;
        head.sequence = 1;
        Ok(head)
    }
    #[test]
    fn failed_verification_keeps_old_head_and_replayable_intention() -> Result<()> {
        let file = tempfile::NamedTempFile::new()?;
        let database = Database::open(file.reopen()?, 16 << 20, true)?;
        let base = head()?;
        database.initialize_head(&base)?;
        let old = database.snapshot()?;
        let prepared = database.prepare(base.token()?, target()?, &[])?;
        assert_eq!(database.pending_publication()?, Some(prepared.clone()));
        database.mark_applying(&prepared)?;
        assert!(
            database
                .apply(&prepared, |_, _| anyhow::bail!("fixture root mismatch"))
                .is_err()
        );
        assert_eq!(database.snapshot()?.head()?, base);
        assert_eq!(database.pending_publication()?, Some(prepared.clone()));
        database.apply(&prepared, |_, head| {
            ensure!(*head == target()?, "wrong fixture head");
            Ok(())
        })?;
        assert_eq!(old.head()?, base);
        assert_eq!(database.snapshot()?.head()?, target()?);
        assert!(database.pending_publication()?.is_none());
        assert_eq!(
            database.apply(&prepared, |_, _| Ok(()))?,
            target()?.token()?,
            "recovery after an installed target is idempotent"
        );
        Ok(())
    }
    #[test]
    fn descriptor_digest_tampering_blocks_recovery_before_head_change() -> Result<()> {
        let file = tempfile::NamedTempFile::new()?;
        let database = Database::open(file.reopen()?, 16 << 20, true)?;
        let base = head()?;
        database.initialize_head(&base)?;
        let prepared = database.prepare(base.token()?, target()?, &[])?;
        let transaction = database.write()?;
        transaction
            .open_table(tables::META)?
            .insert(DESCRIPTOR_HASH, [0xA5; 32].as_slice())?;
        transaction.commit()?;
        assert!(database.pending_publication().is_err());
        assert!(database.mark_applying(&prepared).is_err());
        assert_eq!(database.snapshot()?.head()?, base);
        Ok(())
    }
    #[test]
    fn changed_missing_and_extra_afterimages_fail_closed() -> Result<()> {
        for mode in 0..3 {
            let file = tempfile::NamedTempFile::new()?;
            let database = Database::open(file.reopen()?, 16 << 20, true)?;
            let base = head()?;
            database.initialize_head(&base)?;
            let prepared = database.prepare(
                base.token()?,
                target()?,
                &[Operation {
                    table: Table::LexPath,
                    key: b"file".to_vec(),
                    value: Some(b"original".to_vec()),
                }],
            )?;
            let transaction = database.write()?;
            {
                let mut rows = transaction.open_table(tables::PENDING)?;
                let key = [vec![Table::LexPath as u8], b"file".to_vec()].concat();
                match mode {
                    0 => {
                        rows.insert(key.as_slice(), b"\x01changed".as_slice())?;
                    }
                    1 => {
                        rows.remove(key.as_slice())?;
                    }
                    _ => {
                        rows.insert(b"\x08other".as_slice(), b"\x00".as_slice())?;
                    }
                }
            }
            transaction.commit()?;
            assert!(database.mark_applying(&prepared).is_err());
            assert_eq!(database.snapshot()?.head()?, base);
        }
        Ok(())
    }
    #[test]
    fn killed_worker_retains_prepared_applying_and_committed_states() -> Result<()> {
        use std::process::{Command, Stdio};
        for phase in ["prepared", "applying", "committed"] {
            let directory = tempfile::tempdir()?;
            let path = directory.path().join("current.airk");
            let mut child = Command::new(std::env::current_exe()?)
                .args([
                    "--exact",
                    "current::kv::journal::tests::crash_child",
                    "--nocapture",
                ])
                .env("AIRK_KV_CRASH_FILE", &path)
                .env("AIRK_KV_CRASH_PHASE", phase)
                .stdout(Stdio::piped())
                .stdin(Stdio::piped())
                .spawn()?;
            let output = child.stdout.take().context("missing child output")?;
            let (sender, receiver) = std::sync::mpsc::channel();
            let observer = std::thread::spawn(move || {
                for line in std::io::BufReader::new(output).lines() {
                    if line.is_ok_and(|line| line == "AIRK_KV_DURABLE") {
                        let _ = sender.send(());
                        break;
                    }
                }
            });
            let ready = receiver.recv_timeout(std::time::Duration::from_secs(30));
            let _ = child.kill();
            child.wait()?;
            observer
                .join()
                .map_err(|_| anyhow::anyhow!("child observer failed"))?;
            ready.context("worker did not reach durable crash boundary")?;
            let file = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(path)?;
            let database = Database::open(file, 16 << 20, false)?;
            if phase == "committed" {
                assert_eq!(database.snapshot()?.head()?, target()?);
                assert!(database.pending_publication()?.is_none());
            } else {
                assert_eq!(database.snapshot()?.head()?, head()?);
                let pending = database
                    .pending_publication()?
                    .context("lost durable intention")?;
                database.mark_applying(&pending)?;
                database.apply(&pending, |_, _| Ok(()))?;
                assert_eq!(database.snapshot()?.head()?, target()?);
            }
        }
        Ok(())
    }
    #[test]
    fn crash_child() -> Result<()> {
        let Some(path) = std::env::var_os("AIRK_KV_CRASH_FILE") else {
            return Ok(());
        };
        let phase = std::env::var("AIRK_KV_CRASH_PHASE")?;
        let file = std::fs::OpenOptions::new()
            .create_new(true)
            .read(true)
            .write(true)
            .open(path)?;
        let database = Database::open(file, 16 << 20, true)?;
        let base = head()?;
        database.initialize_head(&base)?;
        let pending = database.prepare(base.token()?, target()?, &[])?;
        if phase != "prepared" {
            database.mark_applying(&pending)?;
        }
        if phase == "committed" {
            database.apply(&pending, |_, _| Ok(()))?;
        }
        println!("AIRK_KV_DURABLE");
        std::io::stdout().flush()?;
        let mut line = String::new();
        std::io::stdin().read_line(&mut line)?;
        anyhow::bail!("parent must kill this worker before it drops its database")
    }
}
