// SPDX-License-Identifier: Apache-2.0
use super::{
    CurrentHead, CurrentScope, CurrentToken, database, decode_head,
    gate::Gate,
    graph,
    journal::{CapturedInput, committed_lexical_manifest},
    map,
    vector_map::{OccurrencePointer, PathOccurrences, VectorBinding},
};
use crate::FileStore;
use airk_core::graph_storage::{GraphCounts, GraphRecord, GraphRecordReader, GraphScope};
use anyhow::{Context, Result, ensure};
use rusqlite::Connection;
use std::sync::{Arc, Mutex};

/// Proven authentication or structural failure of retained private evidence.
/// Ordinary I/O, admission, and invalid caller arguments must not carry this marker.
#[derive(Clone, Copy, Debug)]
pub struct PrivateIntegrityFailure;
impl std::fmt::Display for PrivateIntegrityFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("private retained evidence failed authentication")
    }
}
impl std::error::Error for PrivateIntegrityFailure {}

/// Request-snapshot expiry refuses this response without invalidating the
/// native owner or treating a caller's elapsed deadline as retained damage.
#[derive(Clone, Copy, Debug)]
pub struct CurrentRequestExpired;
impl std::fmt::Display for CurrentRequestExpired {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("current request snapshot expired; repeat against the current revision")
    }
}
impl std::error::Error for CurrentRequestExpired {}

/// Only a read-only SQLite operation returning BUSY/LOCKED carries this marker.
/// Corruption, expired revisions, permissions and other I/O errors never do.
#[derive(Clone, Copy, Debug)]
pub struct CurrentReadContention;
impl std::fmt::Display for CurrentReadContention {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("current publication read is temporarily busy")
    }
}
impl std::error::Error for CurrentReadContention {}

fn read_error(error: anyhow::Error) -> anyhow::Error {
    if matches!(error.downcast_ref::<rusqlite::Error>(), Some(rusqlite::Error::SqliteFailure(code, _)) if matches!(code.code, rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked))
    {
        error.context(CurrentReadContention)
    } else {
        error
    }
}

/// Isolate imported-origin tests from graph publication while retaining the
/// actual authenticated map, resource reader, collection gate and projection.
#[cfg(test)]
pub(super) fn scoped_import_test_reader(
    store: &FileStore,
    scope: CurrentScope,
    artifact_root: airk_core::identity::Sha256Digest,
    connection: Connection,
) -> Result<CurrentRead> {
    use airk_core::{
        graph_storage::GraphHead,
        identity::{Incarnation, Sha256Digest},
    };
    let head = CurrentHead {
        version: 2,
        scope: scope.clone(),
        sequence: 1,
        artifact_root,
        lexical_sequence: 1,
        lexical_commit: Sha256Digest::from_bytes([9; 32]),
        graph_head: GraphHead::empty(GraphScope {
            root: scope.root,
            collection: scope.collection,
            incarnation: scope.incarnation,
            graph_instance: Incarnation::from_bytes([2; 16]),
            specification: Sha256Digest::from_bytes([3; 32]),
            record_bytes: 16,
        })?,
    };
    Ok(CurrentRead {
        source_policy_fence: Default::default(),
        source_policy: Default::default(),
        read_scope: Default::default(),
        read_scope_fence: Default::default(),
        read_scope_restricts_paths: Default::default(),
        scope_endpoints: Default::default(),
        scope_origin_budget: Default::default(),
        connection: Mutex::new(connection),
        head,
        query_projection_version: Some(2),
        private_query: Default::default(),
        lexical_path: store.directory().join("unused-import-test.sqlite3"),
        lexical_verified: Default::default(),
        deadline: None,
        writer_active: None,
        _gate: Gate::shared(store)?,
    })
}

/// The connection/read transaction drops before the gate. Providers may clone
/// the Arc for one request, but neither a kernel cache nor a sealed delta owns it.
pub struct CurrentRead {
    pub(super) source_policy_fence: std::sync::OnceLock<Arc<std::sync::atomic::AtomicBool>>,
    pub(super) source_policy: std::sync::OnceLock<super::source_policy::ReadPolicy>,
    pub(super) read_scope: std::sync::OnceLock<Arc<airk_core::scoped_reads::CompiledReadScope>>,
    pub(super) read_scope_fence: std::sync::OnceLock<Arc<std::sync::atomic::AtomicBool>>,
    pub(super) read_scope_restricts_paths: std::sync::OnceLock<bool>,
    pub(super) scope_endpoints:
        std::sync::OnceLock<Mutex<std::collections::BTreeMap<String, bool>>>,
    pub(super) scope_origin_budget: std::sync::OnceLock<Mutex<super::read_scope::OriginBudget>>,
    connection: Mutex<Connection>,
    head: CurrentHead,
    query_projection_version: Option<u32>,
    pub(crate) private_query: std::sync::OnceLock<
        std::sync::Arc<crate::service::collection::integrity::native_owner::PrivateQueryAuthority>,
    >,
    pub(super) lexical_path: std::path::PathBuf,
    pub(super) lexical_verified: std::sync::OnceLock<()>,
    deadline: Option<std::time::Instant>,
    writer_active: Option<Arc<std::sync::atomic::AtomicBool>>,
    _gate: Gate,
}

impl CurrentRead {
    /// Ordinary public readers have no capability and remain strict. A native
    /// owner installs the sealed capability before exposing its request reader.
    pub fn private_query_eligible(&self) -> Result<bool> {
        self.live()?;
        if let Some(authority) = self.private_query.get() {
            authority.recheck(self)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Authentication failures revoke the entire live owner before returning.
    /// Busy, permission, quota and input errors do not fabricate damage evidence.
    pub fn observe_private_integrity<T>(&self, result: Result<T>) -> Result<T> {
        match result {
            Ok(value) => Ok(value),
            Err(error) => {
                if let Some(authority) = self.private_query.get()
                    && let Err(persistence) = authority.observe_failure(self, &error)
                {
                    return Err(error.context(format!(
                        "private integrity failure persistence: {persistence:#}"
                    )));
                }
                Err(error)
            }
        }
    }

    pub(crate) fn native_directory(&self) -> &std::path::Path {
        self.lexical_path.parent().expect("database has parent")
    }

    pub fn query_projection_version(&self) -> Result<Option<u32>> {
        self.live()?;
        Ok(self.query_projection_version)
    }
    pub(super) fn reclaim_exclusive(&mut self) -> Result<bool> {
        self._gate.try_exclusive()
    }

    #[cfg(test)]
    pub(super) fn elapse_for_test(&mut self, elapsed: std::time::Duration) {
        self.deadline = self.deadline.map(|deadline| deadline - elapsed);
    }

    #[cfg(all(test, windows))]
    pub(crate) fn expire_request_for_test(&mut self) {
        self.deadline = Some(std::time::Instant::now());
    }

    /// Re-resolve the authenticated pointer under this read gate before loading
    /// its retained body. Detached or superseded candidate pointers are rejected.
    pub fn occurrence_hit(
        &self,
        store: &FileStore,
        pointer: &OccurrencePointer,
    ) -> Result<airk_core::Hit> {
        self.require_path(&pointer.path)?;
        pointer.validate()?;
        let pointers = self
            .vector_occurrence_pointers(pointer.embedding_key)?
            .context("current vector binding missing")?;
        ensure!(
            pointers.iter().any(|current| current == pointer),
            "current occurrence expired or differs from committed evidence"
        );
        let compact = self
            .artifact(&pointer.path)?
            .context("current occurrence artifact missing")?;
        ensure!(
            compact
                .content_hash
                .parse::<airk_core::identity::Sha256Digest>()?
                == pointer.source_hash
                && compact
                    .retained
                    .as_ref()
                    .context("current occurrence extraction missing")?
                    .object
                    .parse::<airk_core::identity::Sha256Digest>()?
                    == pointer.object,
            "current occurrence source or extraction mismatch"
        );
        let retained = store.load_retrieval_artifact(&compact)?;
        ensure!(
            retained.payload_digest == pointer.payload_digest,
            "current occurrence payload mismatch"
        );
        store.verify_retained_source(&compact)?;
        airk_core::vector_evidence::materialize(&retained, &pointer.members)
    }

    /// Authenticate the path-local occurrence count and its exact source
    /// identity. The count addresses old IDs during an incremental replacement.
    pub fn path_occurrences(&self, path: &str) -> Result<Option<PathOccurrences>> {
        let Some(bytes) = self.map_get(map::MapDomain::PathOccurrences, path.as_bytes())? else {
            return Ok(None);
        };
        let index = PathOccurrences::decode(&bytes, path)?;
        let artifact = self
            .artifact(path)?
            .context("current path occurrence index has no catalog artifact")?;
        ensure!(
            artifact
                .content_hash
                .parse::<airk_core::identity::Sha256Digest>()?
                == index.source_hash,
            "current path occurrence source identity mismatch"
        );
        Ok(Some(index))
    }

    /// Exact manifest bytes bound to this request's current head. Retrieval
    /// must also verify the FTS5 shadow rows before trusting candidates.
    pub fn lexical_manifest(&self) -> Result<Vec<u8>> {
        self.with_connection(|connection| committed_lexical_manifest(connection, &self.head))
    }
    /// An exact embedding-key lookup checked against the authenticated graph
    /// record. An absent key has a complete bucket proof; no SQL candidate list
    /// is trusted to establish absence.
    pub fn vector_binding(
        &self,
        embedding_key: airk_core::identity::Sha256Digest,
    ) -> Result<Option<VectorBinding>> {
        let Some(bytes) = self.map_get(map::MapDomain::Vector, embedding_key.as_bytes())? else {
            return Ok(None);
        };
        let binding = VectorBinding::decode(&bytes, &self.head, embedding_key)?;
        let record = self.read(binding.graph_id)?;
        ensure!(
            record.embedding_key == Some(embedding_key)
                && record.live == !binding.occurrence_ids.is_empty(),
            "current vector binding differs from graph record"
        );
        Ok(Some(binding))
    }

    /// Resolve an ANN candidate by physical ID. A live graph node without an
    /// exact authenticated key mapping is corruption, never an empty result.
    pub fn vector_for_graph_id(&self, graph_id: u32) -> Result<VectorBinding> {
        let record = self.read(graph_id)?;
        ensure!(record.live, "current graph candidate is not live");
        let embedding_key = record
            .embedding_key
            .context("current graph candidate has no key")?;
        let binding = self
            .vector_binding(embedding_key)?
            .context("live graph candidate has no current vector binding")?;
        ensure!(
            binding.graph_id == graph_id,
            "current graph candidate ID mismatch"
        );
        Ok(binding)
    }

    /// Returns bounded authenticated pointers only. The caller must load and
    /// verify each pointed-to occurrence object before rendering a citation.
    pub fn vector_occurrence_pointers(
        &self,
        embedding_key: airk_core::identity::Sha256Digest,
    ) -> Result<Option<Vec<OccurrencePointer>>> {
        let Some(binding) = self.vector_binding(embedding_key)? else {
            return Ok(None);
        };
        let mut pointers = Vec::with_capacity(binding.occurrence_ids.len());
        for occurrence_id in binding.occurrence_ids {
            let bytes = self
                .map_get(map::MapDomain::Occurrence, occurrence_id.as_bytes())?
                .context("current vector occurrence pointer missing")?;
            let pointer = OccurrencePointer::decode(&bytes, occurrence_id)?;
            if !self.accepts_path(&pointer.path) {
                continue;
            }
            ensure!(
                pointer.embedding_key == embedding_key,
                "current occurrence vector key mismatch"
            );
            let path_index = self
                .path_occurrences(&pointer.path)?
                .context("current occurrence path index missing")?;
            ensure!(
                path_index.occurrence_id(&self.head.scope, pointer.path_ordinal)? == occurrence_id
                    && path_index.source_hash == pointer.source_hash,
                "current occurrence path ordinal or source differs"
            );
            pointers.push(pointer);
        }
        Ok(Some(pointers))
    }

    /// Authenticates the compact record and path. Call FileStore's verified
    /// retained-object reader before using referenced source or extraction bytes.
    pub fn artifact(&self, path: &str) -> Result<Option<airk_core::Artifact>> {
        Ok(self.captured_input(path)?.map(|input| input.artifact))
    }
    /// Retains the authenticated posting format for physical-only maintenance.
    pub fn captured_input(&self, path: &str) -> Result<Option<CapturedInput>> {
        let Some(bytes) = self.map_get(map::MapDomain::Artifact, path.as_bytes())? else {
            return Ok(None);
        };
        let captured: CapturedInput = serde_json::from_slice(&bytes)?;
        ensure!(
            captured.artifact.path == path && captured.bytes()? == bytes,
            "current artifact record binding mismatch"
        );
        Ok(Some(captured))
    }
    /// Returns authenticated current-map bytes. Referenced retained objects must
    /// still be loaded through FileStore's verified object readers.
    pub fn map_get(&self, domain: map::MapDomain, identity: &[u8]) -> Result<Option<Vec<u8>>> {
        if matches!(
            domain,
            map::MapDomain::Artifact
                | map::MapDomain::FileMetadata
                | map::MapDomain::PathOccurrences
                | map::MapDomain::SourceMeasurement
        ) {
            self.require_path(std::str::from_utf8(identity)?)?;
        }
        let key = map::key(&self.head.scope, domain, identity)?;
        self.with_connection(|connection| {
            map::get(connection, &self.head.scope, self.head.artifact_root, key)
        })
    }

    /// A complete proof-checked enumeration, limited independently by visited
    /// nodes, entries and bytes. This returns no partial list on budget failure.
    pub fn map_entries(
        &self,
        limits: map::MapVisitLimits,
    ) -> Result<Vec<(airk_core::identity::Sha256Digest, Vec<u8>)>> {
        ensure!(
            self.read_scope.get().is_none(),
            airk_core::scoped_reads::ScopeReadRefused
        );
        self.with_connection(|connection| {
            let mut entries = Vec::new();
            map::visit_all(
                connection,
                &self.head.scope,
                self.head.artifact_root,
                limits,
                &mut |key, value| {
                    entries.push((key, value.to_vec()));
                    Ok(())
                },
            )?;
            Ok(entries)
        })
    }
    pub fn has_prepared_update(&self) -> Result<bool> {
        self.with_connection(|connection| {
            Ok(
                connection.query_row("SELECT phase FROM current_state WHERE id=1", [], |row| {
                    row.get::<_, String>(0)
                })? != "ready",
            )
        })
    }
    pub fn head(&self) -> &CurrentHead {
        &self.head
    }
    pub fn token(&self) -> Result<CurrentToken> {
        self.live()?;
        if let Some(authority) = self.private_query.get() {
            authority.recheck(self)?;
        }
        self.head.token()
    }
    /// Damage publication must remain possible after the relaxed capability is
    /// revoked. This is not query authority; ordinary snapshot liveness remains.
    pub(crate) fn integrity_observation_token(&self) -> Result<CurrentToken> {
        self.live()?;
        self.head.token()
    }
    pub fn require_token(&self, token: &CurrentToken) -> Result<()> {
        ensure!(
            *token == self.token()?,
            "current revision expired; repeat the request against the current index"
        );
        Ok(())
    }
    pub(crate) fn with_connection<T>(&self, f: impl FnOnce(&Connection) -> Result<T>) -> Result<T> {
        self.live()?;
        let connection = self
            .connection
            .lock()
            .map_err(|_| anyhow::anyhow!("current reader connection poisoned"))?;
        let result = self.observe_private_integrity(f(&connection).map_err(read_error))?;
        self.live()?;
        Ok(result)
    }
    fn live(&self) -> Result<()> {
        self.require_read_scope_live()?;
        ensure!(
            self.writer_active
                .as_ref()
                .is_none_or(|active| active.load(std::sync::atomic::Ordering::Acquire)),
            "current build snapshot writer has been released"
        );
        if self
            .deadline
            .is_some_and(|deadline| std::time::Instant::now() >= deadline)
        {
            return Err(anyhow::Error::new(CurrentRequestExpired));
        }
        Ok(())
    }
}

impl GraphRecordReader for CurrentRead {
    fn scope(&self) -> &GraphScope {
        &self.head.graph_head.scope
    }
    fn counts(&self) -> Result<GraphCounts> {
        self.live()?;
        Ok(self.head.graph_head.counts())
    }
    fn read(&self, id: u32) -> Result<GraphRecord> {
        self.with_connection(|connection| graph::read_record(connection, &self.head.graph_head, id))
    }
}

impl FileStore {
    pub fn current_read(&self, scope: &CurrentScope) -> Result<Arc<CurrentRead>> {
        super::admission::admit_open(|| self.current_read_once(scope, None).map_err(read_error))
    }
    /// A publication build pins its exact base while the matching collection writer
    /// is held. Query deadlines do not limit inference/graph construction. This
    /// snapshot cannot authorize reads after the writer is released; its gate and
    /// SQL transaction still protect captured evidence until all clones drop.
    pub fn current_build_read(
        &self,
        scope: &CurrentScope,
        writer: &crate::WriterLock,
    ) -> Result<Arc<CurrentRead>> {
        super::gate::check_writer(self, writer)?;
        super::admission::admit_open(|| {
            self.current_read_once(scope, Some(writer.active.clone()))
                .map_err(read_error)
        })
    }
    fn current_read_once(
        &self,
        scope: &CurrentScope,
        writer_active: Option<Arc<std::sync::atomic::AtomicBool>>,
    ) -> Result<Arc<CurrentRead>> {
        scope.validate()?;
        ensure!(
            self.collection_id()? == scope.collection,
            "foreign current collection path"
        );
        let gate = Gate::shared(self)?;
        let deadline = (writer_active.is_none() && super::wal::enabled(self.directory()))
            .then(|| std::time::Instant::now() + std::time::Duration::from_secs(120));
        database::reject_legacy_collection(self.directory())?;
        self.ensure_index_ready()?;
        let (capacity, query_projection_version) =
            database::check_owner_with_projection(self.directory(), scope)?;
        let mut legacy_budget = false;
        if super::wal::enabled(self.directory()) {
            let allowance = crate::quota::Quota::for_store(self)?.verify_current_allowance(
                &crate::quota::CurrentGrantScope {
                    collection: scope.collection.clone(),
                    incarnation: scope.incarnation,
                    transaction: super::wal::id(scope),
                },
                super::wal::legacy_total(capacity)?,
            )?;
            legacy_budget = allowance < super::wal::total(capacity)?;
            if legacy_budget {
                // Retained reads must remain available before the first upgraded
                // writer can reserve more capacity, including during recovery.
                super::wal::validate_legacy_files(self.directory(), capacity)?;
            }
        }
        ensure!(
            std::fs::metadata(self.directory().join("current.sqlite3"))?.len() <= capacity,
            "current database exceeds captured capacity"
        );
        let connection =
            database::open_reader_budget(&self.directory().join("current.sqlite3"), legacy_budget)
                .map_err(read_error)?;
        if let Some(deadline) = deadline {
            connection
                .progress_handler(1000, Some(move || std::time::Instant::now() >= deadline))?;
        }
        let (phase, bytes): (String, Vec<u8>) = connection
            .query_row(
                "SELECT phase,head FROM current_state WHERE id=1 AND length(head)<=65536",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(|error| read_error(error.into()))
            .context("current index metadata missing")?;
        ensure!(
            phase == "ready"
                || phase == "prepared"
                || (phase == "applying" && super::wal::enabled(self.directory())),
            "current index requires writer recovery before reads"
        );
        let head = decode_head(&bytes, scope)?;
        Ok(Arc::new(CurrentRead {
            source_policy_fence: Default::default(),
            source_policy: Default::default(),
            read_scope: std::sync::OnceLock::new(),
            read_scope_fence: std::sync::OnceLock::new(),
            read_scope_restricts_paths: std::sync::OnceLock::new(),
            scope_endpoints: std::sync::OnceLock::new(),
            scope_origin_budget: std::sync::OnceLock::new(),
            connection: Mutex::new(connection),
            head,
            query_projection_version,
            private_query: std::sync::OnceLock::new(),
            lexical_path: self.directory().join("current.sqlite3"),
            lexical_verified: std::sync::OnceLock::new(),
            deadline,
            writer_active,
            _gate: gate,
        }))
    }
}

#[cfg(test)]
mod retry_tests {
    use super::*;
    #[test]
    fn only_sqlite_busy_and_locked_read_errors_gain_a_retry_marker() {
        for (code, retryable) in [
            (rusqlite::ffi::SQLITE_BUSY, true),
            (rusqlite::ffi::SQLITE_LOCKED, true),
            (rusqlite::ffi::SQLITE_CORRUPT, false),
            (rusqlite::ffi::SQLITE_IOERR, false),
            (rusqlite::ffi::SQLITE_PERM, false),
            (rusqlite::ffi::SQLITE_READONLY, false),
        ] {
            let error = rusqlite::Error::SqliteFailure(
                rusqlite::ffi::Error::new(code),
                Some("busy fixture wording".into()),
            );
            let marked = read_error(anyhow::Error::new(error).context("read-only query"));
            assert_eq!(
                marked.downcast_ref::<CurrentReadContention>().is_some(),
                retryable
            );
            assert!(marked.downcast_ref::<rusqlite::Error>().is_some());
        }
        let io =
            read_error(std::io::Error::new(std::io::ErrorKind::PermissionDenied, "busy").into());
        assert!(io.downcast_ref::<CurrentReadContention>().is_none());
    }
}

#[cfg(test)]
mod scope_tests {
    use super::*;
    use airk_core::{
        Artifact, Chunk, DocumentLocation, SearchQuery,
        graph_storage::{GraphHead, GraphScope},
        identity::{Incarnation, Sha256Digest},
        index_catalog::PathScope,
        scoped_reads::{CompiledPathAtom, CompiledReadScope, ScopeReadRefused},
    };

    fn fixture() -> Result<(tempfile::TempDir, CurrentRead)> {
        let directory = tempfile::tempdir()?;
        let store = FileStore::new(directory.path().join("collections-1.2/scoped"));
        drop(store.lock()?);
        let scope = CurrentScope {
            root: "fixture-root".into(),
            collection: "scoped".into(),
            incarnation: Incarnation::from_bytes([1; 16]),
        };
        let connection = Connection::open_in_memory()?;
        // The current reader's mandatory authenticated map backs collection
        // metadata even when selected-page projections are unavailable. Use
        // the production schema owner, not a lexical-only synthetic database.
        map::create(&connection)?;
        super::super::lexical::create(&connection)?;
        for path in [
            "private/a.rs",
            "public/a.rs",
            "publicity/a.rs",
            "@docs/a.md",
            "@notes.md",
            "@types/a.ts",
        ] {
            let artifact = Artifact {
                path: path.into(),
                content_hash: Sha256Digest::from_bytes([3; 32]).to_string(),
                bytes: 6,
                chunks: vec![Chunk {
                    start_line: 1,
                    end_line: 1,
                    text: "needle".into(),
                    location: DocumentLocation::Lines { start: 1, end: 1 },
                }],
                language: "rust".into(),
                extraction_method: "fixture".into(),
                parse_status: "ok".into(),
                facts: vec![],
                retained: None,
            };
            super::super::lexical::replace(&connection, path, Some(&artifact))?;
        }
        super::super::lexical::flush(&connection)?;
        let head = CurrentHead {
            version: 2,
            scope: scope.clone(),
            sequence: 1,
            artifact_root: map::empty_root(&scope)?,
            lexical_sequence: 1,
            lexical_commit: Sha256Digest::from_bytes([4; 32]),
            graph_head: GraphHead::empty(GraphScope {
                root: scope.root.clone(),
                collection: scope.collection.clone(),
                incarnation: scope.incarnation,
                graph_instance: scope.incarnation,
                specification: Sha256Digest::from_bytes([5; 32]),
                record_bytes: 16,
            })?,
        };
        let read = CurrentRead {
            source_policy_fence: Default::default(),
            source_policy: Default::default(),
            connection: Mutex::new(connection),
            head,
            query_projection_version: None,
            read_scope: Default::default(),
            read_scope_fence: Default::default(),
            read_scope_restricts_paths: Default::default(),
            scope_endpoints: Default::default(),
            scope_origin_budget: Default::default(),
            private_query: Default::default(),
            lexical_path: store.directory().join("fixture.sqlite3"),
            lexical_verified: Default::default(),
            deadline: None,
            writer_active: None,
            _gate: Gate::shared(&store)?,
        };
        // This fixture targets actual candidate SQL, not physical manifest audit.
        read.lexical_verified.set(()).expect("new fixture");
        assert!(
            read.collection_state()?.is_none(),
            "production empty-map absence proof must remain available without optional projections"
        );
        Ok((directory, read))
    }

    fn scope(kind: PathScope, path: &str) -> Arc<CompiledReadScope> {
        Arc::new(CompiledReadScope {
            fingerprint: Sha256Digest::from_bytes([8; 32]),
            atoms: vec![CompiledPathAtom {
                kind,
                path: path.into(),
            }],
            source_atoms: vec![],
            external_aliases: vec!["docs".into()],
        })
    }
    fn query(limit: usize) -> SearchQuery {
        SearchQuery {
            text: "needle".into(),
            path_prefix: None,
            limit,
            max_bytes: 4096,
        }
    }

    #[test]
    fn scoped_lexical_and_reference_limits_select_authorized_candidates() -> Result<()> {
        let (_directory, read) = fixture()?;
        read.attach_read_scope(scope(PathScope::Exact, "public/a.rs"))?;
        let result = read.lexical_search(&query(1))?;
        assert_eq!(result.values.len(), 1);
        assert_eq!(result.values[0].citation.path, "public/a.rs");
        assert!(!result.incomplete);
        let selected = std::collections::BTreeSet::from(["private/a.rs".to_owned()]);
        assert!(
            read.lexical_search_paths(&query(1), Some(&selected))?
                .values
                .is_empty()
        );
        for path_order in [false, true] {
            let result = read.lexical_reference_page(&query(1), None, false, path_order, None)?;
            assert_eq!(result.values.len(), 1);
            assert_eq!(result.values[0].citation.path, "public/a.rs");
            assert!(!result.incomplete);
        }
        let error = read.artifact("private/a.rs").unwrap_err();
        assert!(error.downcast_ref::<ScopeReadRefused>().is_some());
        assert!(
            read.attach_read_scope(scope(PathScope::Subtree, "public"))
                .is_err()
        );
        Ok(())
    }

    #[test]
    fn permission_only_lexical_scope_precedes_candidate_limit() -> Result<()> {
        let (_directory, read) = fixture()?;
        let baseline = read.lexical_search(&query(100))?;
        let denied = &baseline
            .values
            .first()
            .expect("fixture candidates")
            .citation
            .path;
        let allowed = baseline
            .values
            .iter()
            .find(|candidate| &candidate.citation.path != denied)
            .expect("fixture contains multiple paths")
            .citation
            .path
            .clone();
        read.attach_read_scope(scope(PathScope::Exact, &allowed))?;
        let result = read.lexical_search(&query(1))?;
        assert_eq!(result.values.len(), 1);
        assert_eq!(result.values[0].citation.path, allowed);
        Ok(())
    }

    #[test]
    fn subtree_and_empty_authorities_do_not_broaden_candidates() -> Result<()> {
        let (_directory, read) = fixture()?;
        read.attach_read_scope(scope(PathScope::Subtree, "public"))?;
        let result = read.lexical_search(&query(100))?;
        assert_eq!(result.values.len(), 1);
        assert_eq!(result.values[0].citation.path, "public/a.rs");
        let (_empty_directory, empty) = fixture()?;
        let mut denied = scope(PathScope::Subtree, "public").as_ref().clone();
        denied.atoms.clear();
        empty.attach_read_scope(Arc::new(denied))?;
        let result = empty.lexical_search(&query(1))?;
        assert!(result.values.is_empty());
        assert!(!result.incomplete);
        Ok(())
    }

    #[test]
    fn primary_scope_preserves_at_files_and_excludes_only_published_aliases() -> Result<()> {
        let (_directory, read) = fixture()?;
        read.attach_read_scope(scope(PathScope::Subtree, ""))?;
        let result = read.lexical_search(&query(100))?;
        let paths = result
            .values
            .iter()
            .map(|hit| hit.citation.path.as_str())
            .collect::<std::collections::BTreeSet<_>>();
        assert!(paths.contains("@notes.md"));
        assert!(paths.contains("@types/a.ts"));
        assert!(!paths.contains("@docs/a.md"));
        Ok(())
    }

    #[test]
    fn revocation_fences_existing_reader_and_cursor_identity() -> Result<()> {
        let (_directory, read) = fixture()?;
        assert_eq!(
            read.scope_bound_cursor_identity("publication"),
            "publication"
        );
        let revoked = Arc::new(std::sync::atomic::AtomicBool::new(false));
        read.attach_read_scope_fence(scope(PathScope::Subtree, "public"), revoked.clone())?;
        assert_ne!(
            read.scope_bound_cursor_identity("publication"),
            "publication"
        );
        revoked.store(true, std::sync::atomic::Ordering::Release);
        assert!(!read.accepts_path("public/a.rs"));
        assert!(
            read.token()
                .unwrap_err()
                .downcast_ref::<ScopeReadRefused>()
                .is_some()
        );
        assert!(
            read.lexical_search(&query(1))
                .unwrap_err()
                .downcast_ref::<ScopeReadRefused>()
                .is_some()
        );
        Ok(())
    }
}
