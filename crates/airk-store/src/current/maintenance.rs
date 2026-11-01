// SPDX-License-Identifier: Apache-2.0
//! Complete ownership proof before bounded current-object reclamation.
use super::{
    CurrentRead, CurrentToken, CurrentWriter, TransactionPhase, map::MapDomain,
    metadata::FileMetadataRef, postings::Lookup,
};
use crate::FileStore;
use airk_core::identity::Sha256Digest;
use anyhow::{Context, Result, ensure};
use std::{collections::BTreeSet, path::PathBuf};

const DELETE_BATCH: usize = 4096;

pub struct ReclamationPlan {
    token: CurrentToken,
    collection: PathBuf,
    retained: BTreeSet<PathBuf>,
    projection_manifest_references: u64,
    projection_leaf_references: u64,
    projection_encoded_bytes: u64,
}
impl ReclamationPlan {
    /// Logical reference/object-byte counts, not physical I/O or unique allocations.
    pub fn projection_counts(&self) -> (u64, u64, u64) {
        (
            self.projection_manifest_references,
            self.projection_leaf_references,
            self.projection_encoded_bytes,
        )
    }
}

#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct ReclamationReport {
    pub removed_files: usize,
    pub removed_bytes: u64,
    pub pending: bool,
}

impl CurrentRead {
    pub fn reclamation_plan(&self, store: &FileStore) -> Result<ReclamationPlan> {
        let state = self
            .collection_state()?
            .context("current reclamation requires collection metadata")?;
        let page = self.lookup_paths(Lookup::Files, "", None, 100_000)?;
        ensure!(
            page.next_after.is_none() && page.paths.len() as u64 == state.counts.files,
            "current reclamation inventory exceeds complete proof budget"
        );
        let mut retained = BTreeSet::new();
        let mut metadata_bytes = 0usize;
        let mut projection_manifest_references = 0u64;
        let mut projection_leaf_references = 0u64;
        let mut projection_encoded_bytes = 0u64;
        for path in page.paths {
            let artifact = self
                .artifact(&path)?
                .context("current reclamation artifact missing")?;
            let reference = artifact
                .retained
                .as_ref()
                .context("current reclamation extraction missing")?;
            reference.object.parse::<Sha256Digest>()?;
            artifact.content_hash.parse::<Sha256Digest>()?;
            let payload = reference
                .payload_encoding
                .as_ref()
                .map_or(reference.object.as_str(), |r| r.stored_digest.as_str());
            payload.parse::<Sha256Digest>()?;
            retained.insert(PathBuf::from("objects/artifacts").join(payload));
            if reference.raw_source {
                let source = reference
                    .source_encoding
                    .as_ref()
                    .map_or(artifact.content_hash.as_str(), |r| r.stored_digest.as_str());
                source.parse::<Sha256Digest>()?;
                retained.insert(PathBuf::from("objects/sources").join(source));
            }
            if let Some(bytes) = self.map_get(MapDomain::FileMetadata, path.as_bytes())? {
                let reference: FileMetadataRef = serde_json::from_slice(&bytes)?;
                ensure!(
                    reference.path == path,
                    "current reclamation metadata binding mismatch"
                );
                metadata_bytes = metadata_bytes
                    .checked_add(reference.bytes)
                    .context("current reclamation metadata overflow")?;
                ensure!(
                    metadata_bytes <= super::MAX_CURRENT_METADATA_READ_BYTES,
                    "current reclamation metadata budget exceeded"
                );
                let file = store.read_current_metadata(&reference)?;
                super::metadata::verify(&artifact, &file, self.head().sequence)?;
                if let Some(native) = &reference.native_facts {
                    metadata_bytes = metadata_bytes
                        .checked_add(native.bytes)
                        .context("projection retention metadata overflow")?;
                    ensure!(
                        metadata_bytes <= super::MAX_CURRENT_METADATA_READ_BYTES,
                        "current reclamation metadata budget exceeded"
                    );
                    let projection = self
                        .native_fact_projection(store, &path)?
                        .context("native retention projection missing")?;
                    let counts = projection.stats();
                    projection.retain_paths(&mut retained)?;
                    projection_manifest_references = projection_manifest_references
                        .checked_add(1)
                        .context("projection retention count overflow")?;
                    projection_leaf_references = projection_leaf_references
                        .checked_add(counts.leaf_objects as u64)
                        .context("projection retention count overflow")?;
                    projection_encoded_bytes = projection_encoded_bytes
                        .checked_add(counts.leaf_encoded_bytes)
                        .and_then(|bytes| bytes.checked_add(counts.manifest_encoded_bytes as u64))
                        .context("projection retention byte overflow")?;
                }

                if let Some(lines) = &reference.native_lines {
                    ensure!(
                        lines.bytes <= super::native_range::MAX_LINE_INDEX_BYTES,
                        "line retention metadata exceeds bound"
                    );
                    retained.insert(
                        PathBuf::from("current-resources")
                            .join(format!("{}.json", lines.object_digest()?)),
                    );
                }
                if let Some(edges) = &reference.native_edges {
                    store.retain_projection_paths(edges, &mut retained)?;
                }
                retained.insert(PathBuf::from("current-metadata").join(format!(
                        "{}.json",
                        reference
                            .encoding
                            .as_ref()
                            .map_or(reference.digest.to_string(), |e| e.stored_digest.clone())
                    )));
            }
        }
        if let Some(model) = state
            .configuration
            .model
            .filter(|model| model.shared.is_none())
        {
            for asset in model.assets.values() {
                retained.insert(PathBuf::from("objects/models").join(asset.sha256.to_string()));
            }
        }
        for key in super::resources::KEYS {
            if let Some(reference) = self.resource_reference(key)? {
                retained.insert(
                    PathBuf::from("current-resources")
                        .join(format!("{}.json", reference.object_digest()?)),
                );
                if key == "analysis-projection" {
                    store.retain_projection_paths(&reference, &mut retained)?;
                }
                if key == "embedding-cache" {
                    let manifest = crate::vector_cache::VectorCacheManifest::from_bytes(
                        &store.read_current_resource(&reference)?,
                    )?;
                    for pack in manifest.packs {
                        retained.insert(
                            PathBuf::from("objects/embedding-packs").join(pack.sha256.to_string()),
                        );
                    }
                }
            }
        }
        Ok(ReclamationPlan {
            token: self.token()?,
            collection: store.directory().canonicalize()?,
            retained,
            projection_manifest_references,
            projection_leaf_references,
            projection_encoded_bytes,
        })
    }
}

impl CurrentWriter<'_> {
    pub fn reclaim_objects(&mut self, plan: &ReclamationPlan) -> Result<ReclamationReport> {
        ensure!(
            self.phase()? == TransactionPhase::Ready
                && self.pending()?.is_none()
                && self.head()?.token()? == plan.token,
            "current reclamation proof expired or publication is pending"
        );
        let collection = self.collection_directory();
        ensure!(
            collection.canonicalize()? == plan.collection,
            "foreign current reclamation plan"
        );
        // A current-head proof does not include the writer's next captured
        // batch or its prior reuse inputs. Defer until that owner releases them.
        if self.has_object_captures() {
            return Ok(ReclamationReport {
                pending: true,
                ..Default::default()
            });
        }
        // A WAL snapshot protects SQL pages, not external retained files. The
        // request's shared OS lease survives commits and disconnect; only actual
        // release/process exit permits this exclusion and subsequent deletion.
        if !self.reclaim_exclusive()? {
            return Ok(ReclamationReport {
                pending: true,
                ..Default::default()
            });
        }
        reclaim_candidates(collection, plan, DELETE_BATCH)
    }
}

impl FileStore {
    /// Reclaim without reserving additional database/WAL capacity. The matching
    /// writer pins the publication, and the exclusive object gate excludes all
    /// readers before any retained file is removed.
    pub fn reclaim_current_objects(
        &self,
        writer: &crate::WriterLock,
        scope: &super::CurrentScope,
    ) -> Result<ReclamationReport> {
        self.reclaim_current_objects_with_limit(writer, scope, DELETE_BATCH)
    }

    pub(crate) fn reclaim_current_objects_with_limit(
        &self,
        writer: &crate::WriterLock,
        scope: &super::CurrentScope,
        limit: usize,
    ) -> Result<ReclamationReport> {
        ensure!(
            limit <= DELETE_BATCH,
            "invalid current reclamation batch limit"
        );
        super::gate::check_writer(self, writer)?;
        if writer.has_object_captures() {
            return Ok(ReclamationReport {
                pending: true,
                ..Default::default()
            });
        }
        let mut read = self.current_build_read(scope, writer)?;
        if read.has_prepared_update()? {
            return Ok(ReclamationReport {
                pending: true,
                ..Default::default()
            });
        }
        if read.collection_state()?.is_none() {
            return Ok(ReclamationReport::default());
        }
        let plan = read.reclamation_plan(self)?;
        // This snapshot is private to the cleanup operation; no escaped clone
        // can continue reading external files after the gate is upgraded.
        if !std::sync::Arc::get_mut(&mut read)
            .context("shared reclamation snapshot")?
            .reclaim_exclusive()?
        {
            return Ok(ReclamationReport {
                pending: true,
                ..Default::default()
            });
        }
        reclaim_candidates(self.directory(), &plan, limit)
    }
}

fn reclaim_candidates(
    collection: &std::path::Path,
    plan: &ReclamationPlan,
    limit: usize,
) -> Result<ReclamationReport> {
    let mut candidates = Vec::new();
    let mut visited = 0usize;
    'directories: for name in [
        "objects/artifacts",
        "objects/sources",
        "objects/models",
        "objects/embedding-packs",
        "current-metadata",
        "current-resources",
    ] {
        let directory = collection.join(name);
        if !directory.try_exists()? {
            continue;
        }
        for ancestor in directory.ancestors().take_while(|p| *p != collection) {
            let metadata = std::fs::symlink_metadata(ancestor)?;
            ensure!(
                metadata.is_dir() && !metadata.file_type().is_symlink(),
                "linked current object directory"
            );
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                ensure!(
                    metadata.file_attributes() & 0x400 == 0,
                    "redirected current object directory"
                );
            }
        }
        for entry in std::fs::read_dir(&directory)? {
            visited += 1;
            ensure!(
                visited <= 1_000_000,
                "current reclamation unrecognized-entry budget exceeded"
            );
            let entry = entry?;
            let metadata = std::fs::symlink_metadata(entry.path())?;
            ensure!(
                metadata.is_file() && !metadata.file_type().is_symlink(),
                "nonregular current object"
            );
            let file = entry.file_name();
            let file = file.to_str().context("non-UTF-8 current object")?;
            let digest = if name.starts_with("current-") {
                file.strip_suffix(".json").unwrap_or("")
            } else {
                file
            };
            if digest.parse::<Sha256Digest>().is_err() {
                continue;
            }
            let relative = PathBuf::from(name).join(file);
            if !plan.retained.contains(&relative) {
                candidates.push((entry.path(), metadata.len()));
                // One extra candidate proves more work remains. Do not
                // require a million-file garbage inventory before the
                // first bounded deletion pass can free any space.
                if candidates.len() > limit {
                    break 'directories;
                }
            }
        }
    }
    // The complete current-head ownership proof precedes every batch.
    // Validate each candidate before removal; unvisited entries stay intact.
    let mut report = ReclamationReport {
        pending: candidates.len() > limit,
        ..Default::default()
    };
    for (path, bytes) in candidates.into_iter().take(limit) {
        ensure!(
            path.canonicalize()?.starts_with(&plan.collection),
            "current reclamation escaped collection"
        );
        crate::managed_collection::reject_file_aliases(&path, &std::fs::symlink_metadata(&path)?)?;
        std::fs::remove_file(path)?;
        report.removed_files += 1;
        report.removed_bytes = report
            .removed_bytes
            .checked_add(bytes)
            .context("current reclamation byte overflow")?;
    }
    Ok(report)
}
