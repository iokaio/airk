// SPDX-License-Identifier: Apache-2.0
//! Exact semantic body coordinates, independent of embedding execution.
use crate::{Citation, DocumentLocation, Hit, RetrievalArtifact, extraction::LocalSpan};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};

pub const MAX_MEMBERS: usize = 64;
pub const MAX_BODY_BYTES: usize = 16_384;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VectorMemberSpan {
    pub chunk_index: u32,
    /// Absolute byte positions within the chunk's host or mapped region.
    pub span: LocalSpan,
}

pub fn validate_members(members: &[VectorMemberSpan]) -> Result<()> {
    ensure!(
        !members.is_empty() && members.len() <= MAX_MEMBERS,
        "invalid semantic member count"
    );
    let mut previous: Option<&VectorMemberSpan> = None;
    let mut bytes = 0usize;
    for member in members {
        ensure!(
            member.span.start < member.span.end
                && member.span.end <= crate::extraction::MAX_TEXT_BYTES
                && (member.chunk_index as usize) < crate::extraction::MAX_EXTRACTION_CHUNKS,
            "invalid semantic member coordinates"
        );
        bytes = bytes
            .checked_add(member.span.end - member.span.start)
            .context("semantic body size overflow")?;
        ensure!(bytes <= MAX_BODY_BYTES, "semantic body byte limit exceeded");
        if let Some(prior) = previous {
            ensure!(
                prior.chunk_index <= member.chunk_index && prior.span.end == member.span.start,
                "semantic members have a gap, overlap or reversed chunk order"
            );
        }
        previous = Some(member);
    }
    Ok(())
}

/// Reconstruct only original body text from a verified retained extraction.
/// Synthetic context used by the embedding model is never citation evidence.
pub fn materialize(retained: &RetrievalArtifact, members: &[VectorMemberSpan]) -> Result<Hit> {
    VectorEvidenceReader::new(retained)?.materialize(members)
}

/// One artifact shares source-map validation and mapping work across all units.
/// Callers bound artifact loads separately from the per-body output bound.
pub struct VectorEvidenceReader<'a> {
    retained: &'a RetrievalArtifact,
    mapper: Option<crate::mapped_binding::MappedBinding<'a>>,
    budget: crate::location::HostBudget,
}

impl<'a> VectorEvidenceReader<'a> {
    pub fn new(retained: &'a RetrievalArtifact) -> Result<Self> {
        let mut budget = crate::location::HostBudget::new(500_000, 10_000, 16 << 20)?;
        let mapper = if retained.payload.regions.is_empty() {
            None
        } else {
            Some(crate::mapped_binding::MappedBinding::new(
                &retained.payload,
                &retained.profile,
                &mut budget,
            )?)
        };
        Ok(Self {
            retained,
            mapper,
            budget,
        })
    }

    pub fn materialize(&mut self, members: &[VectorMemberSpan]) -> Result<Hit> {
        let retained = self.retained;
        validate_members(members)?;
        let first = &members[0];
        let first_local = retained
            .payload
            .chunks
            .get(first.chunk_index as usize)
            .context("semantic chunk missing")?;
        let first_chunk = retained
            .artifact
            .chunks
            .get(first.chunk_index as usize)
            .context("bound semantic chunk missing")?;
        let mut text = String::new();
        let mut first_line = 0;
        let mut last_line = 0;
        for member in members {
            let local = retained
                .payload
                .chunks
                .get(member.chunk_index as usize)
                .context("semantic chunk missing")?;
            let chunk = retained
                .artifact
                .chunks
                .get(member.chunk_index as usize)
                .context("bound semantic chunk missing")?;
            ensure!(
                local.region == first_local.region
                    && local.span.start <= member.span.start
                    && member.span.end <= local.span.end,
                "semantic member crosses a chunk or region boundary"
            );
            if local.region.is_none() {
                ensure!(
                    match (&first_chunk.location, &chunk.location) {
                        (DocumentLocation::Lines { .. }, DocumentLocation::Lines { .. }) => true,
                        (a, b) => a == b,
                    },
                    "semantic body crosses a native document location"
                );
            }
            let start = member.span.start - local.span.start;
            let end = member.span.end - local.span.start;
            let part = chunk
                .text
                .get(start..end)
                .context("semantic member is outside UTF-8 text")?;
            let start_line =
                chunk.start_line + chunk.text[..start].bytes().filter(|b| *b == b'\n').count();
            if text.is_empty() {
                first_line = start_line;
            }
            last_line = start_line
                + part.as_bytes()[..part.len() - 1]
                    .iter()
                    .filter(|b| **b == b'\n')
                    .count();
            text.push_str(part);
        }
        let location = if let Some(region) = first_local.region {
            let mapper = self
                .mapper
                .as_ref()
                .context("semantic mapped region missing")?;
            let span = LocalSpan {
                start: first.span.start,
                end: members.last().expect("nonempty members").span.end,
            };
            let (location, start, end) = mapper.mapped_location(region, span, &mut self.budget)?;
            first_line = start;
            last_line = end;
            location
        } else {
            match &first_chunk.location {
                DocumentLocation::Lines { .. } => DocumentLocation::Lines {
                    start: first_line,
                    end: last_line,
                },
                location => location.clone(),
            }
        };
        Ok(Hit {
            citation: Citation {
                path: retained.artifact.path.clone(),
                content_hash: retained.artifact.content_hash.clone(),
                start_line: first_line,
                end_line: last_line,
                location,
            },
            text,
            truncated: false,
        })
    }
}
