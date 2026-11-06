// SPDX-License-Identifier: Apache-2.0
//! One request gate for lexical selection, current DiskANN and exact citations.
use super::{MAX_LEXICAL_VALIDATION_BYTES, open_selected_lexical};
use airk_core::search_execution::{
    IncompleteReason, RequestedMode, SearchExecution, SearchUnavailable, ServedMode,
};
use airk_core::{Hit, SearchFilters, SearchQuery, SearchResponse, identity::Sha256Digest};
use airk_retrieval::{bounded::Charge, terms, truncate_utf8, vector::mutable::MutableDiskIndex};
use airk_store::current::lexical::{CurrentLexicalCandidate, stable_id};
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
const MAX_FILTER_PATH_BYTES: usize = 4 << 20;
const MAX_HINT_FILES: usize = 4;
const MAX_HINT_READ_BYTES: usize = 1 << 20;
const MAX_HINT_OUTPUT_BYTES: usize = 4096;

#[derive(Serialize)]
pub struct SearchWithDefinitionsResponse {
    #[serde(flatten)]
    pub evidence: SearchResponse,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub definitions: Vec<DefinitionHint>,
}

#[derive(Serialize)]
pub struct DefinitionHint {
    pub name: String,
    pub total_matches: Option<usize>,
    pub lower_bound: usize,
    pub complete: bool,
    pub evidence_scope: &'static str,
    pub suggested_tool: &'static str,
    pub matches: Vec<DefinitionPointer>,
}

#[derive(Serialize)]
pub struct DefinitionPointer {
    pub citation: airk_core::Citation,
    pub symbol_id: Option<String>,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CurrentSearchMode {
    Default,
    Lexical,
    Vector,
    Hybrid,
}

impl From<CurrentSearchMode> for RequestedMode {
    fn from(mode: CurrentSearchMode) -> Self {
        match mode {
            CurrentSearchMode::Default => Self::Default,
            CurrentSearchMode::Lexical => Self::Lexical,
            CurrentSearchMode::Vector => Self::Vector,
            CurrentSearchMode::Hybrid => Self::Hybrid,
        }
    }
}

#[derive(Serialize)]
pub struct CurrentSearchEvidence {
    pub token: CurrentToken,
    pub collection: String,
    pub mode: CurrentSearchMode,
    pub execution: SearchExecution,
    pub semantic_readiness: airk_core::readiness::SemanticReadiness,
    pub discovery_readiness: airk_core::readiness::DiscoveryReadiness,
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

pub(crate) struct SearchOptions<'a> {
    /// Retain Default when a saved opt-out has already selected lexical search.
    pub requested_mode: Option<RequestedMode>,
    pub snippet_bytes: Option<usize>,
    pub filters: &'a SearchFilters,
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
    search_current_with_snippets(store, scope, expected, mode, query, charge, None)
}

pub(crate) fn search_current_with_snippets(
    store: &FileStore,
    scope: &CurrentScope,
    expected: Option<&CurrentToken>,
    mode: CurrentSearchMode,
    query: &SearchQuery,
    _charge: Charge,
    snippet_bytes: Option<usize>,
) -> Result<CurrentSearchEvidence> {
    search_current_read_with_snippets(
        store,
        store.current_read(scope)?,
        expected,
        mode,
        query,
        _charge,
        snippet_bytes,
    )
}

/// Kernel callers provide the exact sealed request reader; ordinary adapters
/// above keep strict store reads and cannot manufacture native authority.
pub(crate) fn search_current_read_with_snippets(
    store: &FileStore,
    read: Arc<CurrentRead>,
    expected: Option<&CurrentToken>,
    mode: CurrentSearchMode,
    query: &SearchQuery,
    _charge: Charge,
    snippet_bytes: Option<usize>,
) -> Result<CurrentSearchEvidence> {
    search_current_read_with_filters(
        store,
        read,
        expected,
        mode,
        query,
        _charge,
        SearchOptions {
            requested_mode: None,
            snippet_bytes,
            filters: &SearchFilters::default(),
        },
    )
}

pub(crate) fn search_current_read_with_filters(
    store: &FileStore,
    read: Arc<CurrentRead>,
    expected: Option<&CurrentToken>,
    mode: CurrentSearchMode,
    query: &SearchQuery,
    _charge: Charge,
    options: SearchOptions<'_>,
) -> Result<CurrentSearchEvidence> {
    let SearchOptions {
        requested_mode,
        snippet_bytes,
        filters,
    } = options;
    let requested_mode = requested_mode.unwrap_or_else(|| mode.into());
    let semantic_requested = !matches!(mode, CurrentSearchMode::Lexical);
    query.validate()?;
    filters.validate()?;
    if let Some(expected) = expected {
        read.require_token(expected)?;
    }
    let semantic_readiness = super::enrichment::readiness(&read, store)?;
    let upgrade_pending = crate::shared_models::semantic_upgrade_pending(
        store,
        read.collection_state()?
            .as_ref()
            .and_then(|s| s.configuration.model.as_ref()),
    );
    let discovery_readiness = super::progress::readiness(&read, store)?;
    let mode = if matches!(mode, CurrentSearchMode::Default) {
        use airk_core::readiness::{DefaultSearchPolicy, DiscoveryState, SemanticState};
        if semantic_readiness.default_search_policy == DefaultSearchPolicy::RequireComplete
            && discovery_readiness.state != DiscoveryState::Ready
        {
            anyhow::bail!(
                "complete discovery coverage required; current state is {:?}",
                discovery_readiness.state
            );
        }
        if semantic_readiness.state == SemanticState::Disabled {
            anyhow::bail!("semantic search is unavailable for this current index")
        } else if semantic_readiness.state != SemanticState::Ready {
            match semantic_readiness.default_search_policy {
                DefaultSearchPolicy::PartialHybrid => CurrentSearchMode::Hybrid,
                DefaultSearchPolicy::LexicalWhilePending => CurrentSearchMode::Lexical,
                DefaultSearchPolicy::RequireComplete => {
                    anyhow::bail!(
                        "complete semantic coverage required; current state is {:?}",
                        semantic_readiness.state
                    )
                }
            }
        } else {
            CurrentSearchMode::Hybrid
        }
    } else {
        mode
    };
    let mut mode = if upgrade_pending {
        ensure!(
            !matches!(mode, CurrentSearchMode::Vector),
            "semantic index upgrade pending; text search remains available"
        );
        CurrentSearchMode::Lexical
    } else {
        mode
    };
    let mut incomplete_reason = if semantic_requested && upgrade_pending {
        Some(IncompleteReason::SemanticUpgradePending)
    } else if semantic_requested
        && semantic_readiness.state != airk_core::readiness::SemanticState::Ready
    {
        Some(IncompleteReason::SemanticCoverageIncomplete)
    } else {
        None
    };
    // Permission-only lexical reads are already scoped inside candidate SQL,
    // before LIMIT. Vector branches require the scoped subset roster.
    let scoped_paths = if matches!(mode, CurrentSearchMode::Lexical) && filters.is_empty() {
        None
    } else {
        filtered_paths(&read, query, filters)?
    };
    let mut branches = Vec::new();
    if matches!(mode, CurrentSearchMode::Lexical | CurrentSearchMode::Hybrid) {
        let lexical = open_selected_lexical(store, &read)?;
        let mut selection = query.clone();
        selection.limit = if matches!(mode, CurrentSearchMode::Hybrid) {
            BRANCH_CANDIDATES
        } else {
            query.limit
        };
        let candidates =
            lexical.search_paths(&selection, scoped_paths.as_ref().map(|scope| &scope.paths))?;
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
        match vector_candidates(store, &read, &selection, scoped_paths.as_ref()) {
            Ok(branch) => branches.push(branch),
            Err(error)
                if error
                    .downcast_ref::<airk_retrieval::embedding_worker::HelperUnavailable>()
                    .is_some() =>
            {
                if requested_mode == RequestedMode::Default
                    && matches!(mode, CurrentSearchMode::Hybrid)
                    && semantic_readiness.default_search_policy
                        != airk_core::readiness::DefaultSearchPolicy::RequireComplete
                {
                    // The lexical branch above is already selected from this
                    // exact reader. Never repeat it or replay failed inference.
                    mode = CurrentSearchMode::Lexical;
                    incomplete_reason = Some(IncompleteReason::HelperUnavailable);
                } else {
                    return Err(error.context(SearchUnavailable {
                        requested_mode,
                        publication: Some(super::runtime::envelope(&read)?),
                        semantic_readiness: (!read.selected_scope_restricts_paths())
                            .then_some(semantic_readiness),
                        discovery_readiness: (!read.selected_scope_restricts_paths())
                            .then_some(discovery_readiness),
                    }));
                }
            }
            Err(error) => return Err(error),
        }
    }
    let mut incomplete = branches.iter().any(|branch| branch.incomplete)
        || scoped_paths.as_ref().is_some_and(|scope| scope.incomplete)
        || discovery_readiness.state != airk_core::readiness::DiscoveryState::Ready;
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
        let allowance = snippet_bytes.unwrap_or(remaining).min(remaining);
        if snippet_bytes.is_some() || semantic {
            focus_hit(&mut hit, &query.text, allowance);
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
        collection: read.head().scope.collection.clone(),
        mode,
        execution: SearchExecution::new(
            requested_mode,
            match mode {
                CurrentSearchMode::Lexical => ServedMode::Lexical,
                CurrentSearchMode::Vector => ServedMode::Vector,
                CurrentSearchMode::Hybrid => ServedMode::Hybrid,
                CurrentSearchMode::Default => unreachable!("default search mode was resolved"),
            },
            incomplete_reason,
        ),
        semantic_readiness,
        discovery_readiness,
        hits,
        truncated: incomplete,
    })
}

struct ScopedPaths {
    paths: BTreeSet<String>,
    incomplete: bool,
}

/// Classify saved metadata before either branch takes its bounded top-k. Limits
/// describe visited roster work, not a promise that the filtered corpus is small.
fn filtered_paths(
    read: &CurrentRead,
    query: &SearchQuery,
    filters: &SearchFilters,
) -> Result<Option<ScopedPaths>> {
    if filters.is_empty() && !read.restricts_paths() {
        return Ok(None);
    }
    let lookup = filters
        .language
        .as_deref()
        .map_or(Lookup::Files, Lookup::Language);
    let roster = read.lookup_paths(
        lookup,
        query.path_prefix.as_deref().unwrap_or(""),
        None,
        MAX_SCOPED_PATHS,
    )?;
    let mut scope = ScopedPaths {
        paths: BTreeSet::new(),
        incomplete: roster.next_after.is_some(),
    };
    let mut bytes = 2usize;
    for path in roster.paths {
        let artifact = read
            .artifact(&path)?
            .context("filtered search artifact missing")?;
        if !filters.matches(&artifact) {
            continue;
        }
        let encoded = serde_json::to_string(&path)?.len() + 1;
        if bytes.saturating_add(encoded) > MAX_FILTER_PATH_BYTES {
            scope.incomplete = true;
            break;
        }
        bytes += encoded;
        scope.paths.insert(path);
    }
    Ok(Some(scope))
}

fn identifier_tokens(text: &str) -> Vec<&str> {
    let words = text
        .split(|character: char| !character.is_alphanumeric() && character != '_')
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>();
    let mut identifiers = BTreeSet::new();
    for word in &words {
        if word.len() > 128 || word.chars().next().is_none_or(|first| first.is_numeric()) {
            continue;
        }
        if words.len() == 1 || word.contains('_') || word.chars().any(char::is_uppercase) {
            identifiers.insert(*word);
        }
    }
    if identifiers.len() > 4 {
        Vec::new()
    } else {
        identifiers.into_iter().collect()
    }
}

/// Navigation is supplemental native evidence, not a rewrite or a reranker.
/// Reads, file visits and serialized pointers have independent small ceilings.
pub(crate) fn definition_hints(
    store: &FileStore,
    read: &CurrentRead,
    query: &SearchQuery,
    filters: &SearchFilters,
) -> Result<Vec<DefinitionHint>> {
    let mut hints = Vec::new();
    let mut files = 0usize;
    let mut work = 0usize;
    let mut output = 2usize;
    for name in identifier_tokens(&query.text) {
        let roster = read.lookup_paths(
            Lookup::FactName(name),
            query.path_prefix.as_deref().unwrap_or(""),
            None,
            MAX_HINT_FILES,
        )?;
        let mut complete = roster.next_after.is_none();
        let mut matches = Vec::new();
        let mut count = 0usize;
        for path in roster.paths {
            let artifact = read
                .artifact(&path)?
                .context("definition hint artifact missing")?;
            if !filters.matches(&artifact) {
                continue;
            }
            let retained = artifact
                .retained
                .as_ref()
                .context("definition hint extraction missing")?;
            let bytes = artifact
                .bytes
                .saturating_add(retained.object_bytes)
                .saturating_add(read.file_metadata_bytes(&path)?);
            if files == MAX_HINT_FILES || bytes > MAX_HINT_READ_BYTES.saturating_sub(work) {
                complete = false;
                break;
            }
            files += 1;
            work += bytes;
            // Supplemental navigation reads only this bounded native extraction,
            // not a global imported-analysis merge whose work is outside this
            // helper's small allowance. Source verification remains mandatory.
            let native = read.observe_private_integrity((|| -> Result<_> {
                let native =
                    store.load_retained_bounded(&artifact, retained.object_bytes as u64)?;
                store.verify_retained_source(&artifact)?;
                Ok(native)
            })())?;
            for fact in native.facts {
                let Some(code) = fact.code else { continue };
                if fact.kind != "definition"
                    || code.basis == airk_core::EvidenceBasis::Imported
                    || (fact.name != name && code.qualified_name != name && code.symbol_id != name)
                {
                    continue;
                }
                count += 1;
                if matches.len() < 4 {
                    matches.push(DefinitionPointer {
                        citation: airk_core::Citation {
                            path: artifact.path.clone(),
                            content_hash: artifact.content_hash.clone(),
                            start_line: fact.start_line,
                            end_line: fact.end_line,
                            location: fact.location,
                        },
                        symbol_id: Some(code.symbol_id),
                    });
                }
            }
        }
        if count == 0 {
            continue;
        }
        let mut hint = DefinitionHint {
            name: name.into(),
            total_matches: complete.then_some(count),
            lower_bound: count,
            complete,
            evidence_scope: "native_definition_records",
            suggested_tool: "symbols",
            matches,
        };
        // Dropping supplemental pointers does not falsify the verified count;
        // counts describe the named native record scope, not returned pointers.
        while !hint.matches.is_empty()
            && output.saturating_add(serde_json::to_vec(&hint)?.len() + 1) > MAX_HINT_OUTPUT_BYTES
        {
            hint.matches.pop();
        }
        let bytes = serde_json::to_vec(&hint)?.len() + 1;
        if output.saturating_add(bytes) > MAX_HINT_OUTPUT_BYTES {
            break;
        }
        output += bytes;
        hints.push(hint);
    }
    Ok(hints)
}

fn vector_candidates(
    store: &FileStore,
    read: &Arc<CurrentRead>,
    query: &SearchQuery,
    filtered_paths: Option<&ScopedPaths>,
) -> Result<Branch> {
    let state = read
        .collection_state()?
        .context("current collection configuration missing")?;
    let objects = state
        .configuration
        .model
        .context("semantic retrieval not ready: no indexed model")?;
    let model = crate::shared_models::open_model(store, &objects)?;
    let vector = model
        .embed_text(&query.text)?
        .context("semantic retrieval not ready: no recognized model tokens")?;
    let graph = MutableDiskIndex::open(read.clone(), model.dimensions(), objects.pipeline_digest)?;
    let wanted = (query.limit * 4).clamp(64, 1000);
    let mut budget = PointerBudget::default();
    let mut incomplete = false;
    let mut selected = Vec::<Arc<OccurrencePointer>>::new();
    let scoped_paths = if let Some(scope) = filtered_paths {
        incomplete |= scope.incomplete;
        Some(scope.paths.iter().cloned().collect::<Vec<_>>())
    } else if let Some(prefix) = query
        .path_prefix
        .as_deref()
        .filter(|prefix| !prefix.is_empty())
    {
        let roster = read.lookup_paths(Lookup::Files, prefix, None, MAX_SCOPED_PATHS)?;
        incomplete |= roster.next_after.is_some();
        Some(roster.paths)
    } else {
        None
    };
    if let Some(paths) = scoped_paths {
        let mut scoped = BTreeMap::<u32, Vec<Arc<OccurrencePointer>>>::new();
        'paths: for path in paths {
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
        Candidate::Lexical(c) => &c.citation.path,
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
            let path = &candidate.citation.path;
            ensure!(
                candidate.stable_id == stable_id(&compact.path, candidate.local)?,
                "current lexical document identity mismatch"
            );
            let private = read.private_query_eligible()?;
            let text = if private {
                read.observe_private_integrity(read.text_range_projection(store, path))?
            } else {
                None
            };
            let chunk = if let Some(text) = text {
                read.observe_private_integrity(
                    text.chunk_private(usize::try_from(candidate.local)?),
                )?
            } else {
                let result = (|| -> Result<_> {
                    let full = store.load_retrieval_artifact(&compact)?.artifact;
                    store.verify_retained_source(&compact)?;
                    Ok(full
                        .chunks
                        .get(usize::try_from(candidate.local)?)
                        .context("current lexical local chunk missing")?
                        .clone())
                })();
                if private {
                    read.observe_private_integrity(result)?
                } else {
                    result?
                }
            };
            read.observe_private_integrity((|| -> Result<()> {
                ensure!(
                    compact.content_hash == candidate.citation.content_hash
                        && chunk.start_line == candidate.citation.start_line
                        && chunk.end_line == candidate.citation.end_line
                        && chunk.location == candidate.citation.location,
                    airk_store::current::PrivateIntegrityFailure
                );
                Ok(())
            })())?;
            let wanted = terms(&query.text);
            read.observe_private_integrity((|| -> Result<()> {
                ensure!(
                    !wanted.is_empty()
                        && wanted.is_subset(&terms(&format!("{} {}", compact.path, chunk.text))),
                    airk_store::current::PrivateIntegrityFailure
                );
                Ok(())
            })())?;
            Ok(Hit {
                citation: candidate.citation,
                text: chunk.text.clone(),
                truncated: false,
            })
        }
    }
}

/// Focus before response budget accounting, preserving original document locations.
pub(crate) fn focus_hit(hit: &mut Hit, query: &str, allowance: usize) {
    if hit.text.len() <= allowance {
        return;
    }
    let span = airk_retrieval::focus_span(&hit.text, query, allowance);
    let first = hit.citation.start_line
        + hit.text[..span.start]
            .bytes()
            .filter(|b| *b == b'\n')
            .count();
    let last = first
        + hit.text.as_bytes()[span.start..span.end.saturating_sub(1).max(span.start)]
            .iter()
            .filter(|b| **b == b'\n')
            .count();
    hit.citation.start_line = first;
    hit.citation.end_line = last;
    if matches!(
        hit.citation.location,
        airk_core::DocumentLocation::Lines { .. }
    ) {
        hit.citation.location = airk_core::DocumentLocation::Lines {
            start: first,
            end: last,
        };
    }
    hit.text = hit.text[span.start..span.end].to_owned();
    hit.truncated = true;
}
