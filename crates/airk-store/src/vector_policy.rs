// SPDX-License-Identifier: Apache-2.0
//! Bounded opaque vector configuration; interpretation belongs to the kernel.
use crate::{FileStore, WriterLock, quota::Quota};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{Read, Write},
};

const MAX_BYTES: usize = 160 * 1024;
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredPolicy {
    version: u32,
    owner_root: String,
    policy: serde_json::Value,
}

impl FileStore {
    pub fn vector_policy_bytes(&self, owner_root: &str) -> Result<Option<Vec<u8>>> {
        let _lease = self.lifecycle(false)?;
        let path = self.directory().join("vector-policy.json");
        let metadata = match fs::symlink_metadata(&path) {
            Ok(value) => value,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        ensure!(
            metadata.is_file()
                && !metadata.file_type().is_symlink()
                && metadata.len() <= MAX_BYTES as u64,
            "invalid vector policy file"
        );
        let mut bytes = Vec::new();
        fs::File::open(path)?
            .take(MAX_BYTES as u64 + 1)
            .read_to_end(&mut bytes)?;
        ensure!(bytes.len() <= MAX_BYTES, "vector policy file exceeds bound");
        let policy: StoredPolicy = serde_json::from_slice(&bytes)?;
        ensure!(
            policy.version == 1 && policy.owner_root == owner_root,
            "foreign vector policy binding"
        );
        let bytes = serde_json::to_vec(&policy.policy)?;
        ensure!(bytes.len() <= 131_072, "vector policy exceeds 128 KiB");
        Ok(Some(bytes))
    }

    pub fn configure_vector_policy_bytes(
        &self,
        writer: &WriterLock,
        quota: &Quota,
        owner_root: &str,
        bytes: &[u8],
    ) -> Result<()> {
        ensure!(
            bytes.len() <= 131_072 && owner_root.len() <= 32_768,
            "vector policy exceeds bound"
        );
        let directory = self.directory().canonicalize()?;
        ensure!(
            writer.path == directory.join("writer.lock"),
            "foreign vector policy writer"
        );
        let collection = directory
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| anyhow::anyhow!("invalid policy collection"))?;
        ensure!(
            quota
                .root()
                .join("collections")
                .join(collection)
                .canonicalize()?
                == directory,
            "foreign vector policy quota"
        );
        self.vector_policy_bytes(owner_root)?;
        let policy = StoredPolicy {
            version: 1,
            owner_root: owner_root.into(),
            policy: serde_json::from_slice(bytes)?,
        };
        let bytes = serde_json::to_vec(&policy)?;
        ensure!(bytes.len() <= MAX_BYTES, "vector policy file exceeds bound");
        let reservation = quota.reserve(collection, bytes.len() as u64)?;
        reservation.charge(bytes.len() as u64)?;
        let mut temporary = tempfile::NamedTempFile::new_in(&directory)?;
        temporary.write_all(&bytes)?;
        temporary.as_file().sync_all()?;
        temporary.persist(directory.join("vector-policy.json"))?;
        #[cfg(unix)]
        fs::File::open(&directory)?.sync_all()?;
        Ok(())
    }
}
