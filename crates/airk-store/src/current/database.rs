// SPDX-License-Identifier: Apache-2.0
//! Bounded ownership metadata; the native engine owns all database operations.
use super::CurrentScope;
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{fs, io::Read, path::Path};
pub const DATABASE_VERSION: u32 = super::kv::FORMAT;
pub const PAGE_BYTES: u64 = 4096;
pub const MAX_DATABASE_BYTES: u64 = 16 << 40;
pub const OWNER_FILE: &str = "current-owner.json";
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Owner {
    version: u32,
    scope: CurrentScope,
    capacity: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    replay: Option<ReplayAdmission>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReplayAdmission {
    pub pending: airk_core::identity::Sha256Digest,
    pub expected: super::CurrentToken,
    pub target: super::CurrentToken,
    pub high_water: u64,
    pub capacity: u64,
}
pub(crate) fn validate_capacity(bytes: u64) -> Result<u64> {
    ensure!(
        (PAGE_BYTES * 16..=MAX_DATABASE_BYTES).contains(&bytes) && bytes.is_multiple_of(PAGE_BYTES),
        "invalid current database capacity"
    );
    Ok(bytes / PAGE_BYTES)
}
fn read_owner_any(directory: &Path) -> Result<Owner> {
    let path = directory.join(OWNER_FILE);
    let metadata = fs::symlink_metadata(&path)
        .context("current ownership header missing; fresh setup and full indexing are required")?;
    crate::managed_collection::reject_file_aliases(&path, &metadata)?;
    ensure!(
        metadata.len() <= 65536,
        "oversized current ownership header"
    );
    let mut bytes = Vec::new();
    fs::File::open(path)?.take(65537).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= 65536,
        "current ownership header exceeds bound"
    );
    let owner: Owner = serde_json::from_slice(&bytes)?;
    validate_capacity(owner.capacity)?;
    owner.scope.validate()?;
    ensure!(
        serde_json::to_vec(&owner)? == bytes,
        "noncanonical current ownership header"
    );
    Ok(owner)
}
fn read_owner(directory: &Path) -> Result<Owner> {
    let owner = read_owner_any(directory)?;
    ensure!(
        owner.version == DATABASE_VERSION,
        "old collection requires ownership-checked fresh setup and full indexing"
    );
    if let Some(incarnation) = crate::FileStore::new(directory).service_incarnation()? {
        ensure!(
            owner.scope.incarnation == incarnation,
            "current owner differs from managed incarnation directory"
        );
    }
    Ok(owner)
}
pub(crate) fn check_owner(directory: &Path, scope: &CurrentScope) -> Result<u64> {
    let owner = read_owner(directory)?;
    ensure!(owner.scope == *scope, "foreign current ownership header");
    Ok(owner.capacity)
}
pub(crate) fn owned_scope(directory: &Path) -> Result<CurrentScope> {
    Ok(read_owner(directory)?.scope)
}
pub(crate) fn discover_scope(
    directory: &Path,
    root: &str,
    collection: &str,
) -> Result<CurrentScope> {
    let scope = owned_scope(directory)?;
    ensure!(
        scope.root == root && scope.collection == collection,
        "foreign current ownership header"
    );
    Ok(scope)
}
pub(crate) fn redb_owner(directory: &Path) -> Result<bool> {
    if !directory.join(OWNER_FILE).try_exists()? {
        return Ok(false);
    }
    Ok(read_owner_any(directory)?.version == DATABASE_VERSION)
}
pub(crate) fn legacy_retirement_scope(directory: &Path) -> Result<CurrentScope> {
    let owner = read_owner_any(directory)?;
    ensure!(
        matches!(owner.version, 120 | 125 | 126 | 140),
        "legacy retirement requires recognized old ownership metadata"
    );
    Ok(owner.scope)
}
pub(crate) fn reject_legacy_collection(directory: &Path) -> Result<()> {
    for name in [
        "index.sqlite3",
        "index.sqlite3-wal",
        "index.sqlite3-shm",
        "index.sqlite3-journal",
        "current.sqlite3",
        "current.sqlite3-wal",
        "current.sqlite3-shm",
        "current.sqlite3-journal",
        "snapshot.json",
        "generations",
    ] {
        ensure!(
            !directory.join(name).try_exists()?,
            "old derived storage requires ownership-checked fresh setup and full indexing"
        );
    }
    Ok(())
}
#[cfg(test)]
pub(crate) fn create_owner(directory: &Path, scope: &CurrentScope, capacity: u64) -> Result<()> {
    use std::io::Write;
    let owner = Owner {
        version: 126,
        scope: scope.clone(),
        capacity,
        replay: None,
    };
    validate_capacity(capacity)?;
    let mut file = tempfile::NamedTempFile::new_in(directory)?;
    file.write_all(&serde_json::to_vec(&owner)?)?;
    crate::durability::persist_synced(file, &directory.join(OWNER_FILE))
}
