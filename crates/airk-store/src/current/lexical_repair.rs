// SPDX-License-Identifier: Apache-2.0
//! Bounded proofs for restoring one absent FTS row from retained source.
use super::{
    CurrentRead, CurrentToken,
    lexical::{CurrentLexicalCandidate, query_terms},
    plain_writer::plain_chunk,
    postings::Lookup,
};
use crate::index::{ChunkEntry, FTS_RESTORE_MAX_BYTES, FtsRestore, FtsWitness};
use airk_core::{SearchQuery, identity::Sha256Digest};
use anyhow::{Result, ensure};
use std::collections::BTreeSet;

const MAX_CHUNKS: usize = 8;
const MAX_CONTROL_PATHS: usize = 8;
const MAX_TERMS: usize = 8;
const MAX_TERM_BYTES: usize = 256;

struct RepairChunk {
    ordinal: u32,
    entry: ChunkEntry,
    present: bool,
}

struct RepairFile {
    source_hash: Sha256Digest,
    payload_digest: Sha256Digest,
    chunks: Vec<RepairChunk>,
}

struct PositiveWitness {
    path: String,
    literal: String,
    ordinal: u32,
    stable_id: Sha256Digest,
    citation: Vec<u8>,
}

/// A nomination is evidence, not permission to mutate. The maintenance owner
/// rechecks authority and settings under its lease and writer lock.
pub(crate) struct LexicalRepairProposal {
    expected: CurrentToken,
    path: String,
    literal: String,
    source_hash: Sha256Digest,
    payload_digest: Sha256Digest,
    ordinal: u32,
    chunk: ChunkEntry,
    positive: PositiveWitness,
    negative: String,
}

impl LexicalRepairProposal {
    pub(crate) fn expected(&self) -> &CurrentToken {
        &self.expected
    }

    /// Used for the ordinary postcommit replay on the exact target file.
    pub(crate) fn target(&self) -> (&str, &str, u32, Sha256Digest) {
        (
            &self.path,
            &self.literal,
            self.ordinal,
            self.chunk.stable_id,
        )
    }

    pub(crate) fn with_restore<T>(
        &self,
        apply: impl FnOnce(&FtsRestore<'_>) -> Result<T>,
    ) -> Result<T> {
        let missing = query_terms(&self.literal);
        let missing: Vec<_> = missing.iter().map(String::as_str).collect();
        let positive = query_terms(&self.positive.literal);
        let positive: Vec<_> = positive.iter().map(String::as_str).collect();
        let negative = query_terms(&self.negative);
        let negative: Vec<_> = negative.iter().map(String::as_str).collect();
        apply(&FtsRestore {
            sequence: self.expected.sequence,
            source_hash: self.source_hash,
            payload_digest: self.payload_digest,
            canonical_terms: &self.chunk.terms,
            missing: FtsWitness {
                path: &self.path,
                terms: &missing,
                expected: Some((self.ordinal, self.chunk.stable_id, &self.chunk.citation)),
            },
            positive: FtsWitness {
                path: &self.positive.path,
                terms: &positive,
                expected: Some((
                    self.positive.ordinal,
                    self.positive.stable_id,
                    &self.positive.citation,
                )),
            },
            no_match: FtsWitness {
                path: &self.path,
                terms: &negative,
                expected: None,
            },
        })
    }
}

impl CurrentRead {
    /// A supplied query must be a complete exact-file lexical miss. A
    /// maintenance sweep can instead nominate one absent row from this path.
    pub(crate) fn nominate_fts_restore(
        &self,
        expected: &CurrentToken,
        path: &str,
        query: Option<&str>,
        mut checkpoint: impl FnMut() -> Result<()>,
    ) -> Result<Option<LexicalRepairProposal>> {
        checkpoint()?;
        self.require_token(expected)?;
        if let Some(literal) = query {
            exact_query(literal).validate()?;
        }
        let Some(file) = self.fts_repair_file(path, &mut checkpoint)? else {
            return Ok(None);
        };
        let selected = match query {
            Some(literal) => unique_chunk(&file, &query_terms(literal))
                .filter(|chunk| !chunk.present)
                .map(|chunk| (literal.to_owned(), chunk)),
            None => {
                let mut missing = file.chunks.iter().filter(|chunk| !chunk.present);
                match (missing.next(), missing.next()) {
                    (Some(chunk), None) => unique_literal(&file, chunk).map(|term| (term, chunk)),
                    _ => None,
                }
            }
        };
        let Some((literal, chunk)) = selected else {
            return Ok(None);
        };
        if self
            .exact_lexical(path, &literal)?
            .is_none_or(|hits| !hits.is_empty())
        {
            return Ok(None);
        }

        let negative = format!("airkrepairnomatch{}", chunk.entry.stable_id);
        let negative_terms = query_terms(&negative);
        if file
            .chunks
            .iter()
            .any(|entry| contains(&entry.entry, &negative_terms))
            || self
                .exact_lexical(path, &negative)?
                .is_none_or(|hits| !hits.is_empty())
        {
            return Ok(None);
        }
        let page = self.lookup_paths_filtered_checked(
            Lookup::Files,
            "",
            None,
            MAX_CONTROL_PATHS,
            |other| Ok(other != path),
            &mut checkpoint,
        )?;
        for other in page.paths {
            let Some(control) = self.fts_repair_file(&other, &mut checkpoint)? else {
                continue;
            };
            for healthy in control.chunks.iter().filter(|chunk| chunk.present) {
                checkpoint()?;
                let Some(control_literal) = unique_literal(&control, healthy) else {
                    continue;
                };
                let Some(hits) = self.exact_lexical(&other, &control_literal)? else {
                    continue;
                };
                if !matches!(hits.as_slice(), [hit] if hit.local == u64::from(healthy.ordinal)
                    && hit.stable_id == healthy.entry.stable_id
                    && serde_json::to_vec(&hit.citation)? == healthy.entry.citation)
                {
                    continue;
                }
                checkpoint()?;
                self.require_token(expected)?;
                self.require_path(path)?;
                self.require_path(&other)?;
                return Ok(Some(LexicalRepairProposal {
                    expected: *expected,
                    path: path.to_owned(),
                    literal,
                    source_hash: file.source_hash,
                    payload_digest: file.payload_digest,
                    ordinal: chunk.ordinal,
                    chunk: ChunkEntry {
                        stable_id: chunk.entry.stable_id,
                        citation: chunk.entry.citation.clone(),
                        terms: chunk.entry.terms.clone(),
                    },
                    positive: PositiveWitness {
                        path: other,
                        literal: control_literal,
                        ordinal: healthy.ordinal,
                        stable_id: healthy.entry.stable_id,
                        citation: healthy.entry.citation.clone(),
                    },
                    negative,
                }));
            }
        }
        Ok(None)
    }

    fn exact_lexical(
        &self,
        path: &str,
        literal: &str,
    ) -> Result<Option<Vec<CurrentLexicalCandidate>>> {
        let paths = BTreeSet::from([path.to_owned()]);
        let result = self.lexical_search_paths(&exact_query(literal), Some(&paths))?;
        Ok((!result.incomplete).then_some(result.values))
    }

    fn fts_repair_file(
        &self,
        path: &str,
        checkpoint: &mut impl FnMut() -> Result<()>,
    ) -> Result<Option<RepairFile>> {
        checkpoint()?;
        self.require_path(path)?;
        let Some(rows) = self.with_index(|reader| reader.fts_repair_rows(path, MAX_CHUNKS))? else {
            return Ok(None);
        };
        let Some(compact) = self.artifact(path)? else {
            return Ok(None);
        };
        if compact.bytes > FTS_RESTORE_MAX_BYTES || !self.retained_source_available(&compact) {
            return Ok(None);
        }
        let Some(reference) = compact.retained.as_ref() else {
            return Ok(None);
        };
        if reference.object_bytes > FTS_RESTORE_MAX_BYTES {
            return Ok(None);
        }

        checkpoint()?;
        self.read_retained_source(&compact)?;
        let full = self.load_retained_bounded(&compact, FTS_RESTORE_MAX_BYTES as u64)?;
        ensure!(
            full.chunks.len() == rows.len(),
            "retained FTS chunk roster differs"
        );
        let mut chunks = Vec::new();
        let mut total_terms = 0usize;
        for (ordinal, (chunk, row)) in full.chunks.iter().zip(rows).enumerate() {
            checkpoint()?;
            let entry = plain_chunk(&compact, ordinal as u64, chunk)?;
            ensure!(
                row.ordinal as usize == ordinal
                    && row.stable_id == entry.stable_id
                    && row.citation == entry.citation,
                "retained FTS chunk identity differs"
            );
            total_terms = total_terms.saturating_add(entry.terms.len());
            if total_terms > FTS_RESTORE_MAX_BYTES {
                return Ok(None);
            }
            chunks.push(RepairChunk {
                ordinal: row.ordinal,
                entry,
                present: row.present,
            });
        }
        Ok(Some(RepairFile {
            source_hash: compact.content_hash.parse()?,
            payload_digest: reference.object.parse()?,
            chunks,
        }))
    }
}

fn exact_query(literal: &str) -> SearchQuery {
    SearchQuery {
        text: literal.to_owned(),
        path_prefix: None,
        limit: 1,
        max_bytes: FTS_RESTORE_MAX_BYTES,
    }
}

fn contains(chunk: &ChunkEntry, wanted: &[String]) -> bool {
    wanted
        .iter()
        .all(|term| chunk.terms.split_whitespace().any(|token| token == term))
}

fn unique_chunk<'a>(file: &'a RepairFile, wanted: &[String]) -> Option<&'a RepairChunk> {
    if wanted.is_empty()
        || wanted.len() > MAX_TERMS
        || wanted.iter().any(|term| term.len() > MAX_TERM_BYTES)
    {
        return None;
    }
    let mut matches = file
        .chunks
        .iter()
        .filter(|chunk| contains(&chunk.entry, wanted));
    let first = matches.next()?;
    matches.next().is_none().then_some(first)
}

fn unique_literal(file: &RepairFile, chunk: &RepairChunk) -> Option<String> {
    let other_terms: BTreeSet<_> = file
        .chunks
        .iter()
        .filter(|other| other.ordinal != chunk.ordinal)
        .flat_map(|other| other.entry.terms.split_whitespace())
        .collect();
    for token in chunk.entry.terms.split_whitespace() {
        let Some(literal) = token.strip_prefix('x') else {
            continue;
        };
        if literal.is_empty() || token.len() > MAX_TERM_BYTES || other_terms.contains(token) {
            continue;
        }
        let terms = query_terms(literal);
        if terms.as_slice() == [token] {
            return Some(literal.to_owned());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fts_nomination_requires_unique_complete_same_chunk_term_match() {
        let chunk = |ordinal, terms: &str, present| RepairChunk {
            ordinal,
            entry: ChunkEntry {
                stable_id: Sha256Digest::from_bytes([ordinal as u8; 32]),
                citation: Vec::new(),
                terms: terms.to_owned(),
            },
            present,
        };
        let file = RepairFile {
            source_hash: Sha256Digest::from_bytes([0; 32]),
            payload_digest: Sha256Digest::from_bytes([0; 32]),
            chunks: vec![
                chunk(0, "xalpha xbeta", false),
                chunk(1, "xalpha xgamma", true),
            ],
        };
        assert!(unique_chunk(&file, &query_terms("alpha")).is_none());
        assert!(unique_chunk(&file, &query_terms("beta gamma")).is_none());
        assert!(unique_chunk(&file, &query_terms("missing")).is_none());
        assert_eq!(
            unique_chunk(&file, &query_terms("alpha beta"))
                .unwrap()
                .ordinal,
            0
        );
        assert_eq!(
            unique_literal(&file, &file.chunks[0]).as_deref(),
            Some("beta")
        );
    }
}
