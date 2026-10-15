// SPDX-License-Identifier: Apache-2.0
//! Conservative, path-free document descriptors over complete extracted spans.
use airk_core::{
    DocumentLocation,
    changes::ChangeDimension as Dimension,
    continuity::{
        CONTINUITY_VERSION, ContinuityBudget, ContinuityDescriptor, ContinuityDraft,
        ContinuityFamily, ContinuitySubject, MAX_CONTINUITY_BYTES,
    },
    extraction::{ExtractionPayload, ExtractionProfile, LocalChunk, LocalFact, LocalSpan},
    identity::{Sha256Digest, framed_digest},
};
use anyhow::{Result, ensure};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::OnceLock,
};

fn hash(domain: &[u8], bytes: &[u8]) -> Sha256Digest {
    framed_digest(domain, &[bytes])
}

// Preserve internal spacing, case, literals and Markdown hard breaks.
fn normalized(text: &str) -> String {
    text.replace("\r\n", "\n").trim().to_owned()
}

fn native(profile: &ExtractionProfile) -> bool {
    matches!(
        profile.language.as_str(),
        "pdf" | "docx" | "pptx" | "xlsx" | "csv" | "ocr"
    )
}

fn category(location: &DocumentLocation) -> &'static str {
    match location {
        DocumentLocation::Lines { .. } => "paragraph",
        DocumentLocation::Block { .. } => "paragraph",
        DocumentLocation::Page { .. } => "page",
        DocumentLocation::Slide { .. } => "slide",
        DocumentLocation::TableRow { .. } => "table_row",
        DocumentLocation::WorksheetRow { .. } => "worksheet_row",
    }
}

struct SubjectContext {
    subject: ContinuitySubject,
    owner: Option<ContinuitySubject>,
    region: Option<u32>,
}

struct Builder<'a> {
    payload: &'a ExtractionPayload,
    profile: Sha256Digest,
    provenance: Sha256Digest,
    budget: &'a mut ContinuityBudget,
    draft: ContinuityDraft,
    encoded_bytes: usize,
    lines: BTreeMap<Option<u32>, Vec<usize>>,
}

impl Builder<'_> {
    fn line_numbers(&self, region: Option<u32>, span: &LocalSpan) -> (usize, usize) {
        let lines = &self.lines[&region];
        (
            lines.partition_point(|offset| *offset <= span.start).max(1),
            lines
                .partition_point(|offset| *offset <= span.end.saturating_sub(1))
                .max(1),
        )
    }
    fn partial(&mut self, reason: &str) {
        self.draft.partial = true;
        if !self.draft.diagnostics.iter().any(|item| item == reason) {
            self.draft.diagnostics.push(reason.into());
        }
    }

    fn add(
        &mut self,
        context: SubjectContext,
        category: &str,
        span: LocalSpan,
        location: DocumentLocation,
        heading: bool,
        fact: Option<&LocalFact>,
    ) -> Result<bool> {
        let SubjectContext {
            subject,
            owner,
            region,
        } = context;
        let text = match region {
            None => self.payload.text.as_str(),
            Some(id) => self
                .payload
                .regions
                .iter()
                .find(|item| item.id == id)
                .ok_or_else(|| anyhow::anyhow!("missing document region"))?
                .text
                .as_str(),
        };
        airk_core::location::ByteSpan {
            start: span.start,
            end: span.end,
        }
        .validate(text)?;
        let body = &text[span.start..span.end];
        let statement = normalized(body);
        let mapping = region.and_then(|id| self.payload.regions.iter().find(|item| item.id == id));
        let coordinates = serde_json::to_vec(&(
            &location,
            &span,
            mapping.map(|item| (&item.location, &item.source_map)),
        ))?;
        let details = fact.and_then(|fact| fact.document.as_ref());
        let details_bytes = serde_json::to_vec(&details)?;
        let region_profile_bytes = serde_json::to_vec(&mapping.map(|item| &item.profile))?;
        let cost = body
            .len()
            .saturating_mul(3)
            .saturating_add(coordinates.len())
            .saturating_add(details_bytes.len().saturating_mul(2))
            .saturating_add(region_profile_bytes.len());
        if self.budget.charge(cost).is_err() {
            self.partial("document continuity work budget exceeded");
            return Ok(false);
        }
        let mut dimensions = BTreeMap::from([
            (
                Dimension::Content,
                hash(b"document-content/1", body.as_bytes()),
            ),
            (
                Dimension::Statement,
                hash(b"document-statement/1", statement.as_bytes()),
            ),
            (
                Dimension::Location,
                hash(b"document-location/1", &coordinates),
            ),
        ]);
        let mut unsupported = vec![];
        let mut complete = !self.payload.partial;
        if let Some(details) = details {
            if details.details_truncated {
                unsupported.extend([Dimension::Quantity, Dimension::Date, Dimension::Modality]);
                complete = false;
            } else {
                dimensions.insert(
                    Dimension::Quantity,
                    hash(
                        b"document-quantity/1",
                        &serde_json::to_vec(&details.quantities)?,
                    ),
                );
                dimensions.insert(
                    Dimension::Date,
                    hash(b"document-date/1", &serde_json::to_vec(&details.dates)?),
                );
                dimensions.insert(
                    Dimension::Modality,
                    hash(
                        b"document-modality/1",
                        &serde_json::to_vec(&details.modality)?,
                    ),
                );
            }
        } else {
            unsupported.extend([Dimension::Quantity, Dimension::Date, Dimension::Modality]);
        }
        let declared = details
            .and_then(|item| item.requirement_id.as_deref())
            .or_else(|| {
                fact.and_then(|item| item.attributes.get("decision_id").map(String::as_str))
            });
        static IDS: OnceLock<regex::Regex> = OnceLock::new();
        let ids = IDS.get_or_init(|| {
            regex::Regex::new(r"(?i)\b(?:REQ|NFR|FR|AC|R|ADR|DEC)[-_]?\d+(?:[._-]\d+)*\b")
                .expect("static written document ID")
        });
        let written: BTreeSet<_> = if declared.is_some() || category == "decision" {
            ids.find_iter(body)
                .map(|item| item.as_str())
                .take(65)
                .collect()
        } else {
            BTreeSet::new()
        };
        let explicit = if written.len() == 1 {
            declared.or_else(|| written.first().copied())
        } else if written.is_empty() {
            declared
        } else {
            complete = false;
            None
        };
        let provenance = hash(
            b"document-provenance/1",
            &serde_json::to_vec(&(
                self.provenance,
                details.map(|item| (&item.parser_version, &item.basis)),
                mapping.map(|item| &item.profile),
                // Native page/slide/table coordinates are part of matching
                // eligibility. A DOCX paragraph ordinal intentionally is not.
                match &location {
                    DocumentLocation::Lines { .. } | DocumentLocation::Block { .. } => None,
                    native => Some(native),
                },
            ))?,
        );
        dimensions.insert(
            Dimension::Evidence,
            hash(
                b"document-evidence/1",
                &serde_json::to_vec(&(
                    self.profile,
                    provenance,
                    details.and_then(|item| item.table.as_ref()),
                ))?,
            ),
        );
        let descriptor = ContinuityDescriptor {
            algorithm_version: CONTINUITY_VERSION,
            subject,
            owner,
            region,
            family: ContinuityFamily::Documents,
            category: category.into(),
            name: heading.then(|| hash(b"document-heading/1", statement.as_bytes())),
            explicit_identity: explicit
                .map(|value| hash(b"document-explicit-id/1", value.as_bytes())),
            body_informative: false,
            profile: self.profile,
            provenance,
            dimensions,
            unsupported_dimensions: unsupported,
            span: span.clone(),
            location: location.clone(),
            complete,
        };
        let size = serde_json::to_vec(&descriptor)?.len().saturating_add(1);
        if self.encoded_bytes.saturating_add(size) > MAX_CONTINUITY_BYTES {
            self.partial("document continuity encoded byte budget exceeded");
            return Ok(false);
        }
        self.encoded_bytes += size;
        if !complete {
            self.partial("document continuity has incomplete extraction evidence");
        }
        let (start_line, end_line) = self.line_numbers(region, &span);
        self.draft.chunk_hints.push(LocalChunk {
            span,
            start_line,
            end_line,
            location,
            region,
        });
        self.draft.descriptors.push(descriptor);
        Ok(true)
    }
}

/// Describe written units only. Uniqueness reconciliation and assigned keys are
/// separate operations; duplicate descriptors are retained, never overwritten.
pub fn describe(
    payload: &ExtractionPayload,
    profile: &ExtractionProfile,
    budget: &mut ContinuityBudget,
) -> Result<ContinuityDraft> {
    payload.validate()?;
    let supported = native(profile)
        || matches!(
            profile.language.as_str(),
            "markdown" | "instruction-markdown" | "text"
        )
        || payload.facts.iter().any(|fact| fact.document.is_some())
        || !payload.regions.is_empty();
    if !supported {
        return Ok(ContinuityDraft::default());
    }
    let provenance_bytes = serde_json::to_vec(&(
        &payload.extraction_method,
        &payload.parse_status,
        &payload.provenance,
    ))?;
    if budget.charge(provenance_bytes.len()).is_err() {
        return Ok(ContinuityDraft {
            supported: true,
            partial: true,
            diagnostics: vec!["document continuity work budget exceeded".into()],
            ..Default::default()
        });
    }
    let mut builder = Builder {
        payload,
        profile: profile.identity()?,
        provenance: hash(b"document-extraction/1", &provenance_bytes),
        budget,
        draft: ContinuityDraft {
            supported: true,
            partial: payload.partial,
            ..Default::default()
        },
        encoded_bytes: 2,
        lines: std::iter::once((None, &payload.text))
            .chain(
                payload
                    .regions
                    .iter()
                    .map(|region| (Some(region.id), &region.text)),
            )
            .map(|(id, text)| {
                (
                    id,
                    std::iter::once(0)
                        .chain(text.match_indices('\n').map(|(index, _)| index + 1))
                        .collect(),
                )
            })
            .collect(),
    };
    let mut used = BTreeSet::new();
    let mut headings: Vec<(u8, usize, ContinuitySubject)> = vec![];
    let mut heading_positions = vec![];
    let mut facts_by_start: BTreeMap<usize, Vec<&LocalFact>> = BTreeMap::new();
    for fact in &payload.facts {
        if fact.region.is_none() && fact.kind == "section" && fact.document.is_some() {
            facts_by_start
                .entry(fact.span.start)
                .or_default()
                .push(fact);
        }
    }
    if !native(profile)
        && matches!(
            profile.language.as_str(),
            "markdown" | "instruction-markdown" | "text"
        )
    {
        let units = match crate::prose::continuity_units(
            &payload.text,
            matches!(
                profile.language.as_str(),
                "markdown" | "instruction-markdown"
            ),
        ) {
            Ok(units) => units,
            Err(_) => {
                builder.partial("document structural extraction exceeded bounds");
                return Ok(builder.draft);
            }
        };
        for (index, unit) in units.into_iter().enumerate() {
            if unit.visible.trim().is_empty() {
                continue;
            }
            if let Some(level) = unit.level {
                headings.retain(|(old, _, _)| *old < level);
            }
            let fact = if unit.level.is_some() {
                facts_by_start.get(&unit.span.start).and_then(|facts| {
                    facts
                        .iter()
                        .copied()
                        .find(|fact| fact.span.end >= unit.span.end)
                })
            } else {
                None
            };
            let subject = fact.map_or(ContinuitySubject::Unit(index as u32), |fact| {
                ContinuitySubject::Fact(fact.id)
            });
            let (start, end) = builder.line_numbers(
                None,
                &LocalSpan {
                    start: unit.span.start,
                    end: unit.span.end,
                },
            );
            let category = if unit.level.is_some() {
                "section"
            } else {
                match unit.origin {
                    "markdown-list-item" => "list_item",
                    "markdown-table-row" | "markdown-table-header" => "table_row",
                    _ => "paragraph",
                }
            };
            if !builder.add(
                SubjectContext {
                    subject,
                    owner: headings.last().map(|item| item.2),
                    region: None,
                },
                category,
                LocalSpan {
                    start: unit.span.start,
                    end: unit.span.end,
                },
                DocumentLocation::Lines { start, end },
                unit.level.is_some(),
                fact,
            )? {
                break;
            }
            if let Some(fact) = fact {
                used.insert(fact.id);
            }
            if let Some(level) = unit.level {
                headings.push((level, unit.span.start, subject));
                heading_positions.push((unit.span.start, subject));
            }
        }
    }
    let mut ordered_facts: Vec<_> = payload.facts.iter().collect();
    ordered_facts.sort_by_key(|fact| (fact.region, fact.span.start, fact.id));
    let mut native_headings: Vec<(usize, ContinuitySubject)> = vec![];
    for fact in ordered_facts {
        if used.contains(&fact.id)
            || fact.code.is_some()
            || fact
                .document
                .as_ref()
                .is_some_and(|details| details.basis == "untrusted_structural_rule")
        {
            continue;
        }
        let generic_native = native(profile) && fact.kind == "section";
        if fact.document.is_none() && !generic_native {
            continue;
        }
        let heading = fact.document.as_ref().is_some_and(|details| {
            details.basis == "explicit-heading" || details.basis == "heuristic-heading"
        });
        if native(profile) && heading {
            let depth = fact
                .document
                .as_ref()
                .map_or(1, |details| details.heading_path.len().max(1));
            native_headings.retain(|(old, _)| *old < depth);
        }
        let owner = if native(profile) {
            native_headings.last().map(|(_, subject)| *subject)
        } else if fact.region.is_none() {
            let index =
                heading_positions.partition_point(|(position, _)| *position < fact.span.start);
            index.checked_sub(1).map(|index| heading_positions[index].1)
        } else {
            None
        };
        let kind = if generic_native && fact.document.is_none() {
            category(&fact.location)
        } else {
            fact.kind.as_str()
        };
        if !builder.add(
            SubjectContext {
                subject: ContinuitySubject::Fact(fact.id),
                owner,
                region: fact.region,
            },
            kind,
            fact.span.clone(),
            fact.location.clone(),
            heading,
            Some(fact),
        )? {
            break;
        }
        if native(profile) && heading && fact.region.is_none() {
            let depth = fact
                .document
                .as_ref()
                .map_or(1, |details| details.heading_path.len().max(1));
            native_headings.push((depth, ContinuitySubject::Fact(fact.id)));
        }
    }
    // Existing region primitives are accepted only with already validated maps
    // and an actual native locator. This does not enable any new host adapter.
    for region in &payload.regions {
        let host = region
            .parent
            .and_then(|id| payload.regions.iter().find(|parent| parent.id == id))
            .map_or(payload.text.as_str(), |parent| parent.text.as_str());
        let original = region.source_map.original_spans(
            airk_core::location::ByteSpan {
                start: 0,
                end: region.text.len(),
            },
            &region.text,
            host,
        )?;
        let location = if region.parent.is_none() {
            original
                .as_ref()
                .filter(|spans| {
                    !spans.is_empty() && spans.windows(2).all(|pair| pair[0].end == pair[1].start)
                })
                .map(|spans| {
                    let (start, end) = builder.line_numbers(
                        None,
                        &LocalSpan {
                            start: spans[0].start,
                            end: spans.last().unwrap().end,
                        },
                    );
                    DocumentLocation::Lines { start, end }
                })
        } else {
            None
        };
        let Some(location) = location else {
            builder.partial("region has no qualified native document locator");
            continue;
        };
        if !builder.add(
            SubjectContext {
                subject: ContinuitySubject::Region(region.id),
                owner: None,
                region: Some(region.id),
            },
            "embedded_region",
            LocalSpan {
                start: 0,
                end: region.text.len(),
            },
            location,
            false,
            None,
        )? {
            break;
        }
    }
    ensure!(
        builder.draft.descriptors.len() <= airk_core::continuity::MAX_CONTINUITY_UNITS,
        "document descriptor overflow"
    );
    Ok(builder.draft)
}

#[cfg(test)]
mod tests {
    use super::*;
    use airk_core::{
        DocumentEvidence,
        extraction::LocalRegion,
        location::{ByteSpan, MapSegment, Mapping, RegionLocation, SourceMap},
    };

    fn profile(language: &str) -> ExtractionProfile {
        ExtractionProfile {
            contract_version: 1,
            adapter_id: language.into(),
            adapter_version: "original-fixture/1".into(),
            language: language.into(),
            dialect: None,
            grammar_identity: None,
            mapping_version: 1,
            chunk_version: 1,
            options: BTreeMap::new(),
            configuration_inputs: BTreeMap::new(),
        }
    }
    fn payload(text: &str) -> ExtractionPayload {
        ExtractionPayload {
            contract_version: 1,
            extraction_method: "original-document-fixture".into(),
            parse_status: "text".into(),
            provenance: vec![],
            text: text.into(),
            facts: vec![],
            syntax_identities: vec![],
            chunks: vec![],
            regions: vec![],
            relationships: vec![],
            continuity: vec![],
            continuity_coverage: BTreeMap::new(),
            diagnostics: vec![],
            partial: false,
        }
    }
    fn fact(id: u32, text: &str, kind: &str, location: DocumentLocation) -> LocalFact {
        LocalFact {
            id,
            owner: None,
            kind: kind.into(),
            name: "deliberately truncated display".into(),
            span: LocalSpan {
                start: 0,
                end: text.len(),
            },
            syntax_kind: "original-prose".into(),
            start_line: 1,
            end_line: text.lines().count().max(1),
            location,
            document: Some(DocumentEvidence::default()),
            code: None,
            region: None,
            attributes: BTreeMap::new(),
        }
    }
    fn draft(text: &str) -> Result<ContinuityDraft> {
        describe(
            &payload(text),
            &profile("markdown"),
            &mut ContinuityBudget::default(),
        )
    }
    #[test]
    fn paragraph_shift_preserves_full_body_but_changes_location() -> Result<()> {
        let a = draft("# Scope\n\nOriginal café paragraph.\n")?;
        let b = draft("# Scope\n\nAn inserted paragraph.\n\nOriginal café paragraph.\n")?;
        let first = a
            .descriptors
            .iter()
            .find(|d| d.category == "paragraph")
            .unwrap();
        let later = b
            .descriptors
            .iter()
            .find(|d| {
                d.dimensions[&Dimension::Statement] == first.dimensions[&Dimension::Statement]
            })
            .unwrap();
        assert_eq!(first.owner, Some(a.descriptors[0].subject));
        assert_ne!(
            first.dimensions[&Dimension::Location],
            later.dimensions[&Dimension::Location]
        );
        assert_eq!(
            first.dimensions[&Dimension::Content],
            later.dimensions[&Dimension::Content]
        );
        assert!(first.explicit_identity.is_none() && first.name.is_none());
        Ok(())
    }
    #[test]
    fn explicit_requirement_and_decision_identity_survives_body_edits() -> Result<()> {
        for (kind, id) in [("requirement", "REQ-42"), ("decision", "ADR-17")] {
            let mut outputs = vec![];
            for value in ["5", "7"] {
                let text = format!("{id} must retain {value} days.\n");
                let mut p = payload(&text);
                let mut f = fact(0, &text, kind, DocumentLocation::Lines { start: 1, end: 1 });
                if kind == "requirement" {
                    f.document.as_mut().unwrap().requirement_id = Some(id.into());
                }
                p.facts.push(f);
                outputs.push(
                    describe(&p, &profile("text"), &mut ContinuityBudget::default())?
                        .descriptors
                        .into_iter()
                        .find(|d| d.subject == ContinuitySubject::Fact(0))
                        .unwrap(),
                );
            }
            assert!(outputs[0].explicit_identity.is_some());
            assert_eq!(outputs[0].explicit_identity, outputs[1].explicit_identity);
            assert_ne!(
                outputs[0].dimensions[&Dimension::Statement],
                outputs[1].dimensions[&Dimension::Statement]
            );
        }
        Ok(())
    }
    #[test]
    fn duplicate_headings_and_paragraphs_are_retained_for_ambiguity_reconciliation() -> Result<()> {
        let result =
            draft("# Same\n\nRepeated text.\n\nRepeated text.\n\n# Same\n\nRepeated text.\n")?;
        let headings: Vec<_> = result
            .descriptors
            .iter()
            .filter(|d| d.category == "section")
            .collect();
        assert_eq!(headings.len(), 2);
        assert_eq!(headings[0].name, headings[1].name);
        assert!(headings.iter().all(|d| d.owner.is_none()));
        let paragraphs: Vec<_> = result
            .descriptors
            .iter()
            .filter(|d| d.category == "paragraph")
            .collect();
        assert_eq!(paragraphs.len(), 3);
        assert_eq!(
            paragraphs[0].dimensions[&Dimension::Statement],
            paragraphs[1].dimensions[&Dimension::Statement]
        );
        assert_eq!(paragraphs[0].owner, paragraphs[1].owner);
        assert_ne!(paragraphs[1].owner, paragraphs[2].owner);
        Ok(())
    }
    #[test]
    fn native_page_slide_table_and_sheet_coordinates_are_preserved() -> Result<()> {
        for location in [
            DocumentLocation::Page { number: 3 },
            DocumentLocation::Slide { number: 2 },
            DocumentLocation::Block { number: 8 },
            DocumentLocation::TableRow { table: 2, row: 4 },
            DocumentLocation::WorksheetRow {
                sheet: "Original sheet".into(),
                row: 12,
            },
        ] {
            let text = "Original native body beyond the display label.\n";
            let mut p = payload(text);
            let mut f = fact(0, text, "section", location.clone());
            f.document = None;
            p.facts.push(f);
            let result = describe(&p, &profile("docx"), &mut ContinuityBudget::default())?;
            assert_eq!(result.descriptors[0].location, location);
            assert_eq!(result.chunk_hints[0].location, location);
            assert_eq!(
                result.descriptors[0].dimensions[&Dimension::Statement],
                hash(b"document-statement/1", normalized(text).as_bytes())
            );
        }
        Ok(())
    }

    #[test]
    fn native_coordinate_and_ocr_provenance_changes_invalidate_matching() -> Result<()> {
        let text = "Original recognized words.\n";
        let build = |location, status: &str| -> Result<ContinuityDescriptor> {
            let mut p = payload(text);
            p.parse_status = status.into();
            p.facts.push(fact(0, text, "requirement", location));
            Ok(
                describe(&p, &profile("ocr"), &mut ContinuityBudget::default())?
                    .descriptors
                    .remove(0),
            )
        };
        let first = build(DocumentLocation::Page { number: 1 }, "original-engine-v1")?;
        let page = build(DocumentLocation::Page { number: 2 }, "original-engine-v1")?;
        let engine = build(DocumentLocation::Page { number: 1 }, "original-engine-v2")?;
        assert_ne!(first.provenance, page.provenance);
        assert_ne!(first.provenance, engine.provenance);
        assert_eq!(
            first.dimensions[&Dimension::Statement],
            page.dimensions[&Dimension::Statement]
        );
        let a = build(DocumentLocation::Block { number: 1 }, "original-engine-v1")?;
        let b = build(DocumentLocation::Block { number: 2 }, "original-engine-v1")?;
        assert_eq!(
            a.provenance, b.provenance,
            "paragraph insertion is a location change"
        );
        assert_ne!(
            a.dimensions[&Dimension::Location],
            b.dimensions[&Dimension::Location]
        );
        Ok(())
    }
    #[test]
    fn crlf_normalization_preserves_literal_spacing_and_utf8() -> Result<()> {
        let a = draft("A café  value.\r\n")?;
        let b = draft("A café  value.\n")?;
        let c = draft("A café value.\n")?;
        assert_eq!(
            a.descriptors[0].dimensions[&Dimension::Statement],
            b.descriptors[0].dimensions[&Dimension::Statement]
        );
        assert_ne!(
            b.descriptors[0].dimensions[&Dimension::Statement],
            c.descriptors[0].dimensions[&Dimension::Statement]
        );
        let mut invalid = payload("café");
        let mut f = fact(
            0,
            "café",
            "requirement",
            DocumentLocation::Lines { start: 1, end: 1 },
        );
        f.span.end = 4;
        invalid.facts.push(f);
        assert!(describe(&invalid, &profile("text"), &mut ContinuityBudget::default()).is_err());
        Ok(())
    }
    #[test]
    fn budgets_yield_partial_without_emitting_unbounded_descriptors() -> Result<()> {
        let mut budget = ContinuityBudget {
            units: airk_core::continuity::MAX_CONTINUITY_UNITS,
            hash_bytes: 0,
        };
        let result = describe(
            &payload("Original paragraph.\n"),
            &profile("text"),
            &mut budget,
        )?;
        assert!(result.supported && result.partial && result.descriptors.is_empty());
        let mut budget = ContinuityBudget {
            units: 0,
            hash_bytes: airk_core::continuity::MAX_CONTINUITY_HASH_BYTES,
        };
        assert!(
            describe(
                &payload("Original paragraph.\n"),
                &profile("text"),
                &mut budget
            )?
            .partial
        );
        Ok(())
    }
    #[test]
    fn mapped_region_uses_exact_host_span_and_rejects_partial_escape_mapping() -> Result<()> {
        let mut p = payload("prefix\nlet café = 1;\n");
        let text = "let café = 1;\n";
        let host = ByteSpan {
            start: 7,
            end: p.text.len(),
        };
        p.regions.push(LocalRegion {
            id: 1,
            parent: None,
            profile: profile("javascript"),
            text: text.into(),
            source_map: SourceMap {
                version: 1,
                segments: vec![MapSegment {
                    local: ByteSpan {
                        start: 0,
                        end: text.len(),
                    },
                    mapping: Mapping::Verbatim { host },
                }],
            },
            location: RegionLocation::Embedded {
                region_id: "original-script".into(),
                local_span: ByteSpan {
                    start: 0,
                    end: text.len(),
                },
                original_spans: vec![host],
            },
        });
        let result = describe(&p, &profile("html"), &mut ContinuityBudget::default())?;
        assert_eq!(result.descriptors[0].subject, ContinuitySubject::Region(1));
        assert_eq!(
            result.descriptors[0].location,
            DocumentLocation::Lines { start: 2, end: 2 }
        );
        let map = SourceMap {
            version: 1,
            segments: vec![MapSegment {
                local: ByteSpan { start: 0, end: 2 },
                mapping: Mapping::Decoded {
                    host: ByteSpan { start: 0, end: 6 },
                },
            }],
        };
        assert!(
            map.original_spans(ByteSpan { start: 0, end: 1 }, "ab", "\\u0061")?
                .is_none()
        );
        Ok(())
    }
    #[test]
    fn malformed_markdown_remains_bounded_and_never_hashes_display_names() -> Result<()> {
        let result = draft("# Original\n\n[unfinished **markup\n\n```rust\nunterminated\n")?;
        assert!(!result.descriptors.is_empty());
        let mut p = payload("REQ-1 must keep the complete original statement.\n");
        let mut f = fact(
            0,
            &p.text,
            "requirement",
            DocumentLocation::Lines { start: 1, end: 1 },
        );
        f.document.as_mut().unwrap().statement_truncated = true;
        p.facts.push(f);
        let result = describe(&p, &profile("text"), &mut ContinuityBudget::default())?;
        let item = result
            .descriptors
            .iter()
            .find(|d| d.subject == ContinuitySubject::Fact(0))
            .unwrap();
        assert!(item.complete);
        assert_eq!(
            item.dimensions[&Dimension::Statement],
            hash(b"document-statement/1", normalized(&p.text).as_bytes())
        );
        Ok(())
    }
}
