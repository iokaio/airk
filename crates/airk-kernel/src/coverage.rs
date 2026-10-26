// SPDX-License-Identifier: Apache-2.0
//! Collection-scoped coverage. Refreshes are explicit; ordinary status reads never walk sources.
use crate::Kernel;
use airk_core::EXTRACTOR_VERSION;
use airk_ingest::inventory::{Eligibility, Inventory, InventoryLimits, POLICY_VERSION};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{Read, Write},
    time::{SystemTime, UNIX_EPOCH},
};

const CACHE_LIMIT: u64 = 32 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SourceSize {
    pub path: String,
    pub source_bytes: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CoverageReport {
    pub collection: String,
    pub root: String,
    pub generation: u64,
    pub indexed: bool,
    pub inventory_revision: u64,
    pub scanned_at_unix_ms: u64,
    pub policy_version: String,
    #[serde(default)]
    pub scan_limits: airk_ingest::ScanLimits,
    pub extractor_version: String,
    pub scan_complete: bool,
    #[serde(default)]
    pub source_configuration_pending: bool,
    pub eligible: usize,
    pub current: usize,
    pub stale: usize,
    pub missing: usize,
    pub extraction_failed: usize,
    pub capacity_blocked: usize,
    pub unreadable: usize,
    pub excluded: usize,
    pub unsupported: usize,
    pub pending_removals: usize,
    pub budget_exhausted: bool,
    pub percent: Option<u8>,
    #[serde(default)]
    pub eligible_source_bytes: usize,
    #[serde(default)]
    pub extracted_bytes: usize,
    /// At most twenty successfully extracted files, largest first.
    #[serde(default)]
    pub largest_sources: Vec<SourceSize>,
}

#[derive(Serialize, Deserialize)]
struct CachedCoverage {
    report: CoverageReport,
    inventory: Inventory,
    #[serde(default)]
    roots: std::collections::BTreeMap<String, String>,
}

impl Kernel {
    /// Cache-only by default. A refreshed report is a measured view at the stated time,
    /// never a claim that a filesystem cannot change immediately afterwards.
    pub fn coverage(&self, refresh: bool) -> Result<CoverageReport> {
        self.coverage_with_limits(refresh, InventoryLimits::default())
    }

    pub fn coverage_with_limits(
        &self,
        refresh: bool,
        limits: InventoryLimits,
    ) -> Result<CoverageReport> {
        let snapshot = self.load_optional()?;
        let configured_roots = self.configured_roots()?;
        let scan_limits = self.scan_limits()?;
        let generation = snapshot.as_ref().map_or(0, |snapshot| snapshot.generation);
        let filename = self.store.directory().join("coverage.json");
        let cached = fs::File::open(&filename)
            .ok()
            .and_then(|file| {
                let mut bytes = Vec::new();
                file.take(CACHE_LIMIT + 1).read_to_end(&mut bytes).ok()?;
                if bytes.len() as u64 > CACHE_LIMIT {
                    return None;
                }
                serde_json::from_slice::<CachedCoverage>(&bytes).ok()
            })
            .filter(|cache| {
                cache.report.collection == self.collection
                    && cache.report.root == self.root_id()
                    && cache.report.policy_version == POLICY_VERSION
                    && cache.report.extractor_version == EXTRACTOR_VERSION
                    && cache.roots == configured_roots
                    && cache.report.scan_limits == scan_limits
            });
        if !refresh && let Some(cache) = &cached {
            let mut report = cache.report.clone();
            if report.generation != generation || report.indexed != snapshot.is_some() {
                report.scan_complete = false;
                report.percent = None;
            }
            return Ok(report);
        }
        let mut report = CoverageReport {
            collection: self.collection.clone(),
            root: self.root_id(),
            generation,
            indexed: snapshot.is_some(),
            inventory_revision: cached
                .as_ref()
                .map_or(0, |cache| cache.report.inventory_revision),
            scanned_at_unix_ms: 0,
            policy_version: POLICY_VERSION.into(),
            scan_limits,
            extractor_version: EXTRACTOR_VERSION.into(),
            scan_complete: false,
            source_configuration_pending: snapshot
                .as_ref()
                .map_or(!configured_roots.is_empty(), |snapshot| {
                    snapshot.roots != configured_roots
                }),
            eligible: 0,
            current: 0,
            stale: 0,
            missing: 0,
            extraction_failed: 0,
            capacity_blocked: 0,
            unreadable: 0,
            excluded: 0,
            unsupported: 0,
            pending_removals: 0,
            budget_exhausted: false,
            percent: None,
            eligible_source_bytes: 0,
            extracted_bytes: 0,
            largest_sources: vec![],
        };
        if !refresh {
            return Ok(report);
        }
        // Establish and validate the managed directory; release the writer before hashing.
        {
            let _lock = self.lock_current_writer()?;
            ensure!(
                !self.root.starts_with(self.data_root.canonicalize()?),
                "data directory must not contain the workspace root"
            );
        }
        let previous = snapshot.unwrap_or_else(|| self.empty_snapshot());
        let inventory = airk_ingest::inventory::scan_roots_with_policy(
            &self.root,
            &self.data_root,
            &previous,
            &configured_roots,
            cached.as_ref().map(|cache| &cache.inventory),
            limits,
            scan_limits,
        )?;
        report.inventory_revision = report
            .inventory_revision
            .checked_add(1)
            .context("coverage inventory revision overflow")?;
        report.scanned_at_unix_ms =
            u64::try_from(SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis())?;
        report.scan_complete = inventory.complete;
        report.unreadable = inventory.unreadable;
        report.excluded = inventory.excluded;
        report.budget_exhausted = inventory.budget_exhausted;
        let mut successful_bytes = 0usize;
        let mut extracted_bytes = 0usize;
        let mut successful_files = 0usize;
        for (path, entry) in &inventory.entries {
            if entry.eligibility == Eligibility::Unsupported {
                report.unsupported += 1;
                continue;
            }
            report.eligible += 1;
            if entry.eligibility == Eligibility::ExtractionFailed {
                report.extraction_failed += 1;
                continue;
            }
            successful_bytes += entry.bytes;
            extracted_bytes += entry.extracted_bytes;
            successful_files += 1;
            report.largest_sources.push(SourceSize {
                path: path.clone(),
                source_bytes: entry.bytes,
            });
            report.largest_sources.sort_by(|a, b| {
                b.source_bytes
                    .cmp(&a.source_bytes)
                    .then(a.path.cmp(&b.path))
            });
            report.largest_sources.truncate(20);
            match previous.artifacts.get(path) {
                Some(old)
                    if old.content_hash == entry.content_hash
                        && previous.extractor_version == EXTRACTOR_VERSION
                        && source_binding_matches(path, &previous.roots, &configured_roots) =>
                {
                    report.current += 1
                }
                Some(_) => report.stale += 1,
                None => report.missing += 1,
            }
        }
        report.eligible_source_bytes = successful_bytes;
        report.extracted_bytes = extracted_bytes;
        if successful_bytes > scan_limits.corpus_bytes
            || successful_files > scan_limits.files
            || extracted_bytes > scan_limits.extracted_bytes
        {
            report.capacity_blocked = report.stale + report.missing;
        }
        if inventory.complete {
            report.pending_removals = previous
                .artifacts
                .keys()
                .filter(|path| {
                    inventory
                        .entries
                        .get(*path)
                        .is_none_or(|entry| entry.eligibility != Eligibility::Eligible)
                })
                .count();
        }
        // A concurrent publication invalidates comparisons, never promotes stale counts.
        let _lock = self.lock_current_writer()?;
        let latest = self.load_optional()?;
        if latest.as_ref().map_or(0, |snapshot| snapshot.generation) != generation
            || latest.is_some() != report.indexed
            || self.configured_roots()? != configured_roots
            || self.scan_limits()? != scan_limits
        {
            report.scan_complete = false;
        }
        if report.scan_complete && report.eligible > 0 && !report.source_configuration_pending {
            report.percent = Some(((report.current as u128 * 100) / report.eligible as u128) as u8);
        }
        // A second refresh may have committed while this scan was hashing.
        if let Ok(file) = fs::File::open(&filename) {
            let mut prior_bytes = Vec::new();
            file.take(CACHE_LIMIT + 1).read_to_end(&mut prior_bytes)?;
            if prior_bytes.len() as u64 <= CACHE_LIMIT
                && let Ok(prior) = serde_json::from_slice::<CachedCoverage>(&prior_bytes)
                && prior.report.root == report.root
                && prior.report.collection == report.collection
            {
                report.inventory_revision = report.inventory_revision.max(
                    prior
                        .report
                        .inventory_revision
                        .checked_add(1)
                        .context("coverage inventory revision overflow")?,
                );
            }
        }
        let bytes = serde_json::to_vec(&CachedCoverage {
            report: report.clone(),
            inventory,
            roots: configured_roots,
        })?;
        ensure!(
            bytes.len() as u64 <= CACHE_LIMIT,
            "coverage cache exceeds bounded size"
        );
        let reservation = self
            .collection_quota()?
            .reserve(&self.collection, bytes.len() as u64)?;
        reservation.charge(bytes.len() as u64)?;
        let temporary = self.store.directory().join(format!(
            ".coverage-{}-{}.tmp",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        ));
        let persisted = (|| -> Result<()> {
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            drop(file);
            fs::rename(&temporary, filename).context("persist coverage inventory")?;
            Ok(())
        })();
        if persisted.is_err() {
            let _ = fs::remove_file(temporary);
        }
        persisted?;
        Ok(report)
    }
}

fn source_binding_matches(
    path: &str,
    indexed: &std::collections::BTreeMap<String, String>,
    configured: &std::collections::BTreeMap<String, String>,
) -> bool {
    let alias = path
        .split_once('/')
        .and_then(|(prefix, _)| prefix.strip_prefix('@'));
    alias.is_none_or(|alias| indexed.get(alias) == configured.get(alias))
}
