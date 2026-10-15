// SPDX-License-Identifier: Apache-2.0
//! Bounded source observations. Reuse is computation reuse, never identity proof.
use airk_core::{Artifact, extraction::ExtractionProfile, identity::Sha256Digest};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::Path};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DisappearanceCause {
    DeletedObserved,
    AliasRemoved,
    AliasRebound,
    ExcludedFromDiscovery,
    Oversized,
    Unsupported,
    ExtractionFailed,
    NonRegular,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReuseBasis {
    SamePath,
    PriorContent,
}

pub struct ReuseRequest<'a> {
    pub path: &'a str,
    pub canonical_root: &'a str,
    pub source_hash: &'a Sha256Digest,
    pub source_bytes: usize,
    pub profile: &'a ExtractionProfile,
}

pub struct VerifiedReuse {
    pub artifact: Artifact,
    pub basis: ReuseBasis,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ScanUsage {
    pub discovery_entries: usize,
    pub metadata_probes: usize,
    pub reconciled_paths: usize,
    pub source_read_bytes: usize,
    pub parser_calls: usize,
    pub same_path_hits: usize,
    pub prior_content_hits: usize,
}

impl ScanUsage {
    pub(crate) fn charged_entries(&self) -> usize {
        self.discovery_entries
            .saturating_add(self.metadata_probes)
            .saturating_add(self.reconciled_paths)
    }

    pub(crate) fn probe(&mut self, limit: usize) -> Result<()> {
        ensure!(
            self.charged_entries() < limit,
            "workspace reconciliation/discovery budget exhausted; previous generation retained"
        );
        self.metadata_probes += 1;
        Ok(())
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ScanObservation {
    /// True only after traversal, reconciliation and final root checks succeed.
    pub complete: bool,
    /// Empty alias denotes the owner root. Values are canonical root bindings.
    pub roots: BTreeMap<String, String>,
    /// Previous indexed bindings only; independent of truncated human diagnostics.
    /// A rebound alias can have a live replacement at the same indexed path.
    pub disappeared: BTreeMap<String, DisappearanceCause>,
    pub usage: ScanUsage,
}

pub(crate) fn verify_root(root: &Path, usage: &mut ScanUsage, limit: usize) -> Result<()> {
    usage.probe(limit)?;
    let metadata = root.symlink_metadata()?;
    ensure!(
        metadata.is_dir() && !metadata.file_type().is_symlink(),
        "source root is no longer a regular directory"
    );
    ensure!(
        root.canonicalize()? == root,
        "source root binding changed during scan"
    );
    Ok(())
}

/// Classify a path omitted by complete discovery, checking each ancestor without
/// traversing a link. Permission/other I/O errors abort instead of proving deletion.
pub(crate) fn reconcile_missing(
    root: &Path,
    relative: &str,
    usage: &mut ScanUsage,
    limit: usize,
) -> Result<DisappearanceCause> {
    crate::adapters::profile::validate_relative_path(relative)?;
    let mut current = root.to_path_buf();
    let parts: Vec<_> = relative.split('/').collect();
    for (index, part) in parts.iter().enumerate() {
        current.push(part);
        usage.probe(limit)?;
        let metadata = match current.symlink_metadata() {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(DisappearanceCause::DeletedObserved);
            }
            Err(error) => return Err(error.into()),
        };
        if metadata.file_type().is_symlink() {
            return Ok(DisappearanceCause::NonRegular);
        }
        // Metadata lookup can resolve a different spelling on case-insensitive
        // filesystems. Citation paths describe observed directory-entry names,
        // so require the original component itself to remain present. Charge
        // every examined name; exhaustion cannot become evidence of deletion.
        let mut exact_spelling = false;
        for entry in current
            .parent()
            .expect("relative component has parent")
            .read_dir()?
        {
            ensure!(
                usage.charged_entries() < limit,
                "exact path reconciliation discovery budget exhausted"
            );
            usage.discovery_entries += 1;
            if entry?.file_name() == std::ffi::OsStr::new(part) {
                exact_spelling = true;
                break;
            }
        }
        if !exact_spelling {
            return Ok(DisappearanceCause::DeletedObserved);
        }
        if index + 1 < parts.len() {
            if !metadata.is_dir() {
                return Ok(DisappearanceCause::NonRegular);
            }
        } else if !metadata.is_file() {
            return Ok(DisappearanceCause::NonRegular);
        }
    }
    Ok(DisappearanceCause::ExcludedFromDiscovery)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_observation_uses_exact_directory_spelling_not_casefold_lookup() -> Result<()> {
        let root = tempfile::tempdir()?;
        std::fs::create_dir(root.path().join("Folder"))?;
        std::fs::write(root.path().join("Folder/Beacon.rs"), "original source")?;
        assert_eq!(
            reconcile_missing(
                root.path(),
                "Folder/Beacon.rs",
                &mut ScanUsage::default(),
                100
            )?,
            DisappearanceCause::ExcludedFromDiscovery
        );
        std::fs::rename(
            root.path().join("Folder/Beacon.rs"),
            root.path().join("Folder/beacon.rs"),
        )?;
        assert_eq!(
            reconcile_missing(
                root.path(),
                "Folder/Beacon.rs",
                &mut ScanUsage::default(),
                100
            )?,
            DisappearanceCause::DeletedObserved
        );
        assert_eq!(
            reconcile_missing(
                root.path(),
                "Folder/beacon.rs",
                &mut ScanUsage::default(),
                100
            )?,
            DisappearanceCause::ExcludedFromDiscovery
        );
        std::fs::rename(root.path().join("Folder"), root.path().join("folder"))?;
        assert_eq!(
            reconcile_missing(
                root.path(),
                "Folder/beacon.rs",
                &mut ScanUsage::default(),
                100
            )?,
            DisappearanceCause::DeletedObserved
        );
        assert_eq!(
            reconcile_missing(
                root.path(),
                "folder/beacon.rs",
                &mut ScanUsage::default(),
                100
            )?,
            DisappearanceCause::ExcludedFromDiscovery
        );
        assert!(
            reconcile_missing(
                root.path(),
                "folder/beacon.rs",
                &mut ScanUsage::default(),
                1
            )
            .is_err()
        );
        Ok(())
    }
}
