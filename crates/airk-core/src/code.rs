// SPDX-License-Identifier: Apache-2.0
//! Code evidence preserves written syntax separately from provider assertions.
use crate::Citation;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceBasis {
    Syntax,
    StructuralRule,
    Imported,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CodeEvidence {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fingerprints: Option<crate::continuity::CodeFingerprints>,
    pub symbol_id: String,
    pub qualified_name: String,
    pub scope: Vec<String>,
    pub owner_id: Option<String>,
    pub signature: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span: Option<CodeSpan>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature_span: Option<CodeSpan>,
    pub parameters: Option<String>,
    pub return_type: Option<String>,
    pub modifiers: Vec<String>,
    pub documentation: Option<String>,
    pub target: Option<String>,
    pub basis: EvidenceBasis,
    pub producer: String,
    pub producer_version: String,
    /// Coverage of this record, never a claim of complete language analysis.
    pub coverage: String,
    pub details_truncated: bool,
}

/// Zero-based UTF-8 byte offsets in retained source, with an exclusive end.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CodeSpan {
    pub start_byte: usize,
    pub end_byte: usize,
}

#[derive(Clone, Debug)]
pub struct FactQuery {
    pub path_prefix: String,
    pub name: Option<String>,
    pub kind: Option<String>,
    pub language: Option<String>,
    pub exact_name: bool,
    pub limit: usize,
    pub max_bytes: usize,
    pub path_exact: bool,
    pub document_only: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RelationshipHit {
    pub source: String,
    pub target: String,
    pub kind: String,
    /// syntax_candidate or provider_asserted; an imported reference is not a call.
    pub resolution: String,
    pub basis: EvidenceBasis,
    pub producer: String,
    pub producer_version: String,
    pub citation: Citation,
}

#[derive(Clone, Debug)]
pub struct RelationshipQuery {
    pub symbol: String,
    pub direction: String,
    pub kind: Option<String>,
    pub path_prefix: String,
    pub hops: usize,
    pub limit: usize,
    pub max_bytes: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct RelationshipsResponse {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publication: Option<crate::evidence::RevisionEnvelope>,
    pub generation: u64,
    pub collection: String,
    pub relationships: Vec<RelationshipHit>,
    pub truncated: bool,
    pub coverage: String,
}

/// Captured BEFORE an external producer runs. Import verifies it again and
/// requires the producer to supply its output digest separately.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnalysisManifest {
    pub version: u32,
    pub root: String,
    pub collection: String,
    pub extractor_version: String,
    pub source_hashes: BTreeMap<String, String>,
    pub build_inputs: BTreeMap<String, String>,
    pub producer: String,
    pub producer_version: String,
    pub index_sha256: Option<String>,
}
