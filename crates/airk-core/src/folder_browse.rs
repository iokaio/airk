// SPDX-License-Identifier: Apache-2.0
//! Folder browsing for path fields (M24): the I1 listing that AIRK Admin's
//! **Browse…** dialog and `airkctl folders` share.
//!
//! A browser page cannot learn an absolute path from a folder picker, so the
//! service, which runs as the same account, lists folders for it. A listing
//! names folders only, never files, and never follows a link or junction. It
//! is bounded by `MAX_FOLDER_ENTRIES` and `MAX_FOLDER_LISTING_BYTES` and
//! continues by the last returned name; it reports truncation instead of
//! hiding it.
use crate::storage_admin::StorageLocationKind;
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

/// Schema of every listing.
pub const FOLDER_SCHEMA: u32 = 1;
/// Folders in one listing page.
pub const MAX_FOLDER_ENTRIES: usize = 200;
/// Encoded size of one listing (one I1 frame is 64 KiB).
pub const MAX_FOLDER_LISTING_BYTES: usize = 48 * 1024;
/// Request path and cursor length.
pub const MAX_FOLDER_PATH_BYTES: usize = 1024;
/// Directory entries examined for one page before it reports `truncated`.
pub const MAX_FOLDER_SCAN: usize = 20_000;
/// Index names one folder carries.
pub const MAX_FOLDER_INDEX_MARKS: usize = 4;

/// Why an entry of the starting list is offered.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FolderStart {
    /// A drive (Windows) or the file system root.
    Drive,
    /// The account's home folder.
    Home,
    /// One of the installation's own locations (`airk_location` names it).
    AirkLocation,
}

/// How an entry may be used.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FolderKind {
    /// A plain folder the listing can enter.
    Folder,
    /// A symbolic link, junction or other reparse point: shown, never entered.
    Link,
}

/// What an existing index does with a folder (from the catalog's indexes).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FolderUse {
    /// The folder is an index's primary root.
    IndexedRoot,
    /// The folder is an index's data root.
    IndexData,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FolderIndexMark {
    pub index: String,
    #[serde(rename = "use")]
    pub use_: FolderUse,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FolderEntry {
    pub name: String,
    pub path: String,
    pub kind: FolderKind,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hidden: bool,
    /// Set in the starting list only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<FolderStart>,
    /// The installation location this folder is, when it is one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub airk_location: Option<StorageLocationKind>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub indexes: Vec<FolderIndexMark>,
}

/// One step of the path from its root down to the listed folder.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FolderCrumb {
    pub name: String,
    pub path: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FolderListing {
    pub schema: u32,
    /// The listed folder; absent for the starting list.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Its parent; absent at a drive or file system root (go to the starting list).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    pub breadcrumb: Vec<FolderCrumb>,
    /// The listed folder's own marks, as an entry would carry them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub airk_location: Option<StorageLocationKind>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub indexes: Vec<FolderIndexMark>,
    pub entries: Vec<FolderEntry>,
    /// The name to pass as `after` for the next page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub continuation: Option<String>,
    /// The scan stopped at `MAX_FOLDER_SCAN` entries; some folders may be missing.
    pub truncated: bool,
    /// False when the index marks could not be read in full.
    pub marks_complete: bool,
}

fn bounded_text(value: &str, what: &str) -> Result<()> {
    ensure!(
        !value.is_empty() && value.len() <= MAX_FOLDER_PATH_BYTES && !value.contains('\0'),
        "invalid folder {what}"
    );
    Ok(())
}

/// The request's own rules: an optional bounded path, and a cursor only
/// together with a path.
pub fn validate_request(path: Option<&str>, after: Option<&str>) -> Result<()> {
    if let Some(path) = path {
        bounded_text(path, "path")?;
    }
    if let Some(after) = after {
        ensure!(path.is_some(), "a folder cursor needs a path");
        bounded_text(after, "cursor")?;
    }
    Ok(())
}

impl FolderListing {
    pub fn validate(&self) -> Result<()> {
        ensure!(self.schema == FOLDER_SCHEMA, "unsupported folder listing");
        ensure!(
            self.entries.len() <= MAX_FOLDER_ENTRIES,
            "folder listing too large"
        );
        ensure!(
            self.path.is_some() || (self.parent.is_none() && self.breadcrumb.is_empty()),
            "the starting list has no parent or breadcrumb"
        );
        // Checked separately so a folder with no subfolders (no entries) is valid.
        ensure!(
            self.path.is_some() || self.entries.iter().all(|entry| entry.start.is_some()),
            "every entry of the starting list carries its start reason"
        );
        ensure!(
            self.path.is_none() || self.entries.iter().all(|entry| entry.start.is_none()),
            "only the starting list carries start reasons"
        );
        ensure!(
            self.entries
                .iter()
                .all(|entry| entry.indexes.len() <= MAX_FOLDER_INDEX_MARKS)
                && self.indexes.len() <= MAX_FOLDER_INDEX_MARKS,
            "too many index marks on one folder"
        );
        ensure!(
            serde_json::to_vec(self)?.len() <= MAX_FOLDER_LISTING_BYTES,
            "folder listing exceeds its byte budget"
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, start: Option<FolderStart>) -> FolderEntry {
        FolderEntry {
            name: name.into(),
            path: format!("C:/{name}"),
            kind: FolderKind::Folder,
            hidden: false,
            start,
            airk_location: None,
            indexes: Vec::new(),
        }
    }

    #[test]
    fn listings_keep_their_shape_rules() {
        let start = FolderListing {
            schema: FOLDER_SCHEMA,
            path: None,
            parent: None,
            breadcrumb: Vec::new(),
            airk_location: None,
            indexes: Vec::new(),
            entries: vec![entry("C:", Some(FolderStart::Drive))],
            continuation: None,
            truncated: false,
            marks_complete: true,
        };
        start.validate().unwrap();
        let mut folder = start.clone();
        folder.path = Some("C:/".into());
        assert!(
            folder.validate().is_err(),
            "a folder listing has no start reasons"
        );
        folder.entries = vec![entry("code", None)];
        folder.validate().unwrap();
        folder.entries = Vec::new();
        folder.validate().unwrap();
        folder.entries = (0..=MAX_FOLDER_ENTRIES)
            .map(|n| entry(&n.to_string(), None))
            .collect();
        assert!(folder.validate().is_err(), "the entry bound holds");
    }

    #[test]
    fn requests_bound_their_path_and_cursor() {
        validate_request(None, None).unwrap();
        validate_request(Some("D:/code"), Some("airk")).unwrap();
        assert!(validate_request(None, Some("airk")).is_err());
        assert!(validate_request(Some(""), None).is_err());
        assert!(validate_request(Some(&"a".repeat(MAX_FOLDER_PATH_BYTES + 1)), None).is_err());
        assert!(validate_request(Some("D:/x\0y"), None).is_err());
    }
}
