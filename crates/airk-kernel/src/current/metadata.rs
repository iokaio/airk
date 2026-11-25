// SPDX-License-Identifier: Apache-2.0
//! Assign current file/unit handles from the file's previous stored binding.
use airk_core::{
    changes::{FileStamps, FileState},
    identity::{ArtifactId, Sha256Digest, framed_digest},
    publication::InventoryFile,
};
use airk_store::current::{
    CapturedInput,
    plain_writer::{CapturedPayload, PlainRead},
};
use anyhow::{Context, Result};
use std::collections::BTreeMap;

/// The `InventoryFile` of one changed file: the artifact id is kept across
/// edits (same path, same root) and across a move (`moved_from`); stamps and
/// continuity follow the prior binding. `sequence` is the publication's.
pub(super) fn prepare(
    read: &PlainRead,
    roots: &BTreeMap<String, String>,
    input: &CapturedInput,
    captured: &CapturedPayload,
    moved_from: Option<&str>,
    sequence: u64,
) -> Result<InventoryFile> {
    let artifact = &input.artifact;
    let path = &artifact.path;
    let scope = read.scope();
    let retained = artifact
        .retained
        .as_ref()
        .context("current retained metadata missing")?;
    let source = airk_store::roots::source_location(&scope.root, roots, path)?;
    let profile_id = retained
        .profile
        .as_ref()
        .context("current profile missing")?
        .identity()?;
    let same = {
        let _stage = airk_core::stage_timing::scope("publish.metadata.prior");
        read.inventory(path)?
            .filter(|old| old.state.root == source.root)
    };
    let moved = moved_from
        .map(|old| read.inventory(old))
        .transpose()?
        .flatten();
    // A move target was already published by an earlier batch of the same
    // scan under a fresh identity; the moved-from binding takes precedence.
    let prior = moved
        .as_ref()
        .zip(moved_from)
        .map(|(f, p)| (p, f))
        .or_else(|| same.as_ref().map(|f| (path.as_str(), f)));
    let artifact_id = match prior {
        Some((_, f)) => f.state.artifact_id,
        None => {
            let mut bytes = [0; 16];
            getrandom::fill(&mut bytes)
                .map_err(|e| anyhow::anyhow!("allocate artifact identity: {e}"))?;
            ArtifactId::from_bytes(bytes)
        }
    };
    let source_hash = artifact.content_hash.parse()?;
    let payload_digest: Sha256Digest = retained.object.parse()?;
    let evidence_digest = framed_digest(
        b"current-bound-evidence/1",
        &[
            scope.incarnation.as_bytes(),
            artifact_id.as_bytes(),
            source.root.as_bytes(),
            source.relative_path.as_bytes(),
            payload_digest.as_bytes(),
            profile_id.as_bytes(),
        ],
    );
    let old = prior.map(|(_, f)| &f.state);
    let stamps = FileStamps {
        first_generation: old.map_or(sequence, |s| s.stamps.first_generation),
        path_generation: old
            .filter(|s| s.path == source.relative_path && s.root == source.root)
            .map_or(sequence, |s| s.stamps.path_generation),
        content_generation: old
            .filter(|s| s.source_hash == source_hash)
            .map_or(sequence, |s| s.stamps.content_generation),
        evidence_generation: old
            .filter(|s| s.evidence_digest == evidence_digest)
            .map_or(sequence, |s| s.stamps.evidence_generation),
    };
    let mut file = InventoryFile {
        continuity: vec![],
        continuity_coverage: Default::default(),
        state: FileState {
            artifact_id,
            stamps,
            source_hash,
            profile_id,
            payload_digest,
            evidence_digest,
            root: source.root,
            path: source.relative_path,
        },
        source_bytes: artifact.bytes,
        payload_bytes: retained.object_bytes,
        extracted_bytes: retained.extracted_bytes,
        chunks: retained.chunks,
        indexed_lines: captured.indexed_lines(),
        facts: retained.facts,
        raw_source: retained.raw_source,
        language: artifact.language.clone(),
        extraction_method: artifact.extraction_method.clone(),
        parse_status: artifact.parse_status.clone(),
    };
    {
        let _stage = airk_core::stage_timing::scope("publish.metadata.continuity");
        crate::continuity::assign(prior, path, &mut file, &captured.payload, sequence)?;
    }
    Ok(file)
}
