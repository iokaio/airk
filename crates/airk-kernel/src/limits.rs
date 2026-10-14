// SPDX-License-Identifier: Apache-2.0
//! Persisted bounded ingestion policy; increasing limits is always explicit.
use crate::Kernel;
pub use airk_ingest::ScanLimits;
use anyhow::{Context, Result, ensure};
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

impl Kernel {
    pub fn scan_limits(&self) -> Result<ScanLimits> {
        let path = self.store.directory().join("scan-limits.json");
        let metadata = match path.symlink_metadata() {
            Ok(value) => value,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(ScanLimits::default());
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
        limits.validate()?;
        let _lock = self.store.lock()?;
        self.load_optional()?;
        self.scan_limits()?; // reject linked/corrupt/foreign existing policy
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
