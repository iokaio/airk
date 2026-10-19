// SPDX-License-Identifier: Apache-2.0
//! Active Tantivy file manifest. Commit preparation reuses hashes only for
//! immutable segment names retained by the prior trusted manifest.
use super::{CurrentLexical, schema};
use airk_core::identity::{Sha256Digest, framed_digest};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs::{self, File},
    io::Read,
    path::Path,
};
use tantivy::{Directory, Index};

pub const MAX_MANIFEST_FILES: usize = 8_192;
pub const MAX_MANIFEST_BYTES: usize = 1024 * 1024;
pub const MAX_INDEX_BYTES: u64 = 1 << 40;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LexicalFile {
    pub name: String,
    pub bytes: u64,
    pub sha256: Sha256Digest,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LexicalManifest {
    pub version: u32,
    pub engine: String,
    pub files: Vec<LexicalFile>,
    pub total_bytes: u64,
}

fn name_valid(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
        && !matches!(name, "." | "..")
}

fn active_names(index: &Index) -> Result<Vec<String>> {
    // `list_files` names optional components too. Opening a searcher first
    // checks the components that this schema actually needs.
    let reader = index.reader()?;
    let _searcher = reader.searcher();
    let mut names = BTreeSet::from(["meta.json".to_owned()]);
    for segment in index.searchable_segment_metas()? {
        for path in segment.list_files() {
            let name = path.to_str().context("non-UTF-8 Tantivy segment name")?;
            ensure!(name_valid(name), "invalid active Tantivy segment name");
            match index.directory().exists(&path) {
                Ok(true) => {
                    names.insert(name.to_owned());
                }
                Ok(false) if name.ends_with(".del") && segment.num_deleted_docs() > 0 => {
                    anyhow::bail!("active Tantivy deletion bitmap missing")
                }
                Ok(false) => (),
                Err(error) => return Err(error.into()),
            }
            ensure!(
                names.len() <= MAX_MANIFEST_FILES,
                "too many active Tantivy files"
            );
        }
    }
    Ok(names.into_iter().collect())
}

fn regular_len(directory: &Path, name: &str) -> Result<u64> {
    ensure!(name_valid(name), "invalid lexical manifest file name");
    let metadata = fs::symlink_metadata(directory.join(name))
        .with_context(|| format!("active Tantivy file {name} is missing"))?;
    ensure!(
        metadata.is_file() && !metadata.file_type().is_symlink(),
        "active Tantivy file is not a regular owned file"
    );
    ensure!(metadata.len() <= MAX_INDEX_BYTES, "oversized Tantivy file");
    Ok(metadata.len())
}

fn hash_file(directory: &Path, name: &str, expected: u64) -> Result<Sha256Digest> {
    ensure!(
        regular_len(directory, name)? == expected,
        "Tantivy file length changed"
    );
    let mut file = File::open(directory.join(name))?;
    ensure!(
        file.metadata()?.len() == expected,
        "Tantivy file replaced before read"
    );
    let mut hasher = Sha256::new();
    let mut remaining = expected;
    let mut buffer = [0u8; 64 * 1024];
    while remaining > 0 {
        let size = usize::try_from(remaining.min(buffer.len() as u64))?;
        let count = file.read(&mut buffer[..size])?;
        ensure!(count > 0, "truncated Tantivy file");
        hasher.update(&buffer[..count]);
        remaining -= count as u64;
    }
    ensure!(
        file.read(&mut buffer[..1])? == 0 && regular_len(directory, name)? == expected,
        "Tantivy file changed during authentication"
    );
    Ok(Sha256Digest::from_bytes(hasher.finalize().into()))
}

impl LexicalManifest {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.version == 1
                && self.engine == "tantivy-0.26.2/current-lexical-1"
                && !self.files.is_empty()
                && self.files.len() <= MAX_MANIFEST_FILES,
            "unsupported or oversized current lexical manifest"
        );
        let mut previous = None;
        let mut total = 0u64;
        for file in &self.files {
            ensure!(
                name_valid(&file.name)
                    && previous.is_none_or(|name: &str| name < file.name.as_str()),
                "unordered or invalid current lexical manifest file"
            );
            total = total
                .checked_add(file.bytes)
                .context("lexical manifest byte overflow")?;
            previous = Some(file.name.as_str());
        }
        ensure!(
            self.files.iter().any(|file| file.name == "meta.json")
                && total == self.total_bytes
                && total <= MAX_INDEX_BYTES,
            "invalid current lexical manifest total or metadata"
        );
        Ok(())
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>> {
        self.validate()?;
        let bytes = serde_json::to_vec(self)?;
        ensure!(
            bytes.len() <= MAX_MANIFEST_BYTES,
            "current lexical manifest exceeds byte bound"
        );
        Ok(bytes)
    }

    pub fn digest(&self) -> Result<Sha256Digest> {
        Ok(framed_digest(
            b"current-lexical-manifest/1",
            &[&self.canonical_bytes()?],
        ))
    }

    pub fn decode(bytes: &[u8], expected: Sha256Digest) -> Result<Self> {
        ensure!(
            bytes.len() <= MAX_MANIFEST_BYTES,
            "oversized lexical manifest bytes"
        );
        let manifest: Self = serde_json::from_slice(bytes)?;
        ensure!(
            manifest.canonical_bytes()? == bytes && manifest.digest()? == expected,
            "current lexical manifest digest or encoding mismatch"
        );
        Ok(manifest)
    }

    /// Verify all active files against a manifest already bound to the trusted
    /// current head. A missing or unlisted active segment fails closed.
    pub fn verify(&self, directory: &Path, trusted_digest: Sha256Digest) -> Result<()> {
        ensure!(
            self.digest()? == trusted_digest,
            "untrusted lexical manifest"
        );
        for file in &self.files {
            ensure!(
                hash_file(directory, &file.name, file.bytes)? == file.sha256,
                "current lexical file digest mismatch"
            );
        }
        let index = Index::open_in_dir(directory)?;
        ensure!(
            index.schema() == schema().0,
            "current lexical schema mismatch"
        );
        ensure!(
            active_names(&index)?
                == self
                    .files
                    .iter()
                    .map(|file| file.name.clone())
                    .collect::<Vec<_>>(),
            "current lexical active file set differs from manifest"
        );
        Ok(())
    }
}

impl CurrentLexical {
    /// Capture the active file set after a synchronized commit. Prior segment
    /// hashes may be reused only when the caller supplies their trusted digest;
    /// `meta.json` is mutable and is always rehashed. A full `verify` remains
    /// necessary before serving a newly opened reader.
    pub fn capture_manifest(
        &self,
        prior: Option<(&LexicalManifest, Sha256Digest)>,
    ) -> Result<LexicalManifest> {
        if let Some((manifest, digest)) = prior {
            ensure!(
                manifest.digest()? == digest,
                "untrusted prior lexical manifest"
            );
        }
        let mut files = Vec::new();
        let mut total_bytes = 0u64;
        for name in active_names(&self.index)? {
            let bytes = regular_len(&self.directory, &name)?;
            total_bytes = total_bytes
                .checked_add(bytes)
                .context("lexical file total overflow")?;
            ensure!(
                total_bytes <= MAX_INDEX_BYTES,
                "current lexical file total exceeded"
            );
            let reused = prior.and_then(|(old, _)| {
                old.files
                    .iter()
                    .find(|file| file.name == name && file.bytes == bytes && name != "meta.json")
            });
            let sha256 = match reused {
                Some(file) => file.sha256,
                None => hash_file(&self.directory, &name, bytes)?,
            };
            files.push(LexicalFile {
                name,
                bytes,
                sha256,
            });
        }
        let manifest = LexicalManifest {
            version: 1,
            engine: "tantivy-0.26.2/current-lexical-1".to_owned(),
            files,
            total_bytes,
        };
        manifest.canonical_bytes()?;
        Ok(manifest)
    }
}

#[cfg(test)]
mod tests;
