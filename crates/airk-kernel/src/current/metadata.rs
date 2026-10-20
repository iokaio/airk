// SPDX-License-Identifier: Apache-2.0
//! Assign current file/unit handles only after authenticating the previous binding.
use airk_core::{
    changes::{FileStamps, FileState},
    identity::{ArtifactId, Sha256Digest, framed_digest},
    publication::InventoryFile,
};
use airk_store::{
    FileStore,
    current::{
        ArtifactChange, CurrentRead, collection::CollectionConfiguration, metadata::FileMetadataRef,
    },
    quota::Reservation,
};
use anyhow::{Context, Result};
use std::collections::BTreeMap;

pub(super) fn prepare(
    store: &FileStore,
    read: &CurrentRead,
    changes: &[ArtifactChange],
    configuration: Option<&CollectionConfiguration>,
    reservation: &Reservation,
    moves: Option<&BTreeMap<String, String>>,
) -> Result<Vec<FileMetadataRef>> {
    let sequence = read
        .head()
        .sequence
        .checked_add(1)
        .context("current sequence overflow")?;
    let roots = configuration.map(|c| c.roots.clone()).unwrap_or_default();
    let mut references = Vec::new();
    let mut prior_bytes = 0usize;
    let mut captured_bytes = 0usize;
    let compression = store.compression_mode(&read.head().scope.root)?;
    for change in changes {
        let ArtifactChange::Upsert(input) = change else {
            continue;
        };
        let artifact = &input.artifact;
        let path = &artifact.path;
        let retained = artifact
            .retained
            .as_ref()
            .context("current retained metadata missing")?;
        let profile = retained
            .profile
            .as_ref()
            .context("current profile missing")?;
        let source = airk_store::roots::source_location(&read.head().scope.root, &roots, path)?;
        let profile_id = profile.identity()?;
        prior_bytes = prior_bytes
            .checked_add(read.file_metadata_bytes(path)?)
            .context("current metadata budget overflow")?;
        if let Some(old) = moves.and_then(|moves| moves.get(path)) {
            prior_bytes = prior_bytes
                .checked_add(read.file_metadata_bytes(old)?)
                .context("current metadata budget overflow")?;
        }
        anyhow::ensure!(
            prior_bytes <= airk_store::current::MAX_CURRENT_METADATA_READ_BYTES,
            "current prior metadata admission exceeded"
        );
        let same = read
            .file_metadata(store, path)?
            .filter(|old| old.state.root == source.root);
        let moved_path = moves.and_then(|moves| moves.get(path));
        let moved = moved_path
            .map(|old| read.file_metadata(store, old))
            .transpose()?
            .flatten();
        let prior = same
            .as_ref()
            .map(|f| (path.as_str(), f))
            .or_else(|| moved.as_ref().zip(moved_path).map(|(f, p)| (p.as_str(), f)));
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
                read.head().scope.incarnation.as_bytes(),
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
        let payload = store.load_extraction_payload(artifact)?;
        let indexed_lines = if profile.contract_version == 2 && artifact.extraction_method == "utf8"
        {
            std::str::from_utf8(&store.read_retained_source(artifact)?)?
                .split_inclusive('\n')
                .count()
        } else {
            store
                .load_retained(artifact)?
                .chunks
                .iter()
                .map(|c| c.end_line)
                .max()
                .unwrap_or(0)
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
            indexed_lines,
            facts: retained.facts,
            raw_source: retained.raw_source,
            language: artifact.language.clone(),
            extraction_method: artifact.extraction_method.clone(),
            parse_status: artifact.parse_status.clone(),
        };
        crate::continuity::assign(prior, path, &mut file, &payload, sequence)?;
        captured_bytes = captured_bytes
            .checked_add(serde_json::to_vec(&file)?.len())
            .context("current metadata budget overflow")?;
        anyhow::ensure!(
            captured_bytes <= airk_store::current::MAX_CURRENT_METADATA_READ_BYTES,
            "current captured metadata admission exceeded"
        );
        references.push(store.retain_current_metadata_encoded(
            path,
            &file,
            reservation,
            compression,
        )?);
    }
    Ok(references)
}
