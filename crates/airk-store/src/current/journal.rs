// SPDX-License-Identifier: Apache-2.0
//! Captured, bounded replay state survives disposable staging and process death.
use super::{
    CurrentHead, CurrentScope, CurrentToken, MAX_INPUT_METADATA_BYTES, MAX_PENDING_INPUTS,
    MAX_PENDING_METADATA_BYTES, database, decode_head, gate::Gate, graph,
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
    fs,
    io::{Read, Write},
    path::Path,
};

const MAX_DESCRIPTOR_BYTES: usize = 196_608;
const MAX_CAPTURED_OBJECT_BYTES: u64 = 64 * 1024 * 1024 * 1024;
const BOOTSTRAP_FILE: &str = "current-bootstrap.json";

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

impl CapturedInput {
    fn bytes(&self) -> Result<Vec<u8>> {
        let artifact = &self.artifact;
        ensure!(
            !artifact.path.is_empty()
                && artifact.path.len() <= 8192
                && !artifact.path.contains('\\')
                && Path::new(&artifact.path).is_relative()
                && artifact
                    .path
                    .split('/')
                    .all(|part| !part.is_empty() && part != "." && part != ".."),
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
    fn verify(&self, store: &FileStore) -> Result<()> {
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
    pub model: Option<ModelObjectSet>,
    pub database_capacity_bytes: u64,
}

impl PendingTransaction {
    fn bytes(&self) -> Result<Vec<u8>> {
        self.target.validate()?;
        self.graph.validate()?;
        database::validate_capacity(self.database_capacity_bytes)?;
        ensure!(
            self.version == 1
                && self.inputs_count <= MAX_PENDING_INPUTS as u64
                && self.metadata_bytes <= MAX_PENDING_METADATA_BYTES
                && self.object_bytes <= MAX_CAPTURED_OBJECT_BYTES
                && self.graph.target == self.target.graph_head
                && self.expected.incarnation == self.target.scope.incarnation
                && self.expected.sequence.checked_add(1) == Some(self.target.sequence),
            "invalid pending current transaction"
        );
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
        CREATE TABLE current_pending_inputs(path TEXT PRIMARY KEY,metadata BLOB NOT NULL CHECK(length(metadata)<=262144)) WITHOUT ROWID;")?;
    graph::create(connection)
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
            && transaction.graph.base == head.graph_head,
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
        let path = self.directory().join("index.sqlite3");
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
                .join("index.sqlite3-journal")
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
                &self.store.directory().join("index.sqlite3"),
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

    pub fn prepare(
        &mut self,
        expected: &CurrentToken,
        target: CurrentHead,
        graph_delta: &dyn PreparedGraphDelta,
        inputs: &dyn CapturedInputs,
        model: Option<ModelObjectSet>,
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
        let pending = PendingTransaction {
            version: 1,
            id,
            expected: *expected,
            target,
            graph: graph_descriptor,
            inputs_count: digest.count,
            metadata_bytes: digest.bytes,
            object_bytes: digest.objects,
            inputs_digest: digest.finish(),
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
        graph::apply(&transaction, pending.id, &pending.graph)?;
        graph::cleanup(&transaction, pending.id)?;
        transaction.execute("DELETE FROM current_pending_inputs", [])?;
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
