// SPDX-License-Identifier: Apache-2.0
//! Qualified single-current-index storage foundation. Runtime adoption is separate.
pub mod analysis;
pub mod collection;
mod database;
pub mod discovery;
pub(crate) mod gate;
pub mod graph;
mod journal;
pub mod kv;
pub mod legacy_retirement;
pub mod lexical;
pub mod maintenance;
pub mod map;
pub mod metadata;
pub mod postings;
#[cfg(test)]
mod publication_spike_tests;
mod read;
mod rebuild;
pub mod recent;
mod recent_events;
pub mod relationships;
pub mod resources;
pub mod vector_map;
mod wal;
mod wal_budget;
#[cfg(test)]
mod wal_budget_spike;

pub use database::{DATABASE_VERSION, PAGE_BYTES, journal_bound};
pub use gate::CurrentGateContention;
pub use journal::{
    ArtifactChange, ArtifactVectorChanges, CapturedInput, CapturedInputs, CurrentCapacityChange,
    CurrentCommit, CurrentWriter, PendingTransaction, TransactionPhase,
};
mod admission;
pub use admission::{CurrentOpenContention, with_work_admission};
pub use read::{CurrentRead, CurrentReadContention};
/// Protect retained assets used by a helper without opening its project database.
pub struct CurrentObjectLease {
    _gate: gate::Gate,
}
impl crate::FileStore {
    pub fn pin_service_objects(&self) -> anyhow::Result<CurrentObjectLease> {
        anyhow::ensure!(
            self.collection_namespace()? == "collections-1.4",
            "service object lease requires service storage"
        );
        Ok(CurrentObjectLease {
            _gate: gate::Gate::shared(self)?,
        })
    }
}

/// Idle verification observers must not pin Windows files during derived-state
/// disposal or data-root relocation. No active request gates are released here.
pub(crate) fn release_lexical_cache() {
    lexical::cache::clear();
}

use airk_core::{
    graph_storage::GraphHead,
    identity::{Incarnation, Sha256Digest, framed_digest},
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

pub const MAX_HEAD_BYTES: usize = 65_536;
// A scan admits 100,000 files by default. Files, vector occurrences and keyed
// map updates have different fan-out and must not share one small row ceiling.
pub const MAX_PENDING_INPUTS: usize = 100_000;
pub const MAX_PENDING_VECTOR_INPUTS: usize = 1_000_000;
pub const MAX_PENDING_MAP_CHANGES: usize = 4_000_000;
pub const MAX_PENDING_MAP_BYTES: usize = 1024 * 1024 * 1024;
/// Cumulative streamed metadata, not simultaneously resident payloads.
pub const MAX_CURRENT_METADATA_READ_BYTES: usize = 8 * 1024 * 1024 * 1024;
pub const MAX_INPUT_METADATA_BYTES: usize = 262_144;
pub const MAX_PENDING_METADATA_BYTES: u64 = 256 * 1024 * 1024;
pub const MAX_LEXICAL_MANIFEST_BYTES: usize = 1024 * 1024;

pub(crate) fn verify_lexical_manifest(bytes: &[u8], expected: Sha256Digest) -> Result<()> {
    ensure!(
        !bytes.is_empty() && bytes.len() <= MAX_LEXICAL_MANIFEST_BYTES,
        "current lexical manifest byte bound exceeded"
    );
    serde_json::from_slice::<serde_json::Value>(bytes)?;
    ensure!(
        framed_digest(b"current-lexical-manifest/1", &[bytes]) == expected,
        "current lexical manifest digest mismatch"
    );
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CurrentScope {
    pub root: String,
    pub collection: String,
    pub incarnation: Incarnation,
}

impl CurrentScope {
    pub fn validate(&self) -> Result<()> {
        crate::validate_collection(&self.collection)?;
        ensure!(
            !self.root.is_empty() && self.root.len() <= 32_768 && !self.root.contains('\0'),
            "invalid current source root"
        );
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CurrentHead {
    pub version: u32,
    pub scope: CurrentScope,
    pub sequence: u64,
    /// Authentication root for per-artifact and keyed current mappings.
    pub artifact_root: Sha256Digest,
    /// Sequence whose authenticated lexical state is selected by this head.
    /// Graph-only commits retain the prior lexical sequence.
    pub lexical_sequence: u64,
    pub lexical_commit: Sha256Digest,
    pub graph_head: GraphHead,
}

impl CurrentHead {
    pub fn validate(&self) -> Result<()> {
        self.scope.validate()?;
        self.graph_head.validate()?;
        ensure!(
            self.version == 2
                && self.sequence <= i64::MAX as u64
                && self.lexical_sequence <= self.sequence,
            "unsupported current head"
        );
        let graph = &self.graph_head.scope;
        ensure!(
            graph.collection == self.scope.collection
                && graph.root == self.scope.root
                && graph.incarnation == self.scope.incarnation,
            "current graph belongs to another collection"
        );
        if self.sequence == 0 {
            ensure!(
                self.artifact_root == map::empty_root(&self.scope)? && self.lexical_sequence == 0,
                "initial current catalog is not empty"
            );
        }
        Ok(())
    }
    pub fn canonical_bytes(&self) -> Result<Vec<u8>> {
        self.validate()?;
        let bytes = serde_json::to_vec(self)?;
        ensure!(
            bytes.len() <= MAX_HEAD_BYTES,
            "current head exceeds byte bound"
        );
        Ok(bytes)
    }
    pub fn token(&self) -> Result<CurrentToken> {
        Ok(CurrentToken {
            incarnation: self.scope.incarnation,
            sequence: self.sequence,
            digest: framed_digest(b"current-index-head/2", &[&self.canonical_bytes()?]),
        })
    }
}

/// Exact source reads remain single-current. Bounded older parent identities
/// are available only through authenticated recent-change metadata.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CurrentToken {
    pub incarnation: Incarnation,
    pub sequence: u64,
    pub digest: Sha256Digest,
}

impl crate::FileStore {
    pub fn runtime_bootstrap(&self) -> Result<Option<Vec<u8>>> {
        use std::io::Read;
        let path = self.directory().join("current-runtime-bootstrap.json");
        let metadata = match std::fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        ensure!(
            metadata.is_file() && !metadata.file_type().is_symlink() && metadata.len() <= 2 << 20,
            "invalid runtime bootstrap ownership or size"
        );
        let mut bytes = Vec::new();
        std::fs::File::open(path)?
            .take((2 << 20) + 1)
            .read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() <= 2 << 20,
            "runtime bootstrap exceeded byte bound"
        );
        Ok(Some(bytes))
    }
    pub fn write_runtime_bootstrap(&self, writer: &crate::WriterLock, bytes: &[u8]) -> Result<()> {
        use std::io::Write;
        gate::check_writer(self, writer)?;
        ensure!(
            bytes.len() <= 2 << 20 && self.runtime_bootstrap()?.is_none(),
            "invalid or existing runtime bootstrap"
        );
        let mut file = tempfile::NamedTempFile::new_in(self.directory())?;
        file.write_all(bytes)?;
        crate::durability::persist_synced(
            file,
            &self.directory().join("current-runtime-bootstrap.json"),
        )
    }
    pub fn finish_runtime_bootstrap(&self, writer: &crate::WriterLock) -> Result<()> {
        gate::check_writer(self, writer)?;
        std::fs::remove_file(self.directory().join("current-runtime-bootstrap.json"))?;
        crate::maintenance::sync_parent(self.directory())
    }
    pub(crate) fn current_owned_scope(&self) -> Result<Option<CurrentScope>> {
        if !self.directory().join(database::OWNER_FILE).try_exists()? {
            return Ok(None);
        }
        let _gate = gate::Gate::shared(self)?;
        let scope = database::owned_scope(self.directory())?;
        ensure!(
            self.collection_id()? == scope.collection,
            "foreign current owner"
        );
        Ok(Some(scope))
    }
    pub(crate) fn current_deletion_gate(&self, writer: &crate::WriterLock) -> Result<gate::Gate> {
        gate::Gate::exclusive(self, writer)
    }

    /// Discover the persisted incarnation without consulting any old catalog.
    /// A missing owner is empty only if no current database/bootstrap exists;
    /// malformed or interrupted ownership is never silently reset.
    pub fn current_scope(&self, root: &str, collection: &str) -> Result<Option<CurrentScope>> {
        ensure!(
            !self.directory().join(rebuild::MARKER).try_exists()?,
            "FTS5 rebuild was interrupted; run airk index --full to resume"
        );
        crate::validate_collection(collection)?;
        ensure!(
            self.collection_id()? == collection,
            "foreign current collection path"
        );
        if !self.directory().try_exists()? {
            return Ok(None);
        }
        let _gate = gate::Gate::shared(self)?;
        database::reject_legacy_collection(self.directory())?;
        if !self.directory().join(database::OWNER_FILE).try_exists()? {
            ensure!(
                !self.directory().join("current.sqlite3").try_exists()?
                    && !self
                        .directory()
                        .join("current-bootstrap.json")
                        .try_exists()?,
                "current ownership header missing; rebuild the managed collection"
            );
            return Ok(None);
        }
        database::discover_scope(self.directory(), root, collection).map(Some)
    }

    /// Read the captured capacity bound before opening a recovering writer.
    pub fn current_database_capacity(&self, scope: &CurrentScope) -> Result<u64> {
        scope.validate()?;
        ensure!(
            self.collection_id()? == scope.collection,
            "foreign current capacity collection"
        );
        database::check_owner(self.directory(), scope)
    }
}

pub(crate) fn decode_head(bytes: &[u8], scope: &CurrentScope) -> Result<CurrentHead> {
    ensure!(
        bytes.len() <= MAX_HEAD_BYTES,
        "current head exceeds byte bound"
    );
    let head: CurrentHead = serde_json::from_slice(bytes)?;
    ensure!(
        head.canonical_bytes()? == bytes && &head.scope == scope,
        "foreign or noncanonical current head"
    );
    Ok(head)
}

/// Relocation owns the exclusive root lifecycle lock, so it must not acquire a
/// second shared lifecycle lease. Pending transactions must recover first.
pub(crate) fn frozen_roots(
    directory: &std::path::Path,
) -> Result<std::collections::BTreeMap<String, String>> {
    use anyhow::Context;
    let scope = database::owned_scope(directory)?;
    let capacity = database::check_owner(directory, &scope)?;
    ensure!(
        std::fs::metadata(directory.join("current.sqlite3"))?.len() <= capacity,
        "current relocation capacity mismatch"
    );
    let connection = database::open_reader(&directory.join("current.sqlite3"))?;
    let (phase, bytes): (String, Vec<u8>) = connection.query_row(
        "SELECT phase,head FROM current_state WHERE id=1 AND length(head)<=65536",
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    ensure!(
        phase == "ready",
        "recover the current index before relocation"
    );
    let head = decode_head(&bytes, &scope)?;
    journal::committed_lexical_manifest(&connection, &head)?;
    let bytes = map::get(
        &connection,
        &scope,
        head.artifact_root,
        map::key(&scope, map::MapDomain::Metadata, b"collection")?,
    )?
    .context("current relocation configuration missing")?;
    let state = collection::CollectionState::decode(&bytes)?;
    let mut roots = crate::roots::RootConfig::load(directory, &scope.root)?.roots;
    roots.insert(String::new(), scope.root);
    for (alias, path) in state.configuration.roots {
        roots.insert(format!("published:{alias}"), path);
    }
    Ok(roots)
}

#[cfg(test)]
mod tests;
