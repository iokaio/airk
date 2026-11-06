// SPDX-License-Identifier: Apache-2.0
//! Production adapters keep the current request gate until evidence is materialized.
use crate::{FileInfo, Kernel, Status};
use airk_core::evidence::RevisionEnvelope;
use airk_core::identity::RevisionToken;
use airk_core::{Artifact, FactsResponse, SearchQuery, SearchResponse, Snapshot};
use airk_store::current::{CurrentRead, CurrentScope, CurrentToken, postings::Lookup};
use anyhow::{Context, Result, ensure};
use std::sync::Arc;

/// Selection is resolved after the same strict source/extraction checks as before.
#[derive(Clone, Copy)]
pub(crate) enum LineSelection {
    Range {
        start: usize,
        end: usize,
    },
    Context {
        line: usize,
        before: usize,
        after: usize,
        /// Conditional context historically uses the maximum chunk end.
        maximum_end: bool,
    },
}
impl LineSelection {
    fn range(self, lines: usize) -> Result<(usize, usize)> {
        match self {
            Self::Range { start, end } => Ok((start, end)),
            Self::Context {
                line,
                before,
                after,
                ..
            } => {
                ensure!(line > 0 && line <= lines, "line is outside indexed source");
                Ok((
                    line.saturating_sub(before).max(1),
                    line.saturating_add(after).min(lines),
                ))
            }
        }
    }
}

pub(crate) fn revision(token: CurrentToken) -> RevisionToken {
    RevisionToken {
        incarnation: token.incarnation,
        generation: token.sequence,
        revision: token.digest,
    }
}

pub(crate) fn envelope(read: &CurrentRead) -> Result<RevisionEnvelope> {
    Ok(RevisionEnvelope::new(
        read.head().scope.collection.clone(),
        revision(read.token()?),
    ))
}

impl Kernel {
    /// A readiness check reads the published scalar metadata, never imported fact
    /// payloads, model files, source files, or the vector graph itself.
    pub fn status_summary(&self) -> Result<serde_json::Value> {
        self.require_source_policy_scope()?;
        let source_policy = self.source_policy_observation()?;
        if self.source_policy_for_work().is_err() {
            return Ok(
                serde_json::json!({"version":env!("CARGO_PKG_VERSION"),"health":"needs_attention","index_state":"unavailable","collection":self.collection,"generation":null,"publication":null,"files":null,"chunks":null,"indexed_vectors":null,"source_policy":source_policy,"freshness":{"state":"blocked","live_source_verified":false},"discovery_readiness":{"state":"unknown"},"semantic_readiness":{"state":"unknown"},"error":crate::SourcePolicyReadUnavailable.receipt()["error"]}),
            );
        }
        let read = self.current_reader()?;
        self.require_index_metadata(&read)?;
        let state = read
            .collection_state()?
            .context("current collection configuration missing")?;
        let policy = super::enrichment::read_policy(&read, &self.store)?;
        let semantic = super::enrichment::readiness_with_state(&read, &self.store, &state, policy)?;
        let discovery = super::progress::readiness_with_state(&read, &self.store, &state)?;
        let pending = crate::shared_models::semantic_upgrade_pending(
            &self.store,
            state.configuration.model.as_ref(),
        );
        let freshness = self.changes_freshness();
        let health = if semantic.state == airk_core::readiness::SemanticState::Failed
            || discovery.state == airk_core::readiness::DiscoveryState::Failed
        {
            "needs_attention"
        } else {
            match freshness.state {
                airk_core::changes::FreshnessState::Failed
                | airk_core::changes::FreshnessState::Blocked => "needs_attention",
                airk_core::changes::FreshnessState::Pending
                | airk_core::changes::FreshnessState::Running => "updating",
                airk_core::changes::FreshnessState::Paused => "paused",
                airk_core::changes::FreshnessState::Unavailable
                | airk_core::changes::FreshnessState::Unregistered => "unknown",
                airk_core::changes::FreshnessState::Idle => {
                    if matches!(
                        semantic.state,
                        airk_core::readiness::SemanticState::Pending
                            | airk_core::readiness::SemanticState::Partial
                    ) {
                        "updating"
                    } else {
                        "ok"
                    }
                }
            }
        };
        let mut result = serde_json::json!({"version":env!("CARGO_PKG_VERSION"),"health":health,"index_state":"available","collection":self.collection,
            "generation":read.head().sequence,"publication":{"token":envelope(&read)?.token},"files":state.counts.files,
            "chunks":state.counts.chunks,"indexed_vectors":if pending {0}else{read.head().graph_head.live_nodes},
            "freshness":{"state":freshness.state,"live_source_verified":freshness.live_source_verified},
            "discovery_readiness":{"state":discovery.state},"semantic_readiness":{"state":semantic.state,"completed_artifacts":semantic.completed_artifacts,"selected_artifacts":semantic.selected_artifacts}});
        if let Some(source_policy) = source_policy {
            result["source_policy"] = source_policy;
        }
        Ok(result)
    }

    pub fn maintain_current(&self) -> Result<airk_store::current::maintenance::ReclamationReport> {
        let scope = self.current_scope()?.context("workspace is not indexed")?;
        let writer = self.store.lock()?;
        self.store.reclaim_current_objects(&writer, &scope)
    }

    pub(crate) fn current_scope(&self) -> Result<Option<CurrentScope>> {
        self.store.current_scope(&self.root_id(), &self.collection)
    }

    pub(crate) fn current_reader(&self) -> Result<Arc<CurrentRead>> {
        let scope = self
            .current_scope()?
            .context("workspace is not indexed; run airk index first")?;
        self.attach_private_read(self.store.current_read(&scope)?)
    }

    /// A bounded compact scan projection, never a persisted snapshot or a live-source read.
    pub(crate) fn current_scan_projection(&self, read: &CurrentRead) -> Result<Snapshot> {
        self.require_unrestricted_read()?;
        let mut snapshot = Snapshot::empty(self.root_id());
        snapshot.collection = self.collection.clone();
        snapshot.generation = read.head().sequence;
        snapshot.storage_format = 120;
        snapshot.persistent_indexes = true;
        if let Some(state) = read.collection_state()? {
            snapshot.roots = state.configuration.roots;
            snapshot.extractor_version = state.configuration.extractor_version;
            snapshot.embedding_model = state
                .configuration
                .model
                .map(|m| m.pipeline_digest.to_string());
        }
        let mut after = None;
        let mut bytes = 0usize;
        loop {
            let page = read.lookup_paths(Lookup::Files, "", after.as_deref(), 256)?;
            for path in page.paths {
                let artifact = read
                    .artifact(&path)?
                    .context("current inventory artifact missing")?;
                bytes = bytes
                    .checked_add(serde_json::to_vec(&artifact)?.len())
                    .context("current inventory overflow")?;
                ensure!(
                    bytes <= 512 << 20 && snapshot.artifacts.len() < 100_000,
                    "current inventory admission exceeded"
                );
                snapshot.artifacts.insert(path, artifact);
            }
            after = page.next_after;
            if after.is_none() {
                break;
            }
        }
        if let Some(analysis) = read.analysis(&self.store)? {
            snapshot.analysis = Some(analysis.manifest);
            snapshot.imported_facts = analysis.facts;
            snapshot.imported_relationships = analysis.relationships;
        }
        Ok(snapshot)
    }

    pub(crate) fn current_status(&self) -> Result<Status> {
        let read = self.current_reader()?;
        self.require_index_metadata(&read)?;
        let state = read
            .collection_state()?
            .context("current collection configuration missing")?;
        let analysis = read.analysis(&self.store)?;
        let c = &state.configuration;
        let semantic_readiness = super::enrichment::readiness(&read, &self.store)?;
        let discovery_readiness = super::progress::readiness(&read, &self.store)?;
        Ok(Status {
            source_policy: self.source_policy_observation()?,
            source_availability: self.service_source_observation(),
            execution: self.execution_status()?,
            indexing: c.index_detail.into(),
            freshness: self.changes_freshness(),
            retention: crate::maintenance::RetentionObservation {
                policy: None,
                last_maintenance: None,
                error: None,
                receipt_matches_publication: false,
                pending_or_unknown: false,
            },
            publication: Some(envelope(&read)?),
            semantic_readiness,
            discovery_readiness,
            source_roots: c
                .roots
                .clone()
                .into_iter()
                .chain([(String::new(), self.root_id())])
                .collect(),
            generation: read.head().sequence,
            collection: self.collection.clone(),
            files: usize::try_from(state.counts.files)?,
            chunks: usize::try_from(state.counts.chunks)?,
            source_bytes: usize::try_from(state.counts.source_bytes)?,
            extractor_version: c.extractor_version.clone(),
            exact_fact_backend: "sqlite",
            lexical_backend: "sqlite-fts5-current",
            vector_backend: "microsoft-diskann-0.59.0/airk-current-provider",
            local_model_ready: c.model.is_some()
                && !crate::shared_models::semantic_upgrade_pending(&self.store, c.model.as_ref()),
            vectors_enabled: self.vectors_enabled()?,
            indexed_vectors: if crate::shared_models::semantic_upgrade_pending(
                &self.store,
                c.model.as_ref(),
            ) {
                0
            } else {
                read.head().graph_head.live_nodes as usize
            },
            vector_selection_digest: Some(c.vector_selection.digest()?),
            vector_eligible_artifacts: usize::try_from(state.counts.vector_eligible)?,
            vector_excluded_artifacts: usize::try_from(state.counts.vector_excluded)?,
            semantic_producer: analysis.as_ref().map(|a| a.manifest.producer.clone()),
            imported_facts: analysis.as_ref().map_or(0, |a| a.facts.len()),
            imported_relationships: analysis.as_ref().map_or(0, |a| a.relationships.len()),
        })
    }

    pub(crate) fn current_search(
        &self,
        query: &SearchQuery,
        mode: super::CurrentSearchMode,
    ) -> Result<SearchResponse> {
        self.search_response(query, mode, None)
    }

    /// Search with an optional per-hit snippet allowance applied before the total budget.
    pub fn search_response(
        &self,
        query: &SearchQuery,
        mode: super::CurrentSearchMode,
        snippet_bytes: Option<usize>,
    ) -> Result<SearchResponse> {
        self.search_response_with_filters(
            query,
            mode,
            snippet_bytes,
            &airk_core::SearchFilters::default(),
        )
    }

    /// Optional selectors apply to authenticated candidates before branch top-k.
    pub fn search_response_with_filters(
        &self,
        query: &SearchQuery,
        mode: super::CurrentSearchMode,
        snippet_bytes: Option<usize>,
        filters: &airk_core::SearchFilters,
    ) -> Result<SearchResponse> {
        self.search_response_on_read(query, mode, snippet_bytes, filters, self.current_reader()?)
    }

    /// Search and automatic definition pointers retain the same sealed reader.
    pub fn search_response_with_navigation(
        &self,
        query: &SearchQuery,
        mode: super::CurrentSearchMode,
        snippet_bytes: Option<usize>,
        filters: &airk_core::SearchFilters,
    ) -> Result<super::SearchWithDefinitionsResponse> {
        let read = self.current_reader()?;
        let evidence =
            self.search_response_on_read(query, mode, snippet_bytes, filters, read.clone())?;
        let definitions = super::search::definition_hints(&self.store, &read, query, filters)?;
        Ok(super::SearchWithDefinitionsResponse {
            evidence,
            definitions,
        })
    }

    fn search_response_on_read(
        &self,
        query: &SearchQuery,
        mode: super::CurrentSearchMode,
        snippet_bytes: Option<usize>,
        filters: &airk_core::SearchFilters,
        read: Arc<CurrentRead>,
    ) -> Result<SearchResponse> {
        let disclose_readiness = !read.selected_scope_restricts_paths();
        if matches!(
            mode,
            super::CurrentSearchMode::Vector | super::CurrentSearchMode::Hybrid
        ) {
            anyhow::ensure!(
                self.vectors_enabled()?,
                "vector search is disabled for this collection"
            );
        }
        let requested_mode = mode.into();
        let mode =
            if matches!(mode, super::CurrentSearchMode::Default) && !self.vectors_enabled()? {
                super::CurrentSearchMode::Lexical
            } else {
                mode
            };
        let evidence = super::search::search_current_read_with_filters(
            &self.store,
            read,
            None,
            mode,
            query,
            Arc::new(|_| Ok(())),
            super::search::SearchOptions {
                requested_mode: Some(requested_mode),
                snippet_bytes,
                filters,
            },
        )?;
        Ok(SearchResponse {
            execution: Some(evidence.execution),
            publication: Some(RevisionEnvelope::new(
                self.collection.clone(),
                revision(evidence.token),
            )),
            semantic_readiness: disclose_readiness.then_some(evidence.semantic_readiness),
            discovery_readiness: disclose_readiness.then_some(evidence.discovery_readiness),
            generation: evidence.token.sequence,
            collection: evidence.collection,
            hits: evidence.hits,
            truncated: evidence.truncated,
        })
    }

    pub(crate) fn current_artifact(
        &self,
        read: &CurrentRead,
        path: &str,
        source_lines: bool,
    ) -> Result<Artifact> {
        let compact = read.artifact(path)?.context("indexed file not found")?;
        let mut artifact = self.store.load_retained(&compact)?;
        let bytes = self.store.read_retained_source(&compact)?;
        if source_lines
            && artifact.extraction_method == "utf8"
            && compact
                .retained
                .as_ref()
                .and_then(|r| r.profile.as_ref())
                .is_some_and(|p| p.contract_version == 2)
        {
            artifact.chunks = airk_ingest::chunk(std::str::from_utf8(&bytes)?);
        }
        Ok(artifact)
    }

    /// Authenticate the complete retained inputs, then prepare only selected text.
    /// Sparse physical reads require the separate private-owner component policy.
    pub(crate) fn current_read_lines(
        &self,
        read: &CurrentRead,
        path: &str,
        selection: LineSelection,
        max_bytes: usize,
    ) -> Result<SearchResponse> {
        let compact = read.artifact(path)?.context("indexed file not found")?;
        if read.private_query_eligible()? {
            let selected = if let Some(lines) =
                read.observe_private_integrity(read.native_line_projection(&self.store, path))?
            {
                let (start, end) = selection.range(lines.lines())?;
                let selected = lines.read_private(start, end, max_bytes)?;
                Some((
                    start,
                    end,
                    airk_core::DocumentLocation::Lines { start, end },
                    selected.text,
                    selected.truncated,
                ))
            } else if let Some(lines) =
                read.observe_private_integrity(read.text_range_projection(&self.store, path))?
            {
                let count = if matches!(
                    selection,
                    LineSelection::Context {
                        maximum_end: true,
                        ..
                    }
                ) {
                    lines.maximum_line_end()
                } else {
                    lines.indexed_lines()
                };
                let (start, end) = selection.range(count)?;
                // Context selection uses the maximum coordinate, but retained
                // snapshot reads still enforce the final chunk end afterward.
                ensure!(
                    end <= lines.indexed_lines(),
                    "line range exceeds indexed source"
                );
                let selected = lines.read_private(start, end, max_bytes)?;
                Some((
                    start,
                    end,
                    selected.location,
                    selected.text,
                    selected.truncated,
                ))
            } else {
                None
            };
            if let Some((start, end, location, text, truncated)) = selected {
                return Ok(SearchResponse {
                    execution: None,
                    publication: Some(envelope(read)?),
                    semantic_readiness: None,
                    discovery_readiness: None,
                    generation: read.head().sequence,
                    collection: self.collection.clone(),
                    hits: vec![airk_core::Hit {
                        citation: airk_core::Citation {
                            path: path.into(),
                            content_hash: compact.content_hash,
                            start_line: start,
                            end_line: end,
                            location,
                        },
                        text,
                        truncated,
                    }],
                    truncated,
                });
            }
        }
        let artifact = self.store.load_retained(&compact)?;
        let bytes = self.store.read_retained_source(&compact)?;
        let mut metadata = self.empty_snapshot();
        metadata.generation = read.head().sequence;
        let publication = Some(envelope(read)?);
        if artifact.extraction_method == "utf8"
            && compact
                .retained
                .as_ref()
                .and_then(|r| r.profile.as_ref())
                .is_some_and(|p| p.contract_version == 2)
        {
            let source = std::str::from_utf8(&bytes)?;
            let lines = source.split_inclusive('\n').count();
            let (start, end) = selection.range(lines)?;
            airk_core::validate_budget(1, max_bytes)?;
            ensure!(start > 0 && end >= start, "invalid line range");
            ensure!(end <= lines, "line range exceeds indexed source");
            return Self::selected_lines_response(
                publication,
                &metadata,
                &artifact,
                path,
                start,
                end,
                airk_core::DocumentLocation::Lines { start, end },
                source
                    .split_inclusive('\n')
                    .take(end)
                    .enumerate()
                    .map(|(offset, line)| (offset + 1, line)),
                max_bytes,
            );
        }
        let lines = if matches!(
            selection,
            LineSelection::Context {
                maximum_end: true,
                ..
            }
        ) {
            artifact
                .chunks
                .iter()
                .map(|chunk| chunk.end_line)
                .max()
                .unwrap_or(0)
        } else {
            artifact.chunks.last().map_or(0, |chunk| chunk.end_line)
        };
        let (start, end) = selection.range(lines)?;
        Self::read_snapshot(
            publication,
            &metadata,
            &artifact,
            path,
            start,
            end,
            max_bytes,
        )
    }

    pub(crate) fn current_file_info(&self, read: &CurrentRead, path: &str) -> Result<FileInfo> {
        self.current_file_info_with_profile_families(read, path, false)
    }
    pub(crate) fn current_file_info_with_profile_families(
        &self,
        read: &CurrentRead,
        path: &str,
        include_profile_families: bool,
    ) -> Result<FileInfo> {
        let artifact = read.artifact(path)?.context("indexed file not found")?;
        let retained = artifact
            .retained
            .as_ref()
            .context("current extraction missing")?;
        let profile = retained
            .profile
            .as_ref()
            .context("current extraction profile missing")?;
        let state = read
            .collection_state()?
            .context("current configuration missing")?;
        let selection = &state.configuration.vector_selection;
        let scalars = if read.private_query_eligible()? {
            read.observe_private_integrity(read.file_scalars(path))?
        } else {
            None
        };
        let indexed_lines = match scalars {
            Some(scalars) => scalars.indexed_lines,
            None => {
                read.file_metadata(&self.store, path)?
                    .context("current file metadata missing")?
                    .indexed_lines
            }
        };
        let profile_families = if include_profile_families {
            Some(self.current_profile_families(
                read,
                path,
                &artifact.content_hash,
                profile,
                &state.configuration,
            )?)
        } else {
            None
        };
        Ok(FileInfo {
            profile_families,
            publication: Some(envelope(read)?),
            source: airk_store::roots::source_location(
                &self.root_id(),
                &state.configuration.roots,
                path,
            )?,
            generation: read.head().sequence,
            collection: self.collection.clone(),
            path: path.into(),
            content_hash: artifact.content_hash,
            source_bytes: artifact.bytes,
            language: artifact.language.clone(),
            extraction_method: artifact.extraction_method.clone(),
            parse_status: artifact.parse_status,
            chunks: retained.chunks,
            facts: retained.facts,
            indexed_lines,
            line_basis: if artifact.extraction_method == "utf8" {
                "source"
            } else {
                "extracted-text; cite original location"
            },
            vector_selection: selection.decision(path, profile, &artifact.language)?,
            vector_selection_digest: selection.digest()?,
        })
    }

    pub(crate) fn current_facts(
        &self,
        read: &CurrentRead,
        query: &airk_core::FactQuery,
        code_only: bool,
        options: &airk_core::discovery::DiscoveryOptions,
    ) -> Result<FactsResponse> {
        self.current_facts_with_options(
            read,
            query,
            code_only,
            options,
            &airk_core::discovery::FactDiscoveryOptions::default(),
        )
    }

    pub(crate) fn current_facts_with_options(
        &self,
        read: &CurrentRead,
        query: &airk_core::FactQuery,
        code_only: bool,
        options: &airk_core::discovery::DiscoveryOptions,
        fact_options: &airk_core::discovery::FactDiscoveryOptions,
    ) -> Result<FactsResponse> {
        let page = read.native_facts_page_with_options(
            &self.store,
            query,
            code_only,
            options,
            fact_options,
        )?;
        let state = read
            .collection_state()?
            .context("current configuration missing")?;
        Ok(FactsResponse {
            minimal: page.minimal,
            page: page.page,
            outline: page.outline,
            indexing: state.configuration.index_detail.into(),
            publication: Some(envelope(read)?),
            generation: read.head().sequence,
            collection: page.collection,
            facts: page.facts,
            truncated: page.truncated,
        })
    }
}

#[cfg(test)]
#[path = "range_preparation_tests.rs"]
mod range_preparation_tests;

#[cfg(test)]
#[path = "projected_consumer_tests.rs"]
mod projected_consumer_tests;
