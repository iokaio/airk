// SPDX-License-Identifier: Apache-2.0
//! Bounded source observations. Reuse is computation reuse, never identity proof.
use airk_core::{Artifact, extraction::ExtractionProfile, identity::Sha256Digest};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::{OsStr, OsString},
    path::{Path, PathBuf},
};

const MAX_EXACT_NAME_CACHE_BYTES: usize = 16 * 1024 * 1024;

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
    /// Ephemeral complete directory-name observations, never persisted/reused
    /// across scans. Names are not metadata or evidence of source contents.
    #[serde(skip)]
    exact_directory_names: BTreeMap<PathBuf, BTreeSet<OsString>>,
    #[serde(skip)]
    exact_directory_name_bytes: usize,
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

    fn contains_exact_name(&mut self, parent: &Path, name: &OsStr, limit: usize) -> Result<bool> {
        if !self.exact_directory_names.contains_key(parent) {
            let mut names = BTreeSet::new();
            let mut bytes = parent.as_os_str().as_encoded_bytes().len();
            for entry in parent.read_dir()? {
                ensure!(
                    self.charged_entries() < limit,
                    "exact path reconciliation discovery budget exhausted"
                );
                self.discovery_entries += 1;
                let name = entry?.file_name();
                bytes = bytes
                    .checked_add(name.as_encoded_bytes().len())
                    .ok_or_else(|| anyhow::anyhow!("exact path cache byte overflow"))?;
                ensure!(
                    bytes
                        <= MAX_EXACT_NAME_CACHE_BYTES
                            .saturating_sub(self.exact_directory_name_bytes),
                    "exact path reconciliation name cache exceeds byte limit"
                );
                names.insert(name);
            }
            ensure!(
                bytes <= MAX_EXACT_NAME_CACHE_BYTES.saturating_sub(self.exact_directory_name_bytes),
                "exact path reconciliation name cache exceeds byte limit"
            );
            // A failed or budget-truncated enumeration must never become a
            // cached negative observation.
            self.exact_directory_name_bytes += bytes;
            self.exact_directory_names
                .insert(parent.to_path_buf(), names);
        }
        Ok(self.exact_directory_names[parent].contains(name))
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
        let exact_spelling = usage.contains_exact_name(
            current.parent().expect("relative component has parent"),
            OsStr::new(part),
            limit,
        )?;
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
    fn bulk_ignore_reconciles_indexed_siblings_with_one_bounded_name_enumeration() -> Result<()> {
        let root = tempfile::tempdir()?;
        let data = tempfile::tempdir()?;
        for index in 0..1000 {
            std::fs::write(
                root.path().join(format!("file-{index:04}.txt")),
                "Original bounded sibling fixture.\n",
            )?;
        }
        let empty =
            airk_core::Snapshot::empty(root.path().canonicalize()?.to_string_lossy().into_owned());
        let (previous, _) = crate::scan(root.path(), data.path(), &empty)?;
        assert_eq!(previous.artifacts.len(), 1000);
        std::fs::write(root.path().join(".airkignore"), "*.txt\n.airkignore\n")?;
        let (next, report) = crate::scan(root.path(), data.path(), &previous)?;
        assert!(next.artifacts.is_empty());
        assert_eq!(report.removed, 1000);
        let mut usage = ScanUsage::default();
        for index in 0..1000 {
            assert_eq!(
                reconcile_missing(
                    root.path(),
                    &format!("file-{index:04}.txt"),
                    &mut usage,
                    5000
                )?,
                DisappearanceCause::ExcludedFromDiscovery
            );
        }
        assert_eq!(usage.discovery_entries, 1001);
        assert_eq!(usage.metadata_probes, 1000);
        let restored: ScanUsage = serde_json::from_slice(&serde_json::to_vec(&usage)?)?;
        assert!(restored.exact_directory_names.is_empty());
        Ok(())
    }

    #[test]
    fn incomplete_name_cache_is_not_reused_and_metadata_is_rechecked() -> Result<()> {
        let root = tempfile::tempdir()?;
        for name in ["a.txt", "b.txt", "c.txt"] {
            std::fs::write(root.path().join(name), "fixture")?;
        }
        let mut usage = ScanUsage::default();
        assert!(reconcile_missing(root.path(), "a.txt", &mut usage, 2).is_err());
        assert!(usage.exact_directory_names.is_empty());
        assert_eq!(
            reconcile_missing(root.path(), "a.txt", &mut usage, 100)?,
            DisappearanceCause::ExcludedFromDiscovery
        );
        std::fs::remove_file(root.path().join("a.txt"))?;
        assert_eq!(
            reconcile_missing(root.path(), "a.txt", &mut usage, 100)?,
            DisappearanceCause::DeletedObserved
        );
        Ok(())
    }

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
