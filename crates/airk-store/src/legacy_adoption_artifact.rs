// SPDX-License-Identifier: Apache-2.0
//! Exact historical payload bytes to inert saved evidence; never semantic inputs.
use super::{PinnedRoot, SavedLegacyArtifact};
use crate::FileStore;
use airk_core::Artifact;
use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use std::io::Read;
#[path = "legacy_adoption_binding.rs"]
mod binding;
#[path = "legacy_adoption_mapped_binding.rs"]
mod mapped_binding;
#[path = "legacy_adoption_payload.rs"]
mod payload;

pub(super) fn load(
    store: &FileStore,
    pins: &PinnedRoot,
    entry: &Artifact,
    maximum: u64,
) -> Result<SavedLegacyArtifact> {
    let reference = entry
        .retained
        .as_ref()
        .context("historical retained reference missing")?;
    let profile = reference
        .profile
        .as_ref()
        .context("historical path-free profile missing")?;
    ensure!(
        reference.legacy_origin.is_none()
            && reference.payload_encoding.is_none()
            && reference.source_encoding.is_none(),
        "unexpected historical payload representation"
    );
    ensure!(
        crate::blobs::valid_digest(&reference.object),
        "invalid historical payload digest"
    );
    ensure!(
        matches!(profile.contract_version, 1 | 2),
        "unsupported historical extraction profile"
    );
    profile.identity()?;
    let payload_maximum = maximum.min(airk_core::extraction::MAX_PAYLOAD_BYTES as u64);
    let file = pins.file(&format!("objects/artifacts/{}", reference.object))?;
    ensure!(
        file.metadata()?.len() <= payload_maximum,
        "historical payload exceeds request bound"
    );
    let mut bytes = Vec::new();
    file.take(payload_maximum.saturating_add(1))
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= payload_maximum
            && format!("{:x}", Sha256::digest(&bytes)) == reference.object,
        "historical payload digest or bound mismatch"
    );
    ensure!(
        bytes.len() == reference.object_bytes,
        "historical payload length differs from pinned inventory"
    );
    let artifact = decode(&bytes, entry)?;
    let raw_source = if reference.raw_source {
        Some(store.read_retained_source_bounded(entry, maximum)?)
    } else {
        None
    };
    pins.revalidate()?;
    Ok(SavedLegacyArtifact {
        artifact,
        raw_source,
        profile: Some(profile.clone()),
        payload: None,
    })
}
fn decode(bytes: &[u8], entry: &Artifact) -> Result<Artifact> {
    let reference = entry
        .retained
        .as_ref()
        .context("historical retained reference missing")?;
    let profile = reference
        .profile
        .as_ref()
        .context("historical profile missing")?;
    let payload = payload::LegacyPayload::from_bytes(bytes)?;
    let artifact = binding::bind_payload(
        &payload,
        profile,
        binding::ArtifactBinding {
            path: &entry.path,
            source_hash: entry.content_hash.parse()?,
            source_bytes: entry.bytes,
        },
    )?;
    ensure!(
        artifact.retained.is_none()
            && artifact.path == entry.path
            && artifact.content_hash == entry.content_hash
            && artifact.bytes == entry.bytes
            && artifact.language == entry.language
            && artifact.extraction_method == entry.extraction_method
            && artifact.parse_status == entry.parse_status
            && artifact.chunks.len() == reference.chunks
            && artifact.extracted_bytes() == reference.extracted_bytes
            && artifact.facts.len() == reference.facts,
        "historical payload differs from pinned artifact inventory"
    );
    Ok(artifact)
}
#[cfg(test)]
#[path = "legacy_adoption_artifact_tests.rs"]
mod tests;
