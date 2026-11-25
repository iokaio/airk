// SPDX-License-Identifier: Apache-2.0
//! Plain index store: ordinary SQLite tables and a flat vector file.
//!
//! Atomicity is the SQLite transaction. Nothing here proves stored bytes
//! against tampering; see CONTRACT.md. The engine stores prepared, neutral
//! records and returns them unchanged: it does not interpret the caller's
//! compact file record, metadata, citations or payloads.
mod reader;
mod schema;
#[cfg(test)]
mod tests;
mod vectors;
mod writer;

pub use reader::{
    FileRow, IndexReader, LexicalAfter, LexicalHit, LexicalOrder, LexicalQuery, MAX_LEXICAL_HITS,
    MAX_PAGE, PathLookup, PathPage, UnitRow, VectorState,
};
pub use schema::IncompatibleIndex;
pub(crate) use schema::is_legacy_database;
pub use vectors::{LiveRows, VectorFile, VectorHit, VectorMatrix};
pub use writer::{
    Batch, Commit, FileVectors, IndexWriter, UnitEntry, VectorBatch, VectorCommit, VectorSetup,
};

use airk_core::identity::Sha256Digest;
use anyhow::{Result, ensure};
use sha2::{Digest, Sha256};

pub const DATABASE_FILE: &str = "index.sqlite3";
pub const FORMAT: &str = "airk-index/1";
pub const MAX_PATH_BYTES: usize = 8192;
/// One stored payload as the caller's raw bytes; larger extractions are refused.
pub const MAX_PAYLOAD_BYTES: usize = 64 << 20;
pub const MAX_RECORD_BYTES: usize = 262_144;
pub const MAX_METADATA_BYTES: usize = 16 << 20;
pub const MAX_RESOURCE_BYTES: usize = 64 << 20;

/// A lookup key that names files, apart from the path and language columns.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum SelectorKind {
    FactName = 1,
    FactKind = 2,
    EdgeSource = 3,
    EdgeTarget = 4,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Selector {
    pub kind: SelectorKind,
    pub value: String,
}

/// A stable identity that resolves to the file currently holding it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum HandleKind {
    Artifact = 1,
    Unit = 2,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Handle {
    pub kind: HandleKind,
    pub value: Vec<u8>,
}

/// What the last scan observed of the source file, for change detection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileStat {
    pub size: u64,
    pub modified_ns: i64,
    pub file_id: Vec<u8>,
}

/// One file's extraction, compressed by the caller's worker before the
/// single writer stores it. The digest names the raw bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EncodedPayload {
    pub digest: Sha256Digest,
    pub raw_bytes: u64,
    pub(crate) encoding: u8,
    pub(crate) body: Vec<u8>,
}

const ENCODING_RAW: u8 = 0;
const ENCODING_LZ4: u8 = 1;

impl EncodedPayload {
    pub fn encode(raw: &[u8]) -> Result<Self> {
        ensure!(
            raw.len() <= MAX_PAYLOAD_BYTES,
            "index payload exceeds byte limit"
        );
        let digest = Sha256Digest::from_bytes(Sha256::digest(raw).into());
        let compressed = lz4_flex::block::compress(raw);
        let (encoding, body) = if compressed.len() < raw.len() {
            (ENCODING_LZ4, compressed)
        } else {
            (ENCODING_RAW, raw.to_vec())
        };
        Ok(Self {
            digest,
            raw_bytes: raw.len() as u64,
            encoding,
            body,
        })
    }

    pub fn stored_bytes(&self) -> usize {
        self.body.len()
    }
}

pub(crate) fn decode_payload(encoding: u8, raw_bytes: u64, body: &[u8]) -> Result<Vec<u8>> {
    let length = usize::try_from(raw_bytes)?;
    ensure!(
        length <= MAX_PAYLOAD_BYTES,
        "index payload exceeds byte limit"
    );
    let raw = match encoding {
        ENCODING_RAW => body.to_vec(),
        ENCODING_LZ4 => lz4_flex::block::decompress(body, length)?,
        _ => anyhow::bail!("unsupported index payload encoding"),
    };
    ensure!(raw.len() == length, "index payload length differs");
    Ok(raw)
}

/// One citation unit of a file with its prepared lexical terms.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChunkEntry {
    pub stable_id: Sha256Digest,
    /// The caller's citation, returned unchanged with a lexical hit.
    pub citation: Vec<u8>,
    /// Space-separated terms, already normalized and prefixed by the caller.
    pub terms: String,
}

/// Everything the writer stores for one file.
#[derive(Clone, Debug)]
pub struct FileEntry {
    pub path: String,
    pub source_hash: Sha256Digest,
    pub language: String,
    /// The caller's compact file record, returned unchanged.
    pub record: Vec<u8>,
    /// The caller's file metadata, returned unchanged.
    pub metadata: Vec<u8>,
    pub stat: Option<FileStat>,
    pub payload: EncodedPayload,
    pub chunks: Vec<ChunkEntry>,
    pub selectors: Vec<Selector>,
    pub handles: Vec<Handle>,
    /// True when the file is selected for vectors and waits for them.
    pub vector_pending: bool,
}

#[derive(Clone, Debug)]
pub enum FileChange {
    Upsert(Box<FileEntry>),
    Delete(String),
}

impl FileChange {
    pub fn path(&self) -> &str {
        match self {
            Self::Upsert(entry) => &entry.path,
            Self::Delete(path) => path,
        }
    }
}

/// A named value outside the file tables. `None` removes it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResourceChange {
    pub key: String,
    pub body: Option<Vec<u8>>,
}

/// Whether a collection-directory entry is one of this engine's files: the
/// database, its SQLite sidecars or a vector generation file.
pub fn is_index_file(name: &str) -> bool {
    name.strip_prefix(DATABASE_FILE)
        .is_some_and(|suffix| matches!(suffix, "" | "-wal" | "-shm" | "-journal"))
        || vectors::generation_of(name).is_some()
}

pub fn validate_path(path: &str) -> Result<()> {
    ensure!(
        !path.is_empty()
            && path.len() <= MAX_PATH_BYTES
            && !path.contains('\0')
            && !path.contains('\\'),
        "invalid index path"
    );
    Ok(())
}

/// The lexical `MATCH` expression for terms that must all be present.
pub(crate) fn match_all(terms: &[String]) -> Result<String> {
    ensure!(
        !terms.is_empty() && terms.len() <= 256,
        "lexical query needs between 1 and 256 terms"
    );
    Ok(terms
        .iter()
        .map(|term| format!("\"{}\"", term.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(" AND "))
}
