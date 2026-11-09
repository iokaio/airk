// SPDX-License-Identifier: Apache-2.0
//! Bounded immutable relationship/import pages owned by the current publication.
use super::{
    analysis::AnalysisEvidence,
    native_projection::ProjectionPreparationLimit,
    resources::{PreparedResource, ResourceReference, prepare_resource},
};
use crate::FileStore;
use airk_core::{
    FactHit, RelationshipHit,
    compression::CompressionMode,
    identity::{Sha256Digest, framed_digest},
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};

mod selectors;
use super::native_projection::bounded_encoding;
pub use selectors::SelectedPath;
#[cfg(test)]
pub(crate) use selectors::select_paths;
use std::collections::BTreeMap;

#[cfg(all(test, windows))]
thread_local! {
    static QUERY_PAGE_READS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}
#[cfg(all(test, windows))]
pub(crate) fn query_page_read_count() -> usize {
    QUERY_PAGE_READS.get()
}

const ROWS: usize = 256;
const PAGE_BYTES: usize = 256 << 10;
const ROSTER_ROWS: usize = 256;
const MAX_BYTES: usize = 64 << 20;
const MAX_ROW: usize = 16 << 20;
const MAX_HEADER: usize = 1 << 20;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", deny_unknown_fields)]
pub enum ProjectionBinding {
    NativeEdges {
        path: String,
        source: Sha256Digest,
        payload: Sha256Digest,
        profile: Sha256Digest,
        metadata: Sha256Digest,
    },
    Imported {
        analysis: Sha256Digest,
        manifest: Sha256Digest,
        inventory: Sha256Digest,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectionSummary {
    pub version: u32,
    pub facts: usize,
    pub edges: usize,
    pub pages: usize,
    pub metadata_bytes: usize,
    pub metadata_objects: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectionHeader {
    pub version: u32,
    pub binding: ProjectionBinding,
    pub facts: usize,
    pub edges: usize,
    pub pages: usize,
    pub roster: Vec<ResourceReference>,
    pub selectors: Option<ResourceReference>,
    pub selector_keys: usize,
    pub metadata_bytes: usize,
    pub metadata_objects: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", deny_unknown_fields)]
pub enum ProjectionRow {
    Fact {
        ordinal: usize,
        path_ordinal: usize,
        hit: Box<FactHit>,
    },
    Edge {
        ordinal: usize,
        hit: Box<RelationshipHit>,
    },
}
#[derive(Serialize)]
#[serde(tag = "kind")]
enum RowRef<'a> {
    Fact {
        ordinal: usize,
        path_ordinal: usize,
        hit: &'a FactHit,
    },
    Edge {
        ordinal: usize,
        hit: &'a RelationshipHit,
    },
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Page<T> {
    version: u32,
    page: usize,
    rows: Vec<T>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Roster {
    version: u32,
    start: usize,
    leaves: Vec<ResourceReference>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QueryProducerCertificate {
    pub version: u32,
    pub manifest: Sha256Digest,
    pub binding: Sha256Digest,
    pub summary: ProjectionSummary,
}
impl ProjectionSummary {
    pub(crate) fn validate(&self) -> Result<()> {
        ensure!(
            self.version == 1
                && self.facts <= 100_000
                && self.edges <= 100_000
                && self.pages <= self.facts.saturating_add(self.edges)
                && self.metadata_bytes <= MAX_BYTES
                && self.metadata_objects <= 2_000_000
                && ((self.metadata_bytes == 0) == (self.metadata_objects == 0)),
            "invalid query projection admission summary"
        );
        Ok(())
    }
}
impl QueryProducerCertificate {
    pub(crate) fn validate(&self) -> Result<()> {
        ensure!(
            self.version == 1,
            "unsupported query projection certificate"
        );
        self.summary.validate()
    }
}
impl ProjectionHeader {
    fn summary(&self) -> ProjectionSummary {
        ProjectionSummary {
            version: 1,
            facts: self.facts,
            edges: self.edges,
            pages: self.pages,
            metadata_bytes: self.metadata_bytes,
            metadata_objects: self.metadata_objects,
        }
    }
}
fn native_binding(
    artifact: &airk_core::Artifact,
    metadata: Sha256Digest,
) -> Result<ProjectionBinding> {
    let retained = artifact
        .retained
        .as_ref()
        .context("native edge extraction missing")?;
    ensure!(
        artifact.facts.len() == retained.facts,
        "native edge extraction count mismatch"
    );
    Ok(ProjectionBinding::NativeEdges {
        path: artifact.path.clone(),
        source: artifact.content_hash.parse()?,
        payload: retained.object.parse()?,
        profile: retained
            .profile
            .as_ref()
            .context("native edge profile missing")?
            .identity()?,
        metadata,
    })
}
fn imported_binding(
    evidence: &AnalysisEvidence,
    analysis: &ResourceReference,
) -> Result<ProjectionBinding> {
    Ok(ProjectionBinding::Imported {
        analysis: analysis.digest,
        manifest: framed_digest(
            b"analysis-projection-manifest/1",
            &[&serde_json::to_vec(&evidence.manifest)?],
        ),
        inventory: framed_digest(
            b"analysis-projection-inventory/1",
            &[&serde_json::to_vec(&evidence.manifest.source_hashes)?],
        ),
    })
}
fn native_edges(artifact: &airk_core::Artifact) -> Result<Vec<RelationshipHit>> {
    let mut edges = Vec::new();
    let mut size = 0usize;
    super::relationships::visit_edges(artifact, |edge| {
        size = size.saturating_add(bounded_encoding(&edge)?.len());
        if size > MAX_BYTES / 4 {
            return Err(ProjectionPreparationLimit.into());
        }
        edges.push(edge);
        Ok(())
    })?;
    super::relationships::sort_edges(&mut edges);
    Ok(edges)
}

/// Sealed exact encoded resources; constructing a plan never writes to storage.
pub struct PreparedQueryProjection {
    pub(crate) resources: Vec<PreparedResource>,
    pub(crate) root: ResourceReference,
    memory: usize,
}

/// A request-held selected component, not a private-owner or audit capability.
pub struct CurrentQueryProjection<'a> {
    read: &'a super::CurrentRead,
    store: &'a FileStore,
    reference: ResourceReference,
    header: ProjectionHeader,
    roster_cache: std::cell::RefCell<Option<(usize, Vec<ResourceReference>)>>,
    // Request-local immutable pages. Imported selectors for multiple paths or
    // traversal symbols often share a physical page; authenticate/decode once.
    query_pages: std::cell::RefCell<BTreeMap<usize, std::sync::Arc<Vec<ProjectionRow>>>>,
    query_bytes: std::cell::Cell<usize>,
    query_limit: usize,
}
impl CurrentQueryProjection<'_> {
    pub fn header(&self) -> &ProjectionHeader {
        &self.header
    }
    pub fn reference(&self) -> &ResourceReference {
        &self.reference
    }
    fn leaf_reference(&self, page: usize) -> Result<ResourceReference> {
        ensure!(page < self.header.pages, "projection page unavailable");
        let block = page / ROSTER_ROWS;
        let mut cache = self.roster_cache.borrow_mut();
        if cache
            .as_ref()
            .is_none_or(|(selected, _)| *selected != block)
        {
            *cache = Some((block, self.store.projection_roster(&self.header, block)?));
        }
        Ok(
            cache.as_ref().context("projection roster cache missing")?.1[page % ROSTER_ROWS]
                .clone(),
        )
    }
    pub fn page(&self, page: usize) -> Result<Vec<ProjectionRow>> {
        self.read.token()?;
        let rows = self
            .store
            .read_projection_page(&self.leaf_reference(page)?, page)?;
        if !self.read.restricts_paths() {
            return Ok(rows);
        }
        let mut selected = Vec::new();
        for row in rows {
            let allowed = match &row {
                ProjectionRow::Fact { hit, .. } => {
                    let allowed = self.read.accepts_path(&hit.citation.path);
                    if allowed {
                        self.read.require_imported_fact(self.store, hit)?;
                    }
                    allowed
                }
                ProjectionRow::Edge { hit, .. } => {
                    self.read.accepts_path(&hit.citation.path)
                        && self.read.accepts_endpoint(self.store, &hit.source)?
                        && self.read.accepts_endpoint(self.store, &hit.target)?
                }
            };
            if allowed {
                selected.push(row);
            }
        }
        self.read.token()?;
        Ok(selected)
    }
    /// Definition-origin proof uses authenticated raw Fact rows. Calling page()
    /// here would recurse through graph/fact admission and could self-authorize
    /// a reference carrying another source's definition ID.
    pub(crate) fn definition_origin(&self, endpoint: &str) -> Result<bool> {
        use super::postings::Lookup;
        use airk_core::scoped_reads::ScopeReadRefused;
        let selected = self.lookup_paths(Lookup::FactName(endpoint), "", None, 129)?;
        ensure!(selected.len() <= 128, ScopeReadRefused);
        self.read.charge_scope_origin(selected.len(), 0, 0, 0)?;
        let mut pages = BTreeMap::<usize, BTreeMap<String, Sha256Digest>>::new();
        for path in selected {
            for page in path.pages {
                pages
                    .entry(page)
                    .or_default()
                    .insert(path.path.clone(), path.source);
            }
        }
        for (page, paths) in pages {
            let reference = self.leaf_reference(page)?;
            let rows = self
                .read
                .origin_projection_page(self.store, &reference, page)?;
            self.read.charge_scope_origin(0, 0, 0, rows.len())?;
            for row in rows.iter() {
                if let ProjectionRow::Fact { hit, .. } = row
                    && hit.kind == "definition"
                    && self.read.accepts_path(&hit.citation.path)
                    && paths
                        .get(&hit.citation.path)
                        .is_some_and(|hash| hit.citation.content_hash == hash.to_string())
                    && hit
                        .code
                        .as_ref()
                        .is_some_and(|code| code.symbol_id == endpoint)
                {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }
    pub(crate) fn query_page(&self, page: usize) -> Result<std::sync::Arc<Vec<ProjectionRow>>> {
        self.read.token()?;
        if let Some(rows) = self.query_pages.borrow().get(&page).cloned() {
            return Ok(rows);
        }
        let reference = self.leaf_reference(page)?;
        let bytes = self
            .query_bytes
            .get()
            .checked_add(reference.bytes)
            .context("query projection page byte overflow")?;
        ensure!(
            bytes <= self.query_limit,
            "query projection page preparation exceeds producer bound"
        );
        let rows = std::sync::Arc::new(self.page(page)?);
        self.query_bytes.set(bytes);
        self.query_pages.borrow_mut().insert(page, rows.clone());
        #[cfg(all(test, windows))]
        QUERY_PAGE_READS.set(QUERY_PAGE_READS.get() + 1);
        Ok(rows)
    }
    pub fn check_certificate(&self, certificate: &QueryProducerCertificate) -> Result<()> {
        self.read.token()?;
        ensure!(
            certificate.version == 1
                && certificate.manifest == self.reference.digest
                && certificate.summary == self.header.summary()
                && certificate.binding
                    == framed_digest(
                        b"query-projection-binding/1",
                        &[&serde_json::to_vec(&self.header.binding)?]
                    ),
            "query projection certificate expired"
        );
        Ok(())
    }
    /// The owning audit charges the admitted header before constructing this
    /// reader. This turn counts that header plus actual roster/page reads.
    pub fn audit_physical(
        &self,
        certificate: &QueryProducerCertificate,
        start: usize,
        max_pages: usize,
        max_bytes: u64,
        mut checkpoint: impl FnMut() -> Result<()>,
    ) -> Result<QueryPhysicalTurn> {
        self.check_certificate(certificate)?;
        ensure!(
            start <= self.header.pages && (1..=128).contains(&max_pages),
            "invalid query projection audit cursor"
        );
        let mut turn = QueryPhysicalTurn {
            next_page: start,
            checked_pages: 0,
            data_bytes: 0,
            checked_bytes: self.reference.bytes as u64,
            complete: false,
            required_bytes: None,
        };
        if turn.checked_bytes > max_bytes {
            turn.required_bytes = Some(turn.checked_bytes);
            return Ok(turn);
        }
        while turn.next_page < self.header.pages && turn.checked_pages < max_pages {
            checkpoint()?;
            let page = turn.next_page;
            let block = page / ROSTER_ROWS;
            let roster_bytes = self.header.roster[block].bytes as u64;
            let cached = self
                .roster_cache
                .borrow()
                .as_ref()
                .is_some_and(|(selected, _)| *selected == block);
            let metadata_bytes = if cached { 0 } else { roster_bytes };
            if turn.checked_bytes.saturating_add(metadata_bytes) > max_bytes {
                turn.required_bytes = Some(self.reference.bytes as u64 + roster_bytes);
                break;
            }
            let reference = self.leaf_reference(page)?;
            turn.checked_bytes += metadata_bytes;
            let required = self.reference.bytes as u64 + roster_bytes + reference.bytes as u64;
            if turn.checked_bytes.saturating_add(reference.bytes as u64) > max_bytes {
                turn.required_bytes = Some(required);
                break;
            }
            self.store.read_projection_page(&reference, page)?;
            turn.checked_bytes += reference.bytes as u64;
            turn.data_bytes += reference.bytes as u64;
            checkpoint()?;
            turn.checked_pages += 1;
            turn.next_page += 1;
        }
        turn.complete = turn.next_page == self.header.pages;
        Ok(turn)
    }
}

#[derive(Clone, Debug)]
pub struct QueryPhysicalTurn {
    pub data_bytes: u64,
    pub next_page: usize,
    pub checked_pages: usize,
    pub checked_bytes: u64,
    pub complete: bool,
    pub required_bytes: Option<u64>,
}

impl super::CurrentRead {
    /// A producer declaration, not query authority. The owning native policy
    /// must admit consumers before this can replace strict evidence checks.
    pub fn native_edges_declared_empty(&self, path: &str) -> Result<bool> {
        let Some(bytes) = self.map_get(super::map::MapDomain::FileMetadata, path.as_bytes())?
        else {
            return Ok(false);
        };
        let metadata: super::metadata::FileMetadataRef = serde_json::from_slice(&bytes)?;
        ensure!(
            metadata.path == path
                && serde_json::to_vec(&metadata)? == bytes
                && (!metadata.native_edges_empty
                    || (metadata.native_edges.is_none() && metadata.native_facts.is_some())),
            "invalid empty native edge declaration"
        );
        Ok(metadata.native_edges_empty)
    }
    pub(crate) fn native_edge_reference(
        &self,
        path: &str,
    ) -> Result<Option<(ResourceReference, Sha256Digest)>> {
        let Some(bytes) = self.map_get(super::map::MapDomain::FileMetadata, path.as_bytes())?
        else {
            return Ok(None);
        };
        let metadata: super::metadata::FileMetadataRef = serde_json::from_slice(&bytes)?;
        ensure!(
            metadata.path == path && serde_json::to_vec(&metadata)? == bytes,
            "native edge metadata binding mismatch"
        );
        Ok(metadata
            .native_edges
            .map(|reference| (reference, metadata.digest)))
    }
    pub fn import_certification_bytes(&self) -> Result<Option<u64>> {
        let analysis = self.resource_reference("analysis")?;
        let projection = self.resource_reference("analysis-projection")?;
        let Some(analysis) = analysis else {
            ensure!(projection.is_none(), "projected analysis missing");
            return Ok(None);
        };
        let Some(projection) = projection else {
            return Ok(Some(analysis.bytes as u64));
        };
        let summary = projection
            .projection
            .as_ref()
            .context("import projection admission summary missing")?;
        (analysis.bytes as u64)
            .checked_add(projection.bytes as u64)
            .and_then(|bytes| bytes.checked_add(summary.metadata_bytes as u64))
            .map(Some)
            .context("import projection certification size overflow")
    }
    pub fn certify_imported_projection(
        &self,
        store: &FileStore,
        mut checkpoint: impl FnMut() -> Result<()>,
    ) -> Result<Option<QueryProducerCertificate>> {
        let Some(projection) = self.resource_reference("analysis-projection")? else {
            return Ok(None);
        };
        let analysis = self
            .resource_reference("analysis")?
            .context("projected analysis missing")?;
        let evidence = self
            .analysis(store)?
            .context("projected analysis missing")?;
        let state = self
            .collection_state()?
            .context("analysis collection missing")?;
        ensure!(
            state.counts.files == evidence.manifest.source_hashes.len() as u64
                && state.configuration.extractor_version == evidence.manifest.extractor_version,
            "analysis projection source inventory incomplete"
        );
        let mut languages = BTreeMap::new();
        for (path, hash) in &evidence.manifest.source_hashes {
            checkpoint()?;
            let artifact = self
                .artifact(path)?
                .context("analysis projection artifact missing")?;
            ensure!(
                artifact.content_hash == *hash,
                "analysis projection source expired"
            );
            languages.insert(path.clone(), artifact.language);
        }
        Ok(Some(store.certify_import_projection(
            &projection,
            &evidence,
            &analysis,
            &languages,
        )?))
    }

    pub fn native_edge_projection<'a>(
        &'a self,
        store: &'a FileStore,
        path: &str,
    ) -> Result<Option<CurrentQueryProjection<'a>>> {
        let Some(bytes) = self.map_get(super::map::MapDomain::FileMetadata, path.as_bytes())?
        else {
            return Ok(None);
        };
        let metadata: super::metadata::FileMetadataRef = serde_json::from_slice(&bytes)?;
        ensure!(
            metadata.path == path && serde_json::to_vec(&metadata)? == bytes,
            "native edge metadata binding mismatch"
        );
        let Some(reference) = metadata.native_edges else {
            return Ok(None);
        };
        ensure!(
            self.query_projection_version()?
                .is_some_and(|version| version >= 2),
            "native edge capability missing"
        );
        let artifact = self
            .artifact(path)?
            .context("native edge artifact missing")?;
        let retained = artifact
            .retained
            .as_ref()
            .context("native edge extraction missing")?;
        let header = store.projection_header(&reference)?;
        let binding = ProjectionBinding::NativeEdges {
            path: path.into(),
            source: artifact.content_hash.parse()?,
            payload: retained.object.parse()?,
            profile: retained
                .profile
                .as_ref()
                .context("native edge profile missing")?
                .identity()?,
            metadata: metadata.digest,
        };
        ensure!(
            header.binding == binding && header.facts == 0,
            "native edge component expired"
        );
        let query_limit = MAX_BYTES / 4 + (header.edges + header.pages) * 128;
        Ok(Some(CurrentQueryProjection {
            read: self,
            store,
            reference,
            header,
            roster_cache: Default::default(),
            query_pages: Default::default(),
            query_bytes: Default::default(),
            query_limit,
        }))
    }

    pub fn imported_projection<'a>(
        &'a self,
        store: &'a FileStore,
    ) -> Result<Option<CurrentQueryProjection<'a>>> {
        let Some(reference) = self.resource_reference("analysis-projection")? else {
            return Ok(None);
        };
        ensure!(
            self.query_projection_version()?
                .is_some_and(|version| version >= 2),
            "import projection capability missing"
        );
        let analysis = self
            .resource_reference("analysis")?
            .context("projected analysis missing")?;
        let header = store.projection_header(&reference)?;
        ensure!(
            matches!(header.binding, ProjectionBinding::Imported { analysis: digest, .. } if digest == analysis.digest),
            "import projection component expired"
        );
        ensure!(
            analysis.bytes <= MAX_BYTES,
            "projected analysis exceeds producer bound"
        );
        let query_limit = analysis.bytes + (header.facts + header.edges + header.pages) * 128;
        Ok(Some(CurrentQueryProjection {
            read: self,
            store,
            reference,
            header,
            roster_cache: Default::default(),
            query_pages: Default::default(),
            query_bytes: Default::default(),
            query_limit,
        }))
    }
}
impl PreparedQueryProjection {
    pub fn memory_bytes(&self) -> usize {
        self.memory
    }
}

struct Builder<'a> {
    mode: CompressionMode,
    resources: Vec<PreparedResource>,
    leaves: Vec<ResourceReference>,
    rows: Vec<Vec<u8>>,
    row_bytes: usize,
    memory: usize,
    selectors: selectors::Selectors,
    languages: Option<&'a BTreeMap<String, String>>,
    audit: bool,
    fact_ordinals: BTreeMap<String, usize>,
}
impl<'a> Builder<'a> {
    fn new(mode: CompressionMode, languages: Option<&'a BTreeMap<String, String>>) -> Self {
        Self {
            mode,
            resources: vec![],
            leaves: vec![],
            rows: vec![],
            row_bytes: 0,
            memory: 0,
            selectors: Default::default(),
            languages,
            audit: false,
            fact_ordinals: BTreeMap::new(),
        }
    }
    fn audit(languages: Option<&'a BTreeMap<String, String>>) -> Self {
        let mut builder = Self::new(CompressionMode::Off, languages);
        builder.audit = true;
        builder
    }
    fn resource(&mut self, bytes: Vec<u8>) -> Result<ResourceReference> {
        let prepared = prepare_resource(bytes, self.mode)?;
        self.memory = self
            .memory
            .checked_add(prepared.memory_bytes())
            .context("projection memory overflow")?;
        if self.memory > MAX_BYTES {
            return Err(ProjectionPreparationLimit.into());
        }
        let reference = prepared.reference.clone();
        self.resources.push(prepared);
        Ok(reference)
    }
    fn push_fact(&mut self, ordinal: usize, hit: &FactHit) -> Result<()> {
        let path = &hit.citation.path;
        let path_ordinal = self.fact_ordinals.get(path).copied().unwrap_or(0);
        self.push(RowRef::Fact {
            ordinal,
            path_ordinal,
            hit,
        })?;
        if let Some(count) = self.fact_ordinals.get_mut(path) {
            *count += 1;
        } else {
            self.fact_ordinals.insert(path.clone(), 1);
        }
        Ok(())
    }
    fn push(&mut self, row: RowRef<'_>) -> Result<()> {
        let bytes = super::native_projection::bounded_encoding(&row)?;
        if bytes.len() > MAX_ROW {
            return Err(ProjectionPreparationLimit.into());
        }
        if !self.rows.is_empty()
            && (self.rows.len() == ROWS
                || self
                    .row_bytes
                    .saturating_add(bytes.len())
                    .saturating_add(128)
                    > PAGE_BYTES)
        {
            self.flush()?;
        }
        if self
            .memory
            .saturating_add(self.row_bytes)
            .saturating_add(bytes.capacity())
            > MAX_BYTES
        {
            return Err(ProjectionPreparationLimit.into());
        }
        let page = self.leaves.len();
        match &row {
            RowRef::Fact { hit, .. } => {
                let language = self
                    .languages
                    .and_then(|languages| languages.get(&hit.citation.path))
                    .map_or(hit.language.as_str(), String::as_str);
                self.selectors.fact(hit, language, page)?;
            }
            RowRef::Edge { hit, .. } => self.selectors.edge(hit, page)?,
        }
        if self
            .memory
            .saturating_add(self.selectors.memory_bytes())
            .saturating_add(self.row_bytes)
            .saturating_add(bytes.capacity())
            > MAX_BYTES
        {
            return Err(ProjectionPreparationLimit.into());
        }
        self.row_bytes += bytes.len() + 1;
        self.rows.push(bytes);
        Ok(())
    }
    fn flush(&mut self) -> Result<()> {
        if self.rows.is_empty() {
            return Ok(());
        }
        let mut bytes =
            format!("{{\"version\":1,\"page\":{},\"rows\":[", self.leaves.len()).into_bytes();
        bytes
            .try_reserve_exact(self.row_bytes.saturating_add(2))
            .map_err(|_| ProjectionPreparationLimit)?;
        for (i, row) in self.rows.drain(..).enumerate() {
            if i > 0 {
                bytes.push(b',');
            }
            bytes.extend(row);
        }
        bytes.extend_from_slice(b"]}");
        self.row_bytes = 0;
        let leaf = if self.audit {
            ResourceReference {
                digest: framed_digest(b"current-resource/1", &[&bytes]),
                bytes: bytes.len(),
                encoding: None,
                projection: None,
            }
        } else {
            self.resource(bytes)?
        };
        self.memory = self
            .memory
            .saturating_add(std::mem::size_of::<ResourceReference>() + leaf.heap_bytes());
        if self.memory > MAX_BYTES {
            return Err(ProjectionPreparationLimit.into());
        }
        self.leaves.push(leaf);
        Ok(())
    }
    fn certify(
        mut self,
        store: &FileStore,
        root: &ResourceReference,
        binding: ProjectionBinding,
        facts: usize,
        edges: usize,
    ) -> Result<QueryProducerCertificate> {
        ensure!(
            self.audit,
            "certification requires bounded digest preparation"
        );
        self.flush()?;
        let header = store.projection_header(root)?;
        ensure!(
            header.binding == binding
                && header.facts == facts
                && header.edges == edges
                && header.pages == self.leaves.len(),
            "projection logical binding mismatch"
        );
        let mut budget = selectors::MetadataBudget::new(header.metadata_bytes);
        for block in 0..header.roster.len() {
            budget.charge(&header.roster[block])?;
            let actual = store.projection_roster(&header, block)?;
            for (offset, leaf) in actual.iter().enumerate() {
                let expected = &self.leaves[block * ROSTER_ROWS + offset];
                ensure!(
                    leaf.projection.is_none()
                        && leaf.digest == expected.digest
                        && leaf.bytes == expected.bytes,
                    "projection logical page membership mismatch"
                );
            }
        }
        self.selectors.verify(store, &header, &mut budget)?;
        budget.finish(&header)?;
        Ok(QueryProducerCertificate {
            version: 1,
            manifest: root.digest,
            binding: framed_digest(
                b"query-projection-binding/1",
                &[&serde_json::to_vec(&header.binding)?],
            ),
            summary: header.summary(),
        })
    }

    fn finish(
        mut self,
        binding: ProjectionBinding,
        facts: usize,
        edges: usize,
    ) -> Result<PreparedQueryProjection> {
        self.flush()?;
        let mut roster = Vec::new();
        let pages = self.leaves.len();
        let data_objects = self.resources.len();
        for start in (0..pages).step_by(ROSTER_ROWS) {
            let end = pages.min(start + ROSTER_ROWS);
            let bytes = super::native_projection::bounded_encoding(&Roster {
                version: 1,
                start,
                leaves: self.leaves[start..end].to_vec(),
            })?;
            roster.push(self.resource(bytes)?);
        }
        let (selectors, selector_keys) = std::mem::take(&mut self.selectors).finish(&mut self)?;
        let metadata_objects = self.resources.len() - data_objects;
        let metadata_bytes =
            self.resources[data_objects..]
                .iter()
                .try_fold(0usize, |sum, value| {
                    sum.checked_add(value.reference.bytes)
                        .context("projection metadata bytes overflow")
                })?;
        if metadata_bytes > MAX_BYTES {
            return Err(ProjectionPreparationLimit.into());
        }
        let bytes = super::native_projection::bounded_encoding(&ProjectionHeader {
            version: 1,
            binding,
            facts,
            edges,
            pages,
            roster,
            selectors,
            selector_keys,
            metadata_bytes,
            metadata_objects,
        })?;
        if bytes.len() > MAX_HEADER {
            return Err(ProjectionPreparationLimit.into());
        }
        let mut root = self.resource(bytes)?;
        root.projection = Some(ProjectionSummary {
            version: 1,
            facts,
            edges,
            pages,
            metadata_bytes,
            metadata_objects,
        });
        Ok(PreparedQueryProjection {
            resources: self.resources,
            root,
            memory: self.memory,
        })
    }
}

impl FileStore {
    pub fn prepare_import_projection(
        &self,
        evidence: &AnalysisEvidence,
        analysis: &ResourceReference,
        languages: &BTreeMap<String, String>,
        mode: CompressionMode,
    ) -> Result<PreparedQueryProjection> {
        let bytes = evidence.canonical_bytes()?;
        ensure!(
            analysis.bytes == bytes.len()
                && analysis.digest == framed_digest(b"current-resource/1", &[&bytes]),
            "import projection differs from authoritative analysis"
        );
        let binding = imported_binding(evidence, analysis)?;
        drop(bytes);
        let mut builder = Builder::new(mode, Some(languages));
        for (ordinal, hit) in evidence.facts.iter().enumerate() {
            builder.push_fact(ordinal, hit)?;
        }
        // Fact and relationship page boundaries remain independent components.
        builder.flush()?;
        for (ordinal, hit) in evidence.relationships.iter().enumerate() {
            builder.push(RowRef::Edge { ordinal, hit })?;
        }
        builder.finish(binding, evidence.facts.len(), evidence.relationships.len())
    }

    pub fn prepare_native_edge_projection(
        &self,
        artifact: &airk_core::Artifact,
        metadata: Sha256Digest,
        mode: CompressionMode,
    ) -> Result<PreparedQueryProjection> {
        let binding = native_binding(artifact, metadata)?;
        let edges = native_edges(artifact)?;
        let mut builder = Builder::new(mode, None);
        for (ordinal, hit) in edges.iter().enumerate() {
            builder.push(RowRef::Edge { ordinal, hit })?;
        }
        builder.finish(binding, 0, edges.len())
    }

    pub(crate) fn stage_query_projection(
        &self,
        writer: &crate::WriterLock,
        quota: &crate::quota::Quota,
        plan: &PreparedQueryProjection,
    ) -> Result<Option<(ResourceReference, Option<crate::quota::Reservation>)>> {
        super::gate::check_writer(self, writer)?;
        let mut values = std::collections::BTreeMap::new();
        let mut required = 0u64;
        for value in &plan.resources {
            if let std::collections::btree_map::Entry::Vacant(entry) =
                values.entry(value.reference.object_digest()?)
            {
                if !self.prepared_resource_exists(value)? {
                    required = required
                        .checked_add(value.reference.bytes as u64)
                        .context("projection quota overflow")?;
                }
                entry.insert(value);
            }
        }
        let reservation = if required > 0 {
            match quota.reserve(self.collection_id()?, required) {
                Ok(reservation) => {
                    reservation.validate_collection_directory(self.directory())?;
                    Some(reservation)
                }
                Err(error) if crate::capacity::is_storage_pressure(&error) => return Ok(None),
                Err(error) => return Err(error),
            }
        } else {
            None
        };
        if required > 0 {
            writer.invalidate_census_before_mutation()?;
        }
        for value in values.values() {
            self.retain_prepared_resource(value, reservation.as_ref(), None)?;
        }
        Ok(Some((plan.root.clone(), reservation)))
    }

    pub(crate) fn analysis_for_projection(
        &self,
        plan: &super::map::MapPlan<'_>,
        scope: &super::CurrentScope,
        reference: &ResourceReference,
    ) -> Result<(AnalysisEvidence, BTreeMap<String, String>)> {
        ensure!(
            reference.bytes <= MAX_BYTES,
            "analysis producer bound exceeded"
        );
        let bytes = self.read_current_resource(reference)?;
        let evidence: AnalysisEvidence = serde_json::from_slice(&bytes)?;
        ensure!(
            evidence.canonical_bytes()? == bytes
                && evidence.manifest.root == scope.root
                && evidence.manifest.collection == scope.collection,
            "foreign analysis projection source"
        );
        let collection = super::collection::CollectionState::decode(
            &plan
                .get(super::map::key(
                    scope,
                    super::map::MapDomain::Metadata,
                    b"collection",
                )?)?
                .context("analysis collection missing")?,
        )?;
        ensure!(
            collection.counts.files == evidence.manifest.source_hashes.len() as u64
                && collection.configuration.extractor_version
                    == evidence.manifest.extractor_version,
            "analysis projection source inventory incomplete"
        );
        let mut languages = BTreeMap::new();
        for (path, hash) in &evidence.manifest.source_hashes {
            let bytes = plan
                .get(super::map::key(
                    scope,
                    super::map::MapDomain::Artifact,
                    path.as_bytes(),
                )?)?
                .context("analysis projection artifact missing")?;
            let input: super::CapturedInput = serde_json::from_slice(&bytes)?;
            ensure!(
                input.bytes()? == bytes
                    && input.artifact.path == *path
                    && input.artifact.content_hash == *hash,
                "analysis projection source expired"
            );
            languages.insert(path.clone(), input.artifact.language);
        }
        Ok((evidence, languages))
    }

    pub(crate) fn verify_import_projection(
        &self,
        reference: &ResourceReference,
        evidence: &AnalysisEvidence,
        analysis: &ResourceReference,
        languages: &BTreeMap<String, String>,
    ) -> Result<()> {
        let header = self.projection_header(reference)?;
        let binding = imported_binding(evidence, analysis)?;
        ensure!(
            header.binding == binding
                && header.facts == evidence.facts.len()
                && header.edges == evidence.relationships.len(),
            "import projection binding mismatch"
        );
        let (mut facts, mut edges) = (0, 0);
        let mut path_ordinals = BTreeMap::<String, usize>::new();
        let mut selectors = selectors::Selectors::default();
        self.visit_projection_rows(&header, |page, row| {
            match row {
                ProjectionRow::Fact {
                    ordinal,
                    path_ordinal,
                    hit,
                } => {
                    let expected_rank = path_ordinals.entry(hit.citation.path.clone()).or_default();
                    ensure!(
                        path_ordinal == *expected_rank,
                        "import path ordinal mismatch"
                    );
                    *expected_rank += 1;
                    ensure!(edges == 0 && ordinal == facts, "import fact order mismatch");
                    let expected = evidence.facts.get(facts).context("extra projected fact")?;
                    ensure!(
                        serde_json::to_vec(expected)? == serde_json::to_vec(&hit)?,
                        "import fact differs from producer"
                    );
                    selectors.fact(
                        &hit,
                        languages
                            .get(&hit.citation.path)
                            .context("import selector language missing")?,
                        page,
                    )?;
                    facts += 1;
                }
                ProjectionRow::Edge { ordinal, hit } => {
                    ensure!(
                        facts == evidence.facts.len() && ordinal == edges,
                        "import edge order mismatch"
                    );
                    let expected = evidence
                        .relationships
                        .get(edges)
                        .context("extra projected edge")?;
                    ensure!(
                        serde_json::to_vec(expected)? == serde_json::to_vec(&hit)?,
                        "import edge differs from producer"
                    );
                    selectors.edge(&hit, page)?;
                    edges += 1;
                }
            }
            Ok(())
        })?;
        ensure!(
            facts == evidence.facts.len() && edges == evidence.relationships.len(),
            "incomplete import projection"
        );
        let mut budget = selectors::MetadataBudget::new(header.metadata_bytes);
        for roster in &header.roster {
            budget.charge(roster)?;
        }
        selectors.verify(self, &header, &mut budget)?;
        budget.finish(&header)?;
        Ok(())
    }

    pub(crate) fn certify_native_edges(
        &self,
        reference: &ResourceReference,
        artifact: &airk_core::Artifact,
        metadata: Sha256Digest,
    ) -> Result<QueryProducerCertificate> {
        let binding = native_binding(artifact, metadata)?;
        let edges = native_edges(artifact)?;
        let mut builder = Builder::audit(None);
        for (ordinal, hit) in edges.iter().enumerate() {
            builder.push(RowRef::Edge { ordinal, hit })?;
        }
        builder.certify(self, reference, binding, 0, edges.len())
    }
    pub(crate) fn verify_empty_native_edges(&self, artifact: &airk_core::Artifact) -> Result<()> {
        super::relationships::visit_edges(artifact, |_| {
            anyhow::bail!("empty native edge declaration differs from retained facts")
        })
    }
    pub(crate) fn certify_import_projection(
        &self,
        reference: &ResourceReference,
        evidence: &AnalysisEvidence,
        analysis: &ResourceReference,
        languages: &BTreeMap<String, String>,
    ) -> Result<QueryProducerCertificate> {
        let mut builder = Builder::audit(Some(languages));
        for (ordinal, hit) in evidence.facts.iter().enumerate() {
            builder.push_fact(ordinal, hit)?;
        }
        builder.flush()?;
        for (ordinal, hit) in evidence.relationships.iter().enumerate() {
            builder.push(RowRef::Edge { ordinal, hit })?;
        }
        builder.certify(
            self,
            reference,
            imported_binding(evidence, analysis)?,
            evidence.facts.len(),
            evidence.relationships.len(),
        )
    }

    pub(crate) fn verify_native_edges(
        &self,
        reference: &ResourceReference,
        artifact: &airk_core::Artifact,
        metadata: Sha256Digest,
    ) -> Result<()> {
        let binding = native_binding(artifact, metadata)?;
        let header = self.projection_header(reference)?;
        let edges = native_edges(artifact)?;
        ensure!(
            header.binding == binding && header.facts == 0 && header.edges == edges.len(),
            "native edge projection binding mismatch"
        );
        let mut next = 0;
        let mut selectors = selectors::Selectors::default();
        self.visit_projection_rows(&header, |page, row| {
            let ProjectionRow::Edge { ordinal, hit } = row else {
                anyhow::bail!("native edge projection contains fact");
            };
            ensure!(ordinal == next, "native edge projection order mismatch");
            let expected = edges.get(next).context("extra native projected edge")?;
            ensure!(
                serde_json::to_vec(expected)? == serde_json::to_vec(&hit)?,
                "native edge projection membership mismatch"
            );
            selectors.edge(&hit, page)?;
            next += 1;
            Ok(())
        })?;
        ensure!(next == edges.len(), "native edge projection incomplete");
        let mut budget = selectors::MetadataBudget::new(header.metadata_bytes);
        for roster in &header.roster {
            budget.charge(roster)?;
        }
        selectors.verify(self, &header, &mut budget)?;
        budget.finish(&header)?;
        Ok(())
    }

    pub(crate) fn retain_projection_paths(
        &self,
        reference: &ResourceReference,
        paths: &mut std::collections::BTreeSet<std::path::PathBuf>,
    ) -> Result<()> {
        let header = self.projection_header(reference)?;
        let mut budget = selectors::MetadataBudget::new(header.metadata_bytes);
        let mut retain = |reference: &ResourceReference| -> Result<()> {
            paths.insert(
                std::path::PathBuf::from("current-resources")
                    .join(format!("{}.json", reference.object_digest()?)),
            );
            Ok(())
        };
        retain(reference)?;
        for roster in &header.roster {
            budget.charge(roster)?;
            retain(roster)?;
        }
        for block in 0..header.roster.len() {
            for reference in self.projection_roster(&header, block)? {
                retain(&reference)?;
            }
        }
        selectors::retain_paths(self, &header, &mut retain, &mut budget)?;
        budget.finish(&header)?;
        Ok(())
    }

    pub(crate) fn projection_header(
        &self,
        reference: &ResourceReference,
    ) -> Result<ProjectionHeader> {
        ensure!(
            reference.bytes <= MAX_HEADER,
            anyhow::Error::new(super::PrivateIntegrityFailure)
                .context("projection header exceeds bound")
        );
        let bytes = self.read_current_resource(reference)?;
        let header: ProjectionHeader =
            serde_json::from_slice(&bytes).context(super::PrivateIntegrityFailure)?;
        let counts_valid = match header.binding {
            ProjectionBinding::NativeEdges { .. } => header.facts == 0 && header.edges <= 20_000,
            ProjectionBinding::Imported { .. } => {
                header.facts <= 100_000 && header.edges <= 100_000
            }
        };
        ensure!(
            header.version == 1
                && counts_valid
                && header.pages <= header.facts.saturating_add(header.edges)
                && header.roster.len() == header.pages.div_ceil(ROSTER_ROWS)
                && header.metadata_bytes <= MAX_BYTES
                && header.metadata_objects <= 2_000_000
                && header.selector_keys <= 2_000_000
                && header.selectors.is_some() == (header.selector_keys > 0)
                && serde_json::to_vec(&header)? == bytes,
            anyhow::Error::new(super::PrivateIntegrityFailure).context("invalid projection header")
        );
        ensure!(
            reference.projection.as_ref()
                == Some(&ProjectionSummary {
                    version: 1,
                    facts: header.facts,
                    edges: header.edges,
                    pages: header.pages,
                    metadata_bytes: header.metadata_bytes,
                    metadata_objects: header.metadata_objects
                }),
            anyhow::Error::new(super::PrivateIntegrityFailure)
                .context("projection admission summary mismatch")
        );
        Ok(header)
    }

    #[cfg(test)]
    pub(crate) fn projection_leaf(
        &self,
        header: &ProjectionHeader,
        page: usize,
    ) -> Result<ResourceReference> {
        ensure!(page < header.pages, "projection page unavailable");
        Ok(self.projection_roster(header, page / ROSTER_ROWS)?[page % ROSTER_ROWS].clone())
    }

    fn projection_roster(
        &self,
        header: &ProjectionHeader,
        block: usize,
    ) -> Result<Vec<ResourceReference>> {
        let reference = header
            .roster
            .get(block)
            .context("projection roster unavailable")?;
        ensure!(
            reference.bytes <= PAGE_BYTES,
            anyhow::Error::new(super::PrivateIntegrityFailure)
                .context("projection roster exceeds bound")
        );
        let bytes = self.read_current_resource(reference)?;
        let roster: Roster =
            serde_json::from_slice(&bytes).context(super::PrivateIntegrityFailure)?;
        let start = block * ROSTER_ROWS;
        ensure!(
            roster.version == 1
                && roster.start == start
                && roster.leaves.len() == (header.pages - start).min(ROSTER_ROWS)
                && serde_json::to_vec(&roster)? == bytes,
            anyhow::Error::new(super::PrivateIntegrityFailure).context("invalid projection roster")
        );
        Ok(roster.leaves)
    }

    #[cfg(test)]
    pub(crate) fn projection_page(
        &self,
        header: &ProjectionHeader,
        page: usize,
    ) -> Result<Vec<ProjectionRow>> {
        let reference = self.projection_leaf(header, page)?;
        self.read_projection_page(&reference, page)
    }

    pub(super) fn read_projection_page(
        &self,
        reference: &ResourceReference,
        page: usize,
    ) -> Result<Vec<ProjectionRow>> {
        ensure!(
            reference.bytes <= MAX_ROW + 128,
            anyhow::Error::new(super::PrivateIntegrityFailure)
                .context("projection leaf exceeds producer bound")
        );
        let bytes = self.read_current_resource(reference)?;
        let value: Page<ProjectionRow> =
            serde_json::from_slice(&bytes).context(super::PrivateIntegrityFailure)?;
        ensure!(
            value.version == 1
                && value.page == page
                && !value.rows.is_empty()
                && value.rows.len() <= ROWS
                && serde_json::to_vec(&value)? == bytes,
            anyhow::Error::new(super::PrivateIntegrityFailure).context("invalid projection page")
        );
        Ok(value.rows)
    }

    pub(crate) fn visit_projection_rows(
        &self,
        header: &ProjectionHeader,
        mut visit: impl FnMut(usize, ProjectionRow) -> Result<()>,
    ) -> Result<()> {
        for block in 0..header.roster.len() {
            for (offset, reference) in self.projection_roster(header, block)?.iter().enumerate() {
                for row in self.read_projection_page(reference, block * ROSTER_ROWS + offset)? {
                    visit(block * ROSTER_ROWS + offset, row)?;
                }
            }
        }
        Ok(())
    }
}
