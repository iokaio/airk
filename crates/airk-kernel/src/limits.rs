// SPDX-License-Identifier: Apache-2.0
//! Machine-aware defaults and explicit persisted ingestion policy.
mod execution;
pub(crate) mod machine;
use crate::Kernel;
pub use airk_ingest::ScanLimits;
use anyhow::{Context, Result, ensure};
pub(crate) use execution::{configured_extraction_execution, execution_from_grant};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{Read, Write},
};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    schema: u32,
    root: String,
    collection: String,
    limits: ScanLimits,
}

#[derive(Serialize)]
pub struct LimitsDescription {
    pub mode: &'static str,
    pub detected_memory_bytes: Option<u64>,
    pub automatic: ScanLimits,
    pub effective: ScanLimits,
    pub maximum: ScanLimits,
}

/// Permit at most one eighth of machine capacity as eligible source, capped at
/// 4 GiB. This is admission policy, not a bound on process RSS or index storage.
pub fn automatic_scan_limits(memory_bytes: Option<u64>) -> ScanLimits {
    let mib = 1024 * 1024;
    let corpus = memory_bytes
        .filter(|value| *value > 0)
        .map(|memory| {
            (memory / 8)
                .max(1)
                .min(airk_ingest::MAX_CORPUS_BYTES as u64) as usize
        })
        .unwrap_or(512 * mib);
    ScanLimits {
        file_bytes: (256 * mib).min(corpus),
        corpus_bytes: corpus,
        extracted_bytes: corpus * 2,
        read_bytes: corpus * 4,
        ..ScanLimits::default()
    }
}

impl Kernel {
    /// Remove only this collection's explicit override. Existing evidence is retained.
    pub fn reset_scan_limits(&self) -> Result<ScanLimits> {
        let _lock = self.store.lock()?;
        self.load_optional()?;
        self.scan_limits()?; // reject linked/corrupt/foreign existing policy
        match fs::remove_file(self.store.directory().join("scan-limits.json")) {
            Ok(()) => (),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) => return Err(error.into()),
        }
        self.scan_limits()
    }

    pub fn describe_scan_limits(&self) -> Result<LimitsDescription> {
        let memory = machine::memory_bytes();
        let automatic = automatic_scan_limits(memory);
        let explicit = self
            .store
            .directory()
            .join("scan-limits.json")
            .symlink_metadata()
            .is_ok();
        Ok(LimitsDescription {
            mode: if explicit { "configured" } else { "automatic" },
            detected_memory_bytes: memory,
            automatic,
            effective: self.scan_limits()?,
            maximum: ScanLimits::maximum(),
        })
    }

    pub fn scan_limits(&self) -> Result<ScanLimits> {
        let path = self.store.directory().join("scan-limits.json");
        let metadata = match path.symlink_metadata() {
            Ok(value) => value,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(automatic_scan_limits(machine::memory_bytes()));
            }
            Err(error) => return Err(error.into()),
        };
        ensure!(
            metadata.is_file() && !metadata.file_type().is_symlink(),
            "linked/non-file scan configuration rejected"
        );
        let mut bytes = Vec::new();
        fs::File::open(path)?.take(16_385).read_to_end(&mut bytes)?;
        ensure!(bytes.len() <= 16_384, "scan configuration exceeds 16 KiB");
        let config: Config = serde_json::from_slice(&bytes)?;
        ensure!(
            config.schema == 1
                && config.root == self.root_id()
                && config.collection == self.collection,
            "scan configuration belongs to another root/collection/schema"
        );
        config.limits.validate()?;
        Ok(config.limits)
    }

    pub fn configure_scan_limits(&self, limits: ScanLimits) -> Result<ScanLimits> {
        self.configure_scan_limits_if_current(limits, None)
    }

    /// A UI approval may wait while another client edits the policy. Never
    /// overwrite that edit with an older displayed proposal.
    pub fn configure_scan_limits_if_current(
        &self,
        limits: ScanLimits,
        expected: Option<ScanLimits>,
    ) -> Result<ScanLimits> {
        limits.validate()?;
        let _lock = self.store.lock()?;
        self.load_optional()?;
        let current = self.scan_limits()?; // reject linked/corrupt/foreign existing policy
        ensure!(
            expected.is_none_or(|expected| expected == current),
            "ingestion limits changed while approval was pending; review the current limits again"
        );
        let config = Config {
            schema: 1,
            root: self.root_id(),
            collection: self.collection.clone(),
            limits,
        };
        let bytes = serde_json::to_vec(&config)?;
        let reservation =
            airk_store::quota::Quota::open(&self.data_root)?.reserve(&self.collection, 16_384)?;
        reservation.charge(bytes.len() as u64)?;
        let mut temporary = tempfile::NamedTempFile::new_in(self.store.directory())?;
        temporary.write_all(&bytes)?;
        temporary.as_file().sync_all()?;
        temporary
            .persist(self.store.directory().join("scan-limits.json"))
            .context("publish scan limits")?;
        Ok(limits)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn automatic_defaults_scale_without_exceeding_four_gib() {
        let gib = 1024 * 1024 * 1024;
        for (memory, expected) in [
            (None, gib / 2),
            (Some(0), gib / 2),
            (Some(2 * gib), gib / 4),
            (Some(8 * gib), gib),
            (Some(16 * gib), 2 * gib),
            (Some(32 * gib), 4 * gib),
            (Some(128 * gib), 4 * gib),
        ] {
            let limits = automatic_scan_limits(memory);
            assert_eq!(limits.corpus_bytes as u64, expected);
            assert_eq!(limits.extracted_bytes, limits.corpus_bytes * 2);
            assert_eq!(limits.read_bytes, limits.corpus_bytes * 4);
            assert!(limits.file_bytes <= limits.corpus_bytes);
            limits.validate().unwrap();
        }
        automatic_scan_limits(Some(1)).validate().unwrap();
        automatic_scan_limits(Some(u64::MAX)).validate().unwrap();
    }
}
