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

impl CodeEvidence {
    pub fn is_call_candidate(&self, kind: &str) -> bool {
        kind == "call" && self.basis != EvidenceBasis::Imported && self.target.is_some()
    }
}

/// Terminal written method spelling for native call candidates. This does not
/// resolve receivers, dynamic subscripts, generic expressions or imported IDs.
pub fn terminal_call_name<'a>(code: &'a CodeEvidence, kind: &str) -> Option<&'a str> {
    if !code.is_call_candidate(kind) {
        return None;
    }
    let target = code.target.as_deref()?;
    let name = target.rsplit(['.', ':', '>']).next()?.trim();
    let mut chars = name.chars();
    let first = chars.next()?;
    ((first.is_alphabetic() || matches!(first, '_' | '$'))
        && chars.all(|c| c.is_alphanumeric() || matches!(c, '_' | '$')))
    .then_some(name)
}

/// Zero-based UTF-8 byte offsets in retained source, with an exclusive end.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CodeSpan {
    pub start_byte: usize,
    pub end_byte: usize,
}

#[derive(Clone, Debug, Serialize)]
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
    #[serde(default)]
    pub indexing: crate::extraction::IndexCoverage,
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

#[cfg(test)]
mod terminal_tests {
    use super::*;
    #[test]
    fn terminal_names_preserve_case_and_reject_dynamic_or_imported_targets() {
        let mut code: CodeEvidence = serde_json::from_value(serde_json::json!({"symbol_id":"s", "qualified_name":"s", "scope":[], "owner_id":null, "signature":null, "parameters":null, "return_type":null, "modifiers":[], "documentation":null, "target":null, "basis":"syntax", "producer":"native", "producer_version":"1", "coverage":"written syntax", "details_truncated":false})).unwrap();
        for target in [
            "reclaim_objects",
            "x.reclaim_objects",
            "factory().reclaim_objects",
            "x->reclaim_objects",
            "Type::reclaim_objects",
            "x?.reclaim_objects",
        ] {
            code.target = Some(target.into());
            assert_eq!(terminal_call_name(&code, "call"), Some("reclaim_objects"));
            assert_eq!(terminal_call_name(&code, "definition"), None);
        }
        code.target = Some("x.Reclaim_objects".into());
        assert_eq!(terminal_call_name(&code, "call"), Some("Reclaim_objects"));
        code.basis = EvidenceBasis::Imported;
        assert_eq!(terminal_call_name(&code, "call"), None);
        code.basis = EvidenceBasis::Syntax;
        for target in [
            "x['reclaim_objects']",
            "x[dynamic]",
            "x.reclaim_objects::<T>",
            "x.",
            "'reclaim_objects'",
            "",
        ] {
            code.target = Some(target.into());
            assert_eq!(terminal_call_name(&code, "call"), None, "{target}");
        }
    }
}
