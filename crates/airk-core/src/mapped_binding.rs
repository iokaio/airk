// SPDX-License-Identifier: Apache-2.0
//! Pure original-host binding of validated region-local extraction evidence.
//! Requires the additive mapped DocumentLocation variant; no live source reads.
use crate::extraction::{
    ExtractionPayload, ExtractionProfile, LocalRegion, LocalSpan, LocalSyntaxId,
};
use crate::location::{ByteSpan, HostBudget, RegionLocation, SourceMap};
use crate::{Artifact, Chunk, CodeEvidence, DocumentLocation, Fact};
use anyhow::{Context, Result, ensure};
use std::collections::BTreeMap;

#[path = "mapped_body.rs"]
mod body;
pub use body::{MAX_MAPPED_BODY_BYTES, MAX_MAPPED_BODY_PROJECTION_BYTES, MappedBodyProjection};

pub struct MappedBinding<'a> {
    payload: &'a ExtractionPayload,
    regions: BTreeMap<u32, &'a LocalRegion>,
    host_lines: Vec<usize>,
}
impl<'a> MappedBinding<'a> {
    /// Each region is admitted once, independently of its fact/chunk count.
    pub fn new(
        payload: &'a ExtractionPayload,
        profile: &ExtractionProfile,
        budget: &mut HostBudget,
    ) -> Result<Self> {
        profile.identity()?;
        budget.charge(
            payload.facts.len() + payload.chunks.len() + payload.syntax_identities.len(),
            payload.facts.len(),
            payload.text.len(),
            0,
        )?;
        for region in &payload.regions {
            budget.charge(1, 0, region.text.len(), region.source_map.segments.len())?;
        }
        payload.validate_with_text_limit(crate::extraction::MAX_TEXT_BYTES)?;
        ensure!(
            payload.relationships.is_empty(),
            "mapped artifact binding cannot represent explicit relationships"
        );
        ensure!(
            payload
                .facts
                .iter()
                .all(|fact| fact.owner.is_none() && fact.attributes.is_empty()),
            "mapped artifact binding cannot represent generic fact owners or attributes"
        );
        let regions: BTreeMap<_, _> = payload
            .regions
            .iter()
            .map(|region| (region.id, region))
            .collect();
        for region in &payload.regions {
            let mut depth = 1;
            let mut parent = region.parent;
            while let Some(id) = parent {
                depth += 1;
                parent = regions[&id].parent;
            }
            budget.enter_region(depth)?;
        }
        let mut host_lines = vec![0];
        host_lines.extend(payload.text.match_indices('\n').map(|(i, _)| i + 1));
        Ok(Self {
            payload,
            regions,
            host_lines,
        })
    }

    pub fn text(&self, region: Option<u32>) -> Result<&'a str> {
        match region {
            None => Ok(&self.payload.text),
            Some(id) => Ok(&self.regions.get(&id).context("missing mapped region")?.text),
        }
    }

    pub fn mapped_location(
        &self,
        region_id: u32,
        span: LocalSpan,
        budget: &mut HostBudget,
    ) -> Result<(DocumentLocation, usize, usize)> {
        let region = *self
            .regions
            .get(&region_id)
            .context("missing mapped region")?;
        let precise = ByteSpan {
            start: span.start,
            end: span.end,
        };
        precise.validate(&region.text)?;
        ensure!(precise.start < precise.end, "empty mapped citation");
        let (mapped_span, local_lines) = match &region.location {
            RegionLocation::NotebookCell { .. } => {
                let first = region.text[..precise.start]
                    .bytes()
                    .filter(|b| *b == b'\n')
                    .count()
                    + 1;
                let last = region.text.as_bytes()[..precise.end.saturating_sub(1)]
                    .iter()
                    .filter(|b| **b == b'\n')
                    .count()
                    + 1;
                let start = region.text[..precise.start]
                    .rfind('\n')
                    .map_or(0, |i| i + 1);
                let end = region.text[precise.end..]
                    .find('\n')
                    .map_or(region.text.len(), |i| precise.end + i + 1);
                // A span ending just after a newline already covers complete lines.
                let end = if region.text.as_bytes()[precise.end - 1] == b'\n' {
                    precise.end
                } else {
                    end
                };
                (ByteSpan { start, end }, Some((first, last)))
            }
            RegionLocation::Embedded { .. } => (precise, None),
        };
        let mut pieces = vec![mapped_span];
        let mut current = region;
        loop {
            let host = match current.parent {
                Some(parent) => self.regions[&parent].text.as_str(),
                None => self.payload.text.as_str(),
            };
            pieces = through(&current.source_map, &current.text, host, pieces, budget)?;
            match current.parent {
                Some(parent) => current = self.regions[&parent],
                None => break,
            }
        }
        ensure!(!pieces.is_empty(), "mapped citation has no original source");
        let first = self
            .host_lines
            .partition_point(|offset| *offset <= pieces[0].start);
        let last = self
            .host_lines
            .partition_point(|offset| *offset < pieces.last().unwrap().end)
            .max(first);
        let location = match &region.location {
            RegionLocation::Embedded { region_id, .. } => RegionLocation::Embedded {
                region_id: region_id.clone(),
                local_span: precise,
                original_spans: pieces,
            },
            RegionLocation::NotebookCell {
                cell_id, position, ..
            } => {
                let (local_start_line, local_end_line) = local_lines.unwrap();
                RegionLocation::NotebookCell {
                    cell_id: cell_id.clone(),
                    position: *position,
                    local_start_line,
                    local_end_line,
                    original_spans: pieces,
                }
            }
        };
        location.validate_shape()?;
        Ok((DocumentLocation::Mapped { region: location }, first, last))
    }
}

fn through(
    map: &SourceMap,
    local: &str,
    host: &str,
    pieces: Vec<ByteSpan>,
    budget: &mut HostBudget,
) -> Result<Vec<ByteSpan>> {
    let mut contiguous: Vec<ByteSpan> = Vec::with_capacity(pieces.len());
    for piece in pieces {
        if let Some(last) = contiguous.last_mut()
            && last.end == piece.start
        {
            last.end = piece.end;
        } else {
            contiguous.push(piece);
        }
    }
    let mut mapped = Vec::new();
    for piece in contiguous {
        let first = map
            .segments
            .partition_point(|segment| segment.local.end <= piece.start);
        let end = map
            .segments
            .partition_point(|segment| segment.local.start < piece.end);
        budget.charge(1 + end.saturating_sub(first), 0, 0, 0)?;
        let spans = map
            .original_spans_validated(piece, local, host)?
            .context("unmapped or partially decoded citation")?;
        ensure!(
            mapped.len().saturating_add(spans.len()) <= crate::location::MAX_MAP_SEGMENTS,
            "composed citation segment limit"
        );
        mapped.extend(spans);
    }
    Ok(mapped)
}

/// Preserve host syntax IDs byte-for-byte; region-local spans use a distinct namespace.
pub fn syntax_id(path: &str, region: Option<u32>, span: &LocalSpan, kind: &str) -> String {
    match region {
        None => format!("syntax:{path}:{}:{}:{kind}", span.start, span.end),
        Some(id) => format!(
            "syntax-region:{path}:{id}:{}:{}:{kind}",
            span.start, span.end
        ),
    }
}

pub fn bind_payload(
    payload: &ExtractionPayload,
    profile: &ExtractionProfile,
    binding: crate::binding::ArtifactBinding<'_>,
    budget: &mut HostBudget,
) -> Result<Artifact> {
    if payload.regions.is_empty() {
        return crate::binding::bind_payload(payload, profile, binding);
    }
    ensure!(
        payload.contract_version == 2 && profile.contract_version == 2,
        "mapped binding requires extraction contract 2"
    );
    ensure!(
        !binding.path.is_empty()
            && binding.path.len() <= 4096
            && !binding.path.starts_with('/')
            && !binding.path.contains(['\\', ':', '\0'])
            && binding
                .path
                .split('/')
                .all(|p| !matches!(p, "" | "." | "..")),
        "invalid mapped artifact path"
    );
    ensure!(
        binding.source_bytes <= crate::extraction::MAX_SOURCE_BYTES,
        "mapped source size exceeds ceiling"
    );
    let mapper = MappedBinding::new(payload, profile, budget)?;
    let identities: BTreeMap<LocalSyntaxId, String> = payload
        .syntax_identities
        .iter()
        .map(|id| {
            (
                id.id,
                syntax_id(binding.path, id.region, &id.span, &id.syntax_kind),
            )
        })
        .collect();
    let mut facts = Vec::with_capacity(payload.facts.len());
    for fact in &payload.facts {
        let (location, start_line, end_line) = match fact.region {
            Some(region) => mapper.mapped_location(region, fact.span.clone(), budget)?,
            None => (fact.location.clone(), fact.start_line, fact.end_line),
        };
        facts.push(Fact {
            name: fact.name.clone(),
            kind: fact.kind.clone(),
            syntax_kind: fact.syntax_kind.clone(),
            start_line,
            end_line,
            location,
            document: fact.document.clone(),
            code: fact.code.as_ref().map(|code| CodeEvidence {
                fingerprints: code.fingerprints.clone(),
                symbol_id: identities[&code.syntax_id].clone(),
                owner_id: code.owner_id.map(|id| identities[&id].clone()),
                qualified_name: code.qualified_name.clone(),
                scope: code.scope.clone(),
                signature: code.signature.clone(),
                span: code.span.clone(),
                signature_span: code.signature_span.clone(),
                parameters: code.parameters.clone(),
                return_type: code.return_type.clone(),
                modifiers: code.modifiers.clone(),
                documentation: code.documentation.clone(),
                target: code.target.clone(),
                basis: code.basis.clone(),
                producer: code.producer.clone(),
                producer_version: code.producer_version.clone(),
                coverage: code.coverage.clone(),
                details_truncated: code.details_truncated,
            }),
        });
    }
    let mut chunks = Vec::with_capacity(payload.chunks.len());
    for chunk in &payload.chunks {
        let (location, start_line, end_line) = match chunk.region {
            Some(region) => mapper.mapped_location(region, chunk.span.clone(), budget)?,
            None => (chunk.location.clone(), chunk.start_line, chunk.end_line),
        };
        chunks.push(Chunk {
            start_line,
            end_line,
            text: mapper.text(chunk.region)?[chunk.span.start..chunk.span.end].into(),
            location,
        });
    }
    Ok(Artifact {
        path: binding.path.into(),
        content_hash: binding.source_hash.to_string(),
        bytes: binding.source_bytes,
        chunks,
        language: profile.language.clone(),
        extraction_method: payload.extraction_method.clone(),
        parse_status: payload.parse_status.clone(),
        facts,
        retained: None,
    })
}
