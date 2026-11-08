// SPDX-License-Identifier: Apache-2.0
//! Authenticated current file identity and selected continuity, outside SQL buckets.
use super::{
    CurrentRead, CurrentScope,
    map::{self, MapDomain},
};
use crate::{FileStore, quota::Reservation};
use airk_core::{
    identity::{Sha256Digest, framed_digest},
    publication::InventoryFile,
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    io::{Read, Write},
    path::PathBuf,
};

pub const MAX_METADATA_BYTES: usize = 16 * 1024 * 1024;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectionOmission {
    LegacySavedEvidence,
    MixedLegacyBatch,
    Budget,
    PreparationLimit,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileMetadataRef {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encoding: Option<airk_core::compression::EncodedObject>,
    pub path: String,
    pub digest: Sha256Digest,
    pub bytes: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scalars: Option<FileScalars>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native_facts: Option<super::resources::ResourceReference>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native_edges: Option<super::resources::ResourceReference>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub native_edges_empty: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native_lines: Option<super::resources::ResourceReference>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_ranges: Option<super::text_range::TextRangeReference>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native_facts_omission: Option<ProjectionOmission>,
    #[serde(skip)]
    line_witness: Option<std::sync::Arc<IndexedLinesWitness>>,
}
fn is_false(value: &bool) -> bool {
    !*value
}
impl FileMetadataRef {
    pub(super) fn validate_projection_declaration(&self) -> Result<()> {
        ensure!(
            self.native_facts.is_none() || self.native_facts_omission.is_none(),
            "conflicting native projection declaration"
        );
        ensure!(
            (self.native_edges.is_none() && !self.native_edges_empty)
                || self.native_facts.is_some(),
            "native edge declaration lacks its native fact component"
        );
        ensure!(
            !self.native_edges_empty || self.native_edges.is_none(),
            "conflicting empty native edge declaration"
        );
        ensure!(
            self.native_lines.is_none() || self.scalars.is_some(),
            "native line declaration lacks scalar component"
        );
        ensure!(
            self.text_ranges.is_none()
                || (self.scalars.is_some()
                    && self.native_facts.is_some()
                    && self.native_lines.is_none()),
            "text range declaration lacks its fact/scalar components or conflicts with native source ranges"
        );
        if let Some(text) = &self.text_ranges {
            text.validate()?;
        }
        Ok(())
    }
}

/// Ephemeral content-bound producer input, never serialized or query authority.
/// The accepting journal still revalidates captured source/payload hashes.
#[derive(Debug)]
struct IndexedLinesWitness {
    source: Sha256Digest,
    payload: Sha256Digest,
    profile: Sha256Digest,
    lines: usize,
}
pub struct PreparedMetadataInput {
    retrieval: airk_core::RetrievalArtifact,
    witness: std::sync::Arc<IndexedLinesWitness>,
}
impl PreparedMetadataInput {
    pub fn artifact(&self) -> &airk_core::Artifact {
        &self.retrieval.artifact
    }
    pub fn payload(&self) -> &airk_core::extraction::ExtractionPayload {
        &self.retrieval.payload
    }
    pub fn indexed_lines(&self) -> usize {
        self.witness.lines
    }
}
impl IndexedLinesWitness {
    fn matches(&self, artifact: &airk_core::Artifact) -> Result<bool> {
        let retained = artifact
            .retained
            .as_ref()
            .context("line witness extraction missing")?;
        let profile = if let Some(origin) = &retained.legacy_origin {
            origin.identity()?
        } else {
            retained
                .profile
                .as_ref()
                .context("line witness profile missing")?
                .identity()?
        };
        Ok(self.source == artifact.content_hash.parse()?
            && self.payload == retained.object.parse()?
            && self.profile == profile)
    }
}

/// Publication-derived scalars, authenticated by the existing current map.
/// Absence on legacy heads means no projection; it is never an empty file claim.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileScalars {
    pub version: u32,
    pub indexed_lines: usize,
    /// Exact content/profile binding; all other file-info scalars already live
    /// in the compact Artifact. Do not duplicate FileState/root/continuity here.
    pub binding: Sha256Digest,
}
impl FileScalars {
    fn binding(source: Sha256Digest, payload: Sha256Digest, profile: Sha256Digest) -> Sha256Digest {
        framed_digest(
            b"current-file-scalars/1",
            &[source.as_bytes(), payload.as_bytes(), profile.as_bytes()],
        )
    }
    fn from_file(file: &InventoryFile) -> Self {
        Self {
            version: 1,
            indexed_lines: file.indexed_lines,
            binding: Self::binding(
                file.state.source_hash,
                file.state.payload_digest,
                file.state.profile_id,
            ),
        }
    }
    fn verify(&self, artifact: &airk_core::Artifact, _sequence: u64) -> Result<()> {
        let retained = artifact
            .retained
            .as_ref()
            .context("current scalar extraction missing")?;
        let profile = if let Some(origin) = &retained.legacy_origin {
            origin.identity()?
        } else {
            retained
                .profile
                .as_ref()
                .context("current scalar profile missing")?
                .identity()?
        };
        let expected = Self::binding(
            artifact.content_hash.parse()?,
            retained.object.parse()?,
            profile,
        );
        ensure!(
            self.version == 1 && self.binding == expected,
            "current scalar metadata source/profile mismatch"
        );
        Ok(())
    }
}

fn verify_indexed_lines(
    store: &FileStore,
    artifact: &airk_core::Artifact,
    scalars: &FileScalars,
) -> Result<()> {
    if artifact
        .retained
        .as_ref()
        .is_some_and(|retained| retained.legacy_origin.is_some())
    {
        let full = store.load_legacy_saved_artifact(artifact)?;
        ensure!(
            scalars.indexed_lines
                == full
                    .chunks
                    .iter()
                    .map(|chunk| chunk.end_line)
                    .max()
                    .unwrap_or(0),
            "legacy scalar indexed line count mismatch"
        );
        return Ok(());
    }
    let profile = artifact
        .retained
        .as_ref()
        .and_then(|retained| retained.profile.as_ref())
        .context("scalar producer extraction profile missing")?;
    let lines = if profile.contract_version == 2 && artifact.extraction_method == "utf8" {
        store.verify_retained_source_utf8_lines(artifact)?
    } else {
        artifact
            .chunks
            .iter()
            .map(|chunk| chunk.end_line)
            .max()
            .unwrap_or(0)
    };
    ensure!(
        scalars.indexed_lines == lines,
        "scalar producer indexed line count mismatch"
    );
    Ok(())
}

fn object(store: &FileStore, digest: Sha256Digest) -> PathBuf {
    store
        .directory()
        .join("current-metadata")
        .join(format!("{digest}.json"))
}

impl FileStore {
    /// A private producer view keeps the authenticated compact reference with
    /// materialized facts. General evidence materialization retains its schema.
    pub(crate) fn load_projection_artifact(
        &self,
        compact: &airk_core::Artifact,
    ) -> Result<airk_core::Artifact> {
        let mut artifact = self.load_retained(compact)?;
        artifact.retained = compact.retained.clone();
        Ok(artifact)
    }
    pub fn retain_current_metadata_encoded_owned(
        &self,
        writer: &crate::WriterLock,
        path: &str,
        file: &InventoryFile,
        reservation: &Reservation,
        mode: airk_core::compression::CompressionMode,
    ) -> Result<FileMetadataRef> {
        super::gate::check_writer(self, writer)?;
        writer.invalidate_census_before_mutation()?;
        self.retain_current_metadata_encoded(path, file, reservation, mode)
    }

    pub fn retain_current_metadata(
        &self,
        path: &str,
        file: &InventoryFile,
        reservation: &Reservation,
    ) -> Result<FileMetadataRef> {
        self.retain_current_metadata_encoded(
            path,
            file,
            reservation,
            airk_core::compression::CompressionMode::Off,
        )
    }
    pub fn retain_current_metadata_encoded(
        &self,
        path: &str,
        file: &InventoryFile,
        reservation: &Reservation,
        mode: airk_core::compression::CompressionMode,
    ) -> Result<FileMetadataRef> {
        ensure!(
            super::journal::valid_current_path(path),
            "invalid current metadata path"
        );
        let bytes = serde_json::to_vec(file)?;
        ensure!(
            bytes.len() <= MAX_METADATA_BYTES,
            "current metadata byte limit exceeded"
        );
        let digest = framed_digest(b"current-file-metadata/1", &[&bytes]);
        let encoded = crate::compression::encode(&bytes, mode)?;
        let reference = FileMetadataRef {
            encoding: encoded.as_ref().map(|(_, encoding)| encoding.clone()),
            path: path.into(),
            digest,
            bytes: bytes.len(),
            scalars: Some(FileScalars::from_file(file)),
            native_facts: None,
            native_edges: None,
            native_edges_empty: false,
            native_lines: None,
            text_ranges: None,
            native_facts_omission: None,
            line_witness: None,
        };
        let destination = object(
            self,
            reference
                .encoding
                .as_ref()
                .map(|e| e.stored_digest.parse())
                .transpose()?
                .unwrap_or(digest),
        );
        let directory = destination.parent().context("metadata parent missing")?;
        if !directory.try_exists()? {
            std::fs::create_dir(directory)?;
        }
        ensure!(
            std::fs::symlink_metadata(directory)?.is_dir()
                && !std::fs::symlink_metadata(directory)?
                    .file_type()
                    .is_symlink(),
            "linked current metadata directory"
        );
        if !destination.try_exists()? {
            reservation.charge(bytes.len() as u64)?;
            std::fs::create_dir_all(destination.parent().context("metadata parent missing")?)?;
            // A killed writer leaves only reservation-owned disposable state.
            let mut temporary = tempfile::NamedTempFile::new_in(reservation.staging_directory()?)?;
            temporary.write_all(
                encoded
                    .as_ref()
                    .map_or(bytes.as_slice(), |(encoded, _)| encoded.as_slice()),
            )?;
            crate::durability::persist_synced(temporary, &destination)?;
        }
        self.read_current_metadata(&reference)?;
        Ok(reference)
    }

    pub fn prepare_current_metadata_input(
        &self,
        artifact: &airk_core::Artifact,
    ) -> Result<PreparedMetadataInput> {
        let mut compact = artifact.clone();
        compact.facts.clear();
        compact.chunks.clear();
        let mut retrieval = self.load_retrieval_artifact(&compact)?;
        retrieval.artifact.retained = compact.retained.clone();
        let lines = if retrieval.profile.contract_version == 2
            && retrieval.artifact.extraction_method == "utf8"
        {
            self.verify_retained_source_utf8_lines(&compact)?
        } else {
            retrieval
                .artifact
                .chunks
                .iter()
                .map(|chunk| chunk.end_line)
                .max()
                .unwrap_or(0)
        };
        let witness = std::sync::Arc::new(IndexedLinesWitness {
            source: compact.content_hash.parse()?,
            payload: retrieval.payload_digest,
            profile: retrieval.profile.identity()?,
            lines,
        });
        Ok(PreparedMetadataInput { retrieval, witness })
    }
    /// Writer-owned F13 projection production preserves F10 invalidation before all writes.
    pub fn retain_current_metadata_prepared_owned(
        &self,
        writer: &crate::WriterLock,
        path: &str,
        file: &InventoryFile,
        prepared: &PreparedMetadataInput,
        reservation: &Reservation,
        mode: airk_core::compression::CompressionMode,
    ) -> Result<FileMetadataRef> {
        super::gate::check_writer(self, writer)?;
        writer.invalidate_census_before_mutation()?;
        self.retain_current_metadata_prepared(path, file, prepared, reservation, mode)
    }
    pub fn retain_current_metadata_scalars_prepared_owned(
        &self,
        writer: &crate::WriterLock,
        path: &str,
        file: &InventoryFile,
        prepared: &PreparedMetadataInput,
        reservation: &Reservation,
        mode: airk_core::compression::CompressionMode,
    ) -> Result<FileMetadataRef> {
        super::gate::check_writer(self, writer)?;
        ensure!(
            prepared.artifact().path == path
                && prepared.witness.matches(prepared.artifact())?
                && file.indexed_lines == prepared.indexed_lines(),
            "prepared scalar/source binding mismatch"
        );
        writer.invalidate_census_before_mutation()?;
        let mut reference = self.retain_current_metadata_encoded(path, file, reservation, mode)?;
        reference.line_witness = Some(prepared.witness.clone());
        Ok(reference)
    }
    pub fn retain_current_metadata_prepared(
        &self,
        path: &str,
        file: &InventoryFile,
        prepared: &PreparedMetadataInput,
        reservation: &Reservation,
        mode: airk_core::compression::CompressionMode,
    ) -> Result<FileMetadataRef> {
        ensure!(
            prepared.artifact().path == path
                && prepared.witness.matches(prepared.artifact())?
                && file.indexed_lines == prepared.indexed_lines(),
            "prepared scalar/source binding mismatch"
        );
        let mut reference = self.retain_current_metadata_encoded(path, file, reservation, mode)?;
        reference.native_facts = Some(self.retain_native_fact_projection_bound(
            prepared.artifact(),
            file,
            reference.digest,
            reservation,
            mode,
            None,
        )?);
        let edges =
            self.prepare_native_edge_projection(prepared.artifact(), reference.digest, mode)?;
        if edges
            .root
            .projection
            .as_ref()
            .context("native edge summary missing")?
            .edges
            == 0
        {
            reference.native_edges_empty = true;
        } else {
            for resource in &edges.resources {
                self.retain_prepared_resource(resource, Some(reservation), None)?;
            }
            reference.native_edges = Some(edges.root);
        }
        if let Some(lines) = self.prepare_native_line_projection(prepared.artifact(), mode)? {
            self.retain_prepared_resource(&lines, Some(reservation), None)?;
            reference.native_lines = Some(lines.reference);
        }
        if let Some(text) = self.prepare_text_ranges(prepared.artifact(), mode)? {
            for resource in &text.resources {
                self.retain_prepared_resource(resource, Some(reservation), None)?;
            }
            reference.text_ranges = Some(text.reference);
        }
        reference.line_witness = Some(prepared.witness.clone());
        Ok(reference)
    }
    pub fn retain_current_metadata_with_facts_encoded(
        &self,
        path: &str,
        file: &InventoryFile,
        artifact: &airk_core::Artifact,
        reservation: &Reservation,
        mode: airk_core::compression::CompressionMode,
    ) -> Result<FileMetadataRef> {
        let prepared = self.prepare_current_metadata_input(artifact)?;
        self.retain_current_metadata_prepared(path, file, &prepared, reservation, mode)
    }

    pub fn read_current_metadata(&self, reference: &FileMetadataRef) -> Result<InventoryFile> {
        reference.validate_projection_declaration()?;
        ensure!(
            reference.bytes <= MAX_METADATA_BYTES
                && super::journal::valid_current_path(&reference.path),
            "invalid current metadata reference"
        );
        let path = object(
            self,
            reference
                .encoding
                .as_ref()
                .map(|e| e.stored_digest.parse())
                .transpose()?
                .unwrap_or(reference.digest),
        );
        let directory = path.parent().context("metadata directory missing")?;
        ensure!(
            std::fs::symlink_metadata(directory)?.is_dir()
                && !std::fs::symlink_metadata(directory)?
                    .file_type()
                    .is_symlink(),
            "linked current metadata directory"
        );
        let metadata = std::fs::symlink_metadata(&path)?;
        ensure!(
            metadata.is_file()
                && !metadata.file_type().is_symlink()
                && metadata.len()
                    == reference
                        .encoding
                        .as_ref()
                        .map_or(reference.bytes as u64, |e| e.stored_bytes),
            "current metadata size or ownership mismatch"
        );
        let mut bytes = Vec::new();
        if let Some(encoding) = &reference.encoding {
            bytes = crate::compression::read_all(
                &path,
                encoding,
                &encoding.logical_digest,
                MAX_METADATA_BYTES as u64,
            )?;
        } else {
            std::fs::File::open(path)?
                .take(reference.bytes as u64 + 1)
                .read_to_end(&mut bytes)?;
        }
        ensure!(
            bytes.len() == reference.bytes
                && framed_digest(b"current-file-metadata/1", &[&bytes]) == reference.digest,
            "current metadata digest mismatch"
        );
        let value: InventoryFile = serde_json::from_slice(&bytes)?;
        ensure!(
            serde_json::to_vec(&value)? == bytes,
            "noncanonical current metadata"
        );
        ensure!(
            reference
                .scalars
                .as_ref()
                .is_none_or(|scalars| *scalars == FileScalars::from_file(&value)),
            "current metadata scalar projection mismatch"
        );
        Ok(value)
    }
}

impl CurrentRead {
    pub fn file_metadata_bytes(&self, path: &str) -> Result<usize> {
        let Some(bytes) = self.map_get(MapDomain::FileMetadata, path.as_bytes())? else {
            return Ok(0);
        };
        let reference: FileMetadataRef = serde_json::from_slice(&bytes)?;
        reference.validate_projection_declaration()?;
        ensure!(
            reference.path == path
                && reference.bytes <= MAX_METADATA_BYTES
                && serde_json::to_vec(&reference)? == bytes,
            "invalid current metadata reference"
        );
        Ok(reference.bytes)
    }
    /// Selected authenticated scalar bytes only. This does not authorize relaxing
    /// private-root/physical integrity checks or certify legacy absent projections.
    pub fn file_scalars(&self, path: &str) -> Result<Option<FileScalars>> {
        if self.query_projection_version()?.is_none() {
            return Ok(None);
        }
        let Some(bytes) = self.map_get(MapDomain::FileMetadata, path.as_bytes())? else {
            return Ok(None);
        };
        let reference: FileMetadataRef = serde_json::from_slice(&bytes)?;
        reference.validate_projection_declaration()?;
        ensure!(
            reference.path == path
                && reference.bytes <= MAX_METADATA_BYTES
                && serde_json::to_vec(&reference)? == bytes,
            "invalid current scalar metadata reference"
        );
        let Some(scalars) = reference.scalars else {
            return Ok(None);
        };
        let artifact = self
            .artifact(path)?
            .context("current scalar artifact missing")?;
        scalars.verify(&artifact, self.head().sequence)?;
        Ok(Some(scalars))
    }
    pub(super) fn audit_scalars_from_verified(
        &self,
        path: &str,
        artifact: &airk_core::Artifact,
        indexed_lines: Option<usize>,
    ) -> Result<bool> {
        let Some(scalars) = self.file_scalars(path)? else {
            return Ok(false);
        };
        scalars.verify(artifact, self.head().sequence)?;
        ensure!(
            Some(scalars.indexed_lines) == indexed_lines,
            "scalar producer indexed line count mismatch"
        );
        Ok(true)
    }
    /// Strict audit compares the publication-derived scalars with the original
    /// retained inventory. Publication prepare uses the same full reader/check.
    pub fn audit_file_scalars(&self, store: &FileStore, path: &str) -> Result<bool> {
        let scalars = self.file_scalars(path)?;
        let _ = self.file_metadata(store, path)?;
        if let Some(scalars) = &scalars {
            let compact = self
                .artifact(path)?
                .context("scalar audit artifact missing")?;
            verify_indexed_lines(store, &store.load_projection_artifact(&compact)?, scalars)?;
        }
        if let Some(reference) = self.native_line_reference(path)? {
            let artifact = self
                .artifact(path)?
                .context("line audit artifact missing")?;
            store.verify_native_line_projection(&reference, &artifact)?;
        }
        Ok(scalars.is_some())
    }
    pub(super) fn native_fact_reference(
        &self,
        path: &str,
    ) -> Result<Option<(super::resources::ResourceReference, Sha256Digest)>> {
        if self.query_projection_version()?.is_none() {
            return Ok(None);
        }
        let Some(bytes) = self.map_get(MapDomain::FileMetadata, path.as_bytes())? else {
            return Ok(None);
        };
        let reference: FileMetadataRef = serde_json::from_slice(&bytes)?;
        reference.validate_projection_declaration()?;
        ensure!(
            reference.path == path
                && reference.bytes <= MAX_METADATA_BYTES
                && serde_json::to_vec(&reference)? == bytes,
            "native projection metadata reference mismatch"
        );
        if let Some(native) = &reference.native_facts {
            ensure!(
                native.bytes <= super::native_projection::MAX_MANIFEST_BYTES,
                "native projection manifest reference exceeds bound"
            );
        }
        Ok(reference
            .native_facts
            .map(|native| (native, reference.digest)))
    }
    pub fn artifact_path(
        &self,
        store: &FileStore,
        id: airk_core::identity::ArtifactId,
    ) -> Result<Option<String>> {
        let Some(bytes) = self.map_get(MapDomain::ArtifactHandle, id.as_bytes())? else {
            return Ok(None);
        };
        let path = String::from_utf8(bytes)?;
        let file = self
            .file_metadata(store, &path)?
            .context("artifact handle owner missing")?;
        ensure!(
            file.state.artifact_id == id,
            "artifact handle binding mismatch"
        );
        Ok(Some(path))
    }

    pub fn unit_owner(
        &self,
        store: &FileStore,
        key: airk_core::continuity::ContinuityKey,
    ) -> Result<Option<(String, InventoryFile)>> {
        let Some(bytes) = self.map_get(MapDomain::UnitHandle, &serde_json::to_vec(&key)?)? else {
            return Ok(None);
        };
        let path = String::from_utf8(bytes)?;
        let file = self
            .file_metadata(store, &path)?
            .context("unit handle owner missing")?;
        ensure!(
            file.continuity.iter().filter(|r| r.key == key).count() == 1,
            "unit handle binding mismatch"
        );
        Ok(Some((path, file)))
    }

    pub fn file_metadata(&self, store: &FileStore, path: &str) -> Result<Option<InventoryFile>> {
        let Some(bytes) = self.map_get(MapDomain::FileMetadata, path.as_bytes())? else {
            return Ok(None);
        };
        let reference: FileMetadataRef = serde_json::from_slice(&bytes)?;
        reference.validate_projection_declaration()?;
        ensure!(
            reference.path == path && serde_json::to_vec(&reference)? == bytes,
            "current metadata path mismatch"
        );
        let file = store.read_current_metadata(&reference)?;
        let artifact = self
            .artifact(path)?
            .context("current metadata artifact missing")?;
        verify(&artifact, &file, self.head().sequence)?;
        Ok(Some(file))
    }
}

pub(crate) fn verify(
    artifact: &airk_core::Artifact,
    file: &InventoryFile,
    sequence: u64,
) -> Result<()> {
    file.state.validate(sequence)?;
    let retained = artifact
        .retained
        .as_ref()
        .context("current metadata extraction missing")?;
    let profile_id = if let Some(origin) = &retained.legacy_origin {
        origin.identity()?
    } else {
        retained
            .profile
            .as_ref()
            .context("current metadata profile missing")?
            .identity()?
    };
    ensure!(
        file.state.source_hash.to_string() == artifact.content_hash
            && file.state.payload_digest.to_string() == retained.object
            && file.state.profile_id == profile_id
            && file.source_bytes == artifact.bytes
            && file.chunks == retained.chunks
            && file.facts == retained.facts,
        "current metadata differs from captured artifact"
    );
    Ok(())
}

pub(super) fn verify_projected_metadata(
    store: &FileStore,
    reference: &FileMetadataRef,
    file: &InventoryFile,
    artifact: &airk_core::Artifact,
    indexed_lines: usize,
    sequence: u64,
) -> Result<()> {
    reference.validate_projection_declaration()?;
    if let Some(scalars) = &reference.scalars {
        scalars.verify(artifact, sequence)?;
        ensure!(
            scalars.indexed_lines == indexed_lines,
            "replayed scalar line count mismatch"
        );
    }
    if let Some(native) = &reference.native_facts {
        super::native_projection::verify_projection(
            store,
            native,
            artifact,
            file,
            reference.digest,
        )?;
    }
    if let Some(edges) = &reference.native_edges {
        store.verify_native_edges(edges, artifact, reference.digest)?;
    }
    if reference.native_edges_empty {
        store.verify_empty_native_edges(artifact)?;
    }
    if let Some(lines) = &reference.native_lines {
        store.verify_native_line_projection(lines, artifact)?;
    }
    if let Some(text) = &reference.text_ranges {
        store.verify_text_ranges(text, artifact)?;
    }
    Ok(())
}

pub(crate) fn prepare(
    store: &FileStore,
    scope: &CurrentScope,
    plan: &mut map::MapPlan<'_>,
    artifacts: &[super::ArtifactChange],
    references: &[FileMetadataRef],
    sequence: u64,
) -> Result<Vec<map::MapChange>> {
    let mut changes = Vec::new();
    let mut refs = references.iter().peekable();
    let mut metadata_bytes = 0usize;
    for change in artifacts {
        let path = change.path();
        let key = map::key(scope, MapDomain::FileMetadata, path.as_bytes())?;
        if let Some(old) = plan.get(key)? {
            let old: FileMetadataRef = serde_json::from_slice(&old)?;
            ensure!(old.path == path, "prior current metadata path mismatch");
            metadata_bytes = metadata_bytes
                .checked_add(old.bytes)
                .context("metadata admission overflow")?;
            ensure!(
                metadata_bytes <= super::MAX_CURRENT_METADATA_READ_BYTES,
                "current metadata admission exceeded"
            );
            let file = store.read_current_metadata(&old)?;
            for (domain, identity) in handles(&file)? {
                let c = map::MapChange::new(scope, domain, &identity, None)?;
                plan.put(c.key(), c.value())?;
                changes.push(c);
            }
        }
    }
    for change in artifacts {
        let path = change.path();
        let materialized = match change {
            super::ArtifactChange::Upsert(input) => {
                Some(store.load_projection_artifact(&input.artifact)?)
            }
            super::ArtifactChange::Delete(_) => None,
        };
        let measurement = materialized
            .as_ref()
            .map(|artifact| {
                Ok::<_, anyhow::Error>(serde_json::to_vec(
                    &airk_core::usage::SourceContextSize::measure(artifact)?,
                )?)
            })
            .transpose()?;
        let measured = map::MapChange::new(
            scope,
            MapDomain::SourceMeasurement,
            path.as_bytes(),
            measurement,
        )?;
        plan.put(measured.key(), measured.value())?;
        changes.push(measured);
        let reference = if refs.peek().is_some_and(|r| r.path == path) {
            refs.next()
        } else {
            None
        };
        if let Some(reference) = reference {
            reference.validate_projection_declaration()?;
            metadata_bytes = metadata_bytes
                .checked_add(reference.bytes)
                .context("metadata admission overflow")?;
            ensure!(
                metadata_bytes <= 2 * super::MAX_CURRENT_METADATA_READ_BYTES,
                "current metadata admission exceeded"
            );
            let super::ArtifactChange::Upsert(input) = change else {
                anyhow::bail!("metadata attached to deleted artifact");
            };
            let file = store.read_current_metadata(reference)?;
            verify(&input.artifact, &file, sequence)?;
            if let Some(scalars) = &reference.scalars {
                scalars.verify(&input.artifact, sequence)?;
                if let Some(witness) = &reference.line_witness {
                    ensure!(
                        witness.matches(&input.artifact)? && witness.lines == scalars.indexed_lines,
                        "scalar content witness mismatch"
                    );
                } else {
                    verify_indexed_lines(
                        store,
                        materialized
                            .as_ref()
                            .context("scalar producer materialization missing")?,
                        scalars,
                    )?;
                }
            }
            if let Some(native) = &reference.native_facts {
                super::native_projection::verify_projection(
                    store,
                    native,
                    materialized
                        .as_ref()
                        .context("native producer materialization missing")?,
                    &file,
                    reference.digest,
                )?;
            }
            if let Some(edges) = &reference.native_edges {
                store.verify_native_edges(
                    edges,
                    materialized
                        .as_ref()
                        .context("native edge materialization missing")?,
                    reference.digest,
                )?;
            }
            if reference.native_edges_empty {
                store.verify_empty_native_edges(
                    materialized
                        .as_ref()
                        .context("native edge materialization missing")?,
                )?;
            }
            if let Some(lines) = &reference.native_lines {
                store.verify_native_line_projection(lines, &input.artifact)?;
            }
            if let Some(text) = &reference.text_ranges {
                store.verify_text_ranges(
                    text,
                    materialized
                        .as_ref()
                        .context("text range materialization missing")?,
                )?;
            }
            for (domain, identity) in handles(&file)? {
                let c =
                    map::MapChange::new(scope, domain, &identity, Some(path.as_bytes().to_vec()))?;
                plan.put(c.key(), c.value())?;
                changes.push(c);
            }
        }
        let c = map::MapChange::new(
            scope,
            MapDomain::FileMetadata,
            path.as_bytes(),
            reference.map(serde_json::to_vec).transpose()?,
        )?;
        plan.put(c.key(), c.value())?;
        changes.push(c);
    }
    ensure!(
        refs.next().is_none(),
        "unsorted or foreign current metadata references"
    );
    // Replacements remove and then insert handles. Capture only their final afterimage.
    let mut unique = std::collections::BTreeMap::new();
    for change in changes {
        unique.insert(change.key(), change);
    }
    Ok(unique.into_values().collect())
}

fn handles(file: &InventoryFile) -> Result<Vec<(MapDomain, Vec<u8>)>> {
    let mut keys = vec![(
        MapDomain::ArtifactHandle,
        file.state.artifact_id.as_bytes().to_vec(),
    )];
    for record in &file.continuity {
        keys.push((MapDomain::UnitHandle, serde_json::to_vec(&record.key)?));
    }
    Ok(keys)
}
