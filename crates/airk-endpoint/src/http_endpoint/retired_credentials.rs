// SPDX-License-Identifier: Apache-2.0
//! Removal of the credential ledger an installation from before local trust
//! plan B (23 November 2026) left in its HTTP private folder. Nothing issues,
//! reads or authenticates credentials any more, and the user decided that the
//! ledger and its token files are deleted on upgrade rather than kept.
//!
//! Only exact names inside the given folder are deleted: the ledger
//! (`credentials.json`) and the canonical `credential-<id>.token` files
//! (tokens and retirement markers alike). Entries are inspected with
//! `symlink_metadata`, never followed: a link, a directory or any other
//! non-regular entry under one of those names is left in place, and so is
//! every other name. The folder itself is removed only when it is then empty
//! and is itself a real directory.
use airk_core::identity::Incarnation;
use airk_core::service_http_credentials::{STORE, filename};
use anyhow::{Result, ensure};
use std::path::Path;

/// The largest folder census this removal reads. A pre-B folder held at most
/// 128 tokens or markers, the ledger and the TLS and endpoint records.
const MAX_ENTRIES: usize = 1024;

/// What one removal deleted.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RetiredCredentials {
    pub ledger: bool,
    pub tokens: usize,
    pub folder: bool,
}

fn regular(path: &Path) -> Result<bool> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) => Ok(metadata.file_type().is_file()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

/// Whether `name` is a canonical `credential-<id>.token` name.
fn token_name(name: &str) -> bool {
    name.strip_prefix("credential-")
        .and_then(|rest| rest.strip_suffix(".token"))
        .and_then(|id| id.parse::<Incarnation>().ok())
        .is_some_and(|id| filename(id) == name)
}

/// Delete the retired ledger and token files of `root` (an installation's
/// `http-1` folder), then the folder if it is empty. A missing folder is
/// nothing to do. A folder that is a link or not a directory refuses.
pub fn remove(root: &Path) -> Result<RetiredCredentials> {
    let mut removed = RetiredCredentials::default();
    let metadata = match std::fs::symlink_metadata(root) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(removed),
        Err(error) => return Err(error.into()),
    };
    ensure!(
        metadata.file_type().is_dir(),
        "HTTP private folder is not a directory"
    );
    let ledger = root.join(STORE);
    if regular(&ledger)? {
        std::fs::remove_file(&ledger)?;
        removed.ledger = true;
    }
    let mut tokens = Vec::new();
    let mut empty = true;
    for (count, entry) in std::fs::read_dir(root)?.enumerate() {
        ensure!(
            count < MAX_ENTRIES,
            "HTTP private folder census exceeds bound"
        );
        let entry = entry?;
        let name = entry.file_name();
        match name.to_str() {
            Some(name) if token_name(name) && entry.file_type()?.is_file() => {
                tokens.push(entry.path());
            }
            _ => empty = false,
        }
    }
    for path in tokens {
        std::fs::remove_file(path)?;
        removed.tokens += 1;
    }
    if empty {
        std::fs::remove_dir(root)?;
        removed.folder = true;
    }
    Ok(removed)
}

#[cfg(test)]
mod tests;
