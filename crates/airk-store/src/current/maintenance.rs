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

pub struct ReclamationPlan {
    token: CurrentToken,
    collection: PathBuf,
    retained: BTreeSet<PathBuf>,
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
                    PathBuf::from("current-resources").join(format!("{}.json", reference.digest)),
                );
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
        let mut candidates = Vec::new();
        let mut count = 0;
        for name in [
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
            }
            for entry in std::fs::read_dir(&directory)? {
                count += 1;
                ensure!(
                    count <= 1_000_000,
                    "current reclamation entry budget exceeded"
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
                }
            }
        }
        // No deletion before the complete directory/link and ownership checks.
        let mut report = ReclamationReport {
            pending: candidates.len() > 4096,
            ..Default::default()
        };
        for (path, bytes) in candidates.into_iter().take(4096) {
            ensure!(
                path.canonicalize()?.starts_with(&plan.collection),
                "current reclamation escaped collection"
            );
            std::fs::remove_file(path)?;
            report.removed_files += 1;
            report.removed_bytes = report
                .removed_bytes
                .checked_add(bytes)
                .context("current reclamation byte overflow")?;
        }
        Ok(report)
    }
}
