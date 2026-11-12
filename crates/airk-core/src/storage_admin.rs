// SPDX-License-Identifier: Apache-2.0
//! Storage inventory and purge (M22/M23, U100) and login startup (M21,
//! U101): the I1 shapes the admin pages and `airkctl service
//! inventory|purge-plan|purge|login` share.
//!
//! The inventory lists the service's own split locations (the install
//! record's `program`, `data`, `state`, `cache` and `logs`) with sizes. A purge
//! deletes only cache contents or closed log files, never the active log,
//! data, state or the program; it applies exactly the plan its digest names.
use crate::identity::{Sha256Digest, framed_digest};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

/// Schema of every document in this module.
pub const STORAGE_SCHEMA: u32 = 1;
/// At most this many files are listed in one purge plan; a larger location
/// is purged in several reviewed plans.
pub const MAX_PURGE_FILES: usize = 4096;
/// Directory entries the inventory measures before reporting `complete:false`.
pub const MAX_INVENTORY_ENTRIES: usize = 200_000;

/// One of the install record's locations.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StorageLocationKind {
    Program,
    Data,
    State,
    Cache,
    Logs,
}

/// What a purge may delete in a location.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PurgeRule {
    /// Nothing; the location is managed elsewhere (indexes, registry, program).
    NotPurgeable,
    /// Every file under the location; the service re-creates what it needs.
    CacheContents,
    /// Closed log files only; the file the service is writing stays.
    ClosedLogs,
}

impl StorageLocationKind {
    pub const ALL: [Self; 5] = [
        Self::Program,
        Self::Data,
        Self::State,
        Self::Cache,
        Self::Logs,
    ];

    /// The one reviewed rule per location (U100).
    pub const fn purge_rule(self) -> PurgeRule {
        match self {
            Self::Cache => PurgeRule::CacheContents,
            Self::Logs => PurgeRule::ClosedLogs,
            Self::Program | Self::Data | Self::State => PurgeRule::NotPurgeable,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StorageLocation {
    pub kind: StorageLocationKind,
    pub path: String,
    pub bytes: u64,
    pub files: u64,
    /// False when the measurement stopped at its entry budget or a part was
    /// unreadable; sizes are then lower bounds.
    pub complete: bool,
    pub purge: PurgeRule,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StorageInventory {
    pub schema: u32,
    pub locations: Vec<StorageLocation>,
    /// Things this inventory does not see, stated so it is never read as
    /// "everything AIRK stores".
    pub not_covered: Vec<String>,
}

impl StorageInventory {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema == STORAGE_SCHEMA,
            "unsupported storage inventory"
        );
        let kinds: Vec<_> = self
            .locations
            .iter()
            .map(|location| location.kind)
            .collect();
        ensure!(
            kinds == StorageLocationKind::ALL,
            "storage inventory must list each location once, in order"
        );
        ensure!(
            self.locations
                .iter()
                .all(|location| location.purge == location.kind.purge_rule()),
            "storage inventory purge rule differs from the reviewed rule"
        );
        Ok(())
    }
}

/// One file a purge plan deletes, by its path relative to the location.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PurgeFile {
    pub relative: String,
    pub bytes: u64,
}

/// The exact files a purge of one location would delete now.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoragePurgePlan {
    pub schema: u32,
    pub kind: StorageLocationKind,
    pub path: String,
    pub files: Vec<PurgeFile>,
    /// Files left out because the plan reached `MAX_PURGE_FILES`.
    pub truncated: bool,
}

impl StoragePurgePlan {
    pub fn validate(&self) -> Result<()> {
        ensure!(self.schema == STORAGE_SCHEMA, "unsupported purge plan");
        ensure!(
            self.kind.purge_rule() != PurgeRule::NotPurgeable,
            "this location is not purgeable"
        );
        ensure!(self.files.len() <= MAX_PURGE_FILES, "purge plan too large");
        ensure!(
            self.files
                .windows(2)
                .all(|pair| pair[0].relative < pair[1].relative),
            "purge plan files must be sorted and unique"
        );
        ensure!(
            self.files.iter().all(|file| relative_ok(&file.relative)),
            "purge plan names a path outside its location"
        );
        Ok(())
    }

    /// The digest a purge must name: the plan's canonical bytes.
    pub fn digest(&self) -> Result<Sha256Digest> {
        self.validate()?;
        Ok(framed_digest(
            b"airk/storage-purge-plan/1",
            &[&serde_json::to_vec(self)?],
        ))
    }
}

/// A plain relative path of `/`-separated normal components.
fn relative_ok(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 4096
        && !path.starts_with('/')
        && !path.contains('\\')
        && !path.contains(':')
        && path.split('/').all(|part| {
            !part.is_empty() && part != "." && part != ".." && !part.chars().any(char::is_control)
        })
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PurgedFile {
    pub relative: String,
    pub deleted: bool,
    /// Why a planned file stayed (gone already, in use, changed, refused).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retained: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoragePurgeReceipt {
    pub schema: u32,
    pub kind: StorageLocationKind,
    pub path: String,
    pub plan_digest: Sha256Digest,
    pub files: Vec<PurgedFile>,
    pub complete: bool,
}

/// M21 action (U101).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LoginStartupAction {
    Status,
    Install,
    Uninstall,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoginStartupStatus {
    pub schema: u32,
    /// False on platforms where the service cannot manage it (macOS, Linux
    /// for now); `registered` is then unknown and reported false.
    pub supported: bool,
    pub registered: bool,
    /// What the registration starts, as the OS records it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rules_and_plans_follow_the_review() {
        assert_eq!(
            StorageLocationKind::Cache.purge_rule(),
            PurgeRule::CacheContents
        );
        assert_eq!(
            StorageLocationKind::Logs.purge_rule(),
            PurgeRule::ClosedLogs
        );
        assert_eq!(
            StorageLocationKind::Data.purge_rule(),
            PurgeRule::NotPurgeable
        );
        let plan = |kind, files: &[&str]| StoragePurgePlan {
            schema: 1,
            kind,
            path: "C:/AIRK Service/cache".into(),
            files: files
                .iter()
                .map(|relative| PurgeFile {
                    relative: (*relative).into(),
                    bytes: 1,
                })
                .collect(),
            truncated: false,
        };
        assert!(
            plan(StorageLocationKind::Cache, &["a", "b/c"])
                .validate()
                .is_ok()
        );
        assert!(plan(StorageLocationKind::Data, &["a"]).validate().is_err());
        for bad in [
            &["b", "a"][..],
            &["a", "a"],
            &["../x"],
            &["/x"],
            &["a\\b"],
            &["c:x"],
            &["a//b"],
        ] {
            assert!(
                plan(StorageLocationKind::Cache, bad).validate().is_err(),
                "{bad:?}"
            );
        }
        let one = plan(StorageLocationKind::Logs, &["old.log"]);
        assert_eq!(one.digest().unwrap(), one.clone().digest().unwrap());
        assert_ne!(
            one.digest().unwrap(),
            plan(StorageLocationKind::Logs, &["old.log", "older.log"])
                .digest()
                .unwrap()
        );
    }
}
