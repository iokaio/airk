// SPDX-License-Identifier: Apache-2.0
//! The write side of the current index over the plain store (`crate::index`).
//!
//! It follows "Mapping of the current index" in `index/CONTRACT.md`: the
//! existing current types (`CapturedInput`, `InventoryFile`, `Citation`,
//! `CollectionState`, resources) are written into the plain tables, one SQLite
//! transaction per batch. There is no Merkle map, journal, object file, grant
//! or reclamation; extraction payloads travel with the batch in memory.
use super::{
    CapturedInput, CurrentScope, CurrentToken,
    collection::{CollectionConfiguration, CollectionPlan, CollectionState},
    lexical, postings,
};
use crate::{
    FileStore, WriterLock,
    index::{
        self, Batch, ChunkEntry, EncodedPayload, FileChange, FileEntry, Handle, HandleKind,
        IndexReader, IndexWriter, PathLookup, ResourceChange, VectorBatch, VectorCommit,
        VectorSetup,
    },
};
use airk_core::{
    Artifact, ArtifactRef, Citation,
    extraction::{ExtractionPayload, ExtractionProfile},
    identity::{Sha256Digest, framed_digest},
    publication::InventoryFile,
};
use anyhow::{Context, Result, ensure};
use std::{collections::BTreeMap, path::Path};

pub use super::plain::{COLLECTION_STATE_KEY, LEXICAL_COMMIT_KEY, SCOPE_KEY, lexical_commit};
const SOURCE_MEASUREMENT_PREFIX: &str = "source-measurement/";
/// The engine bounds resource keys; longer paths use their digest.
const MAX_RESOURCE_KEY_BYTES: usize = 256;
/// Size cap of one collection's plain index (D8).
pub const MAX_INDEX_BYTES: u64 = 1 << 40;

/// The resource key of one file's usage measurement. A path too long for the
/// engine's key bound is named by its digest instead.
pub fn source_measurement_key(path: &str) -> String {
    let key = format!("{SOURCE_MEASUREMENT_PREFIX}{path}");
    if key.len() <= MAX_RESOURCE_KEY_BYTES {
        key
    } else {
        format!(
            "{SOURCE_MEASUREMENT_PREFIX}#{}",
            framed_digest(b"airk-index/source-measurement/1", &[path.as_bytes()])
        )
    }
}

/// Bound of payloads producers stage for the next publication, per process.
const MAX_STAGED_PAYLOAD_BYTES: usize = 1 << 30;
static STAGED_PAYLOADS: std::sync::LazyLock<
    std::sync::Mutex<std::collections::HashMap<Sha256Digest, Vec<u8>>>,
> = std::sync::LazyLock::new(Default::default);

/// Stage a producer's payload (its raw JSON bytes) for the next publication
/// that upserts a record naming its digest. Keyed by content digest.
pub fn stage_payload(raw: Vec<u8>) -> Result<Sha256Digest> {
    let encoded = EncodedPayload::encode(&raw)?;
    let mut staged = STAGED_PAYLOADS
        .lock()
        .map_err(|_| anyhow::anyhow!("staged payloads poisoned"))?;
    let total: usize = staged.values().map(Vec::len).sum();
    if total.saturating_add(raw.len()) > MAX_STAGED_PAYLOAD_BYTES {
        staged.clear();
    }
    staged.insert(encoded.digest, raw);
    Ok(encoded.digest)
}

fn take_staged_payload(digest: &Sha256Digest) -> Result<Option<Vec<u8>>> {
    Ok(STAGED_PAYLOADS
        .lock()
        .map_err(|_| anyhow::anyhow!("staged payloads poisoned"))?
        .remove(digest))
}

/// Whether `directory` holds a plain index (the engine's format check), as
/// opposed to the 1.1 SQLite store that used the same file name.
pub fn is_plain_index(directory: &Path) -> bool {
    directory.join(index::DATABASE_FILE).is_file() && IndexReader::open(directory).is_ok()
}

/// The token of one plain publication; the reader's `CurrentHead::token`
/// uses the same digest (`plain::head_digest`).
pub fn plain_token(scope: &CurrentScope, sequence: u64) -> CurrentToken {
    CurrentToken {
        incarnation: scope.incarnation,
        sequence,
        digest: super::plain::head_digest(scope, sequence),
    }
}
/// The plain index ran out of room (D8). The batch was not written.
#[derive(Clone, Copy, Debug)]
pub struct IndexSizeCap {
    pub index_bytes: u64,
    pub cap_bytes: u64,
}
impl std::fmt::Display for IndexSizeCap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "index size cap reached: {} bytes stored, cap {} bytes",
            self.index_bytes, self.cap_bytes
        )
    }
}
impl std::error::Error for IndexSizeCap {}

/// One file's extraction as the batch carries it: the materialized artifact
/// (chunks and facts), the path-free payload and its stored encoding.
pub struct CapturedPayload {
    pub artifact: Artifact,
    pub payload: ExtractionPayload,
    pub encoded: EncodedPayload,
}

impl CapturedPayload {
    /// Capture a fresh extraction. Returns the compact artifact that becomes
    /// the file record; nothing is written until the batch is applied.
    pub fn capture(
        artifact: &Artifact,
        payload: &ExtractionPayload,
        profile: &ExtractionProfile,
    ) -> Result<(Artifact, Self)> {
        let _stage = airk_core::stage_timing::scope("blob.retain");
        profile.identity()?;
        let bytes = serde_json::to_vec(payload)?;
        let encoded = EncodedPayload::encode(&bytes)?;
        let mut compact = artifact.clone();
        compact.retained = Some(compact_reference(artifact, &encoded, profile));
        compact.chunks.clear();
        compact.facts.clear();
        let mut full = artifact.clone();
        full.retained = compact.retained.clone();
        Ok((
            compact,
            Self {
                artifact: full,
                payload: payload.clone(),
                encoded,
            },
        ))
    }

    /// Rebuild the batch form of a stored payload for a compact record.
    pub fn from_stored(compact: &Artifact, raw: Vec<u8>) -> Result<Self> {
        let retained = compact
            .retained
            .as_ref()
            .context("stored record has no extraction reference")?;
        let profile = retained
            .profile
            .as_ref()
            .context("stored record has no extraction profile; rebuild the index")?;
        let payload: ExtractionPayload = serde_json::from_slice(&raw)?;
        let encoded = EncodedPayload::encode(&raw)?;
        ensure!(
            encoded.digest == retained.object.parse::<Sha256Digest>()?,
            "stored payload differs from its record"
        );
        let mut artifact = airk_core::binding::bind_payload(
            &payload,
            profile,
            airk_core::binding::ArtifactBinding {
                path: &compact.path,
                source_hash: compact.content_hash.parse()?,
                source_bytes: compact.bytes,
            },
        )?;
        artifact.retained = compact.retained.clone();
        Ok(Self {
            artifact,
            payload,
            encoded,
        })
    }

    /// Indexed lines as the old store counted them: for a UTF-8 file of
    /// extraction contract 2 the lines of its source (the payload text).
    pub fn indexed_lines(&self) -> usize {
        let profile = self
            .artifact
            .retained
            .as_ref()
            .and_then(|retained| retained.profile.as_ref());
        if profile.is_some_and(|p| p.contract_version == 2)
            && self.artifact.extraction_method == "utf8"
        {
            let text = self.payload.text.as_bytes();
            text.iter().filter(|byte| **byte == b'\n').count()
                + usize::from(text.last().is_some_and(|byte| *byte != b'\n'))
        } else {
            self.artifact
                .chunks
                .iter()
                .map(|chunk| chunk.end_line)
                .max()
                .unwrap_or(0)
        }
    }
}

fn compact_reference(
    artifact: &Artifact,
    encoded: &EncodedPayload,
    profile: &ExtractionProfile,
) -> ArtifactRef {
    ArtifactRef {
        legacy_origin: None,
        source_encoding: None,
        payload_encoding: None,
        object: encoded.digest.to_string(),
        object_bytes: usize::try_from(encoded.raw_bytes).unwrap_or(usize::MAX),
        // The plain index keeps no source copy (Q5). The flag keeps today's
        // record shape; readers decide source availability by extraction
        // method, as the mapping contract states.
        raw_source: true,
        chunks: artifact.chunks.len(),
        extracted_bytes: artifact.extracted_bytes(),
        facts: artifact.facts.len(),
        profile: Some(profile.clone()),
    }
}

/// One changed file of a plain publication.
pub struct PlainUpsert {
    pub input: CapturedInput,
    pub file: InventoryFile,
    pub captured: CapturedPayload,
    pub vector_pending: bool,
}

pub enum PlainChange {
    Upsert(Box<PlainUpsert>),
    Delete(String),
}

impl PlainChange {
    pub fn path(&self) -> &str {
        match self {
            Self::Upsert(upsert) => &upsert.input.artifact.path,
            Self::Delete(path) => path,
        }
    }
}

/// A resource body (or its removal) under one of today's resource keys.
#[derive(Clone, Debug)]
pub struct PlainResource {
    pub key: String,
    pub body: Option<Vec<u8>>,
}

pub struct PlainPublication<'a> {
    /// Sorted by path, unique.
    pub changes: Vec<PlainChange>,
    pub resources: Vec<PlainResource>,
    /// Applied configuration published with this batch, if it changed.
    pub configuration: Option<&'a CollectionConfiguration>,
    /// Recount every file after this batch (a configuration change that
    /// replaced every file over several batches).
    pub recount: bool,
}

#[derive(Clone, Debug)]
pub struct PlainCommit {
    pub token: CurrentToken,
    pub files_changed: usize,
    pub chunks_added: u64,
    pub changed_artifacts: usize,
}

/// A read snapshot of the plain index for the write side.
pub struct PlainRead {
    reader: IndexReader,
    scope: CurrentScope,
}

impl PlainRead {
    pub fn open(directory: &Path) -> Result<Self> {
        let reader = IndexReader::open(directory)?;
        let scope: CurrentScope = serde_json::from_slice(
            &reader
                .meta(SCOPE_KEY)?
                .context("plain index has no scope; rebuild the index")?,
        )?;
        scope.validate()?;
        Ok(Self { reader, scope })
    }
    pub fn reader(&self) -> &IndexReader {
        &self.reader
    }
    pub fn scope(&self) -> &CurrentScope {
        &self.scope
    }
    pub fn sequence(&self) -> u64 {
        self.reader.sequence()
    }
    pub fn token(&self) -> CurrentToken {
        plain_token(&self.scope, self.sequence())
    }
    pub fn collection_state(&self) -> Result<Option<CollectionState>> {
        self.reader
            .meta(COLLECTION_STATE_KEY)?
            .as_deref()
            .map(CollectionState::decode)
            .transpose()
    }
    pub fn resource(&self, key: &str) -> Result<Option<Vec<u8>>> {
        self.reader.resource(key)
    }
    /// The compact captured input stored for one path.
    pub fn input(&self, path: &str) -> Result<Option<CapturedInput>> {
        self.reader
            .file(path)?
            .map(|row| decode_record(&row.record))
            .transpose()
    }
    pub fn inventory(&self, path: &str) -> Result<Option<InventoryFile>> {
        self.reader
            .file(path)?
            .map(|row| Ok(serde_json::from_slice(&row.metadata)?))
            .transpose()
    }
    /// Every stored compact artifact, by path.
    pub fn artifacts(&self) -> Result<BTreeMap<String, Artifact>> {
        let mut artifacts = BTreeMap::new();
        self.visit_paths(&mut |path| {
            let input = self.input(path)?.context("listed index file missing")?;
            artifacts.insert(path.to_owned(), input.artifact);
            Ok(())
        })?;
        Ok(artifacts)
    }
    pub fn visit_paths(&self, visit: &mut dyn FnMut(&str) -> Result<()>) -> Result<()> {
        let mut after: Option<String> = None;
        loop {
            let page = self
                .reader
                .lookup(&PathLookup::Files, "", after.as_deref(), 4096)?;
            for path in &page.paths {
                visit(path)?;
            }
            after = page.next_after;
            if after.is_none() {
                return Ok(());
            }
        }
    }
    /// The batch form of a stored file's extraction: the payload this index
    /// stores under the record's digest, else one a producer staged for this
    /// publication (`stage_payload`).
    pub fn captured(&self, compact: &Artifact) -> Result<CapturedPayload> {
        let digest: Sha256Digest = compact
            .retained
            .as_ref()
            .context("stored record has no extraction reference")?
            .object
            .parse()?;
        let raw = match self.reader.payload(&digest) {
            Ok(raw) => raw,
            Err(stored) => take_staged_payload(&digest)?.ok_or(stored)?,
        };
        CapturedPayload::from_stored(compact, raw)
    }
}

fn decode_record(bytes: &[u8]) -> Result<CapturedInput> {
    let input: CapturedInput = serde_json::from_slice(bytes)?;
    ensure!(
        input.bytes()? == bytes,
        "noncanonical plain index file record"
    );
    Ok(input)
}

impl FileStore {
    pub fn plain_index_present(&self) -> Result<bool> {
        Ok(self.directory().join(index::DATABASE_FILE).try_exists()?)
    }

    /// A collection directory holding the old current store and no plain index.
    pub fn plain_old_format(&self) -> Result<bool> {
        Ok(
            self.directory().join("current.sqlite3").try_exists()?
                && !self.plain_index_present()?,
        )
    }

    pub fn plain_read(&self) -> Result<PlainRead> {
        PlainRead::open(self.directory())
    }

    /// Create the owner header (if absent) and an empty plain index for this
    /// scope and configuration. Refuses an existing index.
    pub fn plain_create(
        &self,
        writer: &WriterLock,
        scope: &CurrentScope,
        configuration: &CollectionConfiguration,
    ) -> Result<()> {
        super::gate::check_writer(self, writer)?;
        writer.invalidate_census_before_mutation()?;
        scope.validate()?;
        configuration.validate()?;
        ensure!(!self.plain_index_present()?, "plain index already exists");
        // Without a plain index an owner header names nothing; one left by an
        // interrupted creation for another scope is replaced.
        let owner = self.directory().join(super::database::OWNER_FILE);
        if owner.try_exists()?
            && super::database::owned_scope(self.directory()).ok().as_ref() != Some(scope)
        {
            std::fs::remove_file(&owner)?;
        }
        super::database::create_owner(
            self.directory(),
            scope,
            super::database::MAX_DATABASE_BYTES,
        )?;
        let state = CollectionState {
            version: 1,
            configuration: configuration.clone(),
            counts: Default::default(),
        };
        let mut index = IndexWriter::create(self.directory())?;
        index.apply(Batch {
            meta: vec![
                (SCOPE_KEY.into(), serde_json::to_vec(scope)?),
                (COLLECTION_STATE_KEY.into(), state.canonical_bytes()?),
                (
                    LEXICAL_COMMIT_KEY.into(),
                    lexical_commit(scope, 0).as_bytes().to_vec(),
                ),
            ],
            ..Default::default()
        })?;
        Ok(())
    }

    pub fn plain_writer(&self, writer: &WriterLock) -> Result<PlainWriter> {
        super::gate::check_writer(self, writer)?;
        let read = self.plain_read()?;
        Ok(PlainWriter {
            index: IndexWriter::open(self.directory())?,
            scope: read.scope.clone(),
            directory: self.directory().to_path_buf(),
        })
    }
}

/// The single writer of one collection's plain index. The caller holds the
/// collection writer lock for its lifetime.
pub struct PlainWriter {
    index: IndexWriter,
    scope: CurrentScope,
    directory: std::path::PathBuf,
}

impl PlainWriter {
    pub fn scope(&self) -> &CurrentScope {
        &self.scope
    }

    /// Select the embedding pipeline, or turn vectors off with `None`.
    pub fn configure_vectors(&mut self, setup: Option<&VectorSetup>) -> Result<bool> {
        self.index.configure_vectors(setup)
    }

    pub fn vector_row(&self, key: &Sha256Digest) -> Result<Option<u64>> {
        self.index.vector_row(key)
    }

    /// Publish vector rows and the caller's semantic progress together.
    pub fn commit_vectors(
        &mut self,
        batch: VectorBatch,
        resources: Vec<PlainResource>,
    ) -> Result<VectorCommit> {
        let growth = batch
            .files
            .iter()
            .flat_map(|file| &file.units)
            .map(|unit| unit.members.len() as u64 + 128)
            .chain(
                batch
                    .vectors
                    .iter()
                    .map(|(_, row)| 4 * row.len() as u64 + 128),
            )
            .chain(resources.iter().map(|resource| {
                resource.key.len() as u64
                    + resource.body.as_ref().map_or(0, |body| body.len() as u64)
            }))
            .fold(0u64, u64::saturating_add);
        self.check_space(growth)?;
        let resources = resources
            .into_iter()
            .map(|resource| ResourceChange {
                key: resource.key,
                body: resource.body,
            })
            .collect::<Vec<_>>();
        self.index.commit_vectors_with_resources(batch, &resources)
    }

    fn check_space(&self, growth: u64) -> Result<()> {
        check_space(&self.directory, growth)
    }

    /// Publish one batch in one transaction.
    pub fn publish(&mut self, publication: PlainPublication<'_>) -> Result<PlainCommit> {
        let _stage = airk_core::stage_timing::scope("publish.plain");
        ensure!(
            publication
                .changes
                .windows(2)
                .all(|pair| pair[0].path() < pair[1].path()),
            "plain publication paths are unsorted or repeated"
        );
        let read = PlainRead::open(&self.directory)?;
        ensure!(read.scope == self.scope, "plain index scope changed");
        let mut plan =
            CollectionPlan::from_state(read.collection_state()?, publication.configuration)?;
        let mut files = Vec::with_capacity(publication.changes.len());
        let mut resources = Vec::new();
        let mut growth = 0u64;
        let mut changed_artifacts = 0usize;
        for change in publication.changes {
            let path = change.path().to_owned();
            let previous = read.input(&path)?;
            match change {
                PlainChange::Delete(path) => {
                    plan.update(previous.as_ref().map(|input| &input.artifact), None)?;
                    if previous.is_some() {
                        changed_artifacts += 1;
                    }
                    resources.push(ResourceChange {
                        key: source_measurement_key(&path),
                        body: None,
                    });
                    files.push(FileChange::Delete(path));
                }
                PlainChange::Upsert(upsert) => {
                    plan.update(
                        previous.as_ref().map(|input| &input.artifact),
                        Some(&upsert.input.artifact),
                    )?;
                    changed_artifacts += 1;
                    let measurement = serde_json::to_vec(
                        &airk_core::usage::SourceContextSize::measure(&upsert.captured.artifact)?,
                    )?;
                    resources.push(ResourceChange {
                        key: source_measurement_key(&path),
                        body: Some(measurement),
                    });
                    let entry = file_entry(*upsert)?;
                    growth = growth
                        .saturating_add(entry.payload.stored_bytes() as u64)
                        .saturating_add(entry.record.len() as u64)
                        .saturating_add(entry.metadata.len() as u64)
                        .saturating_add(
                            entry
                                .chunks
                                .iter()
                                .map(|c| (c.citation.len() + c.terms.len()) as u64)
                                .sum::<u64>(),
                        );
                    files.push(FileChange::Upsert(Box::new(entry)));
                }
            }
        }
        for resource in publication.resources {
            ensure!(
                !resource.key.starts_with(SOURCE_MEASUREMENT_PREFIX)
                    && resource.key != "embedding-cache",
                "plain publication resource key is reserved"
            );
            growth = growth.saturating_add(resource.body.as_ref().map_or(0, Vec::len) as u64);
            resources.push(ResourceChange {
                key: resource.key,
                body: resource.body,
            });
        }
        let mut state = plan.into_state();
        if publication.recount
            && let Some(state) = state.as_mut()
        {
            // Every stored file after this batch: unchanged rows from the
            // snapshot, changed rows from the batch.
            let mut artifacts = read.artifacts()?;
            for change in &files {
                match change {
                    FileChange::Delete(path) => {
                        artifacts.remove(path);
                    }
                    FileChange::Upsert(entry) => {
                        artifacts
                            .insert(entry.path.clone(), decode_record(&entry.record)?.artifact);
                    }
                }
            }
            state.recount(artifacts.values())?;
        }
        let next = read
            .sequence()
            .checked_add(1)
            .context("index sequence overflow")?;
        drop(read);
        let mut meta = Vec::new();
        if let Some(state) = &state {
            meta.push((COLLECTION_STATE_KEY.to_owned(), state.canonical_bytes()?));
        }
        let lexical = !files.is_empty();
        if lexical {
            meta.push((
                LEXICAL_COMMIT_KEY.to_owned(),
                lexical_commit(&self.scope, next).as_bytes().to_vec(),
            ));
        }
        self.check_space(growth)?;
        let files_changed = files.len();
        let commit = self.index.apply(Batch {
            files,
            stats: vec![],
            resources,
            change: None,
            meta,
        })?;
        ensure!(
            commit.sequence <= next,
            "plain index sequence moved under the writer"
        );
        Ok(PlainCommit {
            token: plain_token(&self.scope, commit.sequence),
            files_changed,
            chunks_added: commit.chunks_added,
            changed_artifacts,
        })
    }
}

/// Free space on the volume and the index size cap (D8), before a batch.
fn check_space(directory: &Path, growth: u64) -> Result<()> {
    let mut stored = 0u64;
    for name in [
        index::DATABASE_FILE,
        &format!("{}-wal", index::DATABASE_FILE),
    ] {
        if let Ok(metadata) = std::fs::metadata(directory.join(name)) {
            stored = stored.saturating_add(metadata.len());
        }
    }
    if stored.saturating_add(growth) > MAX_INDEX_BYTES {
        return Err(anyhow::Error::new(IndexSizeCap {
            index_bytes: stored,
            cap_bytes: MAX_INDEX_BYTES,
        }));
    }
    // A batch may briefly need its growth twice (database and WAL).
    crate::capacity::probe(directory)?.require_growth(growth.saturating_mul(2))
}

pub(crate) struct CurrentFtsPermit {
    directory: std::path::PathBuf,
    scope: CurrentScope,
    token: CurrentToken,
    settings: Sha256Digest,
    collection_state: Vec<u8>,
    lexical_commit: Sha256Digest,
}

impl FileStore {
    pub(crate) fn capture_current_fts_permit(
        &self,
        writer: &WriterLock,
    ) -> Result<CurrentFtsPermit> {
        crate::service::collection::validate_split_store_access(self)?;
        let directory = super::gate::check_writer(self, writer)?;
        let snapshot = IndexWriter::fts_publication_snapshot(&directory)?;
        let scope = super::plain::decode_stored_scope(&snapshot.scope)?;
        ensure!(
            scope.collection == self.collection_id()?,
            "foreign Current repair scope"
        );
        CollectionState::decode_canonical(&snapshot.collection_state)?;
        let token = plain_token(&scope, snapshot.sequence);
        let settings =
            crate::service::collection::settings_digest_with_writer(self, &scope.root, writer)?;
        Ok(CurrentFtsPermit {
            directory,
            scope,
            token,
            settings,
            collection_state: snapshot.collection_state,
            lexical_commit: snapshot.lexical_commit,
        })
    }

    pub(crate) fn restore_current_fts(
        &self,
        writer: &WriterLock,
        permit: &CurrentFtsPermit,
        proposal: &index::FtsRestore<'_>,
        deadline: std::time::Instant,
        cancelled: impl Fn() -> bool,
    ) -> Result<CurrentToken> {
        let directory = super::gate::check_writer(self, writer)?;
        ensure!(
            directory == permit.directory
                && permit.token == plain_token(&permit.scope, proposal.sequence),
            "stale or foreign Current FTS repair permit"
        );
        ensure!(
            !cancelled() && std::time::Instant::now() < deadline,
            "Current FTS repair cancelled or expired"
        );
        crate::service::collection::validate_split_store_access(self)?;
        ensure!(
            crate::service::collection::settings_digest_with_writer(
                self,
                &permit.scope.root,
                writer,
            )? == permit.settings,
            "Current FTS repair saved settings changed"
        );
        check_space(&directory, 4 * index::FTS_RESTORE_MAX_BYTES as u64)?;
        writer.invalidate_census_before_mutation()?;
        let next = IndexWriter::restore_fts(
            &directory,
            proposal,
            index::FtsPublication::Current {
                scope: &permit.scope,
                collection_state: &permit.collection_state,
                lexical_commit: permit.lexical_commit,
            },
            deadline,
            cancelled,
            |record, retained| repair_chunk_terms(proposal, record, retained),
        )?;
        Ok(plain_token(&permit.scope, next))
    }
}

#[cfg(test)]
impl PlainWriter {
    pub(super) fn restore_missing_fts_row(
        &self,
        store: &FileStore,
        writer: &WriterLock,
        permit: &CurrentFtsPermit,
        proposal: &index::FtsRestore<'_>,
        deadline: std::time::Instant,
        cancelled: impl Fn() -> bool,
    ) -> Result<CurrentToken> {
        ensure!(
            self.directory.canonicalize()? == permit.directory && self.scope == permit.scope,
            "stale or foreign Current FTS repair permit"
        );
        store.restore_current_fts(writer, permit, proposal, deadline, cancelled)
    }
}

fn repair_chunk_terms(
    proposal: &index::FtsRestore<'_>,
    record: &[u8],
    retained: &[u8],
) -> Result<String> {
    use sha2::{Digest, Sha256};

    let compact = decode_record(record)?.artifact;
    let reference = compact
        .retained
        .as_ref()
        .context("Current FTS repair has no retained reference")?;
    let profile = reference
        .profile
        .as_ref()
        .context("Current FTS repair has no extraction profile")?;
    ensure!(
        compact.path == proposal.missing.path
            && compact.extraction_method == "utf8"
            && compact.content_hash.parse::<Sha256Digest>()? == proposal.source_hash
            && reference.legacy_origin.is_none()
            && reference.object.parse::<Sha256Digest>()? == proposal.payload_digest
            && reference.object_bytes == retained.len(),
        "Current FTS repair retained binding differs"
    );
    let (artifact, payload) = super::plain::decode_retained_payload(&compact, profile, retained)?;
    ensure!(
        payload.text.len() == compact.bytes
            && Sha256Digest::from_bytes(Sha256::digest(payload.text.as_bytes()).into())
                == proposal.source_hash,
        "Current FTS repair original source differs"
    );
    let (ordinal, stable_id, citation) = proposal
        .missing
        .expected
        .context("Current FTS repair expected chunk is absent")?;
    let chunk = artifact
        .chunks
        .get(usize::try_from(ordinal)?)
        .context("Current FTS repair retained chunk ordinal is absent")?;
    let rebuilt = plain_chunk(&compact, u64::from(ordinal), chunk)?;
    ensure!(
        rebuilt.stable_id == stable_id && rebuilt.citation.as_slice() == citation,
        "Current FTS repair retained citation differs"
    );
    Ok(rebuilt.terms)
}

/// The engine entry of one changed file, per the mapping contract.
fn file_entry(upsert: PlainUpsert) -> Result<FileEntry> {
    let PlainUpsert {
        input,
        file,
        captured,
        vector_pending,
    } = upsert;
    let compact = &input.artifact;
    let path = &compact.path;
    let full = &captured.artifact;
    ensure!(
        full.path == *path
            && full.content_hash == compact.content_hash
            && compact.retained.as_ref().map(|r| r.object.as_str())
                == Some(captured.encoded.digest.to_string().as_str()),
        "captured extraction differs from its file record"
    );
    let mut chunks = Vec::with_capacity(full.chunks.len());
    for (ordinal, chunk) in full.chunks.iter().enumerate() {
        chunks.push(plain_chunk(compact, ordinal as u64, chunk)?);
    }
    let mut handles = vec![Handle {
        kind: HandleKind::Artifact,
        value: file.state.artifact_id.as_bytes().to_vec(),
    }];
    let mut units = std::collections::BTreeSet::new();
    for record in &file.continuity {
        let value = serde_json::to_vec(&record.key)?;
        if units.insert(value.clone()) {
            handles.push(Handle {
                kind: HandleKind::Unit,
                value,
            });
        }
    }
    Ok(FileEntry {
        path: path.clone(),
        source_hash: compact.content_hash.parse()?,
        language: compact.language.clone(),
        record: input.bytes()?,
        metadata: serde_json::to_vec(&file)?,
        stat: None,
        selectors: postings::plain_selectors(full, input.postings_version)?,
        payload: captured.encoded,
        chunks,
        handles,
        vector_pending,
    })
}

pub(super) fn plain_chunk(
    compact: &Artifact,
    ordinal: u64,
    chunk: &airk_core::Chunk,
) -> Result<ChunkEntry> {
    let path = &compact.path;
    let citation = Citation {
        path: path.clone(),
        content_hash: compact.content_hash.clone(),
        start_line: chunk.start_line,
        end_line: chunk.end_line,
        location: chunk.location.clone(),
    };
    let terms = lexical::chunk_terms(path, &chunk.text);
    Ok(ChunkEntry {
        stable_id: lexical::stable_id(path, ordinal)?,
        citation: serde_json::to_string(&citation)?.into_bytes(),
        terms,
    })
}
