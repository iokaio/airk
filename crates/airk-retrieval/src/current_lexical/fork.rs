// SPDX-License-Identifier: Apache-2.0
//! Private changed-path Tantivy candidate assembled from immutable hardlinks.
use super::{CurrentLexical, LexicalChange, LexicalWork, VerifiedLexicalManifest};
use crate::bounded::Charge;
use anyhow::{Context, Result, ensure};
use std::{
    fs::{self, OpenOptions},
    io,
    path::Path,
};
use tantivy::Directory;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LexicalForkWork {
    pub hardlinked_files: usize,
    pub hardlinked_bytes: u64,
    pub copied_metadata_bytes: u64,
    /// Charged conservatively as though hardlinks consumed independent bytes.
    pub admitted_clone_bytes: u64,
    pub changed: LexicalWork,
}

impl CurrentLexical {
    /// Build a private sibling component while the verified base stays intact.
    /// The owner must hold the writer lock, reserve staging headroom, and bind
    /// the returned manifest digest to the prepared current head before use.
    /// There is no byte-copy fallback when hardlinks are unavailable.
    pub fn fork_changed(
        base: &VerifiedLexicalManifest,
        target: &Path,
        charge: Charge,
        changes: &[LexicalChange<'_>],
    ) -> Result<(Self, VerifiedLexicalManifest, LexicalForkWork)> {
        Self::fork_update(base, target, charge, |private| {
            private.apply_changes(changes)
        })
    }

    pub fn fork_loaded(
        base: &VerifiedLexicalManifest,
        target: &Path,
        charge: Charge,
        paths: &[&str],
        load: &mut dyn FnMut(&str) -> Result<Option<airk_core::Artifact>>,
    ) -> Result<(Self, VerifiedLexicalManifest, LexicalForkWork)> {
        Self::fork_update(base, target, charge, |private| {
            private.apply_loaded(paths, load)
        })
    }

    fn fork_update(
        base: &VerifiedLexicalManifest,
        target: &Path,
        charge: Charge,
        update: impl FnOnce(&Self) -> Result<Option<LexicalWork>>,
    ) -> Result<(Self, VerifiedLexicalManifest, LexicalForkWork)> {
        ensure!(
            base.manifest.digest()? == base.digest,
            "untrusted lexical fork base"
        );
        let parent = base
            .directory
            .parent()
            .context("lexical base has no parent")?;
        ensure!(
            target.parent() == Some(parent)
                && target != base.directory
                && target.file_name().is_some(),
            "lexical fork target is not a sibling"
        );
        match fs::symlink_metadata(target) {
            Ok(_) => anyhow::bail!("lexical fork target already exists"),
            Err(error) if error.kind() == io::ErrorKind::NotFound => (),
            Err(error) => return Err(error.into()),
        }
        let base_meta = fs::symlink_metadata(&base.directory)?;
        ensure!(
            base_meta.is_dir() && !base_meta.file_type().is_symlink(),
            "lexical fork base is not an owned directory"
        );
        let managed: Vec<_> = base
            .manifest
            .files
            .iter()
            .map(|file| file.name.as_str())
            .collect();
        let mut managed_bytes = serde_json::to_vec(&managed)?;
        managed_bytes.push(b'\n');
        let meta = base
            .manifest
            .files
            .iter()
            .find(|file| file.name == "meta.json")
            .context("lexical base metadata missing")?;
        let admitted_clone_bytes = base
            .manifest
            .total_bytes
            .checked_add(managed_bytes.len() as u64)
            .context("lexical fork admission overflow")?;
        charge(admitted_clone_bytes)?;
        let staging = tempfile::Builder::new()
            .prefix(".current-lexical-")
            .tempdir_in(parent)?;
        let mut hardlinked_files = 0usize;
        let mut hardlinked_bytes = 0u64;
        for file in &base.manifest.files {
            let source = base.directory.join(&file.name);
            let metadata = fs::symlink_metadata(&source)?;
            ensure!(
                metadata.is_file()
                    && !metadata.file_type().is_symlink()
                    && metadata.len() == file.bytes,
                "lexical fork source changed"
            );
            if file.name == "meta.json" {
                ensure!(
                    fs::copy(source, staging.path().join("meta.json"))? == file.bytes,
                    "lexical fork metadata copy changed length"
                );
            } else {
                fs::hard_link(source, staging.path().join(&file.name))?;
                hardlinked_files += 1;
                hardlinked_bytes = hardlinked_bytes
                    .checked_add(file.bytes)
                    .context("lexical hardlink byte overflow")?;
            }
        }
        fs::write(staging.path().join(".managed.json"), &managed_bytes)?;
        OpenOptions::new()
            .write(true)
            .open(staging.path().join("meta.json"))?
            .sync_all()?;
        OpenOptions::new()
            .write(true)
            .open(staging.path().join(".managed.json"))?
            .sync_all()?;
        Directory::sync_directory(&tantivy::directory::MmapDirectory::open(staging.path())?)?;
        let private = CurrentLexical::open(staging.path(), charge.clone())?;
        let changed = update(&private)?.unwrap_or(LexicalWork {
            changed_artifacts: 0,
            added_documents: 0,
            commit_opstamp: 0,
        });
        let manifest = private.capture_manifest(Some((&base.manifest, base.digest)))?;
        drop(private);
        fs::rename(staging.path(), target)?;
        let _ = staging.keep();
        Directory::sync_directory(&tantivy::directory::MmapDirectory::open(parent)?)?;
        let result = CurrentLexical::open(target, charge)?;
        let verified = manifest.verify_fork(target, manifest.digest()?, base)?;
        let work = LexicalForkWork {
            hardlinked_files,
            hardlinked_bytes,
            copied_metadata_bytes: meta.bytes + managed_bytes.len() as u64,
            admitted_clone_bytes,
            changed,
        };
        Ok((result, verified, work))
    }
}

#[cfg(test)]
mod tests;
