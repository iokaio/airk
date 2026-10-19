// SPDX-License-Identifier: Apache-2.0
//! Changed-artifact Tantivy updates for the single-current-index path.
//! The owning kernel must hold the exclusive current gate and journal this
//! external component before calling `apply_changes`.
use crate::{
    bounded::{BoundedDirectory, Charge},
    terms,
};
use airk_core::{
    Artifact, Citation, Hit, SearchQuery,
    identity::{Sha256Digest, framed_digest},
};
use anyhow::{Context, Result, ensure};
use std::{
    fs,
    path::{Path, PathBuf},
};
use tantivy::{
    Index, Order, TantivyDocument, Term,
    collector::TopDocs,
    query::{BooleanQuery, Occur, Query, RegexQuery, TermQuery},
    schema::{FAST, Field, IndexRecordOption, STORED, STRING, Schema, Value},
};

mod manifest;
pub use manifest::{LexicalFile, LexicalManifest, VerifiedLexicalManifest};
mod fork;
pub use fork::LexicalForkWork;

pub const MAX_CHANGED_ARTIFACTS: usize = 65_536;
pub const MAX_PATH_DOCUMENTS: usize = 8_192;

struct Fields {
    terms: Field,
    path: Field,
    stable_id: Field,
    sort_key: Field,
    local: Field,
    payload: Field,
}

fn schema() -> (Schema, Fields) {
    let mut builder = Schema::builder();
    let fields = Fields {
        terms: builder.add_text_field("terms", STRING),
        path: builder.add_text_field("path", STRING | STORED),
        stable_id: builder.add_text_field("stable_id", STRING | STORED),
        sort_key: builder.add_u64_field("sort_key", FAST | STORED),
        local: builder.add_u64_field("local", FAST | STORED),
        payload: builder.add_text_field("payload", STORED),
    };
    (builder.build(), fields)
}

fn validate_path(path: &str) -> Result<()> {
    ensure!(
        !path.is_empty()
            && path.len() <= 8_192
            && !path.contains(['\\', ':', '\0'])
            && path.split('/').all(|part| !matches!(part, "" | "." | "..")),
        "invalid current lexical path"
    );
    Ok(())
}

pub fn stable_id(path: &str, local: u64) -> Result<Sha256Digest> {
    validate_path(path)?;
    Ok(framed_digest(
        b"current-lexical-document/1",
        &[path.as_bytes(), &local.to_be_bytes()],
    ))
}

pub struct LexicalChange<'a> {
    pub path: &'a str,
    /// None deletes the path. A retained-only carrier must first be loaded to
    /// its verified full chunks by the owning store.
    pub artifact: Option<&'a Artifact>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LexicalWork {
    pub changed_artifacts: usize,
    pub added_documents: u64,
    pub commit_opstamp: u64,
}

pub struct CurrentLexical {
    directory: PathBuf,
    index: Index,
    fields: Fields,
}

/// A request-scoped reader whose active Tantivy files were authenticated
/// against the exact current-head manifest before opening the searcher.
/// The owner holds the collection's shared gate until all hits are checked
/// against retained current artifacts and the response has been materialized.
pub struct VerifiedCurrentLexical {
    index: CurrentLexical,
    ticket: VerifiedLexicalManifest,
}

/// An engine candidate, not publishable evidence. The owning kernel verifies
/// its path, source hash, local chunk and text against the current catalog's
/// retained object under the same read gate before returning it to a caller.
#[derive(Clone, Debug)]
pub struct CurrentLexicalCandidate {
    pub stable_id: Sha256Digest,
    pub local: u64,
    pub hit: Hit,
}

#[derive(Clone, Debug)]
pub struct CurrentLexicalCandidates {
    /// Exact lexical component verified when this candidate set was selected.
    pub manifest_digest: Sha256Digest,
    pub values: Vec<CurrentLexicalCandidate>,
    /// A match was omitted by the candidate count. The owner additionally
    /// reports any text-budget truncation during response materialization.
    pub incomplete: bool,
}

impl VerifiedCurrentLexical {
    pub fn open(
        directory: &Path,
        charge: Charge,
        manifest_bytes: &[u8],
        expected: Sha256Digest,
    ) -> Result<Self> {
        let manifest = LexicalManifest::decode(manifest_bytes, expected)?;
        let ticket = manifest.verify_owned(directory, expected)?;
        let index = CurrentLexical::open(directory, charge)?;
        Ok(Self { index, ticket })
    }

    pub fn search(&self, query: &SearchQuery) -> Result<CurrentLexicalCandidates> {
        query.validate()?;
        let query_terms = terms(&query.text);
        ensure!(
            !query_terms.is_empty(),
            "query must contain a letter or number"
        );
        let mut clauses: Vec<(Occur, Box<dyn Query>)> = query_terms
            .into_iter()
            .map(|term| {
                (
                    Occur::Must,
                    Box::new(TermQuery::new(
                        Term::from_field_text(self.index.fields.terms, &term),
                        IndexRecordOption::Basic,
                    )) as Box<dyn Query>,
                )
            })
            .collect();
        if let Some(prefix) = query
            .path_prefix
            .as_deref()
            .filter(|value| !value.is_empty())
        {
            let escaped: String = prefix
                .chars()
                .flat_map(|character| {
                    if ".+*?()|[]{}^$\\".contains(character) {
                        vec!['\\', character]
                    } else {
                        vec![character]
                    }
                })
                .collect();
            clauses.push((
                Occur::Must,
                Box::new(RegexQuery::from_pattern(
                    &format!("(?s:{escaped}.*)"),
                    self.index.fields.path,
                )?),
            ));
        }
        let reader = self.index.index.reader()?;
        reader.reload()?;
        let searcher = reader.searcher();
        let matches = searcher.search(
            &BooleanQuery::new(clauses),
            &TopDocs::with_limit(query.limit + 1).order_by_u64_field("sort_key", Order::Asc),
        )?;
        let incomplete = matches.len() > query.limit;
        let mut values = Vec::with_capacity(matches.len().min(query.limit));
        for (_, address) in matches.into_iter().take(query.limit) {
            let document: TantivyDocument = searcher.doc(address)?;
            let field_text = |field: Field| -> Result<&str> {
                document
                    .get_first(field)
                    .and_then(|value| value.as_str())
                    .context("current lexical stored field missing")
            };
            let path = field_text(self.index.fields.path)?;
            validate_path(path)?;
            ensure!(
                query
                    .path_prefix
                    .as_ref()
                    .is_none_or(|prefix| path.starts_with(prefix)),
                "current lexical candidate escaped path scope"
            );
            let local = document
                .get_first(self.index.fields.local)
                .and_then(|value| value.as_u64())
                .context("current lexical local ordinal missing")?;
            let stable_id = field_text(self.index.fields.stable_id)?.parse::<Sha256Digest>()?;
            ensure!(
                stable_id == self::stable_id(path, local)?,
                "current lexical candidate identity mismatch"
            );
            let sort_key = document
                .get_first(self.index.fields.sort_key)
                .and_then(|value| value.as_u64())
                .context("current lexical sort key missing")?;
            ensure!(
                sort_key == u64::from_be_bytes(stable_id.as_bytes()[..8].try_into()?),
                "current lexical sort key mismatch"
            );
            let hit: Hit = serde_json::from_str(field_text(self.index.fields.payload)?)?;
            ensure!(
                hit.citation.path == path
                    && hit.citation.start_line > 0
                    && hit.citation.start_line <= hit.citation.end_line
                    && hit.citation.content_hash.parse::<Sha256Digest>().is_ok()
                    && !hit.truncated,
                "invalid current lexical candidate citation"
            );
            values.push(CurrentLexicalCandidate {
                stable_id,
                local,
                hit,
            });
        }
        ensure!(
            {
                let mut seen = std::collections::BTreeSet::new();
                values
                    .iter()
                    .all(|candidate| seen.insert(candidate.stable_id))
            },
            "duplicate current lexical candidate identity"
        );
        Ok(CurrentLexicalCandidates {
            manifest_digest: self.ticket.digest(),
            values,
            incomplete,
        })
    }
}

impl CurrentLexical {
    /// Creates a new derived component. Existing state is never overwritten.
    pub fn create(directory: &Path, charge: Charge) -> Result<Self> {
        if directory.exists() {
            let metadata = fs::symlink_metadata(directory)?;
            ensure!(
                metadata.is_dir()
                    && !metadata.file_type().is_symlink()
                    && directory.read_dir()?.next().is_none(),
                "current lexical directory is not empty"
            );
        } else {
            fs::create_dir(directory)?;
        }
        let (schema, fields) = schema();
        let index = Index::create(
            BoundedDirectory::new(tantivy::directory::MmapDirectory::open(directory)?, charge),
            schema,
            tantivy::IndexSettings::default(),
        )?;
        Ok(Self {
            directory: directory.to_owned(),
            index,
            fields,
        })
    }

    pub fn open(directory: &Path, charge: Charge) -> Result<Self> {
        let metadata = fs::symlink_metadata(directory)?;
        ensure!(
            metadata.is_dir() && !metadata.file_type().is_symlink(),
            "current lexical directory is not an owned directory"
        );
        let index = Index::open(BoundedDirectory::new(
            tantivy::directory::MmapDirectory::open(directory)?,
            charge,
        ))?;
        let (expected, fields) = schema();
        ensure!(
            index.schema() == expected,
            "unsupported current lexical schema; rebuild index"
        );
        Ok(Self {
            directory: directory.to_owned(),
            index,
            fields,
        })
    }

    /// The owner must have captured and verified exact source/extraction bytes,
    /// reserved quota, and excluded readers. A failed external commit requires
    /// the owner's journal recovery before reads resume.
    pub fn apply_changes(&self, changes: &[LexicalChange<'_>]) -> Result<Option<LexicalWork>> {
        ensure!(
            changes.len() <= MAX_CHANGED_ARTIFACTS,
            "too many current lexical changes"
        );
        let mut previous = None;
        for change in changes {
            validate_path(change.path)?;
            ensure!(
                previous.is_none_or(|path| path < change.path),
                "current lexical changes are not sorted and unique"
            );
            previous = Some(change.path);
            if let Some(artifact) = change.artifact {
                ensure!(
                    artifact.path == change.path && artifact.chunks.len() <= MAX_PATH_DOCUMENTS,
                    "current lexical artifact binding or document bound failed"
                );
                artifact.content_hash.parse::<Sha256Digest>()?;
                ensure!(
                    artifact.chunks.iter().all(|chunk| chunk.start_line > 0
                        && chunk.start_line <= chunk.end_line
                        && chunk.text.len() <= airk_core::extraction::MAX_TEXT_BYTES),
                    "invalid current lexical chunk"
                );
            }
        }
        if changes.is_empty() {
            return Ok(None);
        }
        let mut writer = self
            .index
            .writer_with_num_threads::<TantivyDocument>(1, 20_000_000)?;
        let mut added_documents = 0u64;
        for change in changes {
            writer.delete_term(Term::from_field_text(self.fields.path, change.path));
            if let Some(artifact) = change.artifact {
                for (local, chunk) in artifact.chunks.iter().enumerate() {
                    let mut document = TantivyDocument::default();
                    for term in terms(&format!("{} {}", change.path, chunk.text)) {
                        document.add_text(self.fields.terms, term);
                    }
                    document.add_text(self.fields.path, change.path);
                    let id = stable_id(change.path, local as u64)?;
                    document.add_text(self.fields.stable_id, id.to_string());
                    document.add_u64(
                        self.fields.sort_key,
                        u64::from_be_bytes(id.as_bytes()[..8].try_into()?),
                    );
                    document.add_u64(self.fields.local, local as u64);
                    document.add_text(
                        self.fields.payload,
                        serde_json::to_string(&Hit {
                            citation: Citation {
                                path: change.path.to_owned(),
                                content_hash: artifact.content_hash.clone(),
                                start_line: chunk.start_line,
                                end_line: chunk.end_line,
                                location: chunk.location.clone(),
                            },
                            text: chunk.text.clone(),
                            truncated: false,
                        })?,
                    );
                    writer.add_document(document)?;
                    added_documents += 1;
                }
            }
        }
        let opstamp = writer.commit()?;
        writer.wait_merging_threads()?;
        tantivy::directory::Directory::sync_directory(&tantivy::directory::MmapDirectory::open(
            &self.directory,
        )?)?;
        Ok(Some(LexicalWork {
            changed_artifacts: changes.len(),
            added_documents,
            commit_opstamp: opstamp,
        }))
    }

    /// Bounded exact-path validation/read helper. Broad candidate retrieval
    /// and authenticated segment manifests are part of the runtime cutover.
    pub fn path_documents(&self, path: &str) -> Result<Vec<(Sha256Digest, Hit)>> {
        validate_path(path)?;
        let reader = self.index.reader()?;
        reader.reload()?;
        let searcher = reader.searcher();
        let query = TermQuery::new(
            Term::from_field_text(self.fields.path, path),
            IndexRecordOption::Basic,
        );
        let matches = searcher.search(
            &query,
            &TopDocs::with_limit(MAX_PATH_DOCUMENTS + 1).order_by_u64_field("local", Order::Asc),
        )?;
        ensure!(
            matches.len() <= MAX_PATH_DOCUMENTS,
            "current lexical path document bound exceeded"
        );
        let mut documents = Vec::with_capacity(matches.len());
        for (_, address) in matches {
            let document: TantivyDocument = searcher.doc(address)?;
            let field_text = |field: Field| -> Result<&str> {
                document
                    .get_first(field)
                    .and_then(|value| value.as_str())
                    .context("current lexical stored field missing")
            };
            ensure!(
                field_text(self.fields.path)? == path,
                "current lexical path mismatch"
            );
            let local = document
                .get_first(self.fields.local)
                .and_then(|value| value.as_u64())
                .context("current lexical local ordinal missing")?;
            let id = field_text(self.fields.stable_id)?.parse::<Sha256Digest>()?;
            ensure!(
                id == stable_id(path, local)?,
                "current lexical document identity mismatch"
            );
            let hit: Hit = serde_json::from_str(field_text(self.fields.payload)?)?;
            ensure!(
                hit.citation.path == path && !hit.truncated,
                "current lexical citation path or truncation mismatch"
            );
            documents.push((id, hit));
        }
        documents.sort_by_key(|(id, _)| *id);
        ensure!(
            documents.windows(2).all(|pair| pair[0].0 != pair[1].0),
            "duplicate current lexical document identity"
        );
        Ok(documents)
    }
}

#[cfg(test)]
mod tests;
