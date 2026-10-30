// SPDX-License-Identifier: Apache-2.0
//! Bounded coverage inventory using exactly the indexer's discovery and extraction policy.
use airk_core::{EXTRACTOR_VERSION, Snapshot};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    path::Path,
    time::{Duration, Instant},
};

pub const POLICY_VERSION: &str = "airk-eligibility-v4";

#[derive(Clone, Copy)]
pub struct InventoryLimits {
    pub candidates: usize,
    pub read_bytes: usize,
    pub duration: Duration,
}
impl Default for InventoryLimits {
    fn default() -> Self {
        Self {
            candidates: super::MAX_DISCOVERY_ENTRIES,
            read_bytes: super::MAX_SCAN_READ_BYTES,
            duration: Duration::from_secs(15),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Eligibility {
    Eligible,
    ExtractionFailed,
    Unsupported,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Entry {
    pub content_hash: String,
    pub bytes: usize,
    pub extracted_bytes: usize,
    pub eligibility: Eligibility,
    /// Set when extraction failed because a document work budget rejected it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub budget_exceeded: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Inventory {
    pub entries: BTreeMap<String, Entry>,
    pub complete: bool,
    pub unreadable: usize,
    pub excluded: usize,
    pub budget_exhausted: bool,
}

/// `cached` is only reusable when its extractor and policy versions match.
/// File metadata never proves freshness: every included candidate is hashed.
pub fn scan(
    root: &Path,
    data_dir: &Path,
    previous: &Snapshot,
    cached: Option<&Inventory>,
    limits: InventoryLimits,
) -> Result<Inventory> {
    scan_roots(root, data_dir, previous, &previous.roots, cached, limits)
}

pub fn scan_roots(
    root: &Path,
    data_dir: &Path,
    previous: &Snapshot,
    roots: &BTreeMap<String, String>,
    cached: Option<&Inventory>,
    limits: InventoryLimits,
) -> Result<Inventory> {
    scan_roots_with_policy(
        root,
        data_dir,
        previous,
        roots,
        cached,
        limits,
        super::ScanLimits::default(),
    )
}

pub fn scan_roots_with_policy(
    root: &Path,
    data_dir: &Path,
    previous: &Snapshot,
    roots: &BTreeMap<String, String>,
    cached: Option<&Inventory>,
    limits: InventoryLimits,
    policy: super::ScanLimits,
) -> Result<Inventory> {
    policy.validate()?;
    let limits = InventoryLimits {
        candidates: limits.candidates.min(policy.discovery_entries),
        read_bytes: limits.read_bytes.min(policy.read_bytes),
        ..limits
    };
    let root = root.canonicalize().context("resolve workspace root")?;
    ensure!(
        root.to_string_lossy() == previous.root,
        "workspace root identity changed; collection binding must be preserved"
    );
    let data_dir = data_dir.canonicalize().context("resolve index directory")?;
    let start = Instant::now();
    let mut result = Inventory {
        complete: true,
        ..Inventory::default()
    };
    let mut read_bytes = 0usize;
    let mut candidates = 0usize;
    let sources = std::iter::once((String::new(), root.clone())).chain(
        roots
            .iter()
            .map(|(alias, path)| (format!("@{alias}/"), std::path::PathBuf::from(path))),
    );
    'sources: for (prefix, source_root) in sources {
        ensure!(
            source_root.canonicalize()? == source_root && source_root.is_dir(),
            "source root identity changed during inventory"
        );
        for entry in super::discovery(&source_root, &data_dir) {
            if candidates >= limits.candidates
                || read_bytes >= limits.read_bytes
                || start.elapsed() >= limits.duration
            {
                result.complete = false;
                result.budget_exhausted = true;
                break 'sources;
            }
            candidates += 1;
            let entry = match entry {
                Ok(entry) => entry,
                Err(_) => {
                    result.unreadable += 1;
                    result.complete = false;
                    continue;
                }
            };
            if !entry.file_type().is_some_and(|kind| kind.is_file()) {
                continue;
            }
            let Some(path) = entry
                .path()
                .strip_prefix(&source_root)
                .ok()
                .and_then(|path| path.to_str())
            else {
                result.unreadable += 1;
                result.complete = false;
                continue;
            };
            let path = format!("{prefix}{}", path.replace('\\', "/"));
            if path.len() > 4096 {
                result.excluded += 1;
                continue;
            }
            if entry
                .path()
                .symlink_metadata()
                .is_ok_and(|metadata| metadata.len() > policy.file_bytes as u64)
            {
                result.excluded += 1;
                continue;
            }
            // Reserve only the remaining allowance. The reader rejects a
            // truncated source and failed reads still consume their reservation.
            let allowance = (policy.file_bytes + 1).min(limits.read_bytes - read_bytes);
            if entry
                .path()
                .symlink_metadata()
                .is_ok_and(|metadata| metadata.len() > allowance as u64)
            {
                result.complete = false;
                result.budget_exhausted = true;
                break 'sources;
            }
            // A failed read may already have consumed bytes. Reserve the maximum
            // before starting and reclaim unused allowance only on success.
            read_bytes += allowance;
            let bytes = match super::read_source_with_budget(&source_root, entry.path(), allowance)
            {
                Ok(bytes) => bytes,
                Err(_) => {
                    result.unreadable += 1;
                    result.complete = false;
                    result.budget_exhausted |= read_bytes >= limits.read_bytes;
                    continue;
                }
            };
            read_bytes -= allowance - bytes.len();
            if bytes.len() > policy.file_bytes {
                result.excluded += 1;
                continue;
            }
            let hash = format!("{:x}", Sha256::digest(&bytes));
            let (eligibility, extracted_bytes, budget_exceeded) = if previous.extractor_version
                == EXTRACTOR_VERSION
                && previous
                    .artifacts
                    .get(&path)
                    .is_some_and(|old| old.content_hash == hash)
            {
                (
                    Eligibility::Eligible,
                    previous.artifacts[&path].extracted_bytes(),
                    false,
                )
            } else if let Some(old) = cached
                .and_then(|inventory| inventory.entries.get(&path))
                .filter(|old| old.content_hash == hash)
            {
                (
                    old.eligibility.clone(),
                    old.extracted_bytes,
                    old.budget_exceeded,
                )
            } else {
                let supported_document = Path::new(&path)
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .is_some_and(|ext| {
                        ["pdf", "docx", "pptx", "xlsx"]
                            .iter()
                            .any(|supported| ext.eq_ignore_ascii_case(supported))
                    });
                if !supported_document
                    && (std::str::from_utf8(&bytes).is_err() || bytes.contains(&0))
                {
                    (Eligibility::Unsupported, 0, false)
                } else {
                    match super::documents::extract(&path, &bytes) {
                        Ok(extracted) => (
                            Eligibility::Eligible,
                            extracted.chunks.iter().map(|chunk| chunk.text.len()).sum(),
                            false,
                        ),
                        Err(error) => (
                            Eligibility::ExtractionFailed,
                            0,
                            super::documents::budget_exceeded(&error),
                        ),
                    }
                }
            };
            result.entries.insert(
                path,
                Entry {
                    content_hash: hash,
                    bytes: bytes.len(),
                    extracted_bytes,
                    eligibility,
                    budget_exceeded,
                },
            );
        }
    }
    Ok(result)
}
