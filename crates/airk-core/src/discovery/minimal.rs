// SPDX-License-Identifier: Apache-2.0
//! Small fact rows with page-local file/provenance bindings, charged before admission.
use crate::{Artifact, DocumentLocation, FactHit, TableEvidence, code::EvidenceBasis};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MinimalFact {
    pub name: String,
    pub kind: String,
    pub lines: [usize; 2],
    pub signature: Option<String>,
    /// Absent only for a single exact-file outline (file zero).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<usize>,
    /// Absent means the referenced file's default provenance.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provenance: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<DocumentLocation>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub candidate: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub details_truncated: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub statement_truncated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub table: Option<TableEvidence>,
}
fn is_false(value: &bool) -> bool {
    !value
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MinimalFile {
    pub path: String,
    pub content_hash: String,
    pub language: String,
    pub parse_status: String,
    pub line_basis: String,
    pub provenance: usize,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MinimalProvenance {
    pub basis: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub producer: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub producer_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coverage: Option<String>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct MinimalFacts {
    pub items: Vec<MinimalFact>,
    pub files: Vec<MinimalFile>,
    pub provenance: Vec<MinimalProvenance>,
}
pub struct MinimalCandidate {
    expected: [usize; 3],
    row: MinimalFact,
    file: Option<MinimalFile>,
    provenance: Option<MinimalProvenance>,
    pub bytes: usize,
}
impl MinimalFacts {
    /// No mutation occurs until the caller admits the complete record plus new
    /// shared bindings. A page never leaves an uncharged dangling reference.
    pub fn candidate(
        &self,
        hit: &FactHit,
        artifact: &Artifact,
        single_file: bool,
    ) -> Result<Option<MinimalCandidate>> {
        ensure!(
            artifact.path == hit.citation.path
                && artifact.content_hash == hit.citation.content_hash,
            "minimal file binding mismatch"
        );
        ensure!(
            !single_file
                || self
                    .files
                    .first()
                    .is_none_or(|file| file.path == hit.citation.path),
            "single-file outline contains another file"
        );
        let descriptor = if let Some(code) = &hit.code {
            MinimalProvenance {
                basis: match code.basis {
                    EvidenceBasis::Syntax => "syntax",
                    EvidenceBasis::StructuralRule => "structural_rule",
                    EvidenceBasis::Imported => "imported",
                }
                .into(),
                producer: Some(code.producer.clone()),
                producer_version: Some(code.producer_version.clone()),
                coverage: Some(code.coverage.clone()),
            }
        } else if let Some(document) = &hit.document {
            MinimalProvenance {
                basis: document.basis.clone(),
                producer: Some(artifact.extraction_method.clone()),
                producer_version: Some(document.parser_version.clone()),
                coverage: None,
            }
        } else {
            MinimalProvenance {
                basis: "indexed_structure".into(),
                producer: Some(artifact.extraction_method.clone()),
                producer_version: None,
                coverage: None,
            }
        };
        let existing_provenance = self
            .provenance
            .iter()
            .position(|value| value == &descriptor);
        if existing_provenance.is_none() && self.provenance.len() >= 64 {
            return Ok(None);
        }
        let provenance = existing_provenance.unwrap_or(self.provenance.len());
        let existing_file = self
            .files
            .iter()
            .position(|file| file.path == hit.citation.path);
        if existing_file.is_none() && self.files.len() >= 256 {
            return Ok(None);
        }
        let file_index = existing_file.unwrap_or(self.files.len());
        let default_provenance =
            existing_file.map_or(provenance, |index| self.files[index].provenance);
        let file = existing_file.is_none().then(|| MinimalFile {
            path: hit.citation.path.clone(),
            content_hash: hit.citation.content_hash.clone(),
            language: artifact.language.clone(),
            parse_status: artifact.parse_status.clone(),
            line_basis: if artifact.extraction_method == "utf8" {
                "source"
            } else {
                "extracted-text; cite original location"
            }
            .into(),
            provenance,
        });
        let row = MinimalFact {
            name: hit.name.clone(),
            kind: hit.kind.clone(),
            lines: [hit.citation.start_line, hit.citation.end_line],
            signature: hit.code.as_ref().and_then(|code| code.signature.clone()),
            file: (!single_file).then_some(file_index),
            provenance: (provenance != default_provenance).then_some(provenance),
            location: (!matches!(hit.citation.location, DocumentLocation::Lines { .. }))
                .then(|| hit.citation.location.clone()),
            candidate: hit
                .code
                .as_ref()
                .is_some_and(|code| code.is_call_candidate(&hit.kind)),
            details_truncated: hit.code.as_ref().is_some_and(|code| code.details_truncated)
                || hit
                    .document
                    .as_ref()
                    .is_some_and(|document| document.details_truncated),
            statement_truncated: hit
                .document
                .as_ref()
                .is_some_and(|document| document.statement_truncated),
            table: hit
                .document
                .as_ref()
                .and_then(|document| document.table.clone()),
        };
        let mut bytes = serde_json::to_vec(&row)?.len();
        if let Some(file) = &file {
            bytes += serde_json::to_vec(file)?.len();
        }
        if existing_provenance.is_none() {
            bytes += serde_json::to_vec(&descriptor)?.len();
        }
        Ok(Some(MinimalCandidate {
            expected: [self.items.len(), self.files.len(), self.provenance.len()],
            row,
            file,
            provenance: existing_provenance.is_none().then_some(descriptor),
            bytes,
        }))
    }
    pub fn admit(&mut self, candidate: MinimalCandidate) -> Result<()> {
        ensure!(
            candidate.expected == [self.items.len(), self.files.len(), self.provenance.len()],
            "minimal admission state changed"
        );
        if let Some(provenance) = candidate.provenance {
            self.provenance.push(provenance);
        }
        if let Some(file) = candidate.file {
            self.files.push(file);
        }
        self.items.push(candidate.row);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (Artifact, FactHit) {
        let artifact = Artifact {
            path: "one.rs".into(),
            content_hash: "a".repeat(64),
            bytes: 30,
            chunks: vec![],
            language: "rust".into(),
            extraction_method: "utf8".into(),
            parse_status: "parsed".into(),
            facts: vec![],
            retained: None,
        };
        let hit = FactHit { candidate: false,
                continuity_key: None, name: "alpha".into(), kind: "definition".into(), syntax_kind: "function_item".into(), language: "rust".into(), citation: crate::Citation { path: artifact.path.clone(), content_hash: artifact.content_hash.clone(), start_line: 1, end_line: 2, location: DocumentLocation::Lines { start: 1, end: 2 } }, document: None, code: Some(serde_json::from_value(serde_json::json!({"symbol_id":"symbol", "qualified_name":"scope.alpha", "scope":["scope"], "owner_id":null, "signature":"fn alpha()", "parameters":null, "return_type":null, "modifiers":[], "documentation":null, "target":null, "basis":"syntax", "producer":"tree-sitter", "producer_version":"1", "coverage":"partial syntax", "details_truncated":false})).unwrap()) };
        (artifact, hit)
    }
    #[test]
    fn ordinary_definition_has_four_small_fields_and_admission_is_atomic() -> Result<()> {
        let (artifact, hit) = fixture();
        let mut page = MinimalFacts::default();
        let first = page.candidate(&hit, &artifact, true)?.unwrap();
        let stale = page.candidate(&hit, &artifact, true)?.unwrap();
        assert!(page.items.is_empty() && page.files.is_empty());
        let charged = first.bytes;
        page.admit(first)?;
        let row = serde_json::to_value(&page.items[0])?;
        assert_eq!(row.as_object().unwrap().len(), 4);
        assert!(serde_json::to_vec(&row)?.len() <= 100);
        assert!(page.admit(stale).is_err());
        let repeated = page.candidate(&hit, &artifact, true)?.unwrap();
        assert_eq!(repeated.bytes, serde_json::to_vec(&row)?.len());
        assert!(charged > repeated.bytes);
        let mut foreign = hit.clone();
        foreign.citation.content_hash = "b".repeat(64);
        assert!(page.candidate(&foreign, &artifact, true).is_err());
        Ok(())
    }
    #[test]
    fn imported_overrides_and_syntax_candidates_keep_their_basis() -> Result<()> {
        let (artifact, mut hit) = fixture();
        let mut page = MinimalFacts::default();
        hit.code.as_mut().unwrap().target = Some("receiver.alpha".into());
        hit.kind = "import".into();
        assert!(
            !page
                .candidate(&hit, &artifact, false)?
                .unwrap()
                .row
                .candidate
        );
        hit.kind = "call".into();
        page.admit(page.candidate(&hit, &artifact, false)?.unwrap())?;
        assert!(page.items[0].candidate);
        let code = hit.code.as_mut().unwrap();
        code.basis = EvidenceBasis::Imported;
        code.producer = "scip".into();
        page.admit(page.candidate(&hit, &artifact, false)?.unwrap())?;
        assert!(!page.items[1].candidate);
        assert_eq!(page.items[1].provenance, Some(1));
        assert_eq!(page.provenance[1].basis, "imported");
        assert_eq!(page.files[0].provenance, 0);
        assert_eq!(page.files.len(), 1);
        Ok(())
    }
    #[test]
    fn original_document_locations_and_cached_formula_warnings_survive() -> Result<()> {
        let (mut artifact, mut hit) = fixture();
        artifact.extraction_method = "xlsx".into();
        hit.code = None;
        hit.document = Some(crate::DocumentEvidence {
            parser_version: "2".into(),
            basis: "stored_cells".into(),
            statement_truncated: true,
            table: Some(TableEvidence {
                row: 4,
                hidden: true,
                cells: vec![crate::TableCellEvidence {
                    formula: Some("A1+1".into()),
                    cached_value: true,
                    ..Default::default()
                }],
                ..Default::default()
            }),
            ..Default::default()
        });
        for location in [
            DocumentLocation::Page { number: 2 },
            DocumentLocation::Block { number: 3 },
            DocumentLocation::Slide { number: 4 },
            DocumentLocation::WorksheetRow {
                sheet: "Sheet 1".into(),
                row: 4,
            },
        ] {
            hit.citation.location = location.clone();
            let mut page = MinimalFacts::default();
            page.admit(page.candidate(&hit, &artifact, true)?.unwrap())?;
            assert_eq!(page.items[0].location, Some(location));
            assert!(page.items[0].statement_truncated);
            assert!(page.items[0].table.as_ref().unwrap().cells[0].cached_value);
            assert_eq!(
                page.files[0].line_basis,
                "extracted-text; cite original location"
            );
        }
        Ok(())
    }
}
