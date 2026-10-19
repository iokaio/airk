// SPDX-License-Identifier: Apache-2.0
//! Bounded opaque vector configuration; interpretation belongs to the kernel.
use crate::{FileStore, WriterLock, quota::Quota};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{Read, Write},
};

// The kernel adds its enabled/selection wrapper around the bounded policy.
const MAX_CONFIG_BYTES: usize = airk_core::vector_selection::MAX_POLICY_BYTES + 64;
const MAX_ROOT_BYTES: usize = 32_768;
// A root byte may require a six-byte JSON escape; allow fixed envelope metadata too.
const MAX_BYTES: usize = MAX_CONFIG_BYTES + 6 * MAX_ROOT_BYTES + 128;
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
            policy.version == 1
                && policy.owner_root == owner_root
                && policy.owner_root.len() <= MAX_ROOT_BYTES,
            "foreign vector policy binding"
        );
        let bytes = serde_json::to_vec(&policy.policy)?;
        ensure!(
            bytes.len() <= MAX_CONFIG_BYTES,
            "vector configuration exceeds bound"
        );
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
            bytes.len() <= MAX_CONFIG_BYTES && owner_root.len() <= MAX_ROOT_BYTES,
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
        ensure!(
            serde_json::to_vec(&policy.policy)?.len() <= MAX_CONFIG_BYTES,
            "vector configuration exceeds bound"
        );
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_configuration_round_trips_with_maximum_escaped_owner_binding() -> Result<()> {
        let root = tempfile::tempdir()?;
        let store = FileStore::new(root.path().join("collections/fixture"));
        let writer = store.lock()?;
        let quota = Quota::open(root.path())?;
        // The store treats owner identity as opaque; test its worst-case JSON
        // encoding without using the synthetic identity as a filesystem path.
        let owner = "\u{1}".repeat(MAX_ROOT_BYTES);
        let bytes = serde_json::to_vec(&"x".repeat(MAX_CONFIG_BYTES - 2))?;
        assert_eq!(bytes.len(), MAX_CONFIG_BYTES);
        store.configure_vector_policy_bytes(&writer, &quota, &owner, &bytes)?;
        assert_eq!(store.vector_policy_bytes(&owner)?, Some(bytes.clone()));
        assert!(store.vector_policy_bytes("foreign-owner").is_err());
        let too_large = serde_json::to_vec(&"x".repeat(MAX_CONFIG_BYTES - 1))?;
        assert!(
            store
                .configure_vector_policy_bytes(&writer, &quota, &owner, &too_large)
                .is_err()
        );
        assert_eq!(store.vector_policy_bytes(&owner)?, Some(bytes));
        Ok(())
    }
}
