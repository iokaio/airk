// SPDX-License-Identifier: Apache-2.0
//! Owner-only local model import input. Not a public management or HTTP schema.
use crate::identity::Sha256Digest;
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::path::{Component, Path};

pub const MAX_OFFLINE_IMPORT_DIRECTORY_BYTES: usize = 4096;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeOfflineImportRequest {
    pub expected_revision: u64,
    pub profile: Sha256Digest,
    pub directory: String,
}
impl NativeOfflineImportRequest {
    /// Structural checks only. This does not open paths or prove local volume,
    /// ownership, no aliases/streams, immutable bytes or profile qualification.
    /// The actual native owner supplies all those proofs before reading assets.
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.expected_revision > 0,
            "offline import requires the original configuration revision"
        );
        ensure!(
            !self.directory.is_empty()
                && self.directory.len() <= MAX_OFFLINE_IMPORT_DIRECTORY_BYTES
                && !self.directory.chars().any(char::is_control),
            "offline import directory bounds"
        );
        let path = Path::new(&self.directory);
        ensure!(
            path.is_absolute(),
            "offline import requires an absolute native LOCAL directory"
        );
        #[cfg(not(windows))]
        ensure!(
            !self.directory.starts_with("//"),
            "offline import refuses network roots"
        );
        // Reject explicit lexical traversal even when Path would normalize a '.'
        // component away. Windows accepts slash and backslash; Unix does not
        // reinterpret a backslash inside a native filename.
        #[cfg(windows)]
        let names = self.directory.split(['/', '\\']).collect::<Vec<_>>();
        #[cfg(not(windows))]
        let names = self.directory.split('/').collect::<Vec<_>>();
        ensure!(
            !names.iter().any(|name| matches!(*name, "." | "..")),
            "offline import refuses traversal"
        );
        for component in path.components() {
            match component {
                #[cfg(windows)]
                Component::Prefix(prefix) => ensure!(
                    matches!(
                        prefix.kind(),
                        std::path::Prefix::Disk(_) | std::path::Prefix::VerbatimDisk(_)
                    ),
                    "offline import refuses UNC and device namespaces"
                ),
                Component::RootDir => {}
                Component::Normal(name) => {
                    #[cfg(windows)]
                    ensure!(
                        !name.to_string_lossy().contains(':'),
                        "offline import refuses alternate streams"
                    );
                    #[cfg(not(windows))]
                    let _ = name;
                }
                _ => anyhow::bail!("offline import requires normal native path components"),
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
