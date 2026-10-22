// SPDX-License-Identifier: Apache-2.0
//! Explicit, resumable disposal of obsolete derived state, including retained
//! source/extraction copies and captured models. Source files and saved settings stay.
use super::{CurrentScope, database, gate};
use crate::{FileStore, WriterLock, quota::Quota};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{Read, Write},
    path::Path,
};

pub(super) const MARKER: &str = "fts5-rebuild.json";
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Rebuild {
    version: u32,
    scope: CurrentScope,
}

fn read_marker(path: &Path) -> Result<Rebuild> {
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        metadata.is_file() && !metadata.file_type().is_symlink() && metadata.len() <= 65_536,
        "invalid FTS5 rebuild intent"
    );
    let mut bytes = Vec::new();
    fs::File::open(path)?.take(65_537).read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= 65_536, "oversized FTS5 rebuild intent");
    let marker: Rebuild = serde_json::from_slice(&bytes)?;
    marker.scope.validate()?;
    ensure!(
        marker.version == 1 && serde_json::to_vec(&marker)? == bytes,
        "invalid FTS5 rebuild intent encoding/version"
    );
    Ok(marker)
}
fn derived(name: &str) -> bool {
    matches!(
        name,
        "current.sqlite3"
            | "current.sqlite3-journal"
            | "current.sqlite3-wal"
            | "current.sqlite3-shm"
            | "current-owner.json"
            | "current-bootstrap.json"
            | "current-runtime-bootstrap.json"
            | "objects"
            | "current-metadata"
            | "current-resources"
    ) || name
        .strip_prefix("lexical-")
        .is_some_and(|n| n.parse::<u64>().is_ok_and(|value| value.to_string() == n))
        || name.starts_with(".current-lexical-")
}
fn validate_tree(path: &Path, root: &Path, count: &mut usize, depth: usize) -> Result<()> {
    ensure!(depth <= 64, "FTS5 rebuild directory depth exceeded");
    *count += 1;
    ensure!(*count <= 1_000_000, "FTS5 rebuild inventory bound exceeded");
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        !metadata.file_type().is_symlink() && path.canonicalize()?.starts_with(root),
        "linked or escaped FTS5 rebuild target"
    );
    ensure!(
        metadata.is_dir() || metadata.is_file(),
        "unsupported FTS5 rebuild entry"
    );
    if metadata.is_dir() {
        for entry in fs::read_dir(path)? {
            validate_tree(&entry?.path(), root, count, depth + 1)?;
        }
    }
    Ok(())
}
impl FileStore {
    /// Called only for an explicitly requested full rebuild. The durable marker
    /// survives interrupted deletion, so no half-cleared collection appears empty.
    pub fn rebuild_obsolete_index(
        &self,
        writer: &WriterLock,
        quota: &Quota,
        root: &str,
        collection: &str,
    ) -> Result<bool> {
        let directory = gate::check_writer(self, writer)?;
        let marker_path = directory.join(MARKER);
        // The writer lock makes owner discovery stable. Compatible full reindex
        // requests must not contend with readers merely to perform this no-op.
        if !marker_path.try_exists()? && database::obsolete_lexical_scope(&directory)?.is_none() {
            return Ok(false);
        }
        let _gate = gate::Gate::exclusive(self, writer)?;
        let marker = if marker_path.try_exists()? {
            let marker = read_marker(&marker_path)?;
            if directory.join(database::OWNER_FILE).try_exists()? {
                ensure!(
                    database::obsolete_lexical_scope(&directory)?.as_ref() == Some(&marker.scope),
                    "rebuild marker differs from collection owner"
                );
            }
            marker
        } else {
            let Some(scope) = database::obsolete_lexical_scope(&directory)? else {
                return Ok(false);
            };
            Rebuild { version: 1, scope }
        };
        ensure!(
            marker.scope.root == root
                && marker.scope.collection == collection
                && directory.file_name().and_then(|v| v.to_str()) == Some(collection),
            "foreign FTS5 rebuild request"
        );
        let mut targets = Vec::new();
        let mut count = 0;
        for (root_entries, entry) in fs::read_dir(&directory)?.enumerate() {
            ensure!(
                root_entries < 16_384,
                "FTS5 rebuild root entry bound exceeded"
            );
            let entry = entry?;
            let name = entry.file_name();
            let name = name.to_str().context("non-UTF-8 collection entry")?;
            if derived(name) {
                validate_tree(&entry.path(), &directory, &mut count, 0)?;
                targets.push(entry.path());
            }
        }
        let grants = quota.current_grants(writer)?;
        ensure!(
            grants
                .iter()
                .all(|g| g.scope().incarnation == marker.scope.incarnation),
            "foreign recovery allowance prevents FTS5 rebuild"
        );
        if !marker_path.try_exists()? {
            let mut temporary = tempfile::NamedTempFile::new_in(&directory)?;
            temporary.write_all(&serde_json::to_vec(&marker)?)?;
            crate::durability::persist_synced(temporary, &marker_path)?;
        }
        for path in targets {
            if fs::symlink_metadata(&path)?.is_dir() {
                fs::remove_dir_all(&path)?;
            } else {
                fs::remove_file(&path)?;
            }
        }
        crate::maintenance::sync_parent(&directory)?;
        for grant in grants {
            grant.release(writer)?;
        }
        fs::remove_file(marker_path)?;
        crate::maintenance::sync_parent(&directory)?;
        Ok(true)
    }
}
