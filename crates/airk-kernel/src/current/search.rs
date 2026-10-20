// SPDX-License-Identifier: Apache-2.0
//! One request gate for lexical selection, current DiskANN and exact citations.
use super::{MAX_LEXICAL_VALIDATION_BYTES, open_selected_lexical};
use airk_core::{Hit, SearchQuery, identity::Sha256Digest};
use airk_retrieval::{
    bounded::Charge,
    current_lexical::{CurrentLexicalCandidate, stable_id},
    model::EmbeddingModel,
    terms, truncate_utf8,
    vector::mutable::MutableDiskIndex,
};
use airk_store::{
    FileStore,
    current::{
        CurrentRead, CurrentScope, CurrentToken, map::MapDomain, postings::Lookup,
        vector_map::OccurrencePointer,
    },
};
use anyhow::{Context, Result, ensure};
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

const BRANCH_CANDIDATES: usize = 100;
const MAX_SCOPED_PATHS: usize = 4096;
const MAX_SCOPED_NODES: usize = 4096;
const MAX_POINTERS: usize = 16_384;
const MAX_MEMBERS: usize = 65_536;
const MAX_POINTER_BYTES: usize = 64 << 20;

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CurrentSearchMode {
    Lexical,
    Vector,
    Hybrid,
}

#[derive(Serialize)]
pub struct CurrentSearchEvidence {
    pub token: CurrentToken,
    pub collection: String,
    pub mode: CurrentSearchMode,
    pub hits: Vec<Hit>,
    pub truncated: bool,
}

#[derive(Clone)]
enum Candidate {
    Lexical(CurrentLexicalCandidate),
    Vector(Arc<OccurrencePointer>),
}

struct Branch {
    values: Vec<(Sha256Digest, usize, Candidate)>,
    incomplete: bool,
}

#[derive(Default)]
struct PointerBudget {
    pointers: usize,
    members: usize,
    bytes: usize,
}

impl PointerBudget {
    fn admit(&mut self, pointer: &OccurrencePointer) -> Result<bool> {
        let bytes = pointer.canonical_bytes()?.len();
        if self.pointers == MAX_POINTERS
            || self.members + pointer.members.len() > MAX_MEMBERS
            || self.bytes + bytes > MAX_POINTER_BYTES
        {
            return Ok(false);
        }
        self.pointers += 1;
        self.members += pointer.members.len();
        self.bytes += bytes;
        Ok(true)
    }
}

/// All engine selection, exact-token validation, retained model loading and
/// evidence rendering take place under one shared current gate. No handle is
/// retained after the response; an expired token fails before query inference.
pub fn search_current(
    store: &FileStore,
    scope: &CurrentScope,
    expected: Option<&CurrentToken>,
    mode: CurrentSearchMode,
    query: &SearchQuery,
    charge: Charge,
) -> Result<CurrentSearchEvidence> {
    query.validate()?;
    let read = store.current_read(scope)?;
    if let Some(expected) = expected {
        read.require_token(expected)?;
    }
    let mut branches = Vec::new();
    if matches!(mode, CurrentSearchMode::Lexical | CurrentSearchMode::Hybrid) {
        let lexical = open_selected_lexical(store, &read, charge)?;
        let mut selection = query.clone();
        selection.limit = if matches!(mode, CurrentSearchMode::Hybrid) {
            BRANCH_CANDIDATES
        } else {
            query.limit
        };
        let candidates = lexical.search(&selection)?;
        ensure!(
            candidates.manifest_digest == read.head().lexical_commit,
            "current lexical candidate manifest mismatch"
        );
        branches.push(Branch {
            values: candidates
                .values
                .into_iter()
                .enumerate()
                .map(|(rank, candidate)| (candidate.stable_id, rank, Candidate::Lexical(candidate)))
                .collect(),
            incomplete: candidates.incomplete,
        });
    }
    if matches!(mode, CurrentSearchMode::Vector | CurrentSearchMode::Hybrid) {
        let mut selection = query.clone();
        if matches!(mode, CurrentSearchMode::Hybrid) {
            selection.limit = BRANCH_CANDIDATES;
        }
        branches.push(vector_candidates(store, &read, &selection)?);
    }
    let mut incomplete = branches.iter().any(|branch| branch.incomplete);
    // The same lexical chunk receives one vote from each branch. Semantic
    // passages can span multiple chunks; their occurrence is emitted only once.
    let mut merged = BTreeMap::<Sha256Digest, (f64, Candidate)>::new();
    for branch in branches {
        for (identity, rank, candidate) in branch.values {
            let score = 1.0 / (61.0 + rank as f64);
            match merged.entry(identity) {
                std::collections::btree_map::Entry::Vacant(entry) => {
                    entry.insert((score, candidate));
                }
                std::collections::btree_map::Entry::Occupied(mut entry) => {
                    entry.get_mut().0 += score;
                    if matches!(candidate, Candidate::Vector(_)) {
                        entry.get_mut().1 = candidate;
                    }
                }
            }
        }
    }
    let mut ranked: Vec<_> = merged.into_iter().collect();
    ranked.sort_by(|a, b| b.1.0.total_cmp(&a.1.0).then(a.0.cmp(&b.0)));
    let mut emitted = BTreeSet::new();
    let mut hits = Vec::new();
    let mut remaining = query.max_bytes;
    let mut read_bytes = 0u64;
    for (_, (_, candidate)) in ranked {
        if let Candidate::Vector(pointer) = &candidate
            && !emitted.insert(pointer.occurrence_id)
        {
            continue;
        }
        if hits.len() == query.limit || remaining == 0 {
            incomplete = true;
            break;
        }
        let semantic = matches!(candidate, Candidate::Vector(_));
        let mut hit = materialize(store, &read, query, candidate, &mut read_bytes)?;
        if semantic && hit.text.len() > remaining {
            let span = airk_retrieval::focus_span(&hit.text, &query.text, remaining);
            if matches!(
                hit.citation.location,
                airk_core::DocumentLocation::Lines { .. }
            ) {
                let first = hit.citation.start_line
                    + hit.text[..span.start]
                        .bytes()
                        .filter(|b| *b == b'\n')
                        .count();
                let last = first
                    + hit.text.as_bytes()[span.start..span.end.saturating_sub(1)]
                        .iter()
                        .filter(|b| **b == b'\n')
                        .count();
                hit.citation.start_line = first;
                hit.citation.end_line = last;
                hit.citation.location = airk_core::DocumentLocation::Lines {
                    start: first,
                    end: last,
                };
            }
            hit.text = hit.text[span.start..span.end].to_owned();
            hit.truncated = true;
        }
        let (text, cut) = truncate_utf8(&hit.text, remaining);
        remaining -= text.len();
        hit.text = text;
        hit.truncated |= cut;
        incomplete |= hit.truncated;
        hits.push(hit);
    }
    Ok(CurrentSearchEvidence {
        token: read.token()?,
        collection: scope.collection.clone(),
        mode,
        hits,
        truncated: incomplete,
    })
}

fn vector_candidates(
    store: &FileStore,
    read: &Arc<CurrentRead>,
    query: &SearchQuery,
) -> Result<Branch> {
    let state = read
        .collection_state()?
        .context("current collection configuration missing")?;
    let objects = state
        .configuration
        .model
        .context("semantic retrieval not ready: no indexed model")?;
    let model = EmbeddingModel::open_objects(store.directory(), &objects)?;
    let vector = model
        .embed_text(&query.text)?
        .context("semantic retrieval not ready: no recognized model tokens")?;
    let graph = MutableDiskIndex::open(read.clone(), model.dimensions(), objects.pipeline_digest)?;
    let wanted = (query.limit * 4).clamp(64, 1000);
    let mut budget = PointerBudget::default();
    let mut incomplete = false;
    let mut selected = Vec::<Arc<OccurrencePointer>>::new();
    if let Some(prefix) = query
        .path_prefix
        .as_deref()
        .filter(|prefix| !prefix.is_empty())
    {
        let paths = read.lookup_paths(Lookup::Files, prefix, None, MAX_SCOPED_PATHS)?;
        incomplete |= paths.next_after.is_some();
        let mut scoped = BTreeMap::<u32, Vec<Arc<OccurrencePointer>>>::new();
        'paths: for path in paths.paths {
            let Some(index) = read.path_occurrences(&path)? else {
                continue;
            };
            for ordinal in 0..index.count {
                let id = index.occurrence_id(&read.head().scope, ordinal)?;
                let bytes = read
                    .map_get(MapDomain::Occurrence, id.as_bytes())?
                    .context("current scoped occurrence missing")?;
                let pointer = OccurrencePointer::decode(&bytes, id)?;
                ensure!(
                    pointer.path == path
                        && pointer.path_ordinal == ordinal
                        && pointer.source_hash == index.source_hash,
                    "current scoped occurrence identity mismatch"
                );
                let binding = read
                    .vector_binding(pointer.embedding_key)?
                    .context("current scoped binding missing")?;
                ensure!(
                    binding.occurrence_ids.binary_search(&id).is_ok(),
                    "current scoped binding omits occurrence"
                );
                if (!scoped.contains_key(&binding.graph_id) && scoped.len() == MAX_SCOPED_NODES)
                    || !budget.admit(&pointer)?
                {
                    incomplete = true;
                    break 'paths;
                }
                scoped
                    .entry(binding.graph_id)
                    .or_default()
                    .push(Arc::new(pointer));
            }
        }
        let ids = scoped.keys().copied().collect::<Vec<_>>();
        let ranked = graph.search_subset(&vector, &ids, wanted)?;
        incomplete |= ranked.hits.len() < scoped.len();
        for (id, _) in ranked.hits {
            selected.extend(
                scoped
                    .remove(&id)
                    .context("current scoped ranking mismatch")?,
            );
        }
    } else {
        let ranked = graph.search(&vector, wanted)?;
        incomplete |= ranked.partial || (ranked.hits.len() as u64) < graph.counts()?.live_nodes;
        'nodes: for (id, _) in ranked.hits {
            let binding = read.vector_for_graph_id(id)?;
            let mut pointers = read
                .vector_occurrence_pointers(binding.embedding_key)?
                .context("current candidate occurrences missing")?;
            pointers.sort_by(|a, b| {
                a.path
                    .cmp(&b.path)
                    .then(a.path_ordinal.cmp(&b.path_ordinal))
            });
            for pointer in pointers {
                if !budget.admit(&pointer)? {
                    incomplete = true;
                    break 'nodes;
                }
                selected.push(Arc::new(pointer));
            }
        }
    }
    let mut seen = BTreeSet::new();
    let mut values = Vec::new();
    for (rank, pointer) in selected.into_iter().enumerate() {
        if rank == BRANCH_CANDIDATES {
            incomplete = true;
            break;
        }
        for member in &pointer.members {
            let identity = stable_id(&pointer.path, u64::from(member.chunk_index))?;
            if seen.insert(identity) {
                values.push((identity, rank, Candidate::Vector(pointer.clone())));
            }
        }
    }
    Ok(Branch { values, incomplete })
}

fn materialize(
    store: &FileStore,
    read: &CurrentRead,
    query: &SearchQuery,
    candidate: Candidate,
    read_bytes: &mut u64,
) -> Result<Hit> {
    let path = match &candidate {
        Candidate::Lexical(c) => &c.hit.citation.path,
        Candidate::Vector(p) => &p.path,
    };
    ensure!(
        query
            .path_prefix
            .as_ref()
            .is_none_or(|prefix| path.starts_with(prefix)),
        "current candidate outside requested scope"
    );
    let compact = read
        .artifact(path)?
        .context("current search artifact missing")?;
    let retained = compact
        .retained
        .as_ref()
        .context("current search retained object missing")?;
    let cost = u64::try_from(compact.bytes)?
        .checked_add(u64::try_from(retained.object_bytes)?)
        .context("current search read cost overflow")?;
    *read_bytes = read_bytes
        .checked_add(cost)
        .context("current search read budget overflow")?;
    ensure!(
        *read_bytes <= MAX_LEXICAL_VALIDATION_BYTES,
        "current search evidence read budget exceeded"
    );
    match candidate {
        Candidate::Vector(pointer) => read.occurrence_hit(store, &pointer),
        Candidate::Lexical(candidate) => {
            ensure!(
                candidate.stable_id == stable_id(&compact.path, candidate.local)?,
                "current lexical document identity mismatch"
            );
            let full = store.load_retrieval_artifact(&compact)?.artifact;
            store.read_retained_source(&compact)?;
            let chunk = full
                .chunks
                .get(usize::try_from(candidate.local)?)
                .context("current lexical local chunk missing")?;
            ensure!(
                full.content_hash == candidate.hit.citation.content_hash
                    && chunk.start_line == candidate.hit.citation.start_line
                    && chunk.end_line == candidate.hit.citation.end_line
                    && chunk.location == candidate.hit.citation.location
                    && chunk.text == candidate.hit.text
                    && !candidate.hit.truncated,
                "current lexical candidate differs from retained evidence"
            );
            let wanted = terms(&query.text);
            ensure!(
                !wanted.is_empty()
                    && wanted.is_subset(&terms(&format!("{} {}", full.path, chunk.text))),
                "current lexical candidate does not match query"
            );
            Ok(candidate.hit)
        }
    }
}
