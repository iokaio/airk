// SPDX-License-Identifier: Apache-2.0
//! Bounded worker ownership census, without opening or recovering SQLite.
use super::{MAX_DIRECTORY_ENTRIES, MAX_WORKER_LOCKS};
use crate::service::ProjectLease;
use airk_core::identity::Sha256Digest;
use anyhow::{Context, Result, ensure};
use std::{collections::BTreeSet, fs, path::Path};

pub(super) fn worker_leases(
    root: &Path,
    known: impl IntoIterator<Item = Sha256Digest>,
) -> Result<Vec<ProjectLease>> {
    let mut bindings = known.into_iter().collect::<BTreeSet<_>>();
    for (number, entry) in fs::read_dir(root)?.enumerate() {
        ensure!(
            number < MAX_DIRECTORY_ENTRIES,
            "catalog ownership census exceeds bound"
        );
        let entry = entry?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if let Some(suffix) = name.strip_prefix("worker-1.4-") {
            bindings.insert(
                suffix
                    .strip_suffix(".lock")
                    .context("unrecognized worker ownership entry")?
                    .parse()?,
            );
        }
    }
    ensure!(
        bindings.len() <= MAX_WORKER_LOCKS,
        "catalog worker ownership census exceeds bound"
    );
    bindings
        .into_iter()
        .map(|binding| ProjectLease::acquire(root, binding))
        .collect()
}
