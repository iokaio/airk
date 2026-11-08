// SPDX-License-Identifier: Apache-2.0
//! Explicit saved predecessor objects. This never certifies a native extractor.
use super::*;
use airk_core::legacy_adoption::LegacySavedOrigin;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SavedPayload {
    schema: u32,
    origin: LegacySavedOrigin,
    artifact: Artifact,
    profile: Option<airk_core::extraction::ExtractionProfile>,
}

impl FileStore {
    pub fn retain_legacy_artifact_owned(
        &self,
        writer: &crate::WriterLock,
        artifact: &Artifact,
        source: Option<&[u8]>,
        profile: Option<&airk_core::extraction::ExtractionProfile>,
        origin: LegacySavedOrigin,
        reservation: &crate::quota::Reservation,
    ) -> Result<Artifact> {
        crate::current::gate::check_writer(self, writer)?;
        origin.validate()?;
        ensure!(
            artifact.retained.is_none()
                && origin.original_path == artifact.path
                && origin.source_available == source.is_some()
                && origin.profile_available == profile.is_some(),
            "saved legacy artifact origin differs"
        );
        artifact
            .content_hash
            .parse::<airk_core::identity::Sha256Digest>()?;
        writer.invalidate_census_before_mutation()?;
        if let Some(bytes) = source {
            ensure!(
                bytes.len() == artifact.bytes
                    && format!("{:x}", Sha256::digest(bytes)) == artifact.content_hash,
                "saved legacy source identity differs"
            );
            put(
                self.directory(),
                "sources",
                bytes,
                &artifact.content_hash,
                Some(reservation),
            )?;
        }
        let payload = SavedPayload {
            schema: 1,
            origin: origin.clone(),
            artifact: artifact.clone(),
            profile: profile.cloned(),
        };
        let bytes = serde_json::to_vec(&payload)?;
        ensure!(
            bytes.len() as u64 <= MAX_ARTIFACT_BYTES,
            "saved legacy artifact exceeds retained budget"
        );
        let object = format!("{:x}", Sha256::digest(&bytes));
        put(
            self.directory(),
            "artifacts",
            &bytes,
            &object,
            Some(reservation),
        )?;
        let mut compact = artifact.clone();
        compact.retained = Some(ArtifactRef {
            legacy_origin: Some(origin),
            source_encoding: None,
            payload_encoding: None,
            object,
            object_bytes: bytes.len(),
            raw_source: source.is_some(),
            chunks: artifact.chunks.len(),
            extracted_bytes: artifact.extracted_bytes(),
            facts: artifact.facts.len(),
            profile: profile.cloned(),
        });
        compact.chunks.clear();
        compact.facts.clear();
        Ok(compact)
    }
    pub fn load_legacy_saved_artifact(&self, entry: &Artifact) -> Result<Artifact> {
        self.load_legacy_saved_artifact_bounded(entry, MAX_ARTIFACT_BYTES)
    }
    pub(super) fn load_legacy_saved_artifact_bounded(
        &self,
        entry: &Artifact,
        maximum: u64,
    ) -> Result<Artifact> {
        let reference = entry
            .retained
            .as_ref()
            .context("saved legacy reference missing")?;
        let origin = reference
            .legacy_origin
            .as_ref()
            .context("saved legacy origin missing")?;
        origin.validate()?;
        ensure!(
            origin.original_path == entry.path
                && origin.source_available == reference.raw_source
                && origin.profile_available == reference.profile.is_some(),
            "saved legacy reference provenance differs"
        );
        let bytes = read_object(
            self.directory(),
            "artifacts",
            &reference.object,
            reference.payload_encoding.as_ref(),
            MAX_ARTIFACT_BYTES.min(maximum),
        )?;
        ensure!(
            bytes.len() == reference.object_bytes,
            "saved legacy object length differs"
        );
        let payload: SavedPayload = serde_json::from_slice(&bytes)?;
        let artifact = payload.artifact;
        ensure!(
            payload.schema == 1
                && payload.origin == *origin
                && payload.profile == reference.profile
                && artifact.retained.is_none()
                && artifact.path == entry.path
                && artifact.content_hash == entry.content_hash
                && artifact.bytes == entry.bytes
                && artifact.language == entry.language
                && artifact.extraction_method == entry.extraction_method
                && artifact.parse_status == entry.parse_status
                && artifact.chunks.len() == reference.chunks
                && artifact.extracted_bytes() == reference.extracted_bytes
                && artifact.facts.len() == reference.facts,
            "saved legacy object differs from authenticated reference"
        );
        Ok(artifact)
    }
}
