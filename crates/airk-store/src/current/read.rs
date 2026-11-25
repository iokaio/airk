// SPDX-License-Identifier: Apache-2.0
//! The request reader over the plain index store (`crate::index`). One reader
//! is one SQLite read transaction: a stable snapshot of one publication. The
//! head is synthesized from the index `meta` table (index CONTRACT.md,
//! "Mapping of the current index"); there is no Merkle map, journal or graph.
use super::{
    CurrentHead, CurrentScope, CurrentToken,
    gate::Gate,
    journal::CapturedInput,
    map,
    vector_map::{OccurrencePointer, PathOccurrences, VectorBinding},
};
use crate::FileStore;
use crate::index::{Handle, HandleKind, IndexReader};
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

pub(super) fn read_error(error: anyhow::Error) -> anyhow::Error {
    if matches!(error.downcast_ref::<rusqlite::Error>(), Some(rusqlite::Error::SqliteFailure(code, _)) if matches!(code.code, rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked))
    {
        error.context(CurrentReadContention)
    } else {
        error
    }
}

/// The old Merkle-map reader of imported-origin tests has no plain counterpart.
/// Kept so those tests compile; they fail until rewritten against the plain index.
#[cfg(test)]
pub(super) fn scoped_import_test_reader(
    _store: &FileStore,
    _scope: CurrentScope,
    _artifact_root: airk_core::identity::Sha256Digest,
    _connection: Connection,
) -> Result<CurrentRead> {
    anyhow::bail!("the plain index has no Merkle-map test reader")
}

/// The index transaction drops before the gate. Providers may clone the Arc
/// for one request, but neither a kernel cache nor a sealed delta owns it.
pub struct CurrentRead {
    pub(super) frozen_sources: std::sync::OnceLock<super::source_policy::ReadSources>,
    pub(super) source_policy_fence: std::sync::OnceLock<Arc<std::sync::atomic::AtomicBool>>,
    pub(super) source_policy: std::sync::OnceLock<super::source_policy::ReadPolicy>,
    pub(super) read_scope: std::sync::OnceLock<Arc<airk_core::scoped_reads::CompiledReadScope>>,
    pub(super) read_scope_fence: std::sync::OnceLock<Arc<std::sync::atomic::AtomicBool>>,
    pub(super) read_scope_restricts_paths: std::sync::OnceLock<bool>,
    pub(super) scope_endpoints:
        std::sync::OnceLock<Mutex<std::collections::BTreeMap<String, bool>>>,
    pub(super) scope_origin_budget: std::sync::OnceLock<Mutex<super::read_scope::OriginBudget>>,
    pub(super) index: Mutex<IndexReader>,
    pub(super) head: CurrentHead,
    /// Meta `collection_state`, decoded once when the reader opened.
    pub(super) state: Option<super::collection::CollectionState>,
    /// Kept so service code that installs an authority still compiles. The
    /// plain index has no private accelerators: readers stay strict.
    pub(crate) private_query: std::sync::OnceLock<
        std::sync::Arc<crate::service::collection::integrity::native_owner::PrivateQueryAuthority>,
    >,
    pub(super) directory: std::path::PathBuf,
    pub(super) deadline: Option<std::time::Instant>,
    /// Time source for `deadline`; injected so deadline decisions are testable.
    pub(super) clock: Arc<dyn super::clock::RequestClock>,
    pub(super) writer_active: Option<Arc<std::sync::atomic::AtomicBool>>,
    pub(super) _gate: Gate,
}

impl CurrentRead {
    /// Freeze the published source roots before exposure. This deliberately
    /// avoids live(): live admission itself consults the frozen roots.
    pub(super) fn freeze_source_roots(&self) -> Result<()> {
        let sources = super::source_policy::ReadSources {
            primary_root: self.head.scope.root.clone(),
            has_configuration: self.state.is_some(),
            aliases: self
                .state
                .as_ref()
                .map(|state| state.configuration.roots.clone())
                .unwrap_or_default(),
        };
        ensure!(
            self.frozen_sources.set(sources).is_ok(),
            "reader source roots already frozen"
        );
        Ok(())
    }
    /// The plain index has no private accelerators; every reader is strict.
    pub fn private_query_eligible(&self) -> Result<bool> {
        self.live()?;
        Ok(false)
    }

    /// Readers never relax checks, so no private capability is consulted for
    /// queries. A service-installed authority still records proven damage.
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
        &self.directory
    }

    /// Query projections do not exist in the plain index.
    pub fn query_projection_version(&self) -> Result<Option<u32>> {
        self.live()?;
        Ok(None)
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
        self.deadline = Some(self.clock.now());
    }

    /// Resolve a ranked vector unit against the pinned extraction.
    pub fn occurrence_hit(
        &self,
        _store: &FileStore,
        pointer: &OccurrencePointer,
    ) -> Result<airk_core::Hit> {
        let compact = self
            .artifact(&pointer.path)?
            .context("ranked vector artifact is absent from the pinned snapshot")?;
        self.vector_unit_hit(&compact, pointer)
    }

    /// Rank committed vector rows from this snapshot, then resolve their
    /// authorized units in rank, path and ordinal order.
    pub fn ranked_vector_units(
        &self,
        pipeline: airk_core::identity::Sha256Digest,
        query: &[f32],
        paths: Option<&[String]>,
        prefix: Option<&str>,
        row_limit: usize,
        unit_limit: usize,
    ) -> Result<(Vec<OccurrencePointer>, bool)> {
        self.ranked_vector_units_for_queries(
            pipeline,
            &[query],
            paths,
            prefix,
            row_limit,
            unit_limit,
        )?
        .pop()
        .context("missing vector query result")
    }

    /// Rank up to four query vectors against one authenticated matrix and scope.
    /// The original query gets half of a shared budget when auxiliary queries exist.
    pub fn ranked_vector_units_for_queries(
        &self,
        pipeline: airk_core::identity::Sha256Digest,
        queries: &[&[f32]],
        paths: Option<&[String]>,
        prefix: Option<&str>,
        row_limit: usize,
        unit_limit: usize,
    ) -> Result<Vec<(Vec<OccurrencePointer>, bool)>> {
        ensure!(
            (1..=4).contains(&queries.len())
                && row_limit / 2 >= queries.len() - 1
                && unit_limit / 2 >= queries.len() - 1,
            "plain vector query count exceeds its shared budget"
        );
        let quota = |limit: usize, position| {
            if queries.len() == 1 {
                limit
            } else if position == 0 {
                limit.div_ceil(2)
            } else {
                let others = queries.len() - 1;
                let remaining = limit / 2;
                remaining / others + usize::from(position - 1 < remaining % others)
            }
        };
        ensure!(
            (1..=1000).contains(&row_limit) && (1..=1000).contains(&unit_limit),
            "plain vector search limit out of range"
        );
        let paths = paths.map(serde_json::to_string).transpose()?;
        self.with_index(|index| {
            let (setup, _, committed) = index
                .vector_setup()?
                .context("semantic retrieval not ready: vector store is unconfigured")?;
            ensure!(
                setup.pipeline == pipeline
                    && queries.iter().all(|query| setup.dimensions as usize == query.len()),
                "indexed vector pipeline differs from query model"
            );
            let live = index.live_rows_for_paths(paths.as_deref(), prefix, committed)?;
            if live.is_empty() {
                return Ok(queries.iter().map(|_| (Vec::new(), false)).collect());
            }
            self.live()?;
            let matrix = index.load_vectors()?.context("indexed vectors disappeared")?;
            self.live()?;
            let mut results = Vec::with_capacity(queries.len());
            for (position, query) in queries.iter().enumerate() {
                self.live()?;
                let row_limit = quota(row_limit, position);
                let unit_limit = quota(unit_limit, position);
                let ranked = matrix.search(query, row_limit, Some(&live))?;
                #[cfg(feature = "test-support")]
                let pointer_timing = (paths.is_none()
                    && prefix.is_none_or(str::is_empty)
                    && std::env::var_os("AIRK_PRIVATE_VECTOR_POINTER_TIMING").as_deref()
                        == Some(std::ffi::OsStr::new("1")))
                .then(|| (std::time::Instant::now(), ranked.len()));
                let mut incomplete = ranked.len() < live.len();
                let mut pointers = Vec::new();
                for hit in ranked {
                    self.live()?;
                    let remaining = unit_limit - pointers.len();
                    let units = index.scoped_units_for_row(
                        hit.row,
                        paths.as_deref(),
                        prefix,
                        remaining + 1,
                    )?;
                    incomplete |= units.len() > remaining;
                    for unit in units.into_iter().take(remaining) {
                        if !self.accepts_path(&unit.path) {
                            incomplete = true;
                            continue;
                        }
                        let occurrence = PathOccurrences {
                            version: 1,
                            path: unit.path,
                            source_hash: unit.source_hash,
                            count: unit.ordinal.checked_add(1).context("unit ordinal overflow")?,
                        };
                        let occurrence_id = occurrence.occurrence_id(&self.head.scope, unit.ordinal)?;
                        let pointer = OccurrencePointer {
                            version: 1,
                            embedding_key: unit.embedding_key,
                            occurrence_id,
                            path: occurrence.path,
                            path_ordinal: unit.ordinal,
                            source_hash: unit.source_hash,
                            payload_digest: unit.payload,
                            object: unit.payload,
                            members: serde_json::from_slice(&unit.members)?,
                        };
                        pointer.validate()?;
                        pointers.push(pointer);
                    }
                    if incomplete && pointers.len() == unit_limit {
                        break;
                    }
                }
                #[cfg(feature = "test-support")]
                if let Some((start, ranked_count)) = pointer_timing {
                    eprintln!(
                        "AIRK_VECTOR_POINTER_TIMING wanted={row_limit} ranked={ranked_count} pointer_count={} elapsed_ns={}",
                        pointers.len(),
                        start.elapsed().as_nanos()
                    );
                }
                results.push((pointers, incomplete));
            }
            Ok(results)
        })
    }

    /// No path occurrence index exists in the plain index.
    pub fn path_occurrences(&self, path: &str) -> Result<Option<PathOccurrences>> {
        self.require_path(path)?;
        self.live()?;
        Ok(None)
    }

    /// The plain index has no lexical manifest; FTS5 consistency is SQLite's.
    pub fn lexical_manifest(&self) -> Result<Vec<u8>> {
        anyhow::bail!("the plain index has no lexical manifest")
    }
    /// Vector bindings are not read from the plain index yet.
    pub fn vector_binding(
        &self,
        _embedding_key: airk_core::identity::Sha256Digest,
    ) -> Result<Option<VectorBinding>> {
        self.live()?;
        Ok(None)
    }

    /// The plain index has no graph records.
    pub fn vector_for_graph_id(&self, _graph_id: u32) -> Result<VectorBinding> {
        anyhow::bail!("the plain index has no graph candidates")
    }

    /// The plain index has no graph records.
    pub fn graph_candidate_pointers(&self, _graph_id: u32) -> Result<Vec<OccurrencePointer>> {
        anyhow::bail!("the plain index has no graph candidates")
    }

    /// Vector occurrences are not read from the plain index yet.
    pub fn vector_occurrence_pointers(
        &self,
        _embedding_key: airk_core::identity::Sha256Digest,
    ) -> Result<Option<Vec<OccurrencePointer>>> {
        self.live()?;
        Ok(None)
    }

    /// The compact record of a path. Load extraction or source bytes through
    /// this reader (`load_retained`, `read_retained_source`).
    pub fn artifact(&self, path: &str) -> Result<Option<airk_core::Artifact>> {
        Ok(self.captured_input(path)?.map(|input| input.artifact))
    }
    /// The stored `files.record`: today's exact `CapturedInput` bytes.
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
    /// Domain lookups with a plain counterpart. Domains whose data the plain
    /// index does not hold (vectors, recent history) are absent; domains that
    /// moved to their own tables are refused so no caller misreads them.
    pub fn map_get(&self, domain: map::MapDomain, identity: &[u8]) -> Result<Option<Vec<u8>>> {
        use map::MapDomain;
        match domain {
            MapDomain::Artifact => {
                let path = std::str::from_utf8(identity)?;
                self.require_path(path)?;
                Ok(self.file_row(path)?.map(|row| row.record))
            }
            MapDomain::SourceMeasurement => {
                let path = std::str::from_utf8(identity)?;
                self.require_path(path)?;
                self.with_index(|index| {
                    index.resource(&super::plain_writer::source_measurement_key(path))
                })
            }
            MapDomain::ArtifactHandle | MapDomain::UnitHandle => {
                let kind = if matches!(domain, MapDomain::ArtifactHandle) {
                    HandleKind::Artifact
                } else {
                    HandleKind::Unit
                };
                let handle = Handle {
                    kind,
                    value: identity.to_vec(),
                };
                Ok(self
                    .with_index(|index| index.handle(&handle))?
                    .map(String::into_bytes))
            }
            MapDomain::Vector
            | MapDomain::Occurrence
            | MapDomain::PathOccurrences
            | MapDomain::RecentChanges => {
                if matches!(domain, MapDomain::PathOccurrences) {
                    self.require_path(std::str::from_utf8(identity)?)?;
                }
                self.live()?;
                Ok(None)
            }
            MapDomain::Metadata
            | MapDomain::Posting
            | MapDomain::FactName
            | MapDomain::EdgeSource
            | MapDomain::EdgeTarget
            | MapDomain::FileMetadata => {
                anyhow::bail!("{domain:?} entries are not stored as map values in the plain index")
            }
        }
    }

    /// The plain index has no map to enumerate.
    pub fn map_entries(
        &self,
        _limits: map::MapVisitLimits,
    ) -> Result<Vec<(airk_core::identity::Sha256Digest, Vec<u8>)>> {
        ensure!(
            self.read_scope.get().is_none(),
            airk_core::scoped_reads::ScopeReadRefused
        );
        anyhow::bail!("the plain index has no map to enumerate")
    }
    /// A batch is one SQLite transaction: a reader never sees a prepared update.
    pub fn has_prepared_update(&self) -> Result<bool> {
        self.live()?;
        Ok(false)
    }
    pub fn head(&self) -> &CurrentHead {
        &self.head
    }
    pub fn token(&self) -> Result<CurrentToken> {
        self.live()?;
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
    /// The plain index exposes no old-format connection. Kept so the old
    /// writer-side callers compile until they are replaced.
    pub(crate) fn with_connection<T>(
        &self,
        _f: impl FnOnce(&Connection) -> Result<T>,
    ) -> Result<T> {
        self.live()?;
        anyhow::bail!("the plain index has no current-format database connection")
    }
    /// One call on this reader's snapshot, under liveness checks.
    pub(crate) fn with_index<T>(&self, f: impl FnOnce(&IndexReader) -> Result<T>) -> Result<T> {
        self.live()?;
        let index = self
            .index
            .lock()
            .map_err(|_| anyhow::anyhow!("current reader index poisoned"))?;
        let result = f(&index).map_err(read_error)?;
        self.live()?;
        Ok(result)
    }
    /// The stored row of a path in this snapshot.
    pub(crate) fn file_row(&self, path: &str) -> Result<Option<crate::index::FileRow>> {
        if !super::journal::valid_current_path(path) {
            return Ok(None);
        }
        self.with_index(|index| index.file(path))
    }
    pub(super) fn live(&self) -> Result<()> {
        self.require_read_scope_live()?;
        ensure!(
            self.writer_active
                .as_ref()
                .is_none_or(|active| active.load(std::sync::atomic::Ordering::Acquire)),
            "current build snapshot writer has been released"
        );
        if self
            .deadline
            .is_some_and(|deadline| self.clock.now() >= deadline)
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
    fn read(&self, _id: u32) -> Result<GraphRecord> {
        anyhow::bail!("the plain index has no graph records")
    }
}

impl FileStore {
    /// Installer metadata observation under an existing native object gate.
    /// Never creates gate/owner/bootstrap files, opens a writer or reads source.
    pub(crate) fn checkpoint_current_head(
        &self,
        root: &str,
        collection: &str,
    ) -> Result<Option<CurrentHead>> {
        crate::validate_collection(collection)?;
        ensure!(
            self.collection_id()? == collection,
            "foreign checkpoint collection"
        );
        let _gate = Gate::shared_existing(self)?;
        self.ensure_index_ready()?;
        let Some(scope) = super::plain::scope_at(self.directory(), root, collection)? else {
            return Ok(None);
        };
        let index = IndexReader::open(self.directory())?;
        Ok(Some(super::plain::synthesize_head(&index, &scope)?.0))
    }

    pub fn current_read(&self, scope: &CurrentScope) -> Result<Arc<CurrentRead>> {
        let clock = super::clock::system();
        super::admission::admit_open(|| {
            self.current_read_once(scope, None, clock.clone())
                .map_err(read_error)
        })
    }
    /// A request reader whose deadline follows `clock` (Windows tests only).
    #[cfg(all(test, windows))]
    pub(crate) fn current_read_with_clock(
        &self,
        scope: &CurrentScope,
        clock: Arc<dyn super::clock::RequestClock>,
    ) -> Result<Arc<CurrentRead>> {
        super::admission::admit_open(|| {
            self.current_read_once(scope, None, clock.clone())
                .map_err(read_error)
        })
    }
    /// A publication build pins its exact base while the matching collection writer
    /// is held. Query deadlines do not limit inference/graph construction. This
    /// snapshot cannot authorize reads after the writer is released; its gate and
    /// read transaction still protect captured evidence until all clones drop.
    pub fn current_build_read(
        &self,
        scope: &CurrentScope,
        writer: &crate::WriterLock,
    ) -> Result<Arc<CurrentRead>> {
        super::gate::check_writer(self, writer)?;
        super::admission::admit_open(|| {
            self.current_read_once(scope, Some(writer.active.clone()), super::clock::system())
                .map_err(read_error)
        })
    }
    fn current_read_once(
        &self,
        scope: &CurrentScope,
        writer_active: Option<Arc<std::sync::atomic::AtomicBool>>,
        clock: Arc<dyn super::clock::RequestClock>,
    ) -> Result<Arc<CurrentRead>> {
        crate::service::collection::validate_split_store_access(self)?;
        scope.validate()?;
        ensure!(
            self.collection_id()? == scope.collection,
            "foreign current collection path"
        );
        let gate = Gate::shared(self)?;
        let deadline = writer_active
            .is_none()
            .then(|| clock.now() + std::time::Duration::from_secs(120));
        self.ensure_index_ready()?;
        let read = Arc::new(super::plain::open_reader(
            self.directory(),
            scope,
            gate,
            deadline,
            clock,
            writer_active,
        )?);
        read.freeze_source_roots()?;
        read.require_source_policy_live()?;
        Ok(read)
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
