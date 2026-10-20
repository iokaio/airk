// SPDX-License-Identifier: Apache-2.0
//! Production adapters keep the current request gate until evidence is materialized.
use crate::{FileInfo, Kernel, Status};
use airk_core::evidence::RevisionEnvelope;
use airk_core::identity::RevisionToken;
use airk_core::{Artifact, FactsResponse, SearchQuery, SearchResponse, Snapshot};
use airk_store::current::{CurrentRead, CurrentScope, CurrentToken, postings::Lookup};
use anyhow::{Context, Result, ensure};
use std::sync::Arc;

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
    pub fn maintain_current(&self) -> Result<airk_store::current::maintenance::ReclamationReport> {
        let scope = self.current_scope()?.context("workspace is not indexed")?;
        let quota = airk_store::quota::Quota::open(&self.data_root)?;
        super::recover_current(&self.store, &quota, &scope)?;
        let writer = self.store.lock()?;
        let read = self.store.current_read(&scope)?;
        let plan = read.reclamation_plan(&self.store)?;
        drop(read);
        let capacity = self.store.current_database_capacity(&scope)?;
        let mut current = self
            .store
            .current_write(&writer, &quota, &scope, capacity)?;
        current.reclaim_lexical_candidates()?;
        current.reclaim_objects(&plan)
    }

    pub(crate) fn current_scope(&self) -> Result<Option<CurrentScope>> {
        self.store.current_scope(&self.root_id(), &self.collection)
    }

    pub(crate) fn current_reader(&self) -> Result<Arc<CurrentRead>> {
        let scope = self
            .current_scope()?
            .context("workspace is not indexed; run airk index first")?;
        self.store.current_read(&scope)
    }

    /// A bounded compact scan projection, never a persisted snapshot or a live-source read.
    pub(crate) fn current_scan_projection(&self, read: &CurrentRead) -> Result<Snapshot> {
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
        let state = read
            .collection_state()?
            .context("current collection configuration missing")?;
        let analysis = read.analysis(&self.store)?;
        let c = &state.configuration;
        let semantic_readiness = super::enrichment::readiness(&read, &self.store)?;
        let discovery_readiness = super::progress::readiness(&read, &self.store)?;
        Ok(Status {
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
            lexical_backend: "tantivy-current",
            vector_backend: "microsoft-diskann-0.59.0/airk-current-provider",
            local_model_ready: c.model.is_some(),
            vectors_enabled: self.vectors_enabled()?,
            indexed_vectors: read.head().graph_head.live_nodes as usize,
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
        let scope = self
            .current_scope()?
            .context("workspace is not indexed; run airk index first")?;
        let evidence =
            super::search_current(&self.store, &scope, None, mode, query, Arc::new(|_| Ok(())))?;
        Ok(SearchResponse {
            publication: Some(RevisionEnvelope::new(
                self.collection.clone(),
                revision(evidence.token),
            )),
            semantic_readiness: Some(evidence.semantic_readiness),
            discovery_readiness: Some(evidence.discovery_readiness),
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

    pub(crate) fn current_file_info(&self, read: &CurrentRead, path: &str) -> Result<FileInfo> {
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
        let indexed_lines = read
            .file_metadata(&self.store, path)?
            .context("current file metadata missing")?
            .indexed_lines;
        Ok(FileInfo {
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
        let page = read.native_facts_page(&self.store, query, code_only, options)?;
        let state = read
            .collection_state()?
            .context("current configuration missing")?;
        Ok(FactsResponse {
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
