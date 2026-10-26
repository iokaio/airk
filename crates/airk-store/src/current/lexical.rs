// SPDX-License-Identifier: Apache-2.0
//! Authenticated native lexical candidates; retained artifacts remain authoritative.
use super::{CurrentRead, journal::valid_current_path};
use airk_core::{
    Citation, SearchQuery,
    identity::{Sha256Digest, framed_digest},
};
use anyhow::{Context, Result, ensure};
pub const ENGINE: &str = super::kv::lexical::ENGINE;
pub const MAX_PATH_DOCUMENTS: usize = 8192;
pub type LexicalManifest = super::kv::lexical::Manifest;
pub fn stable_id(path: &str, local: u64) -> Result<Sha256Digest> {
    ensure!(valid_current_path(path), "invalid current lexical path");
    Ok(framed_digest(
        b"current-lexical-document/1",
        &[path.as_bytes(), &local.to_be_bytes()],
    ))
}

#[derive(Clone, Debug)]
pub struct CurrentLexicalCandidate {
    pub stable_id: Sha256Digest,
    pub local: u64,
    pub citation: Citation,
}
#[derive(Clone, Debug)]
pub struct CurrentLexicalCandidates {
    pub manifest_digest: Sha256Digest,
    pub values: Vec<CurrentLexicalCandidate>,
    pub incomplete: bool,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LexicalWork {
    pub changed_artifacts: usize,
    pub added_documents: u64,
}

impl CurrentRead {
    pub fn verify_lexical_integrity(&self) -> Result<()> {
        self.with_redb(|snapshot, storage| {
            super::kv::lexical::Index::new(&self.head().scope)?.verify(
                storage,
                &snapshot.lexical_manifest()?,
                &mut super::kv::directory::Budget::publication(),
            )
        })?
        .context("native lexical snapshot missing")
    }
    pub fn verify_lexical(&self) -> Result<()> {
        self.with_redb(|snapshot, _| {
            snapshot.lexical_manifest()?;
            Ok(())
        })?
        .context("native lexical snapshot missing")
    }
    pub fn lexical_search(&self, query: &SearchQuery) -> Result<CurrentLexicalCandidates> {
        self.with_redb(|snapshot, storage| {
            let cancelled = std::sync::atomic::AtomicBool::new(false);
            super::kv::lexical::Index::new(&self.head().scope)?.search(
                storage,
                &snapshot.lexical_manifest()?,
                query,
                &mut super::kv::lexical::QueryWork::new(&cancelled),
            )
        })?
        .context("native lexical snapshot missing")
    }
}
