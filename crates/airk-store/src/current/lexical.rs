// SPDX-License-Identifier: Apache-2.0
//! FTS5 candidates in the current database. Physical shadow rows are authenticated
//! before lookup, including empty results, or reused under cache.rs's bounded
//! change-witness contract; retained artifacts remain authoritative.
use super::{CurrentRead, journal::valid_current_path};
use airk_core::{
    Artifact, Citation, SearchQuery,
    identity::{Sha256Digest, framed_digest},
    lexical::terms,
};
use anyhow::{Context, Result, ensure};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
pub(super) mod cache;
mod ranked;
mod reference;
mod seal;
pub use ranked::{MAX_RANKED_GROUPS, RankedLexicalCandidates, TermGroup, valid_alternative};
pub use reference::LexicalReferencePosition;
pub(crate) use seal::Snapshot;

pub const ENGINE: &str = "sqlite-fts5/current-lexical-1";
pub const MAX_PATH_DOCUMENTS: usize = 8192;
const MAX_ROW_BYTES: usize = 64 << 20;
const MAX_INDEX_BYTES: u64 = 1 << 40;
const SCHEMA: &str = "CREATE TABLE current_lexical_docs(id INTEGER PRIMARY KEY,path TEXT NOT NULL,local INTEGER NOT NULL,stable_id TEXT NOT NULL UNIQUE,payload TEXT NOT NULL,terms TEXT NOT NULL);
CREATE INDEX current_lexical_paths ON current_lexical_docs(path);
CREATE VIRTUAL TABLE current_fts USING fts5(terms,content='current_lexical_docs',content_rowid='id',tokenize='ascii tokenchars ''_''',detail=none,columnsize=0);
CREATE TABLE current_pending_fts(path TEXT PRIMARY KEY CHECK(length(path) BETWEEN 1 AND 8192),metadata BLOB CHECK(metadata IS NULL OR (typeof(metadata)='blob' AND length(metadata)<=262144))) WITHOUT ROWID;";

pub(crate) fn create(connection: &Connection) -> Result<()> {
    connection.execute_batch(SCHEMA)?;
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LexicalManifest {
    pub version: u32,
    pub engine: String,
    pub root: Sha256Digest,
    pub bytes: u64,
    pub updates: Sha256Digest,
}
impl LexicalManifest {
    pub fn empty() -> Result<Self> {
        let connection = Connection::open_in_memory()?;
        create(&connection)?;
        capture(&connection, updates_digest(&[])?)
    }
    pub fn canonical_bytes(&self) -> Result<Vec<u8>> {
        ensure!(
            self.version == 2 && self.engine == ENGINE && self.bytes <= MAX_INDEX_BYTES,
            "unsupported lexical format; rebuild the collection with SQLite FTS5"
        );
        Ok(serde_json::to_vec(self)?)
    }
    pub fn digest(&self) -> Result<Sha256Digest> {
        Ok(framed_digest(
            b"current-lexical-manifest/1",
            &[&self.canonical_bytes()?],
        ))
    }
    pub fn decode(bytes: &[u8], expected: Sha256Digest) -> Result<Self> {
        ensure!(
            bytes.len() <= 4096,
            "oversized FTS5 manifest; rebuild incompatible lexical storage"
        );
        let manifest: Self = serde_json::from_slice(bytes)
            .context("incompatible lexical manifest; rebuild the collection with SQLite FTS5")?;
        ensure!(
            manifest.canonical_bytes()? == bytes && manifest.digest()? == expected,
            "current lexical manifest digest or encoding mismatch"
        );
        Ok(manifest)
    }
    // Old-format lexical seal; the plain reader no longer checks it. Removed
    // with the old journal once the writer side lands.
    #[allow(dead_code)]
    pub(crate) fn verify(&self, connection: &Connection) -> Result<()> {
        ensure!(
            capture(connection, self.updates)? == *self,
            anyhow::Error::new(super::PrivateIntegrityFailure)
                .context("current FTS5 physical index digest mismatch")
        );
        Ok(())
    }
}

pub fn stable_id(path: &str, local: u64) -> Result<Sha256Digest> {
    ensure!(valid_current_path(path), "invalid current lexical path");
    Ok(framed_digest(
        b"current-lexical-document/1",
        &[path.as_bytes(), &local.to_be_bytes()],
    ))
}

// Prefixes separate literal aliases from digested overlong terms. The ascii
// tokenizer is byte-stable for non-ASCII text; '_' explicitly preserves whole
// identifier aliases alongside the components emitted by terms().
// The plain index stores and queries chunk terms in this same encoding.
pub(crate) fn token(term: &str) -> String {
    if term.len() > 8000 {
        format!(
            "h{}",
            framed_digest(b"fts5-long-term/1", &[term.as_bytes()])
        )
    } else {
        format!("x{term}")
    }
}

/// The stored terms of one chunk (index CONTRACT.md, "chunks"): the terms of
/// `"{path} {text}"` in token form, joined with spaces.
pub fn chunk_terms(path: &str, text: &str) -> String {
    terms(&format!("{path} {text}"))
        .iter()
        .map(|term| token(term))
        .collect::<Vec<_>>()
        .join(" ")
}

/// The terms of a query in token form; every one must occur in a hit.
pub fn query_terms(text: &str) -> Vec<String> {
    terms(text).iter().map(|term| token(term)).collect()
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

pub(crate) fn capture(connection: &Connection, updates: Sha256Digest) -> Result<LexicalManifest> {
    Snapshot::capture(connection)?.manifest(updates)
}

pub(crate) fn updates_digest(updates: &[(String, Option<Vec<u8>>)]) -> Result<Sha256Digest> {
    let mut hash = Sha256::new();
    hash.update(b"fts5-captured-updates/1");
    let mut previous: Option<&str> = None;
    for (path, value) in updates {
        ensure!(
            valid_current_path(path) && previous.is_none_or(|p| p < path.as_str()),
            "invalid FTS5 update path/order"
        );
        previous = Some(path);
        hash.update((path.len() as u64).to_be_bytes());
        hash.update(path.as_bytes());
        match value {
            Some(bytes) => {
                ensure!(
                    bytes.len() <= super::MAX_INPUT_METADATA_BYTES,
                    "oversized FTS5 captured input"
                );
                hash.update([1]);
                hash.update((bytes.len() as u64).to_be_bytes());
                hash.update(bytes);
            }
            None => hash.update([0]),
        }
    }
    Ok(Sha256Digest::from_bytes(hash.finalize().into()))
}

pub(crate) fn replace(
    connection: &Connection,
    path: &str,
    artifact: Option<&Artifact>,
) -> Result<u64> {
    ensure!(valid_current_path(path), "invalid FTS5 path");
    connection.execute("INSERT INTO current_fts(current_fts,rowid,terms) SELECT 'delete',id,terms FROM current_lexical_docs WHERE path=?1", [path])?;
    connection.execute("DELETE FROM current_lexical_docs WHERE path=?1", [path])?;
    let Some(artifact) = artifact else {
        return Ok(0);
    };
    ensure!(
        artifact.path == path && artifact.chunks.len() <= MAX_PATH_DOCUMENTS,
        "invalid FTS5 artifact/path or document count"
    );
    for (local, chunk) in artifact.chunks.iter().enumerate() {
        let citation = Citation {
            path: path.into(),
            content_hash: artifact.content_hash.clone(),
            start_line: chunk.start_line,
            end_line: chunk.end_line,
            location: chunk.location.clone(),
        };
        let payload = serde_json::to_string(&citation)?;
        let encoded = chunk_terms(path, &chunk.text);
        ensure!(
            payload.len().saturating_add(encoded.len()) <= MAX_ROW_BYTES,
            "FTS5 document byte bound exceeded"
        );
        connection.execute("INSERT INTO current_lexical_docs(path,local,stable_id,payload,terms) VALUES(?1,?2,?3,?4,?5)", params![path,local as i64,stable_id(path,local as u64)?.to_string(),payload,encoded])?;
        let id = connection.last_insert_rowid();
        connection.execute(
            "INSERT INTO current_fts(rowid,terms) VALUES(?1,?2)",
            params![id, encoded],
        )?;
    }
    Ok(artifact.chunks.len() as u64)
}

pub(crate) fn flush(connection: &Connection) -> Result<()> {
    // Supported by the pinned SQLite 3.53.3 fts5SpecialInsert implementation.
    // Flush pending postings before sealing shadow rows; do not force optimize.
    connection.execute("INSERT INTO current_fts(current_fts) VALUES('flush')", [])?;
    Ok(())
}

/// How a plain lexical page is ordered and continued.
#[derive(Clone, Debug)]
pub(crate) enum CandidateOrder {
    /// BM25, best first, then stable identity (decision Q4).
    Ranked,
    /// Stable-identity or path order with an optional keyset continuation.
    Keyset(
        crate::index::LexicalOrder,
        Option<crate::index::LexicalAfter>,
    ),
}

impl CurrentRead {
    /// SQLite owns FTS5 consistency in the plain index; nothing to verify here.
    pub fn verify_lexical_integrity(&self) -> Result<()> {
        self.live()
    }
    pub fn verify_lexical(&self) -> Result<()> {
        self.live()
    }
    pub fn lexical_search(&self, query: &SearchQuery) -> Result<CurrentLexicalCandidates> {
        self.lexical_search_paths(query, None)
    }

    /// BM25-ranked candidates containing every query term. The prefix, the
    /// caller's path set and this reader's read scope and source policy all
    /// apply before the limit.
    pub fn lexical_search_paths(
        &self,
        query: &SearchQuery,
        paths: Option<&std::collections::BTreeSet<String>>,
    ) -> Result<CurrentLexicalCandidates> {
        query.validate()?;
        let result = self.lexical_candidates(
            &query.text,
            query.path_prefix.as_deref(),
            paths,
            query.limit,
            CandidateOrder::Ranked,
        )?;
        self.token()?;
        Ok(result)
    }

    /// One filtered candidate page from the plain index.
    pub(crate) fn lexical_candidates(
        &self,
        text: &str,
        prefix: Option<&str>,
        paths: Option<&std::collections::BTreeSet<String>>,
        limit: usize,
        order: CandidateOrder,
    ) -> Result<CurrentLexicalCandidates> {
        use crate::index::{LexicalAfter, LexicalQuery, MAX_LEXICAL_HITS};
        let wanted = query_terms(text);
        ensure!(!wanted.is_empty(), "query must contain a letter or number");
        ensure!(
            (1..MAX_LEXICAL_HITS).contains(&limit),
            "lexical candidate limit out of range"
        );
        let paths: Option<Vec<String>> = paths.map(|paths| paths.iter().cloned().collect());
        ensure!(
            paths
                .as_ref()
                .is_none_or(|paths| paths.iter().map(String::len).sum::<usize>() <= 4 << 20),
            "filtered lexical path selector exceeds byte budget"
        );
        if paths.as_ref().is_some_and(Vec::is_empty) {
            return Ok(CurrentLexicalCandidates {
                manifest_digest: self.head().lexical_commit,
                values: Vec::new(),
                incomplete: false,
            });
        }
        let mut values = Vec::new();
        let mut incomplete = false;
        match order {
            CandidateOrder::Ranked => {
                // The engine cannot continue a ranked order, so a filtered page
                // fetches the largest window once more before giving up.
                let mut fetch = limit + 1;
                loop {
                    let (hits, more) = self.with_index(|index| {
                        index.lexical(&LexicalQuery {
                            terms: wanted.clone(),
                            path_prefix: prefix.map(str::to_owned),
                            paths: paths.clone(),
                            limit: fetch,
                            ranked: true,
                        })
                    })?;
                    values.clear();
                    for hit in hits {
                        if self.accepts_path(&hit.path) {
                            values.push(candidate(hit)?);
                        }
                    }
                    if values.len() > limit {
                        incomplete = true;
                        break;
                    }
                    if !more {
                        break;
                    }
                    if fetch >= MAX_LEXICAL_HITS {
                        incomplete = true;
                        break;
                    }
                    fetch = MAX_LEXICAL_HITS;
                }
            }
            CandidateOrder::Keyset(order, mut after) => {
                let page = (limit + 1).clamp(256, MAX_LEXICAL_HITS);
                loop {
                    let (hits, more) = self.with_index(|index| {
                        index.lexical_keyset(
                            &LexicalQuery {
                                terms: wanted.clone(),
                                path_prefix: prefix.map(str::to_owned),
                                paths: paths.clone(),
                                limit: page,
                                ranked: false,
                            },
                            order,
                            after.as_ref(),
                        )
                    })?;
                    let last = hits.last().map(|hit| match order {
                        crate::index::LexicalOrder::StableId => LexicalAfter::StableId {
                            id: hit.stable_id,
                            inclusive: false,
                        },
                        crate::index::LexicalOrder::Path => LexicalAfter::Path {
                            path: hit.path.clone(),
                            ordinal: hit.ordinal,
                            inclusive: false,
                        },
                    });
                    for hit in hits {
                        if values.len() > limit {
                            break;
                        }
                        if self.accepts_path(&hit.path) {
                            values.push(candidate(hit)?);
                        }
                    }
                    if values.len() > limit || !more {
                        break;
                    }
                    after = last;
                }
                incomplete = values.len() > limit;
            }
        }
        values.truncate(limit);
        Ok(CurrentLexicalCandidates {
            manifest_digest: self.head().lexical_commit,
            values,
            incomplete,
        })
    }
}

/// A stored chunk as a lexical candidate, checked against its identity.
pub(crate) fn candidate(hit: crate::index::LexicalHit) -> Result<CurrentLexicalCandidate> {
    let local = u64::from(hit.ordinal);
    ensure!(
        hit.stable_id == stable_id(&hit.path, local)?,
        "lexical candidate identity mismatch; rebuild the index"
    );
    ensure!(
        hit.citation.len() <= MAX_ROW_BYTES,
        "lexical candidate byte bound exceeded"
    );
    let citation: Citation = serde_json::from_slice(&hit.citation)?;
    ensure!(
        citation.path == hit.path,
        "invalid lexical candidate citation"
    );
    Ok(CurrentLexicalCandidate {
        stable_id: hit.stable_id,
        local,
        citation,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn flush_rollback_and_replay_have_identical_seals() -> Result<()> {
        let mut c = Connection::open_in_memory()?;
        create(&c)?;
        let empty = LexicalManifest::empty()?;
        empty.verify(&c)?;
        let artifact = Artifact {
            path: "a.cs".into(),
            content_hash: Sha256Digest::from_bytes([1; 32]).to_string(),
            bytes: 12,
            chunks: vec![airk_core::Chunk {
                start_line: 1,
                end_line: 1,
                text: "CamelCase café λ".into(),
                location: airk_core::DocumentLocation::Lines { start: 1, end: 1 },
            }],
            language: "csharp".into(),
            extraction_method: "fixture".into(),
            parse_status: "ok".into(),
            facts: vec![],
            retained: None,
        };
        let manifest = {
            let tx = c.transaction()?;
            replace(&tx, &artifact.path, Some(&artifact))?;
            flush(&tx)?;
            let m = capture(&tx, empty.updates)?;
            tx.rollback()?;
            m
        };
        empty.verify(&c)?;
        {
            let tx = c.transaction()?;
            replace(&tx, &artifact.path, Some(&artifact))?;
            flush(&tx)?;
            manifest.verify(&tx)?;
            tx.commit()?;
        }
        manifest.verify(&c)?;
        c.execute(
            "INSERT INTO current_fts(current_fts,rank) VALUES('integrity-check',1)",
            [],
        )?;
        manifest.verify(&c)?;
        Ok(())
    }
    #[test]
    fn tracked_seals_match_full_scans_through_merges_replacements_and_deletions() -> Result<()> {
        let mut c = Connection::open_in_memory()?;
        create(&c)?;
        c.execute(
            "INSERT INTO current_fts(current_fts,rank) VALUES('automerge',2)",
            [],
        )?;
        let updates = LexicalManifest::empty()?.updates;
        let mut base = Snapshot::capture(&c)?;
        for batch in 0..70 {
            let apply = |c: &Connection| -> Result<()> {
                for i in 0..12 {
                    let path = format!("file_{}.rs", (batch * 12 + i) % 360);
                    if batch > 35 && i % 3 == 0 {
                        replace(c, &path, None)?;
                        continue;
                    }
                    let text = format!(
                        "CamelCase snake_case café λ {} {}",
                        "z".repeat(9000),
                        (0..100)
                            .map(|n| format!("word{batch}item{n}"))
                            .collect::<Vec<_>>()
                            .join(" ")
                    );
                    let artifact = Artifact {
                        path: path.clone(),
                        content_hash: "hash".into(),
                        bytes: text.len(),
                        chunks: vec![airk_core::Chunk {
                            text,
                            start_line: 1,
                            end_line: 1,
                            location: airk_core::DocumentLocation::Lines { start: 1, end: 1 },
                        }],
                        language: "rust".into(),
                        extraction_method: "fixture".into(),
                        parse_status: "ok".into(),
                        facts: vec![],
                        retained: None,
                    };
                    replace(c, &path, Some(&artifact))?;
                }
                flush(c)
            };
            let target = {
                let tx = c.transaction()?;
                let next = base.apply_tracked(&tx, || apply(&tx))?.0;
                assert_eq!(
                    next.manifest(updates)?,
                    capture(&tx, updates)?,
                    "prepare batch {batch}"
                );
                tx.rollback()?;
                next.manifest(updates)?
            };
            let tx = c.transaction()?;
            let next = base.apply_tracked(&tx, || apply(&tx))?.0;
            assert_eq!(next.manifest(updates)?, target, "replay batch {batch}");
            assert_eq!(target, capture(&tx, updates)?, "physical batch {batch}");
            tx.commit()?;
            base = next;
        }
        let expression = format!(
            "\"{}\" AND \"{}\"",
            token("snake_case"),
            token(&"z".repeat(9000))
        );
        let count: i64 = c.query_row(
            "SELECT count(*) FROM current_fts WHERE current_fts MATCH ?1",
            [&expression],
            |r| r.get(0),
        )?;
        assert!(count > 0);
        c.execute(
            "INSERT INTO current_fts(current_fts,rank) VALUES('integrity-check',1)",
            [],
        )?;
        Ok(())
    }
}
