// SPDX-License-Identifier: Apache-2.0
//! `CurrentRead` over the plain index store: opening a snapshot, synthesizing
//! the head, and reading retained extraction, source and resources from it.
//! The mapping is fixed by `crate::index` CONTRACT.md, "Mapping of the current
//! index".
use super::{CurrentHead, CurrentRead, CurrentScope, collection::CollectionState, gate::Gate};
use crate::index::{DATABASE_FILE, FORMAT, IncompatibleIndex, IndexReader, IndexWriter};
use airk_core::{
    Artifact,
    graph_storage::{GraphHead, GraphScope, MAX_GRAPH_RECORD_BYTES},
    identity::{Sha256Digest, framed_digest},
};
use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use std::path::Path;
use std::sync::Arc;

/// The old store's database, which the plain index never reads.
const OLD_DATABASE: &str = "current.sqlite3";
const MAX_RETAINED_BYTES: u64 = airk_core::extraction::MAX_PAYLOAD_BYTES as u64;
/// The mutable graph record size today: `4 * (dimensions + degree 48 + 1)`.
const GRAPH_DEGREE: u64 = 48;

/// Caller meta keys of the plain index (mapping contract).
pub const SCOPE_KEY: &str = "scope";
pub const COLLECTION_STATE_KEY: &str = "collection_state";
pub const LEXICAL_COMMIT_KEY: &str = "lexical_commit";

/// The digest of a publication's artifact root (contract: `airk-index/root/1`).
pub fn artifact_root(scope: &CurrentScope, sequence: u64) -> Sha256Digest {
    framed_digest(
        b"airk-index/root/1",
        &[scope.incarnation.as_bytes(), &sequence.to_be_bytes()],
    )
}

/// The lexical commit of the publication that last changed lexical rows.
pub fn lexical_commit(scope: &CurrentScope, sequence: u64) -> Sha256Digest {
    framed_digest(
        b"airk-index/lexical/1",
        &[scope.incarnation.as_bytes(), &sequence.to_be_bytes()],
    )
}

/// The token digest of a synthesized head (contract: `current-index-head/3`).
pub fn head_digest(scope: &CurrentScope, sequence: u64) -> Sha256Digest {
    framed_digest(
        b"current-index-head/3",
        &[
            scope.incarnation.as_bytes(),
            &sequence.to_be_bytes(),
            FORMAT.as_bytes(),
        ],
    )
}

impl CurrentRead {
    pub(crate) fn verify_plain_snapshot(
        &self,
        store: &crate::FileStore,
        writer: &crate::WriterLock,
    ) -> Result<()> {
        let directory = super::gate::check_writer(store, writer)?;
        ensure!(
            directory == self.native_directory().canonicalize()?,
            "plain Verify reader and writer select different stores"
        );
        self.live()?;
        IndexWriter::verify_snapshot(&directory, self.head().sequence).map_err(|error| {
            if error
                .downcast_ref::<rusqlite::Error>()
                .is_some_and(|sqlite| {
                    matches!(
                        sqlite.sqlite_error_code(),
                        Some(
                            rusqlite::ErrorCode::DatabaseCorrupt
                                | rusqlite::ErrorCode::NotADatabase
                        )
                    )
                })
            {
                error.context(super::PrivateIntegrityFailure)
            } else {
                error
            }
        })?;
        self.live()
    }
}

fn old_index_present(directory: &Path) -> Result<bool> {
    Ok(directory.join(OLD_DATABASE).try_exists()?
        || directory.join(super::database::OWNER_FILE).try_exists()?)
}

/// The scope stored in a directory's plain index, when it has one. A
/// directory holding only an old index has none: it is not read, and the
/// writer rebuilds it.
pub(super) fn scope_at(
    directory: &Path,
    root: &str,
    collection: &str,
) -> Result<Option<CurrentScope>> {
    if !directory.join(DATABASE_FILE).try_exists()? {
        return Ok(None);
    }
    let index = IndexReader::open(directory)?;
    let scope = stored_scope(&index)?;
    ensure!(
        scope.root == root && scope.collection == collection,
        "foreign current index scope"
    );
    Ok(Some(scope))
}

fn stored_scope(index: &IndexReader) -> Result<CurrentScope> {
    let bytes = index
        .meta(SCOPE_KEY)?
        .context("index scope missing; rebuild the index")?;
    let scope: CurrentScope = serde_json::from_slice(&bytes)?;
    scope.validate()?;
    ensure!(
        serde_json::to_vec(&scope)? == bytes,
        "noncanonical index scope; rebuild the index"
    );
    Ok(scope)
}

/// The head a reader reports for this snapshot, and its collection state.
pub(super) fn synthesize_head(
    index: &IndexReader,
    scope: &CurrentScope,
) -> Result<(CurrentHead, Option<CollectionState>)> {
    ensure!(
        stored_scope(index)? == *scope,
        "foreign current index scope"
    );
    let sequence = index.sequence();
    let state = index
        .meta(COLLECTION_STATE_KEY)?
        .as_deref()
        .map(CollectionState::decode)
        .transpose()?;
    let lexical = match index.meta(LEXICAL_COMMIT_KEY)? {
        Some(bytes) => Sha256Digest::from_bytes(
            bytes
                .try_into()
                .map_err(|_| anyhow::anyhow!("index lexical commit has the wrong length"))?,
        ),
        // A new index before its first lexical publication.
        None => lexical_commit(scope, 0),
    };
    let dimensions = match index.vector_setup()? {
        Some((setup, _, _)) => u64::from(setup.dimensions),
        None => 1,
    };
    let record_bytes = (4 * (dimensions + GRAPH_DEGREE + 1)).min(MAX_GRAPH_RECORD_BYTES as u64);
    let specification = state
        .as_ref()
        .and_then(|state| state.configuration.model.as_ref())
        .map(|model| model.pipeline_digest)
        .unwrap_or_else(|| framed_digest(b"current-lexical-only/1", &[]));
    let mut graph_head = GraphHead::empty(GraphScope {
        root: scope.root.clone(),
        collection: scope.collection.clone(),
        incarnation: scope.incarnation,
        graph_instance: scope.incarnation,
        specification,
        record_bytes: u32::try_from(record_bytes)?,
    })?;
    graph_head.live_nodes = index.live_vector_rows()?;
    let head = CurrentHead {
        version: 2,
        scope: scope.clone(),
        sequence,
        artifact_root: artifact_root(scope, sequence),
        lexical_sequence: sequence,
        lexical_commit: lexical,
        graph_head,
    };
    head.validate()?;
    Ok((head, state))
}

pub(super) fn open_reader(
    directory: &Path,
    scope: &CurrentScope,
    gate: Gate,
    deadline: Option<std::time::Instant>,
    clock: Arc<dyn super::clock::RequestClock>,
    writer_active: Option<Arc<std::sync::atomic::AtomicBool>>,
) -> Result<CurrentRead> {
    if !directory.join(DATABASE_FILE).try_exists()? {
        if old_index_present(directory)? {
            return Err(anyhow::Error::new(IncompatibleIndex)
                .context("this collection holds an old-format index; rebuild the index"));
        }
        anyhow::bail!("current index missing; run airk index");
    }
    let index = IndexReader::open(directory)?;
    let (head, state) = synthesize_head(&index, scope)?;
    Ok(CurrentRead {
        frozen_sources: Default::default(),
        source_policy_fence: Default::default(),
        source_policy: Default::default(),
        read_scope: Default::default(),
        read_scope_fence: Default::default(),
        read_scope_restricts_paths: Default::default(),
        scope_endpoints: Default::default(),
        scope_origin_budget: Default::default(),
        index: std::sync::Mutex::new(index),
        head,
        state,
        private_query: Default::default(),
        directory: directory.to_path_buf(),
        deadline,
        clock,
        writer_active,
        _gate: gate,
    })
}

pub(super) fn decode_retained_payload(
    compact: &Artifact,
    profile: &airk_core::extraction::ExtractionProfile,
    bytes: &[u8],
) -> Result<(Artifact, airk_core::extraction::ExtractionPayload)> {
    let reference = compact
        .retained
        .as_ref()
        .context("retained reference missing")?;
    let payload: airk_core::extraction::ExtractionPayload = serde_json::from_slice(bytes)?;
    let artifact = airk_core::binding::bind_payload(
        &payload,
        profile,
        airk_core::binding::ArtifactBinding {
            path: &compact.path,
            source_hash: compact.content_hash.parse()?,
            source_bytes: compact.bytes,
        },
    )?;
    ensure!(
        artifact.retained.is_none()
            && artifact.path == compact.path
            && artifact.content_hash == compact.content_hash
            && artifact.bytes == compact.bytes
            && artifact.language == compact.language
            && artifact.extraction_method == compact.extraction_method
            && artifact.parse_status == compact.parse_status
            && artifact.chunks.len() == reference.chunks
            && artifact.extracted_bytes() == reference.extracted_bytes
            && artifact.facts.len() == reference.facts,
        "retained artifact does not match pinned inventory"
    );
    Ok((artifact, payload))
}

impl CurrentRead {
    /// The stored payload bytes of a compact record, bound to this snapshot's row.
    fn stored_payload(&self, compact: &Artifact, maximum: u64) -> Result<Vec<u8>> {
        self.require_path(&compact.path)?;
        let reference = compact
            .retained
            .as_ref()
            .context("retained reference missing")?;
        ensure!(
            reference.legacy_origin.is_none(),
            "legacy saved evidence is not available in this index; rebuild the index"
        );
        ensure!(
            reference.object_bytes as u64 <= maximum.min(MAX_RETAINED_BYTES),
            "artifact exceeds request byte limit"
        );
        let row = self
            .file_row(&compact.path)?
            .with_context(|| format!("indexed path {:?} is not in this snapshot", compact.path))?;
        let digest: Sha256Digest = reference.object.parse()?;
        ensure!(
            row.payload == digest && row.source_hash.to_string() == compact.content_hash,
            "retained extraction differs from the indexed record of {:?}",
            compact.path
        );
        let bytes = self
            .with_index(|index| index.payload(&digest))
            .with_context(|| {
                format!(
                    "load retained extraction for indexed path {:?}",
                    compact.path
                )
            })?;
        ensure!(
            bytes.len() == reference.object_bytes,
            "retained artifact length does not match pinned inventory"
        );
        Ok(bytes)
    }

    fn decode_retained(
        &self,
        compact: &Artifact,
        maximum: u64,
    ) -> Result<(Artifact, airk_core::extraction::ExtractionPayload)> {
        let reference = compact
            .retained
            .as_ref()
            .context("retained reference missing")?;
        let profile = reference
            .profile
            .as_ref()
            .context("obsolete extraction format; rebuild the index")?;
        let bytes = self.stored_payload(compact, maximum)?;
        decode_retained_payload(compact, profile, &bytes)
    }

    /// The full artifact of a compact record, rebuilt from this snapshot's payload.
    pub fn load_retained(&self, compact: &Artifact) -> Result<Artifact> {
        self.load_retained_bounded(compact, MAX_RETAINED_BYTES)
    }

    pub fn load_retained_bounded(&self, compact: &Artifact, maximum: u64) -> Result<Artifact> {
        Ok(self.decode_retained(compact, maximum)?.0)
    }

    /// One decode supplies both the evidence projection and the extraction
    /// coordinates needed by semantic-unit planning.
    pub fn load_retrieval_artifact(
        &self,
        compact: &Artifact,
    ) -> Result<airk_core::RetrievalArtifact> {
        let reference = compact
            .retained
            .as_ref()
            .context("semantic extraction metadata missing; rebuild the index")?;
        let profile = reference
            .profile
            .as_ref()
            .context("obsolete extraction format; rebuild the index")?;
        ensure!(
            profile.chunk_version == 3,
            "obsolete semantic extraction format; rebuild the index"
        );
        let (artifact, payload) = self.decode_retained(compact, MAX_RETAINED_BYTES)?;
        Ok(airk_core::RetrievalArtifact {
            artifact,
            profile: profile.clone(),
            payload,
            payload_digest: reference.object.parse()?,
        })
    }

    /// Materialize a ranked unit against the same snapshot's verified extraction.
    pub fn vector_unit_hit(
        &self,
        compact: &Artifact,
        pointer: &super::vector_map::OccurrencePointer,
    ) -> Result<airk_core::Hit> {
        ensure!(
            compact.path == pointer.path && compact.content_hash == pointer.source_hash.to_string(),
            "vector unit source differs from retained artifact"
        );
        self.require_path(&pointer.path)?;
        let unit = self
            .with_index(|index| index.unit_at(&pointer.path, pointer.path_ordinal))?
            .context("ranked vector unit is absent from the pinned snapshot")?;
        ensure!(
            unit.source_hash == pointer.source_hash
                && unit.payload == pointer.payload_digest
                && unit.embedding_key == pointer.embedding_key
                && unit.members == serde_json::to_vec(&pointer.members)?,
            "ranked vector unit differs from the pinned snapshot"
        );
        let retained = self.load_retrieval_artifact(compact)?;
        ensure!(
            retained.payload_digest == pointer.payload_digest,
            "vector unit payload differs from retained artifact"
        );
        let hit = airk_core::vector_evidence::materialize(&retained, &pointer.members)?;
        self.require_path(&hit.citation.path)?;
        self.live()?;
        Ok(hit)
    }

    pub fn load_extraction_payload(
        &self,
        compact: &Artifact,
    ) -> Result<airk_core::extraction::ExtractionPayload> {
        Ok(self.decode_retained(compact, MAX_RETAINED_BYTES)?.1)
    }

    /// Whether this index holds the source bytes of a file (decision Q5:
    /// only `utf8` extractions, whose payload text is the source).
    pub fn retained_source_available(&self, compact: &Artifact) -> bool {
        compact.extraction_method == "utf8"
            && compact
                .retained
                .as_ref()
                .is_some_and(|reference| reference.legacy_origin.is_none())
    }

    /// The original bytes of a `utf8` file. Other files have no stored source.
    pub fn read_retained_source(&self, compact: &Artifact) -> Result<Vec<u8>> {
        ensure!(
            self.retained_source_available(compact),
            "the index does not keep the source of {:?} (extraction {:?}); read the file itself",
            compact.path,
            compact.extraction_method
        );
        let payload = self.load_extraction_payload(compact)?;
        let bytes = payload.text.into_bytes();
        ensure!(
            bytes.len() == compact.bytes
                && format!("{:x}", Sha256::digest(&bytes)) == compact.content_hash,
            "stored source of {:?} does not match its content hash; rebuild the index",
            compact.path
        );
        Ok(bytes)
    }

    /// The stored record names the same source as the file row (Q5). This
    /// does not read source bytes; no copy is kept for most files.
    pub fn verify_retained_source(&self, compact: &Artifact) -> Result<()> {
        self.require_path(&compact.path)?;
        let row = self
            .file_row(&compact.path)?
            .with_context(|| format!("indexed path {:?} is not in this snapshot", compact.path))?;
        ensure!(
            row.source_hash.to_string() == compact.content_hash,
            "indexed record of {:?} differs from its stored source hash",
            compact.path
        );
        Ok(())
    }

    /// A stored resource body under its key.
    pub(crate) fn resource_body(&self, key: &str) -> Result<Option<Vec<u8>>> {
        self.with_index(|index| index.resource(key))
    }

    /// Selected snapshot counts for status, without decoding any record.
    pub fn indexed_file_count(&self) -> Result<u64> {
        self.with_index(|index| index.file_count())
    }
}
