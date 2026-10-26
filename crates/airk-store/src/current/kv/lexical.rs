// SPDX-License-Identifier: Apache-2.0
//! Authenticated lexical state over ordered redb directories. Original document
//! locations and stable IDs retain the current public contract.
use super::{
    directory::{self, Budget, Root, Tree},
    posting::{Header, Membership, Postings},
    tables,
};
use crate::current::{
    CurrentScope,
    lexical::{CurrentLexicalCandidate, CurrentLexicalCandidates},
};
use airk_core::{
    Artifact, Citation, SearchQuery,
    identity::{Sha256Digest, framed_digest},
    lexical::bounded_terms,
};
use anyhow::{Context, Result, ensure};
use redb::ReadableTable;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet, BinaryHeap},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

pub const ENGINE: &str = "airk-kv/lexical-1";
pub const MAX_PATH_TERMS: usize = 262_144;
pub const MAX_PATH_TERM_DOCUMENTS: usize = 4_194_304;
pub const MAX_TERM_BYTES: usize = 32 << 20;
pub const MAX_PATH_TEXT_BYTES: usize = 64 << 20;
pub const MAX_DOCUMENT_BYTES: usize = 64 << 10;
pub const MAX_QUERY_TERMS: usize = 64;
pub const MAX_CANDIDATES: u64 = 262_144;
pub const MAX_PATH_DOCUMENTS: usize = 8192;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub version: u32,
    pub engine: String,
    pub scope: Sha256Digest,
    pub terms: Option<Root>,
    pub paths: Option<Root>,
    pub documents: Option<Root>,
    /// Allows the exhausted sentinel 2^32. Retired IDs are never reused.
    pub next_path: u64,
    pub updates: Sha256Digest,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Path {
    number: u32,
    chunks: u16,
    forward: Option<Root>,
}
impl Path {
    fn validate(&self, next: u64) -> Result<()> {
        ensure!(
            u64::from(self.number) < next && usize::from(self.chunks) <= MAX_PATH_DOCUMENTS,
            "invalid lexical path allocation/count"
        );
        if let Some(root) = &self.forward {
            root.validate()?;
            ensure!(
                root.count <= MAX_PATH_TERMS as u64 && self.chunks > 0,
                "invalid lexical forward count"
            );
        }
        Ok(())
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    local: u64,
    stable_id: Sha256Digest,
    citation: Citation,
}
fn scope_digest(scope: &CurrentScope) -> Sha256Digest {
    framed_digest(
        b"airk-kv-lexical-scope/1",
        &[
            scope.root.as_bytes(),
            scope.collection.as_bytes(),
            scope.incarnation.as_bytes(),
        ],
    )
}
impl Manifest {
    pub fn empty(scope: &CurrentScope) -> Result<Self> {
        scope.validate()?;
        Ok(Self {
            version: 3,
            engine: ENGINE.into(),
            scope: scope_digest(scope),
            terms: None,
            paths: None,
            documents: None,
            next_path: 0,
            updates: framed_digest(b"airk-kv-lexical-updates/1", &[]),
        })
    }
    pub fn canonical_bytes(&self) -> Result<Vec<u8>> {
        for root in [&self.terms, &self.paths, &self.documents]
            .into_iter()
            .flatten()
        {
            root.validate()?;
        }
        ensure!(
            self.version == 3 && self.engine == ENGINE && self.next_path <= 1u64 << 32,
            "unsupported redb lexical manifest"
        );
        let bytes = serde_json::to_vec(self)?;
        ensure!(
            bytes.len() <= crate::current::MAX_LEXICAL_MANIFEST_BYTES,
            "oversized lexical manifest"
        );
        Ok(bytes)
    }
    pub fn digest(&self) -> Result<Sha256Digest> {
        Ok(framed_digest(
            b"current-lexical-manifest/1",
            &[&self.canonical_bytes()?],
        ))
    }
    pub fn decode(bytes: &[u8], expected: Sha256Digest, scope: &CurrentScope) -> Result<Self> {
        ensure!(
            bytes.len() <= crate::current::MAX_LEXICAL_MANIFEST_BYTES,
            "oversized lexical manifest"
        );
        let result: Self = serde_json::from_slice(bytes)?;
        ensure!(
            result.canonical_bytes()? == bytes
                && result.digest()? == expected
                && result.scope == scope_digest(scope),
            "lexical manifest identity/digest mismatch"
        );
        Ok(result)
    }
}
pub trait ReadLexical: super::posting::ReadPostings {
    fn document(&self, id: u64, budget: &mut Budget) -> Result<Vec<u8>>;
}
pub trait WriteLexical: ReadLexical + super::posting::WritePostings {
    fn put_document(&self, id: u64, bytes: Option<&[u8]>) -> Result<()>;
}
impl WriteLexical for redb::WriteTransaction {
    fn put_document(&self, id: u64, bytes: Option<&[u8]>) -> Result<()> {
        let mut table = self.open_table(tables::LEX_DOCUMENT)?;
        if let Some(bytes) = bytes {
            ensure!(
                bytes.len() <= MAX_DOCUMENT_BYTES,
                "oversized lexical document"
            );
            table.insert(id, bytes)?;
        } else {
            table.remove(id)?;
        }
        Ok(())
    }
}
macro_rules! read_lexical {
    ($transaction:ty) => {
        impl ReadLexical for $transaction {
            fn document(&self, id: u64, budget: &mut Budget) -> Result<Vec<u8>> {
                ensure!(
                    budget.bytes >= MAX_DOCUMENT_BYTES as u64,
                    directory::Exhausted("document byte")
                );
                let table = self.open_table(tables::LEX_DOCUMENT)?;
                let row = table.get(id)?.context("lexical document missing")?;
                ensure!(
                    row.value().len() <= MAX_DOCUMENT_BYTES,
                    "oversized lexical document"
                );
                budget.bytes -= row.value().len() as u64;
                Ok(row.value().to_vec())
            }
        }
    };
}
read_lexical!(redb::ReadTransaction);
read_lexical!(redb::WriteTransaction);

pub struct Index<'a> {
    scope: &'a CurrentScope,
    terms: Tree,
    paths: Tree,
    documents: Tree,
}
fn term_id(term: &str) -> Sha256Digest {
    // Preserve the old long-term alias identity, followed by one fixed-width
    // key for both ordinary aliases and digested long aliases.
    let alias = if term.len() > 8000 {
        format!(
            "h{}",
            framed_digest(b"fts5-long-term/1", &[term.as_bytes()])
        )
    } else {
        format!("x{term}")
    };
    framed_digest(b"airk-kv-lexical-term/1", &[alias.as_bytes()])
}
fn decode<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T> {
    ensure!(
        bytes.len() <= directory::MAX_VALUE_BYTES,
        "oversized lexical header"
    );
    Ok(serde_json::from_slice(bytes)?)
}
impl<'a> Index<'a> {
    /// Complete cross-table verification for a candidate before its head is
    /// installed. Query proofs remain proportional to the evidence traversed.
    pub fn verify(
        &self,
        storage: &impl ReadLexical,
        manifest: &Manifest,
        budget: &mut Budget,
    ) -> Result<()> {
        self.validate(manifest)?;
        let mut allocated = BTreeMap::new();
        let mut documents = 0u64;
        self.paths.visit_budgeted(
            storage,
            manifest.paths.as_ref(),
            &[],
            None,
            budget,
            &mut |k, v, budget| {
                let path = std::str::from_utf8(k)?;
                ensure!(
                    crate::current::journal::valid_current_path(path),
                    "invalid authenticated lexical path"
                );
                let p: Path = decode(v)?;
                p.validate(manifest.next_path)?;
                ensure!(
                    u64::from(p.number) < manifest.next_path
                        && usize::from(p.chunks) <= MAX_PATH_DOCUMENTS
                        && allocated.len() < 1_000_000
                        && allocated
                            .insert(p.number, (p.chunks, p.forward.clone()))
                            .is_none(),
                    "invalid/duplicate lexical path allocation"
                );
                documents = documents
                    .checked_add(u64::from(p.chunks))
                    .context("lexical document count overflow")?;
                for local in 0..p.chunks {
                    self.document(
                        storage,
                        manifest,
                        (u64::from(p.number) << 16) | u64::from(local),
                        budget,
                    )?;
                }
                self.forward(p.number)?.visit_budgeted(
                    storage,
                    p.forward.as_ref(),
                    &[],
                    None,
                    budget,
                    &mut |term, value, budget| {
                        ensure!(
                            term.len() == 32 && value.is_empty(),
                            "invalid authenticated forward entry"
                        );
                        let term = Sha256Digest::from_bytes(term.try_into()?);
                        let header = self
                            .header(storage, manifest, term, budget)?
                            .context("forward term missing")?;
                        let first = Postings::new(self.scope, term)?.first_at_or_after(
                            storage,
                            &header,
                            u64::from(p.number) << 16,
                            budget,
                        )?;
                        ensure!(
                            first.is_some_and(|id| id >> 16 == u64::from(p.number)
                                && (id & 65535) < u64::from(p.chunks)),
                            "forward term has no path posting"
                        );
                        Ok(())
                    },
                )?;
                Ok(())
            },
        )?;
        ensure!(
            manifest.documents.as_ref().map_or(0, |r| r.count) == documents,
            "lexical document completeness mismatch"
        );
        self.terms.visit_budgeted(
            storage,
            manifest.terms.as_ref(),
            &[],
            None,
            budget,
            &mut |term, value, budget| {
                ensure!(term.len() == 32, "invalid authenticated term key");
                let term = Sha256Digest::from_bytes(term.try_into()?);
                let header: Header = decode(value)?;
                ensure!(
                    header.count > 0 && header.root.is_some(),
                    "invalid live posting header"
                );
                let mut last_path = None;
                Postings::new(self.scope, term)?.visit_budgeted(
                    storage,
                    &header,
                    budget,
                    &mut |id, budget| {
                        // Every expected document was authenticated above and
                        // the directory count proves there are no extra IDs.
                        // Posting IDs are ordered, so verify forward membership
                        // once per term/path instead of rereading each document
                        // and its path for every occurrence of a common term.
                        let number = u32::try_from(id >> 16).context("posting path ID overflow")?;
                        let (chunks, forward) =
                            allocated.get(&number).context("posting path missing")?;
                        ensure!(
                            (id & 65535) < u64::from(*chunks),
                            "posting document missing"
                        );
                        if last_path != Some(number) {
                            ensure!(
                                self.forward(number)?
                                    .get(storage, forward.as_ref(), term.as_bytes(), budget)?
                                    .is_some_and(|value| value.is_empty()),
                                "posting term missing from forward list"
                            );
                            last_path = Some(number);
                        }
                        Ok(())
                    },
                )
            },
        )
    }
    pub fn new(scope: &'a CurrentScope) -> Result<Self> {
        Ok(Self {
            scope,
            terms: Tree::new(scope, b"terms/1")?,
            paths: Tree::new(scope, b"paths/1")?,
            documents: Tree::new(scope, b"documents/1")?,
        })
    }
    fn validate(&self, manifest: &Manifest) -> Result<()> {
        manifest.canonical_bytes()?;
        ensure!(
            manifest.scope == scope_digest(self.scope),
            "foreign lexical scope"
        );
        Ok(())
    }
    fn forward(&self, number: u32) -> Result<Tree> {
        let mut domain = b"forward/1/".to_vec();
        domain.extend_from_slice(&number.to_be_bytes());
        Tree::new(self.scope, &domain)
    }
    fn path(
        &self,
        storage: &impl ReadLexical,
        manifest: &Manifest,
        path: &str,
        budget: &mut Budget,
    ) -> Result<Option<Path>> {
        self.paths
            .get(storage, manifest.paths.as_ref(), path.as_bytes(), budget)?
            .map(|b| {
                let p: Path = decode(&b)?;
                p.validate(manifest.next_path)?;
                Ok(p)
            })
            .transpose()
    }
    fn header(
        &self,
        storage: &impl directory::ReadPages,
        manifest: &Manifest,
        term: Sha256Digest,
        budget: &mut Budget,
    ) -> Result<Option<Header>> {
        self.terms
            .get(storage, manifest.terms.as_ref(), term.as_bytes(), budget)?
            .map(|b| {
                let header: Header = decode(&b)?;
                ensure!(
                    header.count > 0 && header.root.is_some(),
                    "invalid live posting header"
                );
                Ok(header)
            })
            .transpose()
    }
    /// Mutate inside a caller-owned unpublished transaction. On error the
    /// caller must abort that transaction; the supplied manifest stays intact.
    pub fn replace(
        &self,
        tx: &impl WriteLexical,
        manifest: &mut Manifest,
        path: &str,
        artifact: Option<&Artifact>,
        budget: &mut Budget,
    ) -> Result<()> {
        self.validate(manifest)?;
        ensure!(
            crate::current::journal::valid_current_path(path),
            "invalid lexical path"
        );
        let mut next = manifest.clone();
        let previous = self.path(tx, &next, path, budget)?;
        let number = match &previous {
            Some(p) => p.number,
            None if artifact.is_none() => return Ok(()),
            None => {
                let n = u32::try_from(next.next_path)
                    .context("lexical path IDs exhausted; rebuild required")?;
                next.next_path += 1;
                n
            }
        };
        let forward = self.forward(number)?;
        let mut old_terms = BTreeSet::new();
        let mut new_terms: BTreeMap<Sha256Digest, Vec<u64>> = BTreeMap::new();
        let mut pairs = 0usize;
        if let Some(previous) = &previous {
            forward.visit(
                tx,
                previous.forward.as_ref(),
                &[],
                None,
                budget,
                &mut |k, v| {
                    ensure!(
                        k.len() == 32 && v.is_empty() && old_terms.len() < MAX_PATH_TERMS,
                        "invalid forward page"
                    );
                    old_terms.insert(Sha256Digest::from_bytes(k.try_into()?));
                    Ok(())
                },
            )?;
            forward.clear(tx, previous.forward.as_ref(), budget)?;
            for local in 0..previous.chunks {
                let id = (u64::from(number) << 16) | u64::from(local);
                self.document(tx, &next, id, budget)?;
                next.documents = self.documents.put(
                    tx,
                    next.documents.as_ref(),
                    &id.to_be_bytes(),
                    None,
                    budget,
                )?;
                tx.put_document(id, None)?;
            }
        }
        next.paths = self
            .paths
            .put(tx, next.paths.as_ref(), path.as_bytes(), None, budget)?;
        if let Some(artifact) = artifact {
            ensure!(
                artifact.path == path && artifact.chunks.len() <= MAX_PATH_DOCUMENTS,
                "invalid lexical artifact binding/count"
            );
            let text_bytes = artifact.chunks.iter().try_fold(0usize, |n, c| {
                n.checked_add(c.text.len())
                    .context("lexical text length overflow")
            })?;
            ensure!(
                text_bytes <= MAX_PATH_TEXT_BYTES,
                "lexical path text budget exceeded"
            );
            for (local, chunk) in artifact.chunks.iter().enumerate() {
                let local = local as u64;
                let id = (u64::from(number) << 16) | local;
                let document = Document {
                    local,
                    stable_id: crate::current::lexical::stable_id(path, local)?,
                    citation: Citation {
                        path: path.into(),
                        content_hash: artifact.content_hash.clone(),
                        start_line: chunk.start_line,
                        end_line: chunk.end_line,
                        location: chunk.location.clone(),
                    },
                };
                let bytes = serde_json::to_vec(&document)?;
                ensure!(
                    bytes.len() <= MAX_DOCUMENT_BYTES,
                    "lexical document budget exceeded"
                );
                let digest = framed_digest(b"airk-kv-lexical-document/1", &[&bytes]);
                tx.put_document(id, Some(&bytes))?;
                next.documents = self.documents.put(
                    tx,
                    next.documents.as_ref(),
                    &id.to_be_bytes(),
                    Some(digest.as_bytes()),
                    budget,
                )?;
                let text = format!("{path} {}", chunk.text);
                for term in bounded_terms(&text, MAX_PATH_TERMS, MAX_TERM_BYTES)? {
                    let term = term_id(&term);
                    ensure!(
                        new_terms.contains_key(&term) || new_terms.len() < MAX_PATH_TERMS,
                        "lexical forward term budget exceeded"
                    );
                    ensure!(
                        pairs < MAX_PATH_TERM_DOCUMENTS,
                        "lexical term/document pair budget exceeded"
                    );
                    pairs += 1;
                    new_terms.entry(term).or_default().push(id);
                }
            }
            let root = forward.build(
                tx,
                new_terms
                    .keys()
                    .map(|term| (term.as_bytes().to_vec(), Vec::new())),
                budget,
            )?;
            let bytes = serde_json::to_vec(&Path {
                number,
                chunks: artifact.chunks.len() as u16,
                forward: root,
            })?;
            next.paths = self.paths.put(
                tx,
                next.paths.as_ref(),
                path.as_bytes(),
                Some(&bytes),
                budget,
            )?;
        }
        let affected: BTreeSet<_> = old_terms.iter().chain(new_terms.keys()).copied().collect();
        for term in affected {
            let existing = self.header(tx, &next, term, budget)?;
            ensure!(
                !old_terms.contains(&term) || existing.is_some(),
                "forward term missing"
            );
            let mut header = existing.unwrap_or_default();
            let ids = new_terms.get(&term).map(Vec::as_slice).unwrap_or(&[]);
            let removed = Postings::new(self.scope, term)?.replace_range(
                tx,
                &mut header,
                u64::from(number) << 16,
                (u64::from(number) + 1) << 16,
                ids,
                budget,
            )?;
            if old_terms.contains(&term) {
                let chunks = u64::from(previous.as_ref().context("missing previous path")?.chunks);
                ensure!(
                    !removed.is_empty() && removed.iter().all(|d| (d & 65535) < chunks),
                    "forward/posting path count mismatch"
                );
            } else {
                ensure!(removed.is_empty(), "posting missing from forward list");
            }
            let bytes = if header.count == 0 {
                None
            } else {
                Some(serde_json::to_vec(&header)?)
            };
            next.terms = self.terms.put(
                tx,
                next.terms.as_ref(),
                term.as_bytes(),
                bytes.as_deref(),
                budget,
            )?;
        }
        next.updates = framed_digest(
            b"airk-kv-lexical-updates/1",
            &[
                manifest.updates.as_bytes(),
                path.as_bytes(),
                &next.next_path.to_be_bytes(),
                &serde_json::to_vec(&next.paths)?,
                &serde_json::to_vec(&next.documents)?,
                &serde_json::to_vec(&next.terms)?,
            ],
        );
        *manifest = next;
        Ok(())
    }
    fn document(
        &self,
        storage: &impl ReadLexical,
        manifest: &Manifest,
        id: u64,
        budget: &mut Budget,
    ) -> Result<Document> {
        let expected = self
            .documents
            .get(
                storage,
                manifest.documents.as_ref(),
                &id.to_be_bytes(),
                budget,
            )?
            .context("lexical document proof missing")?;
        ensure!(expected.len() == 32, "invalid lexical document digest");
        let bytes = storage.document(id, budget)?;
        ensure!(
            framed_digest(b"airk-kv-lexical-document/1", &[&bytes]).as_bytes()
                == expected.as_slice(),
            "lexical document digest mismatch"
        );
        let document: Document = serde_json::from_slice(&bytes)?;
        ensure!(
            document.local == id & 65535
                && document.stable_id
                    == crate::current::lexical::stable_id(&document.citation.path, document.local)?,
            "lexical document identity mismatch"
        );
        let p = self
            .path(storage, manifest, &document.citation.path, budget)?
            .context("lexical document path missing")?;
        ensure!(
            id >> 16 == u64::from(p.number) && document.local < u64::from(p.chunks),
            "lexical document path binding mismatch"
        );
        Ok(document)
    }
    pub fn search(
        &self,
        storage: &impl ReadLexical,
        manifest: &Manifest,
        query: &SearchQuery,
        work: &mut QueryWork<'_>,
    ) -> Result<CurrentLexicalCandidates> {
        self.validate(manifest)?;
        query.validate()?;
        work.check()?;
        let terms = bounded_terms(&query.text, MAX_QUERY_TERMS, 16_384)
            .context(directory::Exhausted("query normalization"))?;
        ensure!(!terms.is_empty(), "query must contain a letter or number");
        let mut headers = Vec::new();
        for term in terms {
            work.check()?;
            let term = term_id(&term);
            let Some(header) = self.header(storage, manifest, term, &mut work.budget)? else {
                return Ok(CurrentLexicalCandidates {
                    manifest_digest: manifest.digest()?,
                    values: vec![],
                    incomplete: false,
                });
            };
            headers.push((term, header));
        }
        headers.sort_by_key(|(_, h)| h.count);
        let mut memberships = headers[1..]
            .iter()
            .map(|(term, header)| Membership::new(self.scope, *term, header.clone()))
            .collect::<Result<Vec<_>>>()?;
        let mut heap = BinaryHeap::new();
        let mut candidates = work.candidates;
        let started = work.started;
        let deadline = work.deadline;
        let cancelled = work.cancelled;
        Postings::new(self.scope, headers[0].0)?.visit_budgeted(
            storage,
            &headers[0].1,
            &mut work.budget,
            &mut |doc, budget| {
                check(started, deadline, cancelled)?;
                candidates = candidates
                    .checked_sub(1)
                    .ok_or(directory::Exhausted("candidate"))?;
                for membership in &mut memberships {
                    if !membership.contains(storage, doc, budget)? {
                        return Ok(());
                    }
                }
                let d = self.document(storage, manifest, doc, budget)?;
                if query
                    .path_prefix
                    .as_ref()
                    .is_some_and(|prefix| !d.citation.path.starts_with(prefix))
                {
                    return Ok(());
                }
                heap.push(Hit(CurrentLexicalCandidate {
                    stable_id: d.stable_id,
                    local: d.local,
                    citation: d.citation,
                }));
                if heap.len() > query.limit + 1 {
                    heap.pop();
                }
                Ok(())
            },
        )?;
        work.candidates = candidates;
        work.check()?;
        let mut values: Vec<_> = heap.into_sorted_vec().into_iter().map(|h| h.0).collect();
        let incomplete = values.len() > query.limit;
        values.truncate(query.limit);
        Ok(CurrentLexicalCandidates {
            manifest_digest: manifest.digest()?,
            values,
            incomplete,
        })
    }
}
struct Hit(CurrentLexicalCandidate);
impl PartialEq for Hit {
    fn eq(&self, other: &Self) -> bool {
        self.0.stable_id == other.0.stable_id
    }
}
impl Eq for Hit {}
impl PartialOrd for Hit {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Hit {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.stable_id.cmp(&other.0.stable_id)
    }
}
pub struct QueryWork<'a> {
    pub budget: Budget,
    pub candidates: u64,
    pub deadline: Duration,
    pub cancelled: &'a AtomicBool,
    started: Instant,
}
#[derive(Debug)]
pub enum Interrupted {
    Deadline,
    Cancelled,
}
impl std::fmt::Display for Interrupted {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "lexical query {:?}", self)
    }
}
impl std::error::Error for Interrupted {}
fn check(started: Instant, deadline: Duration, cancelled: &AtomicBool) -> Result<()> {
    ensure!(!cancelled.load(Ordering::Relaxed), Interrupted::Cancelled);
    ensure!(started.elapsed() < deadline, Interrupted::Deadline);
    Ok(())
}
impl<'a> QueryWork<'a> {
    pub fn new(cancelled: &'a AtomicBool) -> Self {
        Self {
            budget: Budget::query(),
            candidates: MAX_CANDIDATES,
            deadline: Duration::from_secs(5),
            cancelled,
            started: Instant::now(),
        }
    }
    fn check(&self) -> Result<()> {
        check(self.started, self.deadline, self.cancelled)
    }
}

#[cfg(test)]
mod tests {
    use super::super::Database;
    use super::*;
    use airk_core::{Chunk, DocumentLocation, identity::Incarnation};
    fn scope() -> CurrentScope {
        CurrentScope {
            root: "fixture/lexical".into(),
            collection: "lexical".into(),
            incarnation: Incarnation::from_bytes([7; 16]),
        }
    }
    fn write_budget() -> Budget {
        Budget {
            pages: 1_000_000,
            bytes: 8 << 30,
            entries: 64_000_000,
        }
    }
    fn artifact(path: &str, texts: &[&str]) -> Artifact {
        Artifact {
            path: path.into(),
            content_hash: Sha256Digest::from_bytes([4; 32]).to_string(),
            bytes: texts.iter().map(|s| s.len()).sum(),
            chunks: texts
                .iter()
                .enumerate()
                .map(|(i, text)| Chunk {
                    start_line: i + 1,
                    end_line: i + 1,
                    text: (*text).into(),
                    location: DocumentLocation::Lines {
                        start: i + 1,
                        end: i + 1,
                    },
                })
                .collect(),
            language: "rust".into(),
            extraction_method: "fixture".into(),
            parse_status: "ok".into(),
            facts: vec![],
            retained: None,
        }
    }
    fn query(text: &str, prefix: Option<&str>, limit: usize) -> SearchQuery {
        SearchQuery {
            text: text.into(),
            path_prefix: prefix.map(str::to_owned),
            limit,
            max_bytes: 65_536,
        }
    }
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Baseline {
        version: u32,
        sqlite_version: String,
        batches: Vec<Vec<BaselineCase>>,
    }
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct BaselineCase {
        text: String,
        prefix: Option<String>,
        limit: usize,
        values: Vec<(String, String)>,
        incomplete: bool,
    }
    #[test]
    fn replacements_deletions_and_retained_queries_match_fts5_candidates_and_citations()
    -> Result<()> {
        let scope = scope();
        let index = Index::new(&scope)?;
        let file = tempfile::NamedTempFile::new()?;
        let db = Database::open(file.reopen()?, 64 << 20, true)?;
        let mut manifest = Manifest::empty(&scope)?;
        let fixture: Baseline = serde_json::from_slice(include_bytes!(
            "../../../tests/fixtures/lexical-fts5-3.53.3.json"
        ))?;
        assert_eq!(fixture.version, 1);
        assert_eq!(fixture.sqlite_version, "3.53.3");
        assert_eq!(fixture.batches.len(), 5);
        let cancelled = AtomicBool::new(false);
        for batch in 0..5 {
            let old = db.snapshot()?;
            let old_manifest = manifest.clone();
            let tx = db.write()?;
            for n in 0..12 {
                let path = format!("src/{:02}_CamelCase.rs", (n * 7 + batch * 3) % 19);
                let content = format!(
                    "shared café λ snake_case common{batch} {}",
                    "z".repeat(9001)
                );
                let a = artifact(&path, &[&content, "shared second CamelCase"]);
                let a = (batch == 0 || n % 4 != 0).then_some(&a);
                index.replace(&tx, &mut manifest, &path, a, &mut write_budget())?;
            }
            tx.commit()?;
            let snapshot = db.snapshot()?;
            index.verify(snapshot.transaction(), &manifest, &mut write_budget())?;
            for expected in &fixture.batches[batch] {
                let q = query(&expected.text, expected.prefix.as_deref(), expected.limit);
                let actual = index.search(
                    snapshot.transaction(),
                    &manifest,
                    &q,
                    &mut QueryWork::new(&cancelled),
                )?;
                assert_eq!(actual.incomplete, expected.incomplete);
                let values = actual
                    .values
                    .into_iter()
                    .map(|c| Ok((c.stable_id.to_string(), serde_json::to_string(&c.citation)?)))
                    .collect::<Result<Vec<_>>>()?;
                assert_eq!(values, expected.values);
            }
            // Old read transactions retain the pages retired by replacement.
            index.search(
                old.transaction(),
                &old_manifest,
                &query("shared", None, 100),
                &mut QueryWork::new(&cancelled),
            )?;
            assert_eq!(
                Manifest::decode(&manifest.canonical_bytes()?, manifest.digest()?, &scope)?,
                manifest
            );
        }
        Ok(())
    }
    #[test]
    fn forged_citations_missing_pages_and_query_exhaustion_fail_without_partial_results()
    -> Result<()> {
        let scope = scope();
        let index = Index::new(&scope)?;
        let file = tempfile::NamedTempFile::new()?;
        let db = Database::open(file.reopen()?, 32 << 20, true)?;
        let mut manifest = Manifest::empty(&scope)?;
        let tx = db.write()?;
        index.replace(
            &tx,
            &mut manifest,
            "a.rs",
            Some(&artifact("a.rs", &["shared word"])),
            &mut write_budget(),
        )?;
        tx.commit()?;
        let cancelled = AtomicBool::new(false);
        let q = query("shared", None, 10);
        let snapshot = db.snapshot()?;
        let mut work = QueryWork::new(&cancelled);
        work.candidates = 0;
        assert!(
            index
                .search(snapshot.transaction(), &manifest, &q, &mut work)
                .unwrap_err()
                .is::<directory::Exhausted>()
        );
        let mut work = QueryWork::new(&cancelled);
        work.budget.pages = 0;
        assert!(
            index
                .search(snapshot.transaction(), &manifest, &q, &mut work)
                .unwrap_err()
                .is::<directory::Exhausted>()
        );
        let mut work = QueryWork::new(&cancelled);
        work.deadline = Duration::ZERO;
        assert!(
            index
                .search(snapshot.transaction(), &manifest, &q, &mut work)
                .unwrap_err()
                .is::<Interrupted>()
        );
        cancelled.store(true, Ordering::Relaxed);
        assert!(
            index
                .search(
                    snapshot.transaction(),
                    &manifest,
                    &q,
                    &mut QueryWork::new(&cancelled)
                )
                .unwrap_err()
                .is::<Interrupted>()
        );
        cancelled.store(false, Ordering::Relaxed);
        let tx = db.write()?;
        tx.open_table(tables::LEX_DOCUMENT)?
            .insert(0, b"{}".as_slice())?;
        tx.commit()?;
        assert!(
            index
                .search(
                    db.snapshot()?.transaction(),
                    &manifest,
                    &q,
                    &mut QueryWork::new(&cancelled)
                )
                .is_err()
        );
        index.search(
            snapshot.transaction(),
            &manifest,
            &q,
            &mut QueryWork::new(&cancelled),
        )?;
        let tx = db.write()?;
        tx.open_table(tables::LEX_DIRECTORY)?.remove(
            manifest
                .terms
                .as_ref()
                .unwrap()
                .digest
                .as_bytes()
                .as_slice(),
        )?;
        tx.commit()?;
        assert!(
            index
                .search(
                    db.snapshot()?.transaction(),
                    &manifest,
                    &query("absent", None, 10),
                    &mut QueryWork::new(&cancelled)
                )
                .is_err()
        );
        Ok(())
    }
    #[test]
    fn path_allocator_is_monotonic_and_exhaustion_preserves_manifest() -> Result<()> {
        let scope = scope();
        let index = Index::new(&scope)?;
        let file = tempfile::NamedTempFile::new()?;
        let db = Database::open(file.reopen()?, 32 << 20, true)?;
        let mut manifest = Manifest::empty(&scope)?;
        let tx = db.write()?;
        index.replace(
            &tx,
            &mut manifest,
            "a.rs",
            Some(&artifact("a.rs", &["one"])),
            &mut write_budget(),
        )?;
        index.replace(&tx, &mut manifest, "a.rs", None, &mut write_budget())?;
        index.replace(
            &tx,
            &mut manifest,
            "b.rs",
            Some(&artifact("b.rs", &["two"])),
            &mut write_budget(),
        )?;
        assert_eq!(
            index
                .path(&tx, &manifest, "b.rs", &mut write_budget())?
                .unwrap()
                .number,
            1
        );
        manifest.next_path = 1u64 << 32;
        let previous = manifest.clone();
        assert!(
            index
                .replace(
                    &tx,
                    &mut manifest,
                    "c.rs",
                    Some(&artifact("c.rs", &["three"])),
                    &mut write_budget()
                )
                .is_err()
        );
        assert_eq!(manifest, previous);
        tx.abort()?;
        Ok(())
    }
    #[test]
    fn forward_lists_exceed_chunk_bound_and_missing_forward_pages_block_replacement() -> Result<()>
    {
        let scope = scope();
        let index = Index::new(&scope)?;
        let file = tempfile::NamedTempFile::new()?;
        let db = Database::open(file.reopen()?, 64 << 20, true)?;
        let text = (0..9000)
            .map(|n| format!("word{n}"))
            .collect::<Vec<_>>()
            .join(" ");
        let mut manifest = Manifest::empty(&scope)?;
        let tx = db.write()?;
        index.replace(
            &tx,
            &mut manifest,
            "a.rs",
            Some(&artifact("a.rs", &[&text])),
            &mut write_budget(),
        )?;
        let path = index
            .path(&tx, &manifest, "a.rs", &mut write_budget())?
            .unwrap();
        assert!(path.forward.as_ref().unwrap().count > 8192);
        tx.commit()?;
        let snapshot = db.snapshot()?;
        let cancelled = AtomicBool::new(false);
        let result = index.search(
            snapshot.transaction(),
            &manifest,
            &query("word8999", None, 10),
            &mut QueryWork::new(&cancelled),
        )?;
        assert_eq!(result.values.len(), 1);
        let tx = db.write()?;
        tx.open_table(tables::LEX_DIRECTORY)?
            .remove(path.forward.unwrap().digest.as_bytes().as_slice())?;
        tx.commit()?;
        let tx = db.write()?;
        let before = manifest.clone();
        assert!(
            index
                .replace(
                    &tx,
                    &mut manifest,
                    "a.rs",
                    Some(&artifact("a.rs", &["replacement"])),
                    &mut write_budget()
                )
                .is_err()
        );
        assert_eq!(manifest, before);
        tx.abort()?;
        Ok(())
    }
}
