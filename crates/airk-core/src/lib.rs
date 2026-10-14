// SPDX-License-Identifier: Apache-2.0
//! Pure contracts. Filesystem, parsing, index engines and transports live elsewhere.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub mod code;
pub mod usage;
pub use code::*;

pub const SCHEMA_VERSION: u32 = 3;
pub const EXTRACTOR_VERSION: &str = "native-facts/4";
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
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Snapshot {
    pub schema_version: u32,
    pub extractor_version: String,
    pub root: String,
    pub collection: String,
    pub generation: u64,
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
            artifacts: BTreeMap::new(),
            analysis: None,
            imported_facts: Vec::new(),
            imported_relationships: Vec::new(),
        }
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
    Lines { start: usize, end: usize },
    Page { number: usize },
    Block { number: usize },
    Slide { number: usize },
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
