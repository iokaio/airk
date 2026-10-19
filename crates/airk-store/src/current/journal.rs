// SPDX-License-Identifier: Apache-2.0
//! Captured, bounded replay state survives disposable staging and process death.
use super::{
    CurrentHead, CurrentScope, CurrentToken, MAX_INPUT_METADATA_BYTES, MAX_LEXICAL_MANIFEST_BYTES,
    MAX_PENDING_INPUTS, MAX_PENDING_METADATA_BYTES, database, decode_head,
    gate::Gate,
    graph, map,
    vector_map::{self, PreparedOccurrence},
    verify_lexical_manifest,
};
use crate::{
    FileStore, WriterLock,
    model_objects::ModelObjects,
    quota::{CurrentGrant, CurrentGrantScope, Quota},
};
use airk_core::{
    Artifact,
    graph_storage::{GraphDeltaDescriptor, GraphHead, PreparedGraphDelta},
    identity::{Sha256Digest, framed_digest},
    model_objects::ModelObjectSet,
};
use anyhow::{Context, Result, ensure};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

const MAX_DESCRIPTOR_BYTES: usize = 196_608;
const MAX_CAPTURED_OBJECT_BYTES: u64 = 64 * 1024 * 1024 * 1024;
const MAX_PENDING_MAP_CHANGES: u64 = 65_536;
const MAX_PENDING_MAP_BYTES: u64 = 64 * 1024 * 1024;
const BOOTSTRAP_FILE: &str = "current-bootstrap.json";
const MAX_LEXICAL_GC_ROOT_ENTRIES: usize = 16_384;
const MAX_LEXICAL_GC_DIRECTORIES: usize = 4_096;
const MAX_LEXICAL_GC_FILES: usize = 32_768;

fn lexical_gc_candidate(name: &str) -> Result<bool> {
    if let Some(sequence) = name.strip_prefix("lexical-") {
        let number: u64 = sequence
            .parse()
            .context("invalid lexical directory sequence")?;
        ensure!(
            name == format!("lexical-{number}"),
            "noncanonical lexical directory name"
        );
        Ok(true)
    } else {
        Ok(name.starts_with(".current-lexical-"))
    }
}

fn validate_flat_lexical_directory(path: &Path, parent: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        metadata.is_dir() && !metadata.file_type().is_symlink(),
        "lexical candidate is not an owned directory"
    );
    ensure!(
        path.canonicalize()?.parent() == Some(parent),
        "lexical candidate leaves the owned collection directory"
    );
    let mut count = 0usize;
    for entry in fs::read_dir(path)? {
        count = count
            .checked_add(1)
            .context("lexical file count overflow")?;
        ensure!(
            count <= MAX_LEXICAL_GC_FILES,
            "lexical file count limit exceeded"
        );
        let metadata = fs::symlink_metadata(entry?.path())?;
        ensure!(
            metadata.is_file() && !metadata.file_type().is_symlink(),
            "lexical directory contains an unowned or nested entry"
        );
    }
    Ok(())
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Bootstrap {
    version: u32,
    head: CurrentHead,
    capacity: u64,
    complete: bool,
}

fn bootstrap_id(head: &CurrentHead) -> Result<Sha256Digest> {
    Ok(framed_digest(
        b"current-initialize/1",
        &[&head.canonical_bytes()?],
    ))
}
fn read_bootstrap(store: &FileStore, scope: &CurrentScope) -> Result<Option<Bootstrap>> {
    let path = store.directory().join(BOOTSTRAP_FILE);
    let metadata = match fs::symlink_metadata(&path) {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    ensure!(
        metadata.is_file()
            && !metadata.file_type().is_symlink()
            && metadata.len() <= MAX_DESCRIPTOR_BYTES as u64,
        "invalid current bootstrap metadata"
    );
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(MAX_DESCRIPTOR_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= MAX_DESCRIPTOR_BYTES,
        "current bootstrap exceeds byte bound"
    );
    let bootstrap: Bootstrap = serde_json::from_slice(&bytes)?;
    bootstrap.head.validate()?;
    database::validate_capacity(bootstrap.capacity)?;
    ensure!(
        bootstrap.version == 1
            && bootstrap.head.scope == *scope
            && bootstrap.head.sequence == 0
            && bootstrap.head.graph_head
                == GraphHead::empty(bootstrap.head.graph_head.scope.clone())?
            && serde_json::to_vec(&bootstrap)? == bytes,
        "invalid current bootstrap binding"
    );
    Ok(Some(bootstrap))
}
fn write_bootstrap(store: &FileStore, bootstrap: &Bootstrap) -> Result<()> {
    let bytes = serde_json::to_vec(bootstrap)?;
    ensure!(
        bytes.len() <= MAX_DESCRIPTOR_BYTES,
        "current bootstrap exceeds byte bound"
    );
    let mut file = tempfile::NamedTempFile::new_in(store.directory())?;
    file.write_all(&bytes)?;
    crate::durability::persist_synced(file, &store.directory().join(BOOTSTRAP_FILE))
}

#[derive(Clone, Copy)]
pub(super) enum InitializationStage {
    BeforeDatabaseOpen,
    BeforeSchemaCommit,
    ReadyCommitted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransactionPhase {
    Ready,
    Prepared,
    Applying,
}

#[derive(Clone, Debug)]
pub struct CurrentCommit {
    pub token: CurrentToken,
    /// A successful durable publication may leave quota/GC cleanup to a retry.
    pub maintenance_error: Option<String>,
}

#[derive(Clone, Debug)]
pub struct CurrentCapacityChange {
    pub capacity_bytes: u64,
    /// The owner limit changed durably; its unused admission grant needs cleanup.
    pub maintenance_error: Option<String>,
}

#[derive(Clone, Copy)]
pub(super) enum CapacityStage {
    BeforeOwner,
    AfterOwner,
}

impl TransactionPhase {
    fn parse(value: &str) -> Result<Self> {
        match value {
            "ready" => Ok(Self::Ready),
            "prepared" => Ok(Self::Prepared),
            "applying" => Ok(Self::Applying),
            _ => anyhow::bail!("invalid current transaction phase"),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapturedInput {
    pub artifact: Artifact,
}

/// A changed source path in a single current-index publication. Upserts carry
/// compact, retained and source-verified metadata; deletes carry the exact
/// normalized path whose current map entry is removed.
#[derive(Clone, Debug)]
pub enum ArtifactChange {
    Upsert(Box<CapturedInput>),
    Delete(String),
}

pub struct ArtifactVectorChanges<'a> {
    pub artifacts: &'a [ArtifactChange],
    pub occurrences: &'a [PreparedOccurrence],
}

impl ArtifactChange {
    pub(crate) fn path(&self) -> &str {
        match self {
            Self::Upsert(input) => &input.artifact.path,
            Self::Delete(path) => path,
        }
    }
}

pub(crate) fn valid_current_path(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 8192
        && !path.contains(['\\', ':', '\0'])
        && Path::new(path).is_relative()
        && path.split('/').all(|part| !matches!(part, "" | "." | ".."))
}

impl CapturedInput {
    pub(super) fn bytes(&self) -> Result<Vec<u8>> {
        let artifact = &self.artifact;
        ensure!(
            valid_current_path(&artifact.path),
            "invalid captured artifact path"
        );
        artifact.content_hash.parse::<Sha256Digest>()?;
        ensure!(
            artifact.chunks.is_empty() && artifact.facts.is_empty(),
            "captured inputs require compact retained metadata"
        );
        let retained = artifact
            .retained
            .as_ref()
            .context("captured extraction reference missing")?;
        retained.object.parse::<Sha256Digest>()?;
        let profile = retained
            .profile
            .as_ref()
            .context("captured extraction profile missing")?;
        profile.identity()?;
        ensure!(
            profile.chunk_version == 3
                && retained.raw_source
                && retained.object_bytes <= airk_core::extraction::MAX_PAYLOAD_BYTES
                && artifact.bytes <= airk_core::extraction::MAX_SOURCE_BYTES,
            "captured input exceeds supported extraction bounds"
        );
        let bytes = serde_json::to_vec(self)?;
        ensure!(
            bytes.len() <= MAX_INPUT_METADATA_BYTES,
            "captured input metadata exceeds byte bound"
        );
        Ok(bytes)
    }
    pub(super) fn verify(&self, store: &FileStore) -> Result<()> {
        self.bytes()?;
        store.load_retrieval_artifact(&self.artifact)?;
        store.read_retained_source(&self.artifact)?;
        Ok(())
    }
}

/// One item at a time, in strictly increasing artifact-path order.
pub trait CapturedInputs {
    fn visit(&self, visitor: &mut dyn FnMut(&CapturedInput) -> Result<()>) -> Result<()>;
}
impl CapturedInputs for [CapturedInput] {
    fn visit(&self, visitor: &mut dyn FnMut(&CapturedInput) -> Result<()>) -> Result<()> {
        for input in self {
            visitor(input)?;
        }
        Ok(())
    }
}
impl CapturedInputs for Vec<CapturedInput> {
    fn visit(&self, visitor: &mut dyn FnMut(&CapturedInput) -> Result<()>) -> Result<()> {
        self.as_slice().visit(visitor)
    }
}
impl<const N: usize> CapturedInputs for [CapturedInput; N] {
    fn visit(&self, visitor: &mut dyn FnMut(&CapturedInput) -> Result<()>) -> Result<()> {
        self.as_slice().visit(visitor)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PendingTransaction {
    pub version: u32,
    pub id: Sha256Digest,
    pub expected: CurrentToken,
    pub target: CurrentHead,
    pub graph: GraphDeltaDescriptor,
    pub inputs_count: u64,
    pub metadata_bytes: u64,
    pub object_bytes: u64,
    pub inputs_digest: Sha256Digest,
    pub map_count: u64,
    pub map_bytes: u64,
    pub map_digest: Sha256Digest,
    pub lexical_manifest_bytes: u64,
    pub model: Option<ModelObjectSet>,
    pub database_capacity_bytes: u64,
}

impl PendingTransaction {
    fn bytes(&self) -> Result<Vec<u8>> {
        self.target.validate()?;
        self.graph.validate()?;
        database::validate_capacity(self.database_capacity_bytes)?;
        ensure!(
            self.version == 3
                && self.inputs_count <= MAX_PENDING_INPUTS as u64
                && self.metadata_bytes <= MAX_PENDING_METADATA_BYTES
                && self.object_bytes <= MAX_CAPTURED_OBJECT_BYTES
                && self.map_count <= MAX_PENDING_MAP_CHANGES
                && self.map_bytes <= MAX_PENDING_MAP_BYTES
                && self.lexical_manifest_bytes <= MAX_LEXICAL_MANIFEST_BYTES as u64
                && self.graph.target == self.target.graph_head
                && self.expected.incarnation == self.target.scope.incarnation
                && self.expected.sequence.checked_add(1) == Some(self.target.sequence),
            "invalid pending current transaction"
        );
        if self.map_count == 0 {
            ensure!(
                self.map_bytes == 0 && self.map_digest == MapDigest::new().finish(),
                "empty pending map has a nonempty descriptor"
            );
        }
        if let Some(model) = &self.model {
            model.validate()?;
        }
        ensure!(
            self.id == transaction_id(&self.expected, &self.target, &self.graph, &self.model)?,
            "pending transaction identity mismatch"
        );
        let bytes = serde_json::to_vec(self)?;
        ensure!(
            bytes.len() <= MAX_DESCRIPTOR_BYTES,
            "pending descriptor exceeds byte bound"
        );
        Ok(bytes)
    }
}

fn transaction_id(
    expected: &CurrentToken,
    target: &CurrentHead,
    graph: &GraphDeltaDescriptor,
    model: &Option<ModelObjectSet>,
) -> Result<Sha256Digest> {
    Ok(framed_digest(
        b"current-pending-transaction/1",
        &[
            &serde_json::to_vec(expected)?,
            &target.canonical_bytes()?,
            &serde_json::to_vec(graph)?,
            &serde_json::to_vec(model)?,
        ],
    ))
}

fn create(connection: &Connection) -> Result<()> {
    connection.execute_batch("CREATE TABLE current_state(id INTEGER PRIMARY KEY CHECK(id=1),phase TEXT NOT NULL CHECK(phase IN('ready','prepared','applying')),head BLOB NOT NULL CHECK(length(head)<=65536));
        CREATE TABLE current_pending(id INTEGER PRIMARY KEY CHECK(id=1),transaction_id TEXT NOT NULL,descriptor BLOB NOT NULL CHECK(length(descriptor)<=196608),digest TEXT NOT NULL);
        CREATE TABLE current_pending_inputs(path TEXT PRIMARY KEY,metadata BLOB NOT NULL CHECK(length(metadata)<=262144)) WITHOUT ROWID;
        CREATE TABLE current_pending_map(key BLOB PRIMARY KEY CHECK(typeof(key)='blob' AND length(key)=32),value BLOB CHECK(value IS NULL OR (typeof(value)='blob' AND length(value)<=262144))) WITHOUT ROWID;
        CREATE TABLE current_lexical_manifest(id INTEGER PRIMARY KEY CHECK(id=1),content BLOB NOT NULL CHECK(typeof(content)='blob' AND length(content) BETWEEN 1 AND 1048576));
        CREATE TABLE current_pending_lexical(id INTEGER PRIMARY KEY CHECK(id=1),content BLOB NOT NULL CHECK(typeof(content)='blob' AND length(content) BETWEEN 1 AND 1048576));")?;
    graph::create(connection)?;
    super::map::create(connection)
}

fn state(connection: &Connection, scope: &CurrentScope) -> Result<(TransactionPhase, CurrentHead)> {
    let (phase, bytes): (String, Vec<u8>) = connection.query_row(
        "SELECT phase,head FROM current_state WHERE id=1 AND length(head)<=65536",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    Ok((
        TransactionPhase::parse(&phase)?,
        decode_head(&bytes, scope)?,
    ))
}

fn pending(connection: &Connection, scope: &CurrentScope) -> Result<Option<PendingTransaction>> {
    let (phase, head) = state(connection, scope)?;
    let row: Option<(String, Vec<u8>, String)> = connection.query_row("SELECT transaction_id,descriptor,digest FROM current_pending WHERE id=1 AND length(descriptor)<=196608", [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).optional()?;
    let Some((id, bytes, hash)) = row else {
        ensure!(
            phase == TransactionPhase::Ready,
            "current transaction journal missing"
        );
        return Ok(None);
    };
    ensure!(
        phase != TransactionPhase::Ready
            && hash.parse::<Sha256Digest>()?
                == framed_digest(b"current-pending-descriptor/1", &[&bytes]),
        "current journal descriptor digest mismatch"
    );
    let transaction: PendingTransaction = serde_json::from_slice(&bytes)?;
    ensure!(
        transaction.bytes()? == bytes
            && transaction.id.to_string() == id
            && transaction.expected == head.token()?
            && transaction.target.scope == *scope
            && transaction.graph.base == head.graph_head
            && (if transaction.lexical_manifest_bytes > 0 {
                transaction.target.lexical_sequence == transaction.target.sequence
            } else {
                transaction.target.lexical_commit == head.lexical_commit
                    && transaction.target.lexical_sequence == head.lexical_sequence
            }),
        "current journal base or scope mismatch"
    );
    Ok(Some(transaction))
}

struct InputDigest {
    hash: Sha256,
    count: u64,
    bytes: u64,
    objects: u64,
    previous: Option<String>,
}
impl InputDigest {
    fn new() -> Self {
        let mut hash = Sha256::new();
        hash.update(b"airk-current-captured-inputs/1");
        Self {
            hash,
            count: 0,
            bytes: 0,
            objects: 0,
            previous: None,
        }
    }
    fn add(&mut self, input: &CapturedInput, bytes: &[u8]) -> Result<()> {
        ensure!(
            self.previous
                .as_ref()
                .is_none_or(|path| path < &input.artifact.path),
            "captured input paths must be unique and ordered"
        );
        self.count += 1;
        self.bytes = self
            .bytes
            .checked_add(bytes.len() as u64)
            .context("captured metadata overflow")?;
        let retained = input
            .artifact
            .retained
            .as_ref()
            .context("captured input reference missing")?;
        self.objects = self
            .objects
            .checked_add(input.artifact.bytes as u64)
            .and_then(|n| n.checked_add(retained.object_bytes as u64))
            .context("captured object bytes overflow")?;
        ensure!(
            self.count <= MAX_PENDING_INPUTS as u64
                && self.bytes <= MAX_PENDING_METADATA_BYTES
                && self.objects <= MAX_CAPTURED_OBJECT_BYTES,
            "captured transaction input budget exceeded"
        );
        self.hash.update((bytes.len() as u64).to_be_bytes());
        self.hash.update(bytes);
        self.previous = Some(input.artifact.path.clone());
        Ok(())
    }
    fn finish(&self) -> Sha256Digest {
        Sha256Digest::from_bytes(self.hash.clone().finalize().into())
    }
}

struct MapDigest {
    hash: Sha256,
    count: u64,
    bytes: u64,
    previous: Option<Sha256Digest>,
}
impl MapDigest {
    fn new() -> Self {
        let mut hash = Sha256::new();
        hash.update(b"airk-current-pending-map/1");
        Self {
            hash,
            count: 0,
            bytes: 0,
            previous: None,
        }
    }
    fn add(&mut self, key: Sha256Digest, value: Option<&[u8]>) -> Result<()> {
        ensure!(
            self.previous.is_none_or(|previous| previous < key),
            "pending map keys must be unique and ordered"
        );
        let value_bytes = value.map_or(0, |bytes| bytes.len());
        ensure!(
            value_bytes <= map::MAX_VALUE_BYTES,
            "pending map value exceeds limit"
        );
        self.count = self
            .count
            .checked_add(1)
            .context("map change count overflow")?;
        self.bytes = self
            .bytes
            .checked_add(32 + 1 + 4 + value_bytes as u64)
            .context("map change byte overflow")?;
        ensure!(
            self.count <= MAX_PENDING_MAP_CHANGES && self.bytes <= MAX_PENDING_MAP_BYTES,
            "pending map change budget exceeded"
        );
        self.hash.update(key.as_bytes());
        self.hash.update([u8::from(value.is_some())]);
        self.hash.update((value_bytes as u32).to_be_bytes());
        if let Some(value) = value {
            self.hash.update(value);
        }
        self.previous = Some(key);
        Ok(())
    }
    fn finish(&self) -> Sha256Digest {
        Sha256Digest::from_bytes(self.hash.clone().finalize().into())
    }
}

type MapVisitor<'a> = dyn FnMut(Sha256Digest, Option<&[u8]>) -> Result<()> + 'a;

fn visit_pending_map(
    connection: &Connection,
    pending: &PendingTransaction,
    visitor: &mut MapVisitor<'_>,
) -> Result<()> {
    let count: i64 =
        connection.query_row("SELECT count(*) FROM current_pending_map", [], |row| {
            row.get(0)
        })?;
    ensure!(
        u64::try_from(count)? == pending.map_count,
        "pending map count mismatch"
    );
    let mut digest = MapDigest::new();
    let mut after = Vec::new();
    loop {
        let row: Option<(Vec<u8>, Option<Vec<u8>>)> = connection.query_row(
            "SELECT key,value FROM current_pending_map WHERE key>?1 AND (value IS NULL OR length(value)<=262144) ORDER BY key LIMIT 1",
            [&after],
            |row| Ok((row.get(0)?, row.get(1)?)),
        ).optional()?;
        let Some((bytes, value)) = row else { break };
        ensure!(bytes.len() == 32, "pending map key length mismatch");
        let key = Sha256Digest::from_bytes(bytes.as_slice().try_into()?);
        digest.add(key, value.as_deref())?;
        visitor(key, value.as_deref())?;
        after = bytes;
    }
    ensure!(
        digest.count == pending.map_count
            && digest.bytes == pending.map_bytes
            && digest.finish() == pending.map_digest,
        "pending map digest or byte count mismatch"
    );
    Ok(())
}

fn pending_lexical(
    connection: &Connection,
    pending: &PendingTransaction,
) -> Result<Option<Vec<u8>>> {
    let count: i64 =
        connection.query_row("SELECT count(*) FROM current_pending_lexical", [], |row| {
            row.get(0)
        })?;
    ensure!(
        count == i64::from(pending.lexical_manifest_bytes > 0),
        "pending lexical manifest row count mismatch"
    );
    if count == 0 {
        return Ok(None);
    }
    let bytes: Option<Vec<u8>> = connection.query_row(
        "SELECT CASE WHEN typeof(content)='blob' AND length(content) BETWEEN 1 AND 1048576 THEN content END FROM current_pending_lexical WHERE id=1",
        [], |row| row.get(0),
    )?;
    let bytes = bytes.context("invalid pending lexical manifest bytes")?;
    ensure!(
        bytes.len() as u64 == pending.lexical_manifest_bytes,
        "pending lexical manifest length mismatch"
    );
    verify_lexical_manifest(&bytes, pending.target.lexical_commit)?;
    Ok(Some(bytes))
}

pub(super) fn committed_lexical_manifest(
    connection: &Connection,
    head: &CurrentHead,
) -> Result<Vec<u8>> {
    let bytes: Option<Vec<u8>> = connection
        .query_row(
            "SELECT CASE WHEN typeof(content)='blob' AND length(content) BETWEEN 1 AND 1048576 THEN content END FROM current_lexical_manifest WHERE id=1",
            [],
            |row| row.get(0),
        )
        .context("committed lexical manifest missing")?;
    let bytes = bytes.context("invalid committed lexical manifest row")?;
    verify_lexical_manifest(&bytes, head.lexical_commit)?;
    Ok(bytes)
}

fn verify_model(store: &FileStore, model: &Option<ModelObjectSet>) -> Result<()> {
    if let Some(model) = model {
        model.validate()?;
        let maximum = model.assets.values().try_fold(0u64, |sum, item| {
            sum.checked_add(item.bytes)
                .context("model verification budget overflow")
        })?;
        ModelObjects::new(store.directory()).verify(model, maximum)?;
    }
    Ok(())
}

/// The caller's writer lease outlives this owned connection and exclusive gate.
pub struct CurrentWriter<'a> {
    connection: Option<Connection>,
    initialized: bool,
    store: FileStore,
    writer: &'a WriterLock,
    quota: Quota,
    scope: CurrentScope,
    capacity: u64,
    _gate: Gate,
}

struct ComponentChanges<'a> {
    map: &'a [map::MapChange],
    lexical_manifest: Option<&'a [u8]>,
}

impl FileStore {
    pub fn current_write<'a>(
        &self,
        writer: &'a WriterLock,
        quota: &Quota,
        scope: &CurrentScope,
        database_capacity_bytes: u64,
    ) -> Result<CurrentWriter<'a>> {
        scope.validate()?;
        database::validate_capacity(database_capacity_bytes)?;
        crate::maintenance::check_writer(self, writer, quota)?;
        ensure!(
            self.directory().file_name().and_then(|name| name.to_str())
                == Some(scope.collection.as_str()),
            "foreign current writer collection"
        );
        let gate = Gate::exclusive(self, writer)?;
        database::reject_legacy_collection(self.directory())?;
        let path = self.directory().join("current.sqlite3");
        let bootstrap = read_bootstrap(self, scope)?;
        let pending_bootstrap = bootstrap.as_ref().filter(|value| !value.complete);
        let mut initialized = false;
        let connection = if path.try_exists()? {
            ensure!(
                database::check_owner(self.directory(), scope)? == database_capacity_bytes,
                "current writer capacity differs from captured owner"
            );
            let recovery = self
                .directory()
                .join("current.sqlite3-journal")
                .try_exists()?;
            if recovery || pending_bootstrap.is_some() {
                let grants = quota.current_grants(writer)?;
                let initialization_id = pending_bootstrap
                    .map(|value| bootstrap_id(&value.head))
                    .transpose()?;
                let maximum = grants
                    .iter()
                    .filter(|grant| {
                        grant.scope().collection == scope.collection
                            && grant.scope().incarnation == scope.incarnation
                            && initialization_id.is_none_or(|id| grant.scope().transaction == id)
                    })
                    .map(CurrentGrant::capacity)
                    .max()
                    .context("current rollback recovery has no durable capacity grant")?;
                database::recovery_bound(&path, database_capacity_bytes, maximum)?;
            }
            let (connection, uninitialized) = if pending_bootstrap.is_some() {
                database::open_bootstrap(&path, database_capacity_bytes)?
            } else {
                (
                    database::open_writer(&path, database_capacity_bytes, false)?,
                    false,
                )
            };
            initialized = !uninitialized;
            if initialized {
                let (_, head) = state(&connection, scope)?;
                head.validate()?;
                if let Some(bootstrap) = pending_bootstrap {
                    ensure!(
                        head == bootstrap.head,
                        "unfinished bootstrap does not match committed initial head"
                    );
                }
                database::restrict(&connection)?;
            }
            Some(connection)
        } else {
            None
        };
        let mut current = CurrentWriter {
            connection,
            initialized,
            store: self.clone(),
            writer,
            quota: quota.clone(),
            scope: scope.clone(),
            capacity: database_capacity_bytes,
            _gate: gate,
        };
        if let Some(bootstrap) = pending_bootstrap {
            ensure!(
                bootstrap.capacity == database_capacity_bytes,
                "bootstrap recovery requires captured database capacity"
            );
            if current.initialized {
                current.complete_bootstrap(bootstrap.head.clone())?;
            } else {
                current.initialize(bootstrap.head.clone())?;
            }
        }
        Ok(current)
    }
}

impl CurrentWriter<'_> {
    pub fn database_capacity_bytes(&self) -> u64 {
        self.capacity
    }

    /// Grow the admitted SQLite limit while Ready, before preparing a transaction.
    /// The owner header is synchronized under the exclusive gate before reopening
    /// SQLite with the larger max_page_count. A crashed writer leaves a durable
    /// allowance that the next Ready writer may explicitly retire.
    pub fn grow_capacity(&mut self, next: u64) -> Result<CurrentCapacityChange> {
        self.grow_capacity_observed(next, &mut |_| Ok(()))
    }

    pub(super) fn grow_capacity_observed(
        &mut self,
        next: u64,
        observe: &mut dyn FnMut(CapacityStage) -> Result<()>,
    ) -> Result<CurrentCapacityChange> {
        database::validate_capacity(next)?;
        ensure!(next > self.capacity, "current database capacity must grow");
        ensure!(
            self.phase()? == TransactionPhase::Ready && self.pending()?.is_none(),
            "current database capacity cannot change during publication"
        );
        let previous = self.capacity;
        let grant_id = framed_digest(
            b"current-database-capacity-grow/1",
            &[
                self.scope.collection.as_bytes(),
                self.scope.root.as_bytes(),
                self.scope.incarnation.as_bytes(),
                &previous.to_be_bytes(),
                &next.to_be_bytes(),
            ],
        );
        let allowance = database::allowance(
            self.connection
                .as_mut()
                .context("current database missing")?,
            next,
        )?;
        let grant =
            self.quota
                .reserve_current(self.writer, self.grant_scope(grant_id), allowance)?;
        if let Err(error) = crate::capacity::require_growth(self.quota.root(), allowance) {
            let _ = grant.release(self.writer);
            return Err(error);
        }
        observe(CapacityStage::BeforeOwner)?;
        database::grow_owner(self.store.directory(), &self.scope, previous, next)?;
        observe(CapacityStage::AfterOwner)?;
        self.capacity = next;
        // The old connection's max_page_count is fixed. Reopen without a live
        // transaction, while still owning the writer and exclusive read gate.
        drop(self.connection.take());
        let connection =
            database::open_writer(&self.store.directory().join("current.sqlite3"), next, false)?;
        database::restrict(&connection)?;
        self.connection = Some(connection);
        let maintenance_error = grant
            .release(self.writer)
            .err()
            .map(|error| error.to_string());
        Ok(CurrentCapacityChange {
            capacity_bytes: next,
            maintenance_error,
        })
    }

    fn connection(&self) -> Result<&Connection> {
        ensure!(self.initialized, "current index is not initialized");
        self.connection
            .as_ref()
            .context("current index is not initialized")
    }
    fn grant_scope(&self, id: Sha256Digest) -> CurrentGrantScope {
        CurrentGrantScope {
            collection: self.scope.collection.clone(),
            incarnation: self.scope.incarnation,
            transaction: id,
        }
    }
    fn grant(&self, transaction: &PendingTransaction) -> Result<CurrentGrant> {
        self.quota
            .current_grant(self.writer, &self.grant_scope(transaction.id))?
            .context("pending current transaction has no durable capacity grant")
    }
    pub fn phase(&self) -> Result<TransactionPhase> {
        Ok(state(self.connection()?, &self.scope)?.0)
    }
    pub fn head(&self) -> Result<CurrentHead> {
        Ok(state(self.connection()?, &self.scope)?.1)
    }

    pub fn committed_lexical_manifest(&self) -> Result<Vec<u8>> {
        committed_lexical_manifest(self.connection()?, &self.head()?)
    }

    pub fn pending_lexical_manifest(
        &self,
        pending: &PendingTransaction,
    ) -> Result<Option<Vec<u8>>> {
        ensure!(
            self.pending()?.as_ref() == Some(pending),
            "pending lexical manifest transaction changed"
        );
        pending_lexical(self.connection()?, pending)
    }

    /// Remove only unselected, flat lexical fork directories while the
    /// exclusive current gate has drained readers. This is maintenance after
    /// a Ready commit; failure does not undo an already committed head.
    pub fn reclaim_lexical_candidates(&mut self) -> Result<usize> {
        ensure!(
            self.phase()? == TransactionPhase::Ready && self.pending()?.is_none(),
            "lexical reclamation requires a ready current head"
        );
        let head = self.head()?;
        committed_lexical_manifest(self.connection()?, &head)?;
        let directory = self.store.directory();
        let parent = directory.canonicalize()?;
        let selected = head.lexical_directory_name()?;
        validate_flat_lexical_directory(&directory.join(&selected), &parent)?;
        let mut root_entries = 0usize;
        let mut obsolete: Vec<PathBuf> = Vec::new();
        for entry in fs::read_dir(directory)? {
            root_entries = root_entries
                .checked_add(1)
                .context("collection entry count overflow")?;
            ensure!(
                root_entries <= MAX_LEXICAL_GC_ROOT_ENTRIES,
                "collection entry count exceeds lexical maintenance bound"
            );
            let entry = entry?;
            let name = entry.file_name();
            let name = name.to_str().context("non-UTF-8 collection entry")?;
            if name == selected || !lexical_gc_candidate(name)? {
                continue;
            }
            ensure!(
                obsolete.len() < MAX_LEXICAL_GC_DIRECTORIES,
                "too many obsolete lexical directories"
            );
            validate_flat_lexical_directory(&entry.path(), &parent)?;
            obsolete.push(entry.path());
        }
        for path in &obsolete {
            fs::remove_dir_all(path)?;
        }
        if !obsolete.is_empty() {
            crate::maintenance::sync_parent(directory)?;
        }
        Ok(obsolete.len())
    }
    pub fn pending(&self) -> Result<Option<PendingTransaction>> {
        let pending = pending(self.connection()?, &self.scope)?;
        if let Some(pending) = &pending {
            ensure!(
                pending.database_capacity_bytes == self.capacity,
                "pending transaction capacity differs from captured owner"
            );
        }
        Ok(pending)
    }

    pub fn initialize(&mut self, head: CurrentHead) -> Result<CurrentHead> {
        self.initialize_observed(head, &mut |_| Ok(()))
    }
    pub(super) fn initialize_observed(
        &mut self,
        head: CurrentHead,
        observe: &mut dyn FnMut(InitializationStage) -> Result<()>,
    ) -> Result<CurrentHead> {
        if self.initialized {
            ensure!(
                self.head()? == head && head.sequence == 0,
                "current index is already initialized"
            );
            return Ok(head);
        }
        ensure!(
            head.scope == self.scope && head.sequence == 0,
            "current initialization requires an empty collection and sequence zero"
        );
        head.validate()?;
        ensure!(
            head.graph_head == GraphHead::empty(head.graph_head.scope.clone())?,
            "current initialization requires an unallocated graph"
        );
        if let Some(bootstrap) = read_bootstrap(&self.store, &self.scope)? {
            ensure!(
                !bootstrap.complete
                    && bootstrap.head == head
                    && bootstrap.capacity == self.capacity,
                "current initialization differs from captured bootstrap"
            );
        }
        let id = bootstrap_id(&head)?;
        let allowance = self
            .capacity
            .checked_add(database::journal_bound(16, 65_536)?)
            .context("initial capacity overflow")?;
        let mut grant = match self
            .quota
            .current_grant(self.writer, &self.grant_scope(id))?
        {
            Some(grant) => grant,
            None => self
                .quota
                .reserve_current(self.writer, self.grant_scope(id), allowance)?,
        };
        if grant.capacity() < allowance {
            grant.resize(self.writer, allowance)?;
        }
        crate::capacity::require_growth(self.quota.root(), allowance)?;
        database::create_owner(self.store.directory(), &self.scope, self.capacity)?;
        write_bootstrap(
            &self.store,
            &Bootstrap {
                version: 1,
                head: head.clone(),
                capacity: self.capacity,
                complete: false,
            },
        )?;
        observe(InitializationStage::BeforeDatabaseOpen)?;
        let mut connection = match self.connection.take() {
            Some(connection) => connection,
            None => database::open_writer(
                &self.store.directory().join("current.sqlite3"),
                self.capacity,
                true,
            )?,
        };
        {
            let transaction = connection.transaction()?;
            transaction.pragma_update(None, "application_id", 0x4149_524bi32)?;
            transaction.pragma_update(None, "user_version", database::DATABASE_VERSION)?;
            create(&transaction)?;
            transaction.execute(
                "INSERT OR ROLLBACK INTO current_state(id,phase,head) VALUES(1,'ready',?1)",
                [head.canonical_bytes()?],
            )?;
            observe(InitializationStage::BeforeSchemaCommit)?;
            transaction.commit()?;
        }
        observe(InitializationStage::ReadyCommitted)?;
        database::restrict(&connection)?;
        self.connection = Some(connection);
        self.initialized = true;
        write_bootstrap(
            &self.store,
            &Bootstrap {
                version: 1,
                head: head.clone(),
                capacity: self.capacity,
                complete: true,
            },
        )?;
        grant.release(self.writer)?;
        Ok(head)
    }

    fn complete_bootstrap(&self, head: CurrentHead) -> Result<()> {
        let grant = self
            .quota
            .current_grant(self.writer, &self.grant_scope(bootstrap_id(&head)?))?
            .context("bootstrap recovery capacity missing")?;
        write_bootstrap(
            &self.store,
            &Bootstrap {
                version: 1,
                head,
                capacity: self.capacity,
                complete: true,
            },
        )?;
        grant.release(self.writer)
    }

    /// Complete a fresh sequence-zero index after the verified empty Tantivy
    /// component has been synced. A restart can retry the exact same manifest;
    /// readers requiring lexical evidence fail until the row exists.
    pub fn install_initial_lexical_manifest(
        &mut self,
        expected: &CurrentToken,
        bytes: &[u8],
    ) -> Result<CurrentCommit> {
        let (phase, head) = state(self.connection()?, &self.scope)?;
        ensure!(
            phase == TransactionPhase::Ready
                && self.pending()?.is_none()
                && head.sequence == 0
                && head.token()? == *expected,
            "initial lexical manifest requires the exact empty current head"
        );
        verify_lexical_manifest(bytes, head.lexical_commit)?;
        let existing: Option<Vec<u8>> = self
            .connection()?
            .query_row(
                "SELECT content FROM current_lexical_manifest WHERE id=1",
                [],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(existing) = existing {
            ensure!(
                existing == bytes,
                "initial lexical manifest differs from committed bytes"
            );
            return Ok(CurrentCommit {
                token: *expected,
                maintenance_error: None,
            });
        }
        let id = framed_digest(
            b"current-initial-lexical-manifest/1",
            &[&serde_json::to_vec(expected)?, bytes],
        );
        let allowance = database::allowance(
            self.connection
                .as_mut()
                .context("current database missing")?,
            self.capacity,
        )?;
        let grant_scope = self.grant_scope(id);
        let mut grant = match self.quota.current_grant(self.writer, &grant_scope)? {
            Some(grant) => grant,
            None => self
                .quota
                .reserve_current(self.writer, grant_scope, allowance)?,
        };
        if grant.capacity() < allowance {
            grant.resize(self.writer, allowance)?;
        }
        crate::capacity::require_growth(self.quota.root(), allowance)?;
        let transaction = self
            .connection
            .as_mut()
            .context("current database missing")?
            .transaction()?;
        transaction.execute(
            "INSERT OR ROLLBACK INTO current_lexical_manifest(id,content) VALUES(1,?1)",
            [bytes],
        )?;
        transaction.commit()?;
        let maintenance_error = grant
            .release(self.writer)
            .err()
            .map(|error| error.to_string());
        Ok(CurrentCommit {
            token: *expected,
            maintenance_error,
        })
    }

    /// Publish a catalog-only artifact change in one rollback-journaled SQL
    /// transaction. This path is useful when no lexical or graph component
    /// changes; combined changes use the captured multi-component protocol.
    pub fn commit_artifact(
        &mut self,
        expected: &CurrentToken,
        path: &str,
        input: Option<&CapturedInput>,
    ) -> Result<CurrentCommit> {
        ensure!(valid_current_path(path), "invalid current artifact path");
        let (phase, base) = state(self.connection()?, &self.scope)?;
        ensure!(
            phase == TransactionPhase::Ready
                && self.pending()?.is_none()
                && *expected == base.token()?,
            "current catalog base expired or publication pending"
        );
        let value = input
            .map(|input| {
                ensure!(
                    input.artifact.path == path,
                    "captured artifact path mismatch"
                );
                let bytes = input.bytes()?;
                input.verify(&self.store)?;
                Ok::<_, anyhow::Error>(bytes)
            })
            .transpose()?;
        let change = map::MapChange::new(
            &self.scope,
            map::MapDomain::Artifact,
            path.as_bytes(),
            value,
        )?;
        let identity = change.key();
        let before = map::get(
            self.connection()?,
            &self.scope,
            base.artifact_root,
            identity,
        )?;
        if before.as_deref() == change.value() {
            return Ok(CurrentCommit {
                token: *expected,
                maintenance_error: None,
            });
        }
        let previous = before
            .as_ref()
            .map(|bytes| -> Result<CapturedInput> {
                let previous: CapturedInput = serde_json::from_slice(bytes)?;
                ensure!(
                    previous.artifact.path == path && previous.bytes()? == *bytes,
                    "invalid prior catalog binding"
                );
                Ok(previous)
            })
            .transpose()?;
        let mut changes = vec![change];
        let planned_root = {
            let mut plan = map::MapPlan::new(self.connection()?, &self.scope, base.artifact_root)?;
            let mut postings = super::postings::PostingChanges::default();
            postings.update(
                &mut plan,
                &self.scope,
                &self.store,
                previous.as_ref().map(|input| &input.artifact),
                input.map(|input| &input.artifact),
            )?;
            plan.put(identity, changes[0].value())?;
            changes.extend(postings.into_changes());
            plan.root()?
        };
        let id = framed_digest(
            b"current-catalog-only-update/1",
            &[
                &serde_json::to_vec(expected)?,
                identity.as_bytes(),
                &framed_digest(
                    b"current-catalog-value/1",
                    &[changes[0].value().unwrap_or(&[])],
                )
                .as_bytes()[..],
                &[u8::from(changes[0].value().is_some())],
            ],
        );
        let allowance = database::allowance(
            self.connection
                .as_mut()
                .context("current database missing")?,
            self.capacity,
        )?;
        let grant_scope = self.grant_scope(id);
        let mut grant = match self.quota.current_grant(self.writer, &grant_scope)? {
            Some(grant) => grant,
            None => self
                .quota
                .reserve_current(self.writer, grant_scope, allowance)?,
        };
        if grant.capacity() < allowance {
            grant.resize(self.writer, allowance)?;
        }
        crate::capacity::require_growth(self.quota.root(), allowance)?;
        let connection = self
            .connection
            .as_mut()
            .context("current database missing")?;
        let transaction = connection.transaction()?;
        let mut next_root = base.artifact_root;
        for change in &changes {
            next_root = map::put(
                &transaction,
                &self.scope,
                next_root,
                change.key(),
                change.value(),
            )?;
        }
        ensure!(
            next_root == planned_root,
            "current catalog plan and SQL root differ"
        );
        let mut target = base;
        target.sequence = target
            .sequence
            .checked_add(1)
            .context("current sequence overflow")?;
        target.artifact_root = next_root;
        transaction.execute(
            "UPDATE OR ROLLBACK current_state SET head=?1 WHERE id=1 AND phase='ready'",
            [target.canonical_bytes()?],
        )?;
        transaction.commit()?;
        let maintenance_error = grant
            .release(self.writer)
            .err()
            .map(|error| error.to_string());
        Ok(CurrentCommit {
            token: target.token()?,
            maintenance_error,
        })
    }

    pub fn prepare(
        &mut self,
        expected: &CurrentToken,
        target: CurrentHead,
        graph_delta: &dyn PreparedGraphDelta,
        inputs: &dyn CapturedInputs,
        model: Option<ModelObjectSet>,
    ) -> Result<PendingTransaction> {
        self.prepare_mapped(expected, target, graph_delta, inputs, model, &[])
    }

    /// Stage a replacement lexical manifest with the graph and current head.
    /// The owning retrieval adapter must authenticate the referenced files
    /// before preparation; this store binds the exact manifest bytes.
    pub fn prepare_lexical(
        &mut self,
        expected: &CurrentToken,
        mut target: CurrentHead,
        graph_delta: &dyn PreparedGraphDelta,
        inputs: &dyn CapturedInputs,
        model: Option<ModelObjectSet>,
        lexical_manifest: &[u8],
    ) -> Result<PendingTransaction> {
        target.lexical_sequence = target.sequence;
        self.prepare_components(
            expected,
            target,
            graph_delta,
            inputs,
            model,
            ComponentChanges {
                map: &[],
                lexical_manifest: Some(lexical_manifest),
            },
        )
    }

    /// Prepare changed artifact rows and a verified private Tantivy candidate
    /// in one durable intent. The caller supplies the sealed graph target;
    /// this method derives the exact catalog root and lexical digest. The
    /// caller must verify the physical lexical candidate before this call and
    /// again after recovery, before it serves candidates to a reader.
    pub fn prepare_artifacts(
        &mut self,
        expected: &CurrentToken,
        target: CurrentHead,
        graph_delta: &dyn PreparedGraphDelta,
        changes: &[ArtifactChange],
        model: Option<ModelObjectSet>,
        lexical_manifest: &[u8],
    ) -> Result<PendingTransaction> {
        self.prepare_artifacts_with_occurrences(
            expected,
            target,
            graph_delta,
            ArtifactVectorChanges {
                artifacts: changes,
                occurrences: &[],
            },
            model,
            lexical_manifest,
        )
    }

    /// Publish new vector citations with changed artifacts after retiring
    /// their old citations. The sealed graph already contains their nodes.
    pub fn prepare_artifacts_with_occurrences(
        &mut self,
        expected: &CurrentToken,
        mut target: CurrentHead,
        graph_delta: &dyn PreparedGraphDelta,
        update: ArtifactVectorChanges<'_>,
        model: Option<ModelObjectSet>,
        lexical_manifest: &[u8],
    ) -> Result<PendingTransaction> {
        let changes = update.artifacts;
        let occurrences = update.occurrences;
        ensure!(
            !changes.is_empty() && changes.len() <= MAX_PENDING_INPUTS,
            "invalid changed-artifact count"
        );
        let mut previous = None;
        let mut inputs = Vec::new();
        let mut prior_sources = BTreeMap::new();
        let mut map_changes = Vec::with_capacity(changes.len());
        for change in changes {
            let path = change.path();
            ensure!(
                valid_current_path(path) && previous.is_none_or(|prior| prior < path),
                "changed artifact paths are invalid, unsorted or repeated"
            );
            previous = Some(path);
            let value = match change {
                ArtifactChange::Upsert(input) => {
                    let bytes = input.bytes()?;
                    inputs.push(input.as_ref().clone());
                    Some(bytes)
                }
                ArtifactChange::Delete(_) => None,
            };
            map_changes.push(map::MapChange::new(
                &self.scope,
                map::MapDomain::Artifact,
                path.as_bytes(),
                value,
            )?);
        }
        map_changes.sort_by_key(map::MapChange::key);
        ensure!(
            map_changes
                .windows(2)
                .all(|pair| pair[0].key() < pair[1].key()),
            "duplicate current artifact map identity"
        );
        let (phase, base) = state(self.connection()?, &self.scope)?;
        ensure!(
            phase == TransactionPhase::Ready
                && self.pending()?.is_none()
                && base.token()? == *expected,
            "changed-artifact publication base expired"
        );
        let mut plan = map::MapPlan::new(self.connection()?, &self.scope, base.artifact_root)?;
        let mut postings = super::postings::PostingChanges::default();
        for change in changes {
            let key = map::key(
                &self.scope,
                map::MapDomain::Artifact,
                change.path().as_bytes(),
            )?;
            let previous = if let Some(bytes) = plan.get(key)? {
                let previous: CapturedInput = serde_json::from_slice(&bytes)?;
                ensure!(
                    previous.artifact.path == change.path() && previous.bytes()? == bytes,
                    "changed artifact has invalid prior catalog binding"
                );
                Some(previous.artifact)
            } else {
                None
            };
            prior_sources.insert(
                change.path().to_owned(),
                previous
                    .as_ref()
                    .map(|a| a.content_hash.parse::<Sha256Digest>())
                    .transpose()?,
            );
            postings.update(
                &mut plan,
                &self.scope,
                &self.store,
                previous.as_ref(),
                match change {
                    ArtifactChange::Upsert(input) => Some(&input.artifact),
                    ArtifactChange::Delete(_) => None,
                },
            )?;
            let value = match change {
                ArtifactChange::Upsert(input) => Some(input.bytes()?),
                ArtifactChange::Delete(_) => None,
            };
            plan.put(key, value.as_deref())?;
        }
        map_changes.extend(postings.into_changes());
        let reattached = occurrences
            .iter()
            .map(|item| item.pointer.embedding_key)
            .collect::<BTreeSet<_>>();
        {
            let mut context = vector_map::VectorPlanContext {
                plan: &mut plan,
                connection: self.connection()?,
                store: &self.store,
                scope: &self.scope,
                base: &base,
                target: &target,
                delta: graph_delta,
            };
            map_changes.extend(vector_map::retire_changed_paths(
                &mut context,
                &prior_sources,
                &reattached,
            )?);
            map_changes.extend(vector_map::attach_new_occurrences(
                &mut context,
                changes,
                occurrences,
            )?);
        }
        // A replaced path may retire and reattach one exact key. Replay only
        // its final afterimage, not both intermediate map changes.
        let mut final_changes = BTreeMap::new();
        for change in map_changes {
            final_changes.insert(change.key(), change);
        }
        let map_changes = final_changes.into_values().collect::<Vec<_>>();
        ensure!(
            map_changes
                .windows(2)
                .all(|pair| pair[0].key() < pair[1].key()),
            "duplicate current component map identity"
        );
        target.artifact_root = plan.root()?;
        target.lexical_sequence = target.sequence;
        target.lexical_commit = framed_digest(b"current-lexical-manifest/1", &[lexical_manifest]);
        self.prepare_components(
            expected,
            target,
            graph_delta,
            &inputs,
            model,
            ComponentChanges {
                map: &map_changes,
                lexical_manifest: Some(lexical_manifest),
            },
        )
    }

    pub(crate) fn prepare_mapped(
        &mut self,
        expected: &CurrentToken,
        target: CurrentHead,
        graph_delta: &dyn PreparedGraphDelta,
        inputs: &dyn CapturedInputs,
        model: Option<ModelObjectSet>,
        map_changes: &[map::MapChange],
    ) -> Result<PendingTransaction> {
        self.prepare_components(
            expected,
            target,
            graph_delta,
            inputs,
            model,
            ComponentChanges {
                map: map_changes,
                lexical_manifest: None,
            },
        )
    }

    fn prepare_components(
        &mut self,
        expected: &CurrentToken,
        target: CurrentHead,
        graph_delta: &dyn PreparedGraphDelta,
        inputs: &dyn CapturedInputs,
        model: Option<ModelObjectSet>,
        changes: ComponentChanges<'_>,
    ) -> Result<PendingTransaction> {
        let (phase, head) = state(self.connection()?, &self.scope)?;
        ensure!(
            phase == TransactionPhase::Ready
                && self.pending()?.is_none()
                && *expected == head.token()?,
            "current publication base expired or transaction already pending"
        );
        target.validate()?;
        let graph_descriptor = graph_delta.descriptor().clone();
        graph_descriptor.validate()?;
        ensure!(
            target.scope == self.scope
                && target.sequence
                    == head
                        .sequence
                        .checked_add(1)
                        .context("current sequence overflow")?
                && graph_descriptor.base == head.graph_head
                && graph_descriptor.target == target.graph_head,
            "current target does not extend the exact base"
        );
        if let Some(bytes) = changes.lexical_manifest {
            ensure!(
                target.lexical_sequence == target.sequence,
                "changed lexical component must select the target sequence directory"
            );
            verify_lexical_manifest(bytes, target.lexical_commit)?;
        } else {
            ensure!(
                target.lexical_commit == head.lexical_commit
                    && target.lexical_sequence == head.lexical_sequence,
                "changed lexical commit has no captured manifest"
            );
        }
        verify_model(&self.store, &model)?;
        let id = transaction_id(expected, &target, &graph_descriptor, &model)?;
        let grant_scope = self.grant_scope(id);
        let connection = self
            .connection
            .as_mut()
            .context("current database missing")?;
        let allowance = database::allowance(connection, self.capacity)?;
        let _grant = self
            .quota
            .reserve_current(self.writer, grant_scope, allowance)?;
        crate::capacity::require_growth(self.quota.root(), allowance)?;
        let transaction = connection.transaction()?;
        let staged_graph = graph::stage(&transaction, graph_delta, id)?;
        ensure!(
            staged_graph == graph_descriptor,
            "sealed graph descriptor changed during capture"
        );
        let mut digest = InputDigest::new();
        inputs.visit(&mut |input| {
            let bytes = input.bytes()?;
            digest.add(input, &bytes)?;
            input.verify(&self.store)?;
            transaction.execute(
                "INSERT OR ROLLBACK INTO current_pending_inputs(path,metadata) VALUES(?1,?2)",
                params![input.artifact.path, bytes],
            )?;
            Ok(())
        })?;
        let mut map_digest = MapDigest::new();
        let mut map_plan = map::MapPlan::new(&transaction, &self.scope, head.artifact_root)?;
        for change in changes.map {
            map_digest.add(change.key(), change.value())?;
            map_plan.put(change.key(), change.value())?;
            transaction.execute(
                "INSERT OR ROLLBACK INTO current_pending_map(key,value) VALUES(?1,?2)",
                params![change.key().as_bytes().as_slice(), change.value()],
            )?;
        }
        ensure!(
            map_plan.root()? == target.artifact_root,
            "planned current map root differs from captured target"
        );
        if let Some(bytes) = changes.lexical_manifest {
            transaction.execute(
                "INSERT OR ROLLBACK INTO current_pending_lexical(id,content) VALUES(1,?1)",
                [bytes],
            )?;
        }
        let pending = PendingTransaction {
            version: 3,
            id,
            expected: *expected,
            target,
            graph: graph_descriptor,
            inputs_count: digest.count,
            metadata_bytes: digest.bytes,
            object_bytes: digest.objects,
            inputs_digest: digest.finish(),
            map_count: map_digest.count,
            map_bytes: map_digest.bytes,
            map_digest: map_digest.finish(),
            lexical_manifest_bytes: changes
                .lexical_manifest
                .map_or(0, |bytes| bytes.len() as u64),
            model,
            database_capacity_bytes: self.capacity,
        };
        let bytes = pending.bytes()?;
        let hash = framed_digest(b"current-pending-descriptor/1", &[&bytes]);
        transaction.execute("INSERT OR ROLLBACK INTO current_pending(id,transaction_id,descriptor,digest) VALUES(1,?1,?2,?3)", params![id.to_string(), bytes, hash.to_string()])?;
        transaction.execute(
            "UPDATE OR ROLLBACK current_state SET phase='prepared' WHERE id=1",
            [],
        )?;
        transaction.commit()?;
        Ok(pending)
    }

    /// Re-read exact retained input objects; never consult live source or config.
    pub fn visit_captured(
        &self,
        transaction: &PendingTransaction,
        visitor: &mut dyn FnMut(&CapturedInput) -> Result<()>,
    ) -> Result<()> {
        ensure!(
            self.pending()?.as_ref() == Some(transaction),
            "pending transaction changed"
        );
        let connection = self.connection()?;
        let count: i64 =
            connection.query_row("SELECT count(*) FROM current_pending_inputs", [], |row| {
                row.get(0)
            })?;
        ensure!(
            u64::try_from(count)? == transaction.inputs_count,
            "captured input count mismatch"
        );
        let mut digest = InputDigest::new();
        let mut after = String::new();
        loop {
            let item: Option<(String, Vec<u8>)> = connection.query_row("SELECT path,metadata FROM current_pending_inputs WHERE path>?1 AND length(metadata)<=262144 ORDER BY path LIMIT 1", [&after], |row| Ok((row.get(0)?, row.get(1)?))).optional()?;
            let Some((path, bytes)) = item else {
                break;
            };
            let input: CapturedInput = serde_json::from_slice(&bytes)?;
            ensure!(
                input.bytes()? == bytes && input.artifact.path == path,
                "captured input metadata binding mismatch"
            );
            digest.add(&input, &bytes)?;
            input.verify(&self.store)?;
            visitor(&input)?;
            after = path;
        }
        ensure!(
            digest.count == transaction.inputs_count
                && digest.bytes == transaction.metadata_bytes
                && digest.objects == transaction.object_bytes
                && digest.finish() == transaction.inputs_digest,
            "captured inputs digest or count mismatch"
        );
        verify_model(&self.store, &transaction.model)
    }

    pub fn mark_applying(&mut self, pending: &PendingTransaction) -> Result<()> {
        ensure!(
            self.pending()?.as_ref() == Some(pending),
            "pending transaction changed"
        );
        let already_applying = self.phase()? == TransactionPhase::Applying;
        self.visit_captured(pending, &mut |_| Ok(()))?;
        visit_pending_map(self.connection()?, pending, &mut |_, _| Ok(()))?;
        pending_lexical(self.connection()?, pending)?;
        let mut grant = self.grant(pending)?;
        let connection = self
            .connection
            .as_mut()
            .context("current database missing")?;
        let allowance = database::allowance(connection, self.capacity)?;
        grant.resize(self.writer, allowance)?;
        crate::capacity::require_growth(self.quota.root(), allowance)?;
        if already_applying {
            return Ok(());
        }
        let transaction = connection.transaction()?;
        transaction.execute(
            "UPDATE OR ROLLBACK current_state SET phase='applying' WHERE id=1 AND phase='prepared'",
            [],
        )?;
        transaction.commit()?;
        Ok(())
    }

    /// External components have already been durably synced to this exact target.
    /// Graph/catalog publication and removal of replay state are one SQL commit.
    pub fn commit(
        &mut self,
        pending: &PendingTransaction,
        lexical_commit: Sha256Digest,
    ) -> Result<CurrentCommit> {
        ensure!(
            self.pending()?.as_ref() == Some(pending)
                && self.phase()? == TransactionPhase::Applying,
            "current transaction is not applying"
        );
        ensure!(
            lexical_commit == pending.target.lexical_commit,
            "external lexical commit does not match captured target"
        );
        let mut grant = self.grant(pending)?;
        let connection = self
            .connection
            .as_mut()
            .context("current database missing")?;
        let allowance = database::allowance(connection, self.capacity)?;
        if allowance > grant.capacity() {
            grant.resize(self.writer, allowance)?;
        }
        crate::capacity::require_growth(self.quota.root(), allowance)?;
        let transaction = connection.transaction()?;
        let lexical_manifest = pending_lexical(&transaction, pending)?;
        graph::apply(&transaction, pending.id, &pending.graph)?;
        // Start from the authenticated committed base, not an empty map.
        let mut map_root = state(&transaction, &self.scope)?.1.artifact_root;
        visit_pending_map(&transaction, pending, &mut |key, value| {
            map_root = map::put(&transaction, &self.scope, map_root, key, value)?;
            Ok(())
        })?;
        ensure!(
            map_root == pending.target.artifact_root,
            "replayed current map root differs from prepared target"
        );
        if let Some(bytes) = lexical_manifest {
            transaction.execute(
                "INSERT INTO current_lexical_manifest(id,content) VALUES(1,?1) ON CONFLICT(id) DO UPDATE SET content=excluded.content",
                [bytes],
            )?;
        }
        graph::cleanup(&transaction, pending.id)?;
        transaction.execute("DELETE FROM current_pending_inputs", [])?;
        transaction.execute("DELETE FROM current_pending_map", [])?;
        transaction.execute("DELETE FROM current_pending_lexical", [])?;
        transaction.execute("DELETE FROM current_pending WHERE id=1", [])?;
        transaction.execute(
            "UPDATE OR ROLLBACK current_state SET phase='ready',head=?1 WHERE id=1",
            [pending.target.canonical_bytes()?],
        )?;
        transaction.commit()?;
        let maintenance_error = grant
            .release(self.writer)
            .err()
            .map(|error| format!("{error:#}"));
        Ok(CurrentCommit {
            token: pending.target.token()?,
            maintenance_error,
        })
    }

    pub fn abort_prepared(&mut self, pending: &PendingTransaction) -> Result<()> {
        ensure!(
            self.pending()?.as_ref() == Some(pending)
                && self.phase()? == TransactionPhase::Prepared,
            "only an unapplied prepared transaction may be aborted"
        );
        let mut grant = self.grant(pending)?;
        let connection = self
            .connection
            .as_mut()
            .context("current database missing")?;
        let allowance = database::allowance(connection, self.capacity)?;
        grant.resize(self.writer, allowance)?;
        crate::capacity::require_growth(self.quota.root(), allowance)?;
        let transaction = connection.transaction()?;
        graph::cleanup(&transaction, pending.id)?;
        transaction.execute("DELETE FROM current_pending_inputs", [])?;
        transaction.execute("DELETE FROM current_pending_map", [])?;
        transaction.execute("DELETE FROM current_pending_lexical", [])?;
        transaction.execute("DELETE FROM current_pending", [])?;
        transaction.execute(
            "UPDATE OR ROLLBACK current_state SET phase='ready' WHERE id=1",
            [],
        )?;
        transaction.commit()?;
        grant.release(self.writer)
    }

    /// Safe after a Ready commit or a rolled-back pre-Prepared write. Generic
    /// quota cleanup cannot infer this by opening a database under ledger.lock.
    pub fn release_orphan_grants(&self) -> Result<usize> {
        ensure!(
            self.phase()? == TransactionPhase::Ready && self.pending()?.is_none(),
            "pending current transaction still owns recovery capacity"
        );
        let grants = self.quota.current_grants(self.writer)?;
        let mut released = 0;
        for grant in grants {
            ensure!(
                grant.scope().incarnation == self.scope.incarnation,
                "foreign current recovery capacity grant"
            );
            grant.release(self.writer)?;
            released += 1;
        }
        Ok(released)
    }
}
