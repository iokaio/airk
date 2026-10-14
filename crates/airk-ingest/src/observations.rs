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
