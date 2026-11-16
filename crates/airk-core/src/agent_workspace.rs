// SPDX-License-Identifier: Apache-2.0
//! U110: workspace default scopes. The Explorer's applied search scope is
//! stored per workspace folder; an agent read that names no index or index
//! set but names a `workspace` path reads the default of the most specific
//! stored folder containing that path. Pure: the service owns the file
//! (`agent-workspace-defaults.json`) and its writes.
use crate::{
    identity::Incarnation,
    index_catalog::{IndexId, MAX_SELECTION, QueryProjectId, unique_bounded},
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

/// The stored file's name inside the service's private HTTP directory.
pub const FILE: &str = "agent-workspace-defaults.json";
/// The stored file's byte bound.
pub const MAX_FILE_BYTES: usize = 4 * 1024 * 1024;
/// At most this many folders keep a default.
pub const MAX_ENTRIES: usize = 256;
/// The longest accepted folder or workspace path.
pub const MAX_PATH_BYTES: usize = 4096;

/// One folder's default scope.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceDefault {
    pub folder: String,
    pub index_ids: Vec<IndexId>,
    pub index_set_ids: Vec<QueryProjectId>,
    pub applied_unix_ms: u64,
}

/// `agent-workspace-defaults.json`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceDefaults {
    pub schema: u32,
    pub namespace: Incarnation,
    pub entries: Vec<WorkspaceDefault>,
}

/// An absolute path as given: `/…`, `X:\…`, `X:/…` or a `\\` UNC path,
/// bounded and without control characters.
pub fn validate_path(path: &str) -> Result<()> {
    let bytes = path.as_bytes();
    let drive = bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'\\' | b'/');
    ensure!(
        !path.is_empty()
            && path.len() <= MAX_PATH_BYTES
            && !path.chars().any(char::is_control)
            && (path.starts_with('/') || path.starts_with("\\\\") || drive),
        "workspace must be an absolute folder path"
    );
    Ok(())
}

/// The comparison key of a path: `\` as `/`, no trailing separator, and
/// case-folded on Windows-style (drive or UNC) paths.
fn key(path: &str) -> String {
    let mut normal = path.replace('\\', "/");
    while normal.len() > 1 && normal.ends_with('/') && !normal.ends_with(":/") {
        normal.pop();
    }
    let windows = normal.starts_with("//") || normal.as_bytes().get(1) == Some(&b':');
    if windows || cfg!(windows) {
        normal.to_lowercase()
    } else {
        normal
    }
}

/// Whether `folder` is `path` or one of its ancestors.
fn contains(folder: &str, path: &str) -> bool {
    let (folder, path) = (key(folder), key(path));
    path == folder
        || path
            .strip_prefix(folder.as_str())
            .is_some_and(|rest| rest.starts_with('/') || folder.ends_with('/'))
}

impl WorkspaceDefaults {
    pub const SCHEMA: u32 = 1;

    pub fn empty(namespace: Incarnation) -> Self {
        Self {
            schema: Self::SCHEMA,
            namespace,
            entries: Vec::new(),
        }
    }

    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema == Self::SCHEMA && self.entries.len() <= MAX_ENTRIES,
            "invalid workspace defaults"
        );
        let mut folders = std::collections::BTreeSet::new();
        for entry in &self.entries {
            validate_path(&entry.folder)?;
            unique_bounded(&entry.index_ids, MAX_SELECTION)?;
            unique_bounded(&entry.index_set_ids, MAX_SELECTION)?;
            ensure!(
                (!entry.index_ids.is_empty() || !entry.index_set_ids.is_empty())
                    && folders.insert(key(&entry.folder)),
                "invalid workspace default"
            );
        }
        Ok(())
    }

    /// The default of the most specific stored folder containing `path`.
    pub fn matching(&self, path: &str) -> Option<&WorkspaceDefault> {
        self.entries
            .iter()
            .filter(|entry| contains(&entry.folder, path))
            .max_by_key(|entry| key(&entry.folder).len())
    }

    /// Set (or, with no indexes and no sets, clear) the default of each of
    /// `folders`. Entries are kept sorted by folder key.
    pub fn apply(
        &mut self,
        folders: &[String],
        index_ids: &[IndexId],
        index_set_ids: &[QueryProjectId],
        now_unix_ms: u64,
    ) -> Result<()> {
        ensure!(
            !folders.is_empty() && folders.len() <= MAX_ENTRIES,
            "name between 1 and 256 workspace folders"
        );
        for folder in folders {
            validate_path(folder)?;
        }
        unique_bounded(index_ids, MAX_SELECTION)?;
        unique_bounded(index_set_ids, MAX_SELECTION)?;
        let clear = index_ids.is_empty() && index_set_ids.is_empty();
        for folder in folders {
            let folder_key = key(folder);
            self.entries
                .retain(|entry| key(&entry.folder) != folder_key);
            if !clear {
                self.entries.push(WorkspaceDefault {
                    folder: folder.clone(),
                    index_ids: index_ids.to_vec(),
                    index_set_ids: index_set_ids.to_vec(),
                    applied_unix_ms: now_unix_ms,
                });
            }
        }
        ensure!(
            self.entries.len() <= MAX_ENTRIES,
            "at most 256 workspace folders keep a default"
        );
        self.entries.sort_by_key(|entry| key(&entry.folder));
        self.validate()
    }
}

/// The local path of a `file:` URI (an MCP root), or `None` for any other
/// scheme. `file:///d%3A/code` and `file:///D:/code` give `D:/code` style
/// drive paths; `file:///home/x` gives `/home/x`.
pub fn path_from_file_uri(uri: &str) -> Option<String> {
    let rest = uri
        .strip_prefix("file://")
        .or_else(|| uri.strip_prefix("FILE://"))?;
    // Skip an authority (`localhost` or empty); UNC hosts become `//host`.
    let at = rest.find('/')?;
    let (host, path) = (&rest[..at], &rest[at..]);
    let mut decoded = Vec::with_capacity(path.len());
    let bytes = path.as_bytes();
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'%' {
            if at + 2 >= bytes.len() {
                return None;
            }
            let hex = std::str::from_utf8(&bytes[at + 1..at + 3]).ok()?;
            decoded.push(u8::from_str_radix(hex, 16).ok()?);
            at += 3;
        } else {
            decoded.push(bytes[at]);
            at += 1;
        }
    }
    let mut path = String::from_utf8(decoded).ok()?;
    let drive = path.len() >= 3
        && path.as_bytes()[0] == b'/'
        && path.as_bytes()[1].is_ascii_alphabetic()
        && path.as_bytes()[2] == b':';
    if drive {
        path.remove(0);
        if path.len() == 2 {
            path.push('/');
        }
    } else if !host.is_empty() && !host.eq_ignore_ascii_case("localhost") {
        path = format!("//{host}{path}");
    }
    validate_path(&path).ok().map(|()| path)
}

#[cfg(test)]
mod tests;
