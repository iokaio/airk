// SPDX-License-Identifier: Apache-2.0
//! One request gate for lexical selection, exact vector ranking and citations.
use super::{MAX_LEXICAL_VALIDATION_BYTES, open_selected_lexical};
use airk_core::search_execution::{
    IncompleteReason, RequestedMode, SearchExecution, SearchUnavailable, ServedMode,
};
use airk_core::{Hit, SearchFilters, SearchQuery, SearchResponse, identity::Sha256Digest};
use airk_retrieval::{bounded::Charge, terms, truncate_utf8};
use airk_store::current::lexical::{CurrentLexicalCandidate, TermGroup, stable_id};
use airk_store::{
    FileStore,
    current::{
        CurrentRead, CurrentScope, CurrentToken, postings::Lookup, vector_map::OccurrencePointer,
    },
};
use anyhow::{Context, Result, ensure};
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

mod adaptive;
mod shortlist;
pub(crate) mod v2;
#[cfg(test)]
mod v2_tests;

const BRANCH_CANDIDATES: usize = 100;
const MAX_SCOPED_PATHS: usize = 4096;
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
    #[serde(skip)]
    pub(super) focus_cutoff: Vec<Hit>,
}

#[derive(Clone)]
enum Candidate {
    Lexical(CurrentLexicalCandidate),
    Vector(Arc<OccurrencePointer>),
    /// A ranked any-term match of these groups.
    LexicalAny(CurrentLexicalCandidate, Arc<Vec<TermGroup>>),
    Structure(Hit),
}

impl Candidate {
    fn path(&self) -> &str {
        match self {
            Self::Lexical(candidate) | Self::LexicalAny(candidate, _) => &candidate.citation.path,
            Self::Vector(pointer) => &pointer.path,
            Self::Structure(hit) => &hit.citation.path,
        }
    }
}

struct Branch {
    values: Vec<(Sha256Digest, usize, Candidate)>,
    incomplete: bool,
    kind: v2::BranchKind,
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
    pub automatic_limit: bool,
    pub structural_hits: &'a [Hit],
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
            automatic_limit: false,
            filters: &SearchFilters::default(),
            structural_hits: &[],
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
    search_with_switch(store, read, expected, mode, query, options, v2::active()?)
}

/// Tests pass an explicit profile to reproduce the baseline and branch variants.
fn search_with_switch(
    store: &FileStore,
    read: Arc<CurrentRead>,
    expected: Option<&CurrentToken>,
    mode: CurrentSearchMode,
    query: &SearchQuery,
    options: SearchOptions<'_>,
    profile: v2::SearchProfile,
) -> Result<CurrentSearchEvidence> {
    let SearchOptions {
        requested_mode,
        snippet_bytes,
        automatic_limit,
        filters,
        structural_hits,
    } = options;
    let requested_mode = requested_mode.unwrap_or_else(|| mode.into());
    let semantic_requested = !matches!(mode, CurrentSearchMode::Lexical);
    query.validate()?;
    filters.validate()?;
    if let Some(expected) = expected {
        read.require_token(expected)?;
    }
    if matches!(mode, CurrentSearchMode::Vector | CurrentSearchMode::Hybrid)
        && read.resource_reference("legacy-adoption")?.is_some()
        && read
            .collection_state()?
            .is_some_and(|state| state.configuration.model.is_none())
    {
        anyhow::bail!(
            "legacy saved semantic assets are archived; current vector conversion is unavailable"
        );
    }
    let mut semantic_readiness = super::enrichment::readiness(&read, store)?;
    if let Some(legacy) = crate::legacy_adoption::current_observation(store, &read)? {
        legacy.qualify(&mut semantic_readiness);
    }
    let upgrade_pending = crate::shared_models::semantic_upgrade_pending(
        store,
        read.collection_state()?
            .as_ref()
            .and_then(|s| s.configuration.model.as_ref()),
    )?;
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
        if semantic_readiness.state == SemanticState::Unavailable {
            // Saved predecessor vectors are not a usable current graph. Serve
            // retained text with the explicit incomplete execution/readiness
            // qualification below; do not reinterpret this as user opt-out.
            CurrentSearchMode::Lexical
        } else if semantic_readiness.state == SemanticState::Disabled {
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
    let (keywords, identifiers) = profile.lexical_branches();
    let plan = if (keywords || identifiers) && matches!(mode, CurrentSearchMode::Hybrid) {
        v2::Plan::for_query(&query.text)
    } else {
        None
    };
    let legacy = profile == v2::SearchProfile::LegacyV2 && plan.is_some();
    #[cfg(feature = "test-support")]
    let census_targets = census_targets()?;
    #[cfg(feature = "test-support")]
    if census_targets.is_some() {
        ensure!(
            !scoped_paths.as_ref().is_some_and(|scope| scope.incomplete),
            "rank census requires a complete authorized path roster"
        );
    }
    let branch_limit = if legacy {
        v2::BRANCH_CANDIDATES
    } else {
        BRANCH_CANDIDATES
    };
    let mut branches = Vec::new();
    if matches!(mode, CurrentSearchMode::Lexical | CurrentSearchMode::Hybrid) {
        let lexical = open_selected_lexical(store, &read)?;
        let mut selection = query.clone();
        // SearchQuery bounds a request at 100; the switch widens only the
        // vector and ranked branches below.
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
            kind: v2::BranchKind::Conjunctive,
        });
    }
    if matches!(mode, CurrentSearchMode::Vector | CurrentSearchMode::Hybrid) {
        let mut selection = query.clone();
        if matches!(mode, CurrentSearchMode::Hybrid) {
            selection.limit = BRANCH_CANDIDATES;
        }
        match vector_candidates(
            store,
            &read,
            &selection,
            scoped_paths.as_ref(),
            branch_limit,
            profile.uses_semantic_subqueries() && matches!(mode, CurrentSearchMode::Hybrid),
        ) {
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
    if let Some(plan) = plan
        .as_ref()
        .filter(|_| matches!(mode, CurrentSearchMode::Hybrid))
    {
        for (enabled, kind, groups) in [
            (keywords, v2::BranchKind::Key, &plan.key),
            (identifiers, v2::BranchKind::Named, &plan.named),
        ] {
            if !enabled || groups.is_empty() {
                continue;
            }
            let scope = scoped_paths.as_ref().map(|scope| &scope.paths);
            #[cfg(feature = "test-support")]
            let ranked = match &census_targets {
                Some(targets) => {
                    let (ranked, projection) = read.audit_lexical_search_ranked_any(
                        groups,
                        query.path_prefix.as_deref(),
                        scope,
                        branch_limit,
                        targets,
                    )?;
                    emit_census(
                        "AIRK_SEARCH_LEXICAL_CENSUS",
                        query,
                        &read,
                        serde_json::json!({
                            "branch": if matches!(kind, v2::BranchKind::Key) { "keyword" } else { "identifier" },
                            "admitted": ranked.candidates.values.len(),
                            "dropped_groups": ranked.dropped_groups,
                            "ranks": projection,
                        }),
                    )?;
                    ranked
                }
                None => read.lexical_search_ranked_any(
                    groups,
                    query.path_prefix.as_deref(),
                    scope,
                    branch_limit,
                )?,
            };
            #[cfg(not(feature = "test-support"))]
            let ranked = read.lexical_search_ranked_any(
                groups,
                query.path_prefix.as_deref(),
                scope,
                branch_limit,
            )?;
            ensure!(
                ranked.candidates.manifest_digest == read.head().lexical_commit,
                "current lexical candidate manifest mismatch"
            );
            let groups = Arc::new(groups.clone());
            branches.push(Branch {
                values: ranked
                    .candidates
                    .values
                    .into_iter()
                    .enumerate()
                    .map(|(rank, candidate)| {
                        (
                            candidate.stable_id,
                            rank,
                            Candidate::LexicalAny(candidate, groups.clone()),
                        )
                    })
                    .collect(),
                incomplete: ranked.candidates.incomplete,
                kind,
            });
        }
    }
    if profile == v2::SearchProfile::StructuralDefinitions
        && matches!(mode, CurrentSearchMode::Hybrid)
        && !structural_hits.is_empty()
    {
        use sha2::{Digest, Sha256};
        let values = structural_hits
            .iter()
            .enumerate()
            .map(|(rank, hit)| {
                let encoded = serde_json::to_vec(&("airk.search.structure", &hit.citation))?;
                let id = Sha256Digest::from_bytes(Sha256::digest(encoded).into());
                Ok((id, rank, Candidate::Structure(hit.clone())))
            })
            .collect::<Result<Vec<_>>>()?;
        branches.push(Branch {
            values,
            incomplete: false,
            kind: v2::BranchKind::Structure,
        });
    }
    let mut incomplete = branches.iter().any(|branch| branch.incomplete)
        || scoped_paths.as_ref().is_some_and(|scope| scope.incomplete)
        || discovery_readiness.state != airk_core::readiness::DiscoveryState::Ready;
    // The same lexical chunk receives one vote from each branch. Semantic
    // passages can span multiple chunks; their occurrence is emitted only once.
    let mut merged = BTreeMap::<Sha256Digest, (f64, Candidate)>::new();
    for branch in branches {
        let kind = branch.kind;
        for (identity, rank, candidate) in branch.values {
            let score = if legacy {
                v2::Plan::weight(kind) / (61.0 + rank as f64)
            } else {
                profile.branch_weight(kind) / (61.0 + rank as f64)
            };
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
    if legacy {
        for (_, (score, candidate)) in &mut ranked {
            *score *= v2::prior(candidate.path());
        }
    }
    ranked.sort_by(|a, b| b.1.0.total_cmp(&a.1.0).then(a.0.cmp(&b.0)));
    let diversify = plan.is_some() && (legacy || matches!(mode, CurrentSearchMode::Hybrid));
    let ranked = if let Some(limit) = profile.file_limit().filter(|_| diversify) {
        // Drop repeated passages first, as the loop below would, so they do
        // not count against their file's place in the order.
        let mut passages = BTreeSet::new();
        ranked.retain(|(_, (_, candidate))| match candidate {
            Candidate::Vector(pointer) => passages.insert(pointer.occurrence_id),
            _ => true,
        });
        v2::diversify(ranked, limit, |(_, (_, candidate))| {
            candidate.path().to_owned()
        })
    } else {
        ranked
    };
    #[cfg(feature = "test-support")]
    if let Some(targets) = &census_targets {
        let mut seen = BTreeSet::new();
        let projection: Vec<_> = ranked
            .iter()
            .map(|(_, (_, candidate))| candidate)
            .filter(|candidate| match candidate {
                Candidate::Vector(pointer) => seen.insert(pointer.occurrence_id),
                _ => true,
            })
            .enumerate()
            .filter(|(_, candidate)| targets.contains(candidate.path()))
            .map(|(index, candidate)| match candidate {
                Candidate::Vector(pointer) => serde_json::json!({
                    "rank": index + 1, "path": pointer.path, "ordinal": pointer.path_ordinal,
                }),
                Candidate::Lexical(candidate) | Candidate::LexicalAny(candidate, _) => {
                    serde_json::json!({"rank": index + 1, "citation": candidate.citation})
                }
                Candidate::Structure(hit) => {
                    serde_json::json!({"rank": index + 1, "citation": hit.citation})
                }
            })
            .collect();
        emit_census(
            "AIRK_SEARCH_FUSION_CENSUS",
            query,
            &read,
            serde_json::json!(projection),
        )?;
    }
    let contextual_passages = profile.keyword_passages()
        && snippet_bytes.is_some()
        && plan.is_some()
        && matches!(mode, CurrentSearchMode::Hybrid);
    let focus_text = plan
        .as_ref()
        .filter(|_| legacy || contextual_passages)
        .map_or(query.text.as_str(), |plan| &plan.focus);
    let wide = profile.wide_shortlist()
        && contextual_passages
        && query.limit == 10
        && query.max_bytes >= query.limit;
    let automatic = automatic_limit
        && profile.adaptive_results()
        && snippet_bytes.is_some()
        && matches!(mode, CurrentSearchMode::Hybrid);
    let result_limit = if automatic {
        adaptive::limit(
            ranked
                .iter()
                .map(|(_, (score, candidate))| (*score, candidate.path())),
            query.limit,
            query.max_bytes,
            plan.as_ref().is_some_and(|plan| plan.key.len() >= 8),
        )
    } else {
        query.limit
    };
    let preserve = automatic && profile.preserves_adaptive_prefix() && result_limit > query.limit;
    let mut gather_limit = if wide { query.limit * 4 } else { result_limit };
    let mut emitted = BTreeSet::new();
    let mut hits = Vec::new();
    let mut shortlist: Vec<(Hit, bool)> = Vec::new();
    let mut remaining = query.max_bytes;
    let mut read_bytes = 0u64;
    for (_, (_, candidate)) in ranked {
        if let Candidate::Vector(pointer) = &candidate
            && !emitted.insert(pointer.occurrence_id)
        {
            continue;
        }
        if preserve && hits.len() == query.limit {
            const MIN_EXTRA_PASSAGE_BYTES: usize = 128;
            gather_limit = result_limit.min(query.limit + remaining / MIN_EXTRA_PASSAGE_BYTES);
        }
        if hits.len() + shortlist.len() == gather_limit || remaining == 0 {
            incomplete = true;
            break;
        }
        let semantic = matches!(candidate, Candidate::Vector(_));
        let structural = matches!(candidate, Candidate::Structure(_));
        let Some(mut hit) = materialize(
            store,
            &read,
            query,
            candidate,
            &mut read_bytes,
            shortlist.len() >= query.limit || (automatic && hits.len() >= query.limit),
        )?
        else {
            incomplete = true;
            break;
        };
        let contextual_hit = contextual_passages
            && matches!(
                hit.citation.location,
                airk_core::DocumentLocation::Lines { .. }
            );
        let allocation_limit = if preserve {
            if hits.len() < query.limit {
                query.limit
            } else {
                gather_limit
            }
        } else {
            result_limit
        };
        let allowance = passage_allowance(
            snippet_bytes,
            remaining,
            allocation_limit - hits.len(),
            contextual_hit,
        );
        let allowance = if wide {
            allowance.min(query.max_bytes / query.limit)
        } else if automatic && !preserve && result_limit > query.limit && hits.len() < 4 {
            let reserve = (result_limit - hits.len() - 1) * adaptive::TAIL_BYTES;
            snippet_bytes
                .unwrap_or(allowance)
                .min(remaining.saturating_sub(reserve))
        } else {
            allowance
        };
        if structural && hit.text.len() > allowance {
            incomplete = true;
            continue;
        }
        if !structural && (snippet_bytes.is_some() || semantic) {
            if contextual_hit {
                focus_hit_with_context(&mut hit, focus_text, allowance);
                if hit.text.is_empty() {
                    incomplete = true;
                    continue;
                }
            } else {
                let snippet_query = if contextual_passages {
                    &query.text
                } else {
                    focus_text
                };
                focus_hit(&mut hit, snippet_query, allowance);
            }
        }
        if !structural_hits.is_empty()
            && hits
                .iter()
                .any(|seen: &Hit| same_citation(&seen.citation, &hit.citation))
        {
            continue;
        }
        if wide {
            if shortlist
                .iter()
                .any(|(seen, _)| shortlist::duplicate(&seen.citation, &hit.citation))
            {
                continue;
            }
            shortlist.push((hit, semantic));
            continue;
        }
        if automatic
            && hits.len() >= query.limit
            && hits
                .iter()
                .any(|seen: &Hit| shortlist::duplicate(&seen.citation, &hit.citation))
        {
            continue;
        }
        let (text, cut) = truncate_utf8(&hit.text, remaining);
        remaining -= text.len();
        hit.text = text;
        hit.truncated |= cut;
        incomplete |= hit.truncated;
        hits.push(hit);
    }
    let token = read.token()?;
    let mut focus_cutoff = Vec::new();
    if wide {
        let selected = if let Some(plan) = plan.as_ref().filter(|_| profile.scores_cutoff()) {
            let band = shortlist::complementary_band(&shortlist, query.limit, &plan.key);
            if band.len() == 4 {
                focus_cutoff = band.iter().map(|&i| shortlist[i].0.clone()).collect();
                (0..query.limit - 2)
                    .chain(band.into_iter().take(2))
                    .collect()
            } else {
                (0..query.limit.min(shortlist.len())).collect()
            }
        } else {
            shortlist::select(&shortlist, query.limit)
        };
        #[cfg(feature = "test-support")]
        trace_shortlist(query, &token, &shortlist, &selected);
        incomplete |= shortlist.len() > selected.len();
        hits = selected
            .into_iter()
            .map(|index| shortlist[index].0.clone())
            .collect();
        incomplete |= hits.iter().any(|hit| hit.truncated);
    }
    #[cfg(feature = "test-support")]
    if automatic {
        use sha2::{Digest, Sha256};
        let paths: BTreeSet<_> = hits.iter().map(|hit| &hit.citation.path).collect();
        eprintln!(
            "AIRK_SEARCH_ADAPTIVE {}",
            serde_json::json!({
                "query_sha256": format!("{:x}", Sha256::digest(query.text.as_bytes())),
                "publication": token,
                "default_limit": query.limit,
                "candidate_limit": result_limit,
                "target": gather_limit,
                "hits": hits.len(),
                "text_bytes": hits.iter().map(|hit| hit.text.len()).sum::<usize>(),
                "unique_paths": paths.len(),
            })
        );
    }
    Ok(CurrentSearchEvidence {
        token,
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
        focus_cutoff,
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
    // A permanent source-policy deny is enforced by its removal publication
    // (reads are admitted only once it is applied) and by the per-pointer and
    // per-artifact path checks of every branch. Only a selected scope that
    // omits sources of this publication needs a bounded roster; a policy alone
    // must not reduce every search to the first roster page of paths.
    if filters.is_empty() && !read.selected_scope_restricts_paths() {
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
    _store: &FileStore,
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
                let native = read.load_retained_bounded(&artifact, retained.object_bytes as u64)?;
                // Checks the record's source hash only; no source is read (Q5).
                read.verify_retained_source(&artifact)?;
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

/// Exact dot products over the pinned plain vector generation.
fn vector_candidates(
    store: &FileStore,
    read: &CurrentRead,
    query: &SearchQuery,
    filtered_paths: Option<&ScopedPaths>,
    branch_limit: usize,
    decompose: bool,
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
    let mut embeddings = vec![vector];
    let parts = if decompose {
        crate::arbiter::independent_parts(&query.text)
    } else {
        Vec::new()
    };
    for part in &parts {
        read.token()?;
        match model.embed_text(part) {
            Ok(Some(embedding)) => embeddings.push(embedding),
            Ok(None) => {
                embeddings.truncate(1);
                break;
            }
            Err(error) if error.is::<airk_retrieval::embedding_worker::HelperUnavailable>() => {
                embeddings.truncate(1);
                break;
            }
            Err(error) => return Err(error),
        }
    }

    // Preserve the product and private evaluation branch bounds.
    let wanted = if branch_limit == BRANCH_CANDIDATES {
        (query.limit * 4).clamp(64, 1000)
    } else {
        (branch_limit * 4).clamp(64, 1000)
    };
    let paths = filtered_paths.map(|scope| scope.paths.iter().cloned().collect::<Vec<_>>());
    #[cfg(feature = "test-support")]
    trace_vector_census(
        query,
        read,
        &parts,
        &embeddings,
        objects.pipeline_digest,
        paths.as_deref(),
    )?;
    let (selected, mut incomplete) = if embeddings.len() == 1 {
        read.ranked_vector_units(
            objects.pipeline_digest,
            &embeddings[0],
            paths.as_deref(),
            query.path_prefix.as_deref(),
            wanted,
            branch_limit,
        )?
    } else {
        let vectors: Vec<_> = embeddings.iter().map(Vec::as_slice).collect();
        let results = read.ranked_vector_units_for_queries(
            objects.pipeline_digest,
            &vectors,
            paths.as_deref(),
            query.path_prefix.as_deref(),
            wanted,
            branch_limit,
        )?;
        #[cfg(feature = "test-support")]
        trace_semantic_queries(query, read, &results)?;
        let incomplete = results.iter().any(|(_, incomplete)| *incomplete);
        let mut queues: Vec<_> = results
            .into_iter()
            .map(|(units, _)| units.into_iter())
            .collect();
        let mut seen = BTreeSet::new();
        let mut selected = Vec::new();
        // Take the strongest remaining passage from each query in turn.
        // The original query wins equal positions and repeated passages get one vote.
        for _ in 0..branch_limit {
            for queue in &mut queues {
                if let Some(pointer) = queue.next()
                    && seen.insert(pointer.occurrence_id)
                {
                    selected.push(pointer);
                }
            }
        }
        (selected, incomplete)
    };
    incomplete |= filtered_paths.is_some_and(|scope| scope.incomplete);

    let mut budget = PointerBudget::default();
    let mut seen = BTreeSet::new();
    let mut values = Vec::new();
    for (rank, pointer) in selected.into_iter().enumerate() {
        if !budget.admit(&pointer)? {
            incomplete = true;
            break;
        }
        let pointer = Arc::new(pointer);
        for member in &pointer.members {
            let identity = stable_id(&pointer.path, u64::from(member.chunk_index))?;
            if seen.insert(identity) {
                values.push((identity, rank, Candidate::Vector(pointer.clone())));
            }
        }
    }
    Ok(Branch {
        values,
        incomplete,
        kind: v2::BranchKind::Vector,
    })
}

#[cfg(feature = "test-support")]
fn census_targets() -> Result<Option<BTreeSet<String>>> {
    std::env::var_os("AIRK_PRIVATE_VECTOR_CENSUS_PATHS")
        .map(|raw| {
            let targets: BTreeSet<String> =
                serde_json::from_str(raw.to_str().context("invalid census paths")?)?;
            ensure!(
                !targets.is_empty() && targets.len() <= 512,
                "invalid target path count"
            );
            Ok(targets)
        })
        .transpose()
}

#[cfg(feature = "test-support")]
fn emit_census(
    marker: &str,
    query: &SearchQuery,
    read: &CurrentRead,
    observations: serde_json::Value,
) -> Result<()> {
    use sha2::{Digest, Sha256};
    eprintln!(
        "{marker} {}",
        serde_json::json!({
            "query_sha256": format!("{:x}", Sha256::digest(query.text.as_bytes())),
            "publication": read.token()?,
            "observations": observations,
        })
    );
    Ok(())
}

#[cfg(feature = "test-support")]
fn trace_vector_census(
    query: &SearchQuery,
    read: &CurrentRead,
    parts: &[&str],
    embeddings: &[Vec<f32>],
    pipeline: Sha256Digest,
    paths: Option<&[String]>,
) -> Result<()> {
    use sha2::{Digest, Sha256};
    let Some(targets) = census_targets()? else {
        return Ok(());
    };
    let focused = parts
        .iter()
        .map(|part| {
            let start = query
                .text
                .find(*part)
                .context("focused query is not a task substring")?;
            Ok((start, start + part.len()))
        })
        .collect::<Result<Vec<_>>>()?;
    if std::env::var_os("AIRK_PRIVATE_BRANCH_CENSUS_ONLY").is_some() {
        return emit_census(
            "AIRK_SEARCH_FOCUSED_CENSUS",
            query,
            read,
            serde_json::json!({
                "byte_ranges": focused,
                "embedded": embeddings.len() - 1,
            }),
        );
    }
    let ranks = embeddings
        .iter()
        .map(|embedding| {
            read.audit_vector_occurrence_ranks(
                pipeline,
                embedding,
                paths,
                query.path_prefix.as_deref(),
                &targets,
            )
        })
        .collect::<Result<Vec<_>>>()?;
    eprintln!(
        "AIRK_SEARCH_VECTOR_CENSUS {}",
        serde_json::json!({
            "query_sha256": format!("{:x}", Sha256::digest(query.text.as_bytes())),
            "publication": read.token()?,
            "pipeline": pipeline,
            "focused_byte_ranges": focused,
            "ranks": ranks,
        })
    );
    Ok(())
}

#[cfg(feature = "test-support")]
fn trace_semantic_queries(
    query: &SearchQuery,
    read: &CurrentRead,
    results: &[(Vec<OccurrencePointer>, bool)],
) -> Result<()> {
    use sha2::{Digest, Sha256};
    if !std::env::var("AIRK_ARBITER_EVALUATION_SEARCH").is_ok_and(|profile| {
        profile == "semantic_subqueries"
            || profile == "structural_definitions"
            || profile == "keyword_passages"
            || profile.starts_with("fusion_")
    }) {
        return Ok(());
    }
    let ranks: Vec<_> = results
        .iter()
        .map(|(pointers, _)| {
            pointers
                .iter()
                .map(|pointer| {
                    serde_json::json!({
                        "occurrence": pointer.occurrence_id,
                        "path": pointer.path,
                        "members": pointer.members,
                    })
                })
                .collect::<Vec<_>>()
        })
        .collect();
    eprintln!(
        "AIRK_SEARCH_SEMANTIC_RANKS {}",
        serde_json::json!({
            "query_sha256": format!("{:x}", Sha256::digest(query.text.as_bytes())),
            "publication": read.token()?,
            "ranked": ranks,
        })
    );
    Ok(())
}

pub(super) fn same_citation(left: &airk_core::Citation, right: &airk_core::Citation) -> bool {
    left.path == right.path
        && left.content_hash == right.content_hash
        && left.start_line == right.start_line
        && left.end_line == right.end_line
        && left.location == right.location
}

fn materialize(
    store: &FileStore,
    read: &CurrentRead,
    query: &SearchQuery,
    candidate: Candidate,
    read_bytes: &mut u64,
    optional: bool,
) -> Result<Option<Hit>> {
    let path = candidate.path();
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
    let charged = read_bytes
        .checked_add(cost)
        .context("current search read budget overflow")?;
    if optional && charged > MAX_LEXICAL_VALIDATION_BYTES {
        return Ok(None);
    }
    ensure!(
        charged <= MAX_LEXICAL_VALIDATION_BYTES,
        "current search evidence read budget exceeded"
    );
    *read_bytes = charged;
    match candidate {
        Candidate::Structure(hit) => {
            ensure!(
                hit.citation.content_hash == compact.content_hash,
                "structural Search citation changed after collection"
            );
            Ok(hit)
        }
        Candidate::Vector(pointer) => read.vector_unit_hit(&compact, &pointer),
        Candidate::Lexical(candidate) => {
            let wanted = terms(&query.text);
            lexical_hit(store, read, &compact, candidate, |found| {
                !wanted.is_empty() && wanted.is_subset(found)
            })
        }
        Candidate::LexicalAny(candidate, groups) => {
            lexical_hit(store, read, &compact, candidate, |found| {
                groups.iter().any(|group| group.matches(found))
            })
        }
    }
    .map(Some)
}

#[cfg(feature = "test-support")]
fn trace_shortlist(
    query: &SearchQuery,
    publication: &CurrentToken,
    candidates: &[(Hit, bool)],
    selected: &[usize],
) {
    use sha2::{Digest, Sha256};
    let citations: Vec<_> = candidates
        .iter()
        .map(|(hit, semantic)| (&hit.citation, semantic))
        .collect();
    eprintln!(
        "AIRK_SEARCH_SHORTLIST {}",
        serde_json::json!({
            "query_sha256": format!("{:x}", Sha256::digest(query.text.as_bytes())),
            "publication": publication,
            "candidates": citations,
            "selected": selected,
        })
    );
}

/// One lexical chunk, verified against its retained extraction; `matches`
/// confirms the chunk's own terms satisfy the query that selected it.
fn lexical_hit(
    store: &FileStore,
    read: &CurrentRead,
    compact: &airk_core::Artifact,
    candidate: CurrentLexicalCandidate,
    matches: impl FnOnce(&BTreeSet<String>) -> bool,
) -> Result<Hit> {
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
        read.observe_private_integrity(text.chunk_private(usize::try_from(candidate.local)?))?
    } else {
        let result = (|| -> Result<_> {
            let full = super::load_lexical_artifact(read, compact)?;
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
    read.observe_private_integrity((|| -> Result<()> {
        ensure!(
            matches(&terms(&format!("{} {}", compact.path, chunk.text))),
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

/// Focus before response budget accounting, preserving original document locations.
pub(crate) fn focus_hit(hit: &mut Hit, query: &str, allowance: usize) {
    if hit.text.len() <= allowance {
        return;
    }
    let span = airk_retrieval::focus_span(&hit.text, query, allowance);
    apply_snippet(hit, span);
}

fn passage_allowance(
    requested: Option<usize>,
    remaining: usize,
    slots: usize,
    balanced: bool,
) -> usize {
    let share = if balanced {
        (remaining / slots).max(1)
    } else {
        remaining
    };
    requested.unwrap_or(share).min(share)
}

fn focus_hit_with_context(hit: &mut Hit, query: &str, allowance: usize) {
    use airk_core::location::ByteSpan;
    if hit.text.len() <= allowance {
        return;
    }
    let anchor = airk_retrieval::focus_span(&hit.text, query, allowance);
    let mut start = anchor
        .start
        .saturating_sub(allowance / 3)
        .min(hit.text.len().saturating_sub(allowance));
    while !hit.text.is_char_boundary(start) {
        start += 1;
    }
    let mut end = start.saturating_add(allowance).min(hit.text.len());
    while !hit.text.is_char_boundary(end) {
        end -= 1;
    }
    apply_snippet(hit, ByteSpan { start, end });
}

fn apply_snippet(hit: &mut Hit, span: airk_core::location::ByteSpan) {
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
