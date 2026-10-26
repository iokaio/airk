// SPDX-License-Identifier: Apache-2.0
use super::{
    CurrentHead, CurrentScope, CurrentToken, graph,
    journal::CapturedInput,
    map,
    vector_map::{OccurrencePointer, PathOccurrences, VectorBinding},
};
use crate::FileStore;
use airk_core::graph_storage::{GraphCounts, GraphRecord, GraphRecordReader, GraphScope};
use anyhow::{Context, Result, ensure};
use std::sync::Arc;

/// Only a native read gate contention carries this marker.
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
    if error.is::<super::CurrentGateContention>() {
        error.context(CurrentReadContention)
    } else {
        error
    }
}

/// The owned native snapshot drops before its external-object gate. Providers may clone
/// the Arc for one request, but neither a kernel cache nor a sealed delta owns it.
pub struct CurrentRead {
    storage: super::kv::OwnedSnapshot,
    head: CurrentHead,
    deadline: Option<std::time::Instant>,
}
pub(crate) struct RedbRead<'a> {
    reader: &'a CurrentRead,
    transaction: &'a redb::ReadTransaction,
}
impl map::MapStorage for RedbRead<'_> {
    fn bucket_bytes(&self, prefix: u16) -> Result<Option<Vec<u8>>> {
        self.reader.live()?;
        map::MapStorage::bucket_bytes(self.transaction, prefix)
    }
    fn node_bytes(&self, height: u8, index: u16) -> Result<Option<Vec<u8>>> {
        self.reader.live()?;
        map::MapStorage::node_bytes(self.transaction, height, index)
    }
}
impl graph::GraphStorage for RedbRead<'_> {
    fn graph_record(&self, scope: &GraphScope, id: u32) -> Result<Option<GraphRecord>> {
        self.reader.live()?;
        graph::GraphStorage::graph_record(self.transaction, scope, id)
    }
    fn graph_node(
        &self,
        height: u8,
        index: u32,
    ) -> Result<Option<airk_core::identity::Sha256Digest>> {
        self.reader.live()?;
        graph::GraphStorage::graph_node(self.transaction, height, index)
    }
}
impl super::kv::directory::ReadPages for RedbRead<'_> {
    fn read_page(&self, digest: airk_core::identity::Sha256Digest) -> Result<Vec<u8>> {
        self.reader.live()?;
        super::kv::directory::ReadPages::read_page(self.transaction, digest)
    }
}
impl super::kv::posting::ReadPostings for RedbRead<'_> {
    fn read_block(
        &self,
        key: &[u8; 40],
        budget: &mut super::kv::directory::Budget,
    ) -> Result<Vec<u8>> {
        self.reader.live()?;
        super::kv::posting::ReadPostings::read_block(self.transaction, key, budget)
    }
}
impl super::kv::lexical::ReadLexical for RedbRead<'_> {
    fn document(&self, id: u64, budget: &mut super::kv::directory::Budget) -> Result<Vec<u8>> {
        self.reader.live()?;
        super::kv::lexical::ReadLexical::document(self.transaction, id, budget)
    }
}

impl CurrentRead {
    /// Re-resolve the authenticated pointer under this read gate before loading
    /// its retained body. Detached or superseded candidate pointers are rejected.
    pub fn occurrence_hit(
        &self,
        store: &FileStore,
        pointer: &OccurrencePointer,
    ) -> Result<airk_core::Hit> {
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
    /// authenticates the pinned native lexical roots before lookup.
    pub fn lexical_manifest(&self) -> Result<Vec<u8>> {
        self.live()?;
        self.storage
            .snapshot()
            .lexical_manifest()?
            .canonical_bytes()
    }
    /// An exact embedding-key lookup checked against the authenticated graph
    /// record. An absent key has a complete bucket proof; no unauthenticated candidate list
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
        let key = map::key(&self.head.scope, domain, identity)?;
        self.with_map(|connection| {
            map::get(connection, &self.head.scope, self.head.artifact_root, key)
        })
    }

    /// A complete proof-checked enumeration, limited independently by visited
    /// nodes, entries and bytes. This returns no partial list on budget failure.
    pub fn map_entries(
        &self,
        limits: map::MapVisitLimits,
    ) -> Result<Vec<(airk_core::identity::Sha256Digest, Vec<u8>)>> {
        self.with_map(|connection| {
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
        self.live()?;
        Ok(self.storage.snapshot().phase()? != super::TransactionPhase::Ready)
    }
    pub fn head(&self) -> &CurrentHead {
        &self.head
    }
    pub fn token(&self) -> Result<CurrentToken> {
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
    pub(crate) fn with_redb<T>(
        &self,
        f: impl FnOnce(&super::kv::Snapshot, &RedbRead<'_>) -> Result<T>,
    ) -> Result<Option<T>> {
        self.live()?;
        let snapshot = self.storage.snapshot();
        let result = f(
            snapshot,
            &RedbRead {
                reader: self,
                transaction: snapshot.transaction(),
            },
        )?;
        self.live()?;
        Ok(Some(result))
    }
    /// Run bounded native planning against this request's pinned redb revision.
    /// The callback must not retain the borrowed snapshot after it returns.
    pub fn with_redb_snapshot<T>(
        &self,
        f: impl FnOnce(&super::kv::Snapshot) -> Result<T>,
    ) -> Result<Option<T>> {
        self.with_redb(|snapshot, _| f(snapshot))
    }
    fn with_map<T>(&self, f: impl FnOnce(&dyn map::MapStorage) -> Result<T>) -> Result<T> {
        self.live()?;
        let result = f(&RedbRead {
            reader: self,
            transaction: self.storage.snapshot().transaction(),
        })?;
        self.live()?;
        Ok(result)
    }
    pub(crate) fn with_graph<T>(
        &self,
        f: impl FnOnce(&dyn graph::GraphStorage) -> Result<T>,
    ) -> Result<T> {
        self.live()?;
        let result = f(&RedbRead {
            reader: self,
            transaction: self.storage.snapshot().transaction(),
        })?;
        self.live()?;
        Ok(result)
    }
    fn live(&self) -> Result<()> {
        ensure!(
            self.deadline
                .is_none_or(|deadline| std::time::Instant::now() < deadline),
            "current request snapshot expired; repeat against the current revision"
        );
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
        self.with_graph(|connection| graph::read_record(connection, &self.head.graph_head, id))
    }
}

impl FileStore {
    pub fn current_redb_read(&self, scope: &CurrentScope) -> Result<Arc<CurrentRead>> {
        let owned = self.redb_read(scope)?;
        owned.snapshot().phase()?;
        let head = owned.snapshot().head()?;
        Ok(Arc::new(CurrentRead {
            storage: owned,
            head,
            deadline: Some(std::time::Instant::now() + std::time::Duration::from_secs(120)),
        }))
    }
    pub fn current_read(&self, scope: &CurrentScope) -> Result<Arc<CurrentRead>> {
        super::admission::admit_open(|| self.current_redb_read(scope).map_err(read_error))
    }
}
