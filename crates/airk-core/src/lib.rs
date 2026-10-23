// SPDX-License-Identifier: Apache-2.0
//! Pure contracts. Filesystem, parsing, index engines and transports live elsewhere.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub mod binding;
pub mod changes;
pub mod code;
pub mod compression;
pub mod continuity;
pub mod discovery;
pub mod embedding;
pub mod evidence;
pub mod execution;
pub mod extraction;
pub mod graph_storage;
pub mod identity;
pub mod lexical;
pub mod location;
pub mod mapped_binding;
pub mod model_objects;
pub mod publication;
pub mod readiness;
pub mod service;
pub mod service_events;
pub mod service_inspection;
pub mod service_progress;
pub mod service_resources;
pub mod service_watch;
pub mod usage;
pub mod vector_evidence;
pub mod vector_selection;
pub use code::*;

pub const SCHEMA_VERSION: u32 = 3;
pub const EXTRACTOR_VERSION: &str = "native-facts/10";
pub const MAX_RESPONSE_BYTES: usize = 65_536;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Chunk {
    pub start_line: usize,
    pub end_line: usize,
    pub text: String,
    pub location: DocumentLocation,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Artifact {
    /// Slash-separated, workspace-relative path. Never an absolute path.
    pub path: String,
    pub content_hash: String,
    pub bytes: usize,
    pub chunks: Vec<Chunk>,
    pub language: String,
    pub extraction_method: String,
    pub parse_status: String,
    pub facts: Vec<Fact>,
    /// Immutable content-addressed payload for a compact on-disk inventory entry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retained: Option<ArtifactRef>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ArtifactRef {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_encoding: Option<compression::EncodedObject>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload_encoding: Option<compression::EncodedObject>,
    pub object: String,
    #[serde(default)]
    pub object_bytes: usize,
    pub raw_source: bool,
    pub chunks: usize,
    pub extracted_bytes: usize,
    pub facts: usize,
    /// Present only for a path-free v2 extraction object; absent legacy objects
    /// retain their original encoded representation and path binding.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<extraction::ExtractionProfile>,
}

impl Artifact {
    pub fn chunk_count(&self) -> usize {
        self.retained
            .as_ref()
            .map_or(self.chunks.len(), |r| r.chunks)
    }
    pub fn extracted_bytes(&self) -> usize {
        self.retained.as_ref().map_or_else(
            || self.chunks.iter().map(|c| c.text.len()).sum(),
            |r| r.extracted_bytes,
        )
    }
}

/// Load one retained artifact; implementations must preserve inventory binding.
pub trait ArtifactSource {
    fn load_artifact(&self, path: &str) -> anyhow::Result<Artifact>;
}

/// Authenticated neutral extraction and its bound display evidence, loaded together.
/// Retrieval units are planned from the payload, independently of display chunks.
pub struct RetrievalArtifact {
    pub artifact: Artifact,
    pub profile: extraction::ExtractionProfile,
    pub payload: extraction::ExtractionPayload,
    pub payload_digest: identity::Sha256Digest,
}

pub trait RetrievalSource {
    fn load_retrieval_artifact(&self, path: &str) -> anyhow::Result<RetrievalArtifact>;
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Snapshot {
    pub schema_version: u32,
    pub extractor_version: String,
    pub root: String,
    pub collection: String,
    pub generation: u64,
    /// Zero denotes the legacy whole-snapshot format; one is a compact blob catalog.
    #[serde(default)]
    pub storage_format: u32,
    /// Additional source aliases bound to this exact published revision.
    /// Paths from these roots use `@alias/relative/path`; the primary root stays unprefixed.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub roots: BTreeMap<String, String>,
    /// New revisions require their immutable lexical/vector components to exist.
    #[serde(default)]
    pub persistent_indexes: bool,
    #[serde(default)]
    pub embedding_model: Option<String>,
    pub artifacts: BTreeMap<String, Artifact>,
    #[serde(default)]
    pub analysis: Option<AnalysisManifest>,
    #[serde(default)]
    pub imported_facts: Vec<FactHit>,
    #[serde(default)]
    pub imported_relationships: Vec<RelationshipHit>,
}

impl Snapshot {
    pub fn empty(root: String) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            extractor_version: EXTRACTOR_VERSION.into(),
            root,
            collection: "default".into(),
            generation: 0,
            storage_format: 0,
            roots: BTreeMap::new(),
            persistent_indexes: false,
            embedding_model: None,
            artifacts: BTreeMap::new(),
            analysis: None,
            imported_facts: Vec::new(),
            imported_relationships: Vec::new(),
        }
    }
}

impl ArtifactSource for Snapshot {
    fn load_artifact(&self, path: &str) -> anyhow::Result<Artifact> {
        self.artifacts
            .get(path)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("artifact is not in indexed inventory"))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Citation {
    pub path: String,
    pub content_hash: String,
    pub start_line: usize,
    pub end_line: usize,
    pub location: DocumentLocation,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Hit {
    pub citation: Citation,
    pub text: String,
    pub truncated: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SearchResponse {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publication: Option<evidence::RevisionEnvelope>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub semantic_readiness: Option<readiness::SemanticReadiness>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub discovery_readiness: Option<readiness::DiscoveryReadiness>,
    pub generation: u64,
    pub collection: String,
    pub hits: Vec<Hit>,
    /// True when the result count or text budget omitted matching content.
    pub truncated: bool,
}

#[derive(Clone, Debug)]
pub struct SearchQuery {
    pub text: String,
    pub path_prefix: Option<String>,
    pub limit: usize,
    /// Text payload only; JSON metadata/framing is additional.
    pub max_bytes: usize,
}

impl SearchQuery {
    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(!self.text.trim().is_empty(), "query must not be empty");
        anyhow::ensure!(self.text.len() <= 4096, "query exceeds 4096 bytes");
        anyhow::ensure!(
            self.path_prefix.as_ref().is_none_or(|p| p.len() <= 4096),
            "path_prefix exceeds 4096 bytes"
        );
        validate_budget(self.limit, self.max_bytes)
    }
}

pub fn validate_budget(limit: usize, max_bytes: usize) -> anyhow::Result<()> {
    anyhow::ensure!((1..=100).contains(&limit), "limit must be 1..100");
    anyhow::ensure!(
        (1..=MAX_RESPONSE_BYTES).contains(&max_bytes),
        "max_bytes must be 1..65536"
    );
    Ok(())
}

/// Future syntax adapters must distinguish definitions from textual occurrences.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Symbol {
    pub name: String,
    pub kind: String,
    pub language: String,
    pub citation: Citation,
}

/// PDF/DOCX adapters use native page/block locations, not invented source lines.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum DocumentLocation {
    /// Original host coordinates with explicitly region-local code spans.
    Mapped {
        region: location::RegionLocation,
    },
    Lines {
        start: usize,
        end: usize,
    },
    Page {
        number: usize,
    },
    Block {
        number: usize,
    },
    Slide {
        number: usize,
    },
    TableRow {
        table: usize,
        row: usize,
    },
    WorksheetRow {
        sheet: String,
        row: usize,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Fact {
    pub name: String,
    pub kind: String,
    pub syntax_kind: String,
    pub start_line: usize,
    pub end_line: usize,
    pub location: DocumentLocation,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub document: Option<DocumentEvidence>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<CodeEvidence>,
}

/// Deterministic document cues, not inferred obligations or design correctness.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DocumentEvidence {
    pub parser_version: String,
    pub basis: String,
    pub heading_path: Vec<String>,
    pub requirement_id: Option<String>,
    pub modality: Option<String>,
    pub requirement_type: Option<String>,
    pub dates: Vec<DocumentDate>,
    pub quantities: Vec<DocumentQuantity>,
    pub details_truncated: bool,
    pub statement_truncated: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub table: Option<TableEvidence>,
}

/// Written table structure. Header candidates and stored formula results are not inferred truth.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct TableEvidence {
    pub table: Option<usize>,
    pub sheet: Option<String>,
    pub row: usize,
    pub header_basis: String,
    /// True only for an explicitly declared header row, never an inferred candidate.
    #[serde(default)]
    pub is_header: bool,
    /// Propagated header labels were bounded; original cell values remain source evidence.
    #[serde(default)]
    pub headers_truncated: bool,
    pub hidden: bool,
    pub cells: Vec<TableCellEvidence>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct TableCellEvidence {
    pub column: usize,
    pub address: Option<String>,
    pub header: Option<String>,
    pub value: String,
    pub value_type: String,
    pub formula: Option<String>,
    pub cached_value: bool,
    pub merged_range: Option<String>,
    pub column_span: usize,
    pub vertical_merge: Option<String>,
    pub number_format: Option<String>,
    pub normalized_value: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DocumentDate {
    pub raw: String,
    /// ISO calendar date only when the source is explicit and valid; no guessed locale.
    pub normalized: Option<String>,
    pub format: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DocumentQuantity {
    pub raw: String,
    /// Recognized written bound, not a logical inference about surrounding negation.
    pub comparator: String,
    pub value: String,
    pub upper_value: Option<String>,
    /// Source unit spelling is preserved; values/units are not converted.
    pub unit: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FactHit {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub continuity_key: Option<continuity::ContinuityKey>,
    pub name: String,
    pub kind: String,
    pub syntax_kind: String,
    pub language: String,
    pub citation: Citation,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub document: Option<DocumentEvidence>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<CodeEvidence>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FactsResponse {
    #[serde(flatten)]
    pub page: discovery::DiscoveryPage,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outline: Vec<discovery::CompactFact>,
    #[serde(default)]
    pub indexing: crate::extraction::IndexCoverage,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publication: Option<evidence::RevisionEnvelope>,
    pub generation: u64,
    pub collection: String,
    pub facts: Vec<FactHit>,
    pub truncated: bool,
}

pub trait SnapshotStore {
    fn load(&self) -> anyhow::Result<Option<Snapshot>>;
    fn save(&self, snapshot: &Snapshot) -> anyhow::Result<()>;
}

pub trait EmbeddingProvider: Send + Sync {
    fn specification(&self) -> &str;
    fn embed(&self, text: &[String]) -> anyhow::Result<Vec<Vec<f32>>>;
}
