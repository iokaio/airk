// SPDX-License-Identifier: Apache-2.0
//! Bounded original-source locators for a retained body, without retaining a host.
use super::MappedBinding;
use crate::DocumentLocation;
use crate::extraction::{LocalRegion, LocalSpan};
use crate::location::{ByteSpan, HostBudget, MAX_MAP_SEGMENTS, Mapping, RegionLocation};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};

pub const MAX_MAPPED_BODY_BYTES: usize = 16_384;
pub const MAX_MAPPED_BODY_PROJECTION_BYTES: usize = 4 * 1024 * 1024;
const MAX_SOURCE_COORDINATE: usize = 16_777_216;

/// An authenticated publication sidecar can carry this small projection beside
/// its body. It contains coordinates only, never original or synthetic text.
/// Decoded atoms and notebook lines remain indivisible citation units: focusing
/// inside one cites its whole origin, not invented encoded-byte coordinates.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MappedBodyProjection {
    version: u32,
    body_span: ByteSpan,
    body_digest: crate::identity::Sha256Digest,
    kind: BodyKind,
    segments: Vec<Segment>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum BodyKind {
    Embedded {
        region_id: String,
    },
    Notebook {
        cell_id: Option<String>,
        position: usize,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Segment {
    body: ByteSpan,
    mapping: BodyMapping,
    notebook_lines: Option<ByteSpan>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum BodyMapping {
    Verbatim { origin: Origin },
    Atomic { origins: Vec<Origin> },
    Unmapped,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Origin {
    // Original-map segment identity permits joining portions of one verbatim
    // run without erasing actual gaps or distinct decoded source units.
    segment: usize,
    span: ByteSpan,
    start_line: usize,
    end_line: usize,
}

enum Mapped {
    Exact { pieces: Vec<Origin>, verbatim: bool },
    Partial,
    Unmapped,
}

impl MappedBinding<'_> {
    /// Capture a body no larger than 16 KiB under the artifact's existing shared
    /// work budget. A partial decoded edge is recorded as unavailable, so a query
    /// can keep its already bound full-body citation instead of fabricating one.
    pub fn capture_body(
        &self,
        region_id: u32,
        body_span: LocalSpan,
        budget: &mut HostBudget,
    ) -> Result<MappedBodyProjection> {
        let region = *self
            .regions
            .get(&region_id)
            .context("missing body region")?;
        let span = ByteSpan {
            start: body_span.start,
            end: body_span.end,
        };
        span.validate(&region.text)?;
        ensure!(
            span.start < span.end && span.end - span.start <= MAX_MAPPED_BODY_BYTES,
            "mapped body exceeds byte bound"
        );
        budget.charge(1, 0, 0, 0)?;
        let (kind, segments) = match &region.location {
            RegionLocation::Embedded { region_id, .. } => (
                BodyKind::Embedded {
                    region_id: region_id.clone(),
                },
                self.embedded_segments(region, span, budget)?,
            ),
            RegionLocation::NotebookCell {
                cell_id, position, ..
            } => (
                BodyKind::Notebook {
                    cell_id: cell_id.clone(),
                    position: *position,
                },
                self.notebook_segments(region, span, budget)?,
            ),
        };
        let projection = MappedBodyProjection {
            version: 1,
            body_span: span,
            body_digest: body_digest(&region.text[span.start..span.end]),
            kind,
            segments,
        };
        projection.validate(span.end - span.start)?;
        ensure!(
            serde_json::to_vec(&projection)?.len() <= MAX_MAPPED_BODY_PROJECTION_BYTES,
            "mapped body projection exceeds encoded byte bound"
        );
        Ok(projection)
    }

    fn project_original(
        &self,
        region: &LocalRegion,
        span: ByteSpan,
        budget: &mut HostBudget,
    ) -> Result<Mapped> {
        let mut pieces = vec![(0, span)];
        let mut current = region;
        let mut verbatim = true;
        loop {
            let mut contiguous: Vec<ByteSpan> = Vec::with_capacity(pieces.len());
            for (_, piece) in pieces {
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
                let first = current
                    .source_map
                    .segments
                    .partition_point(|s| s.local.end <= piece.start);
                let end = current
                    .source_map
                    .segments
                    .partition_point(|s| s.local.start < piece.end);
                budget.charge(1 + end.saturating_sub(first), 0, 0, 0)?;
                for (offset, segment) in current.source_map.segments[first..end].iter().enumerate()
                {
                    let start = segment.local.start.max(piece.start);
                    let end = segment.local.end.min(piece.end);
                    let host = match segment.mapping {
                        Mapping::Verbatim { host } => ByteSpan {
                            start: host.start + start - segment.local.start,
                            end: host.start + end - segment.local.start,
                        },
                        Mapping::Decoded { host }
                            if start == segment.local.start && end == segment.local.end =>
                        {
                            verbatim = false;
                            host
                        }
                        Mapping::Decoded { .. } => return Ok(Mapped::Partial),
                        Mapping::Unmapped => return Ok(Mapped::Unmapped),
                    };
                    ensure!(
                        mapped.len() < MAX_MAP_SEGMENTS,
                        "body origin count exceeds bound"
                    );
                    mapped.push((first + offset, host));
                }
            }
            pieces = mapped;
            if let Some(parent) = current.parent {
                current = self.regions[&parent];
            } else {
                break;
            }
        }
        let pieces = pieces
            .into_iter()
            .map(|(segment, span)| Origin {
                segment,
                span,
                start_line: self
                    .host_lines
                    .partition_point(|offset| *offset <= span.start),
                end_line: self.host_lines.partition_point(|offset| *offset < span.end),
            })
            .collect();
        Ok(Mapped::Exact { pieces, verbatim })
    }

    /// Return the next boundary in this region or any verbatim ancestor. Text
    /// within the resulting run needs one projection regardless of byte count.
    /// A decoded ancestor ends the walk: its entire atom must be collected before
    /// project_original can return an exact origin.
    fn next_run_end(
        &self,
        region: &LocalRegion,
        start: usize,
        body_end: usize,
        budget: &mut HostBudget,
    ) -> Result<usize> {
        let mut current = region;
        let mut cursor = start;
        let mut length = body_end - start;
        loop {
            budget.charge(1, 0, 0, 0)?;
            let index = current
                .source_map
                .segments
                .partition_point(|segment| segment.local.end <= cursor);
            let segment = current
                .source_map
                .segments
                .get(index)
                .context("missing body source-map segment")?;
            length = length.min(segment.local.end - cursor);
            let Mapping::Verbatim { host } = segment.mapping else {
                break;
            };
            let Some(parent) = current.parent else {
                break;
            };
            cursor = host.start + cursor - segment.local.start;
            current = self.regions[&parent];
        }
        ensure!(length > 0, "empty body source-map run");
        Ok(start + length)
    }

    fn embedded_segments(
        &self,
        region: &LocalRegion,
        body: ByteSpan,
        budget: &mut HostBudget,
    ) -> Result<Vec<Segment>> {
        let text = &region.text[body.start..body.end];
        let mut segments: Vec<Segment> = Vec::new();
        let mut start = 0;
        let mut origins = 0usize;
        let mut end = 0;
        while end < text.len() {
            end = self.next_run_end(region, body.start + end, body.end, budget)? - body.start;
            let mapping = match self.project_original(
                region,
                ByteSpan {
                    start: body.start + start,
                    end: body.start + end,
                },
                budget,
            )? {
                Mapped::Partial => continue,
                Mapped::Unmapped => BodyMapping::Unmapped,
                Mapped::Exact {
                    mut pieces,
                    verbatim,
                } => {
                    origins = origins
                        .checked_add(pieces.len())
                        .context("body origin count overflow")?;
                    ensure!(
                        origins <= MAX_MAP_SEGMENTS,
                        "body origin count exceeds bound"
                    );
                    if verbatim && pieces.len() == 1 {
                        BodyMapping::Verbatim {
                            origin: pieces.remove(0),
                        }
                    } else {
                        BodyMapping::Atomic { origins: pieces }
                    }
                }
            };
            if let Some(previous) = segments.last_mut()
                && let (
                    BodyMapping::Verbatim { origin: prior },
                    BodyMapping::Verbatim { origin: next },
                ) = (&mut previous.mapping, &mapping)
                && prior.segment == next.segment
                && prior.span.end == next.span.start
            {
                previous.body.end = end;
                prior.span.end = next.span.end;
                prior.end_line = next.end_line;
            } else {
                segments.push(Segment {
                    body: ByteSpan { start, end },
                    mapping,
                    notebook_lines: None,
                });
            }
            start = end;
        }
        if start < text.len() {
            segments.push(Segment {
                body: ByteSpan {
                    start,
                    end: text.len(),
                },
                mapping: BodyMapping::Unmapped,
                notebook_lines: None,
            });
        }
        Ok(segments)
    }

    fn notebook_segments(
        &self,
        region: &LocalRegion,
        body: ByteSpan,
        budget: &mut HostBudget,
    ) -> Result<Vec<Segment>> {
        let mut line_start = region.text[..body.start]
            .rfind('\n')
            .map_or(0, |index| index + 1);
        let mut first_line = region.text[..line_start]
            .bytes()
            .filter(|byte| *byte == b'\n')
            .count()
            + 1;
        let mut current_line = first_line;
        let mut cursor = line_start;
        let mut body_start = 0;
        let mut segments = Vec::new();
        let mut origins = 0usize;
        loop {
            let end = region.text[cursor..]
                .find('\n')
                .map_or(region.text.len(), |index| cursor + index + 1);
            let mapping = match self.project_original(
                region,
                ByteSpan {
                    start: line_start,
                    end,
                },
                budget,
            )? {
                Mapped::Partial if end < region.text.len() => {
                    cursor = end;
                    current_line += 1;
                    continue;
                }
                Mapped::Partial | Mapped::Unmapped => BodyMapping::Unmapped,
                Mapped::Exact { pieces, .. } => {
                    origins = origins
                        .checked_add(pieces.len())
                        .context("body origin count overflow")?;
                    ensure!(
                        origins <= MAX_MAP_SEGMENTS,
                        "body origin count exceeds bound"
                    );
                    BodyMapping::Atomic { origins: pieces }
                }
            };
            let body_end = end.min(body.end) - body.start;
            ensure!(
                segments.len() < MAX_MAPPED_BODY_BYTES,
                "body line count exceeds bound"
            );
            segments.push(Segment {
                body: ByteSpan {
                    start: body_start,
                    end: body_end,
                },
                mapping,
                notebook_lines: Some(ByteSpan {
                    start: first_line,
                    end: current_line,
                }),
            });
            if end >= body.end {
                break;
            }
            line_start = end;
            cursor = end;
            current_line += 1;
            first_line = current_line;
            body_start = body_end;
        }
        Ok(segments)
    }
}

impl MappedBodyProjection {
    /// Validate bounded locator metadata after decoding the authenticated record.
    /// Original bytes and map provenance were checked by MappedBinding at capture.
    pub fn validate(&self, body_bytes: usize) -> Result<()> {
        ensure!(
            self.version == 1
                && body_bytes > 0
                && body_bytes <= MAX_MAPPED_BODY_BYTES
                && self.body_span.start < self.body_span.end
                && self.body_span.end <= MAX_SOURCE_COORDINATE
                && self.body_span.end - self.body_span.start == body_bytes,
            "invalid mapped body projection bounds"
        );
        ensure!(
            !self.segments.is_empty() && self.segments.len() <= MAX_MAPPED_BODY_BYTES,
            "invalid mapped body segment count"
        );
        let notebook = matches!(self.kind, BodyKind::Notebook { .. });
        match &self.kind {
            BodyKind::Embedded { region_id } => ensure!(
                !region_id.is_empty() && region_id.len() <= 4096 && !region_id.contains('\0'),
                "invalid body region ID"
            ),
            BodyKind::Notebook { cell_id, position } => ensure!(
                (1..=10_000).contains(position)
                    && cell_id
                        .as_ref()
                        .is_none_or(|id| !id.is_empty() && id.len() <= 4096 && !id.contains('\0')),
                "invalid body notebook identity"
            ),
        }
        let mut end = 0;
        let mut count = 0;
        let mut prior_line = 0;
        for segment in &self.segments {
            ensure!(
                segment.body.start == end
                    && segment.body.start < segment.body.end
                    && segment.body.end <= body_bytes,
                "mapped body partition gap or overlap"
            );
            end = segment.body.end;
            if notebook {
                let lines = segment
                    .notebook_lines
                    .context("body notebook lines missing")?;
                ensure!(
                    lines.start > prior_line
                        && lines.end >= lines.start
                        && lines.end <= MAX_SOURCE_COORDINATE + 1,
                    "invalid body notebook lines"
                );
                prior_line = lines.end;
            } else {
                ensure!(
                    segment.notebook_lines.is_none(),
                    "unexpected notebook body metadata"
                );
            }
            let origins = match &segment.mapping {
                BodyMapping::Verbatim { origin } => {
                    ensure!(
                        !notebook
                            && origin.span.end.saturating_sub(origin.span.start)
                                == segment.body.end - segment.body.start,
                        "invalid verbatim body length"
                    );
                    std::slice::from_ref(origin)
                }
                BodyMapping::Atomic { origins } => {
                    ensure!(!origins.is_empty(), "empty body origins");
                    origins.as_slice()
                }
                BodyMapping::Unmapped => &[],
            };
            count += origins.len();
            ensure!(count <= MAX_MAP_SEGMENTS, "body origin count exceeds bound");
            let mut previous_end = 0;
            for origin in origins {
                ensure!(
                    origin.segment < MAX_MAP_SEGMENTS
                        && origin.span.start >= previous_end
                        && origin.span.start < origin.span.end
                        && origin.span.end <= MAX_SOURCE_COORDINATE
                        && origin.start_line > 0
                        && origin.end_line >= origin.start_line
                        && origin.end_line <= MAX_SOURCE_COORDINATE + 1,
                    "invalid body origin"
                );
                previous_end = origin.span.end;
            }
        }
        ensure!(end == body_bytes, "incomplete mapped body projection");
        Ok(())
    }

    /// Focus a UTF-8 slice of the exact captured body. The returned citation may
    /// include a whole decoded atom or notebook line; it never claims an unknown
    /// interior encoded offset. None means an origin is explicitly unavailable.
    pub fn focus(
        &self,
        body_text: &str,
        relative_span: ByteSpan,
    ) -> Result<Option<(DocumentLocation, usize, usize)>> {
        self.validate(body_text.len())?;
        ensure!(
            self.body_digest == body_digest(body_text),
            "mapped body content changed"
        );
        relative_span.validate(body_text)?;
        ensure!(relative_span.start < relative_span.end, "empty body focus");
        let mut origins: Vec<Origin> = Vec::new();
        let mut expanded = relative_span;
        let mut local_lines: Option<ByteSpan> = None;
        for segment in &self.segments {
            segment.body.validate(body_text)?;
            if segment.body.end <= relative_span.start || segment.body.start >= relative_span.end {
                continue;
            }
            if let Some(lines) = segment.notebook_lines {
                local_lines = Some(ByteSpan {
                    start: local_lines.map_or(lines.start, |old| old.start),
                    end: lines.end,
                });
            }
            let pieces = match &segment.mapping {
                BodyMapping::Unmapped => return Ok(None),
                BodyMapping::Atomic { origins } => {
                    expanded.start = expanded.start.min(segment.body.start);
                    expanded.end = expanded.end.max(segment.body.end);
                    origins.clone()
                }
                BodyMapping::Verbatim { origin } => {
                    let start = relative_span.start.max(segment.body.start);
                    let end = relative_span.end.min(segment.body.end);
                    let start_line = origin.start_line
                        + body_text[segment.body.start..start]
                            .bytes()
                            .filter(|byte| *byte == b'\n')
                            .count();
                    let end_line = start_line
                        + body_text.as_bytes()[start..end - 1]
                            .iter()
                            .filter(|byte| **byte == b'\n')
                            .count();
                    vec![Origin {
                        segment: origin.segment,
                        span: ByteSpan {
                            start: origin.span.start + start - segment.body.start,
                            end: origin.span.start + end - segment.body.start,
                        },
                        start_line,
                        end_line,
                    }]
                }
            };
            for piece in pieces {
                if let Some(last) = origins.last_mut()
                    && last.segment == piece.segment
                    && last.span.end >= piece.span.start
                {
                    last.span.end = last.span.end.max(piece.span.end);
                    last.end_line = last.end_line.max(piece.end_line);
                } else {
                    ensure!(
                        origins.len() < MAX_MAP_SEGMENTS,
                        "focused body origin count exceeds bound"
                    );
                    origins.push(piece);
                }
            }
        }
        let start_line = origins
            .first()
            .context("body focus origin missing")?
            .start_line;
        let end_line = origins
            .last()
            .context("body focus origin missing")?
            .end_line;
        let original_spans = origins.into_iter().map(|origin| origin.span).collect();
        let region = match &self.kind {
            BodyKind::Embedded { region_id } => RegionLocation::Embedded {
                region_id: region_id.clone(),
                local_span: ByteSpan {
                    start: self.body_span.start + expanded.start,
                    end: self.body_span.start + expanded.end,
                },
                original_spans,
            },
            BodyKind::Notebook { cell_id, position } => {
                let lines = local_lines.context("focused notebook lines missing")?;
                RegionLocation::NotebookCell {
                    cell_id: cell_id.clone(),
                    position: *position,
                    local_start_line: lines.start,
                    local_end_line: lines.end,
                    original_spans,
                }
            }
        };
        region.validate_shape()?;
        Ok(Some((
            DocumentLocation::Mapped { region },
            start_line,
            end_line,
        )))
    }
}

fn body_digest(text: &str) -> crate::identity::Sha256Digest {
    crate::identity::framed_digest(b"mapped-body/1", &[text.as_bytes()])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extraction::{ExtractionPayload, ExtractionProfile};
    use crate::location::{MapSegment, SourceMap};
    use std::collections::BTreeMap;

    fn profile() -> ExtractionProfile {
        ExtractionProfile {
            contract_version: 2,
            adapter_id: "fixture".into(),
            adapter_version: "1".into(),
            language: "fixture".into(),
            dialect: None,
            grammar_identity: None,
            mapping_version: 1,
            chunk_version: 3,
            options: BTreeMap::new(),
            configuration_inputs: BTreeMap::new(),
        }
    }

    fn payload(text: String, regions: Vec<LocalRegion>) -> ExtractionPayload {
        ExtractionPayload {
            contract_version: 2,
            extraction_method: "utf8".into(),
            parse_status: "fixture".into(),
            semantic_projection: Default::default(),
            continuity_coverage: BTreeMap::new(),
            text,
            regions,
            provenance: vec![],
            facts: vec![],
            syntax_identities: vec![],
            chunks: vec![],
            relationships: vec![],
            continuity: vec![],
            diagnostics: vec![],
            partial: false,
        }
    }

    fn budget() -> HostBudget {
        HostBudget::new(500_000, 10_000, 16_777_216).unwrap()
    }

    fn embedded(
        id: u32,
        parent: Option<u32>,
        text: &str,
        map: SourceMap,
        host: &str,
    ) -> Result<LocalRegion> {
        let local_span = ByteSpan {
            start: 0,
            end: text.len(),
        };
        let original_spans = map.original_spans(local_span, text, host)?.unwrap();
        Ok(LocalRegion {
            id,
            parent,
            profile: profile(),
            text: text.into(),
            source_map: map,
            location: RegionLocation::Embedded {
                region_id: format!("region-{id}"),
                local_span,
                original_spans,
            },
        })
    }

    fn notebook() -> Result<ExtractionPayload> {
        let text = "one α\nneedle 😀\n";
        let mut host = String::from("[");
        let mut segments = Vec::new();
        let mut local = 0;
        for (index, line) in text.split_inclusive('\n').enumerate() {
            if index > 0 {
                host.push_str(",\n");
            }
            host.push('"');
            for character in line.chars() {
                let encoded = match character {
                    '\n' => "\\n".into(),
                    'α' => "\\u03b1".into(),
                    '😀' => "\\uD83D\\uDE00".into(),
                    other => other.to_string(),
                };
                let start = host.len();
                host.push_str(&encoded);
                let origin = ByteSpan {
                    start,
                    end: host.len(),
                };
                segments.push(MapSegment {
                    local: ByteSpan {
                        start: local,
                        end: local + character.len_utf8(),
                    },
                    mapping: if encoded == character.to_string() {
                        Mapping::Verbatim { host: origin }
                    } else {
                        Mapping::Decoded { host: origin }
                    },
                });
                local += character.len_utf8();
            }
            host.push('"');
        }
        host.push(']');
        let map = SourceMap {
            version: 1,
            segments,
        };
        let original_spans = map
            .original_spans(
                ByteSpan {
                    start: 0,
                    end: text.len(),
                },
                text,
                &host,
            )?
            .unwrap();
        Ok(payload(
            host,
            vec![LocalRegion {
                id: 1,
                parent: None,
                profile: profile(),
                text: text.into(),
                source_map: map,
                location: RegionLocation::NotebookCell {
                    cell_id: Some("cell-original".into()),
                    position: 2,
                    local_start_line: 1,
                    local_end_line: 2,
                    original_spans,
                },
            }],
        ))
    }

    #[test]
    fn escaped_notebook_focus_preserves_original_lines_and_json_gaps() -> Result<()> {
        let value = notebook()?;
        let mut allowance = budget();
        let mapper = MappedBinding::new(&value, &profile(), &mut allowance)?;
        let text = &value.regions[0].text;
        let start = 2;
        let end = text.len() - 1;
        let body = &text[start..end];
        let captured = mapper.capture_body(1, LocalSpan { start, end }, &mut allowance)?;
        let encoded = serde_json::to_vec(&captured)?;
        assert!(encoded.len() < 8192);
        let captured: MappedBodyProjection = serde_json::from_slice(&encoded)?;
        for selection in [
            ByteSpan {
                start: body.find("needle").unwrap(),
                end: body.len(),
            },
            ByteSpan {
                start: 0,
                end: body.len(),
            },
        ] {
            let expected = mapper.mapped_location(
                1,
                LocalSpan {
                    start: start + selection.start,
                    end: start + selection.end,
                },
                &mut allowance,
            )?;
            assert_eq!(captured.focus(body, selection)?.unwrap(), expected);
        }
        let target = body.find("needle").unwrap();
        let (_, first, last) = captured
            .focus(
                body,
                ByteSpan {
                    start: target,
                    end: target + 6,
                },
            )?
            .unwrap();
        assert_eq!((first, last), (2, 2));
        assert!(
            captured
                .focus(&"x".repeat(body.len()), ByteSpan { start: 0, end: 1 })
                .is_err()
        );
        Ok(())
    }

    #[test]
    fn nested_embedded_focus_preserves_discontiguous_decoded_origins() -> Result<()> {
        let host = "<pre>[ab|\\u03b1|cd]</pre>";
        let parent = "abαcd";
        let a = host.find("ab").unwrap();
        let alpha = host.find("\\u03b1").unwrap();
        let c = host.find("cd").unwrap();
        let map = SourceMap {
            version: 1,
            segments: vec![
                MapSegment {
                    local: ByteSpan { start: 0, end: 2 },
                    mapping: Mapping::Verbatim {
                        host: ByteSpan {
                            start: a,
                            end: a + 2,
                        },
                    },
                },
                MapSegment {
                    local: ByteSpan { start: 2, end: 4 },
                    mapping: Mapping::Decoded {
                        host: ByteSpan {
                            start: alpha,
                            end: alpha + 6,
                        },
                    },
                },
                MapSegment {
                    local: ByteSpan { start: 4, end: 6 },
                    mapping: Mapping::Verbatim {
                        host: ByteSpan {
                            start: c,
                            end: c + 2,
                        },
                    },
                },
            ],
        };
        let child = "bαc";
        let child_map = SourceMap {
            version: 1,
            segments: vec![MapSegment {
                local: ByteSpan {
                    start: 0,
                    end: child.len(),
                },
                mapping: Mapping::Verbatim {
                    host: ByteSpan { start: 1, end: 5 },
                },
            }],
        };
        let value = payload(
            host.into(),
            vec![
                embedded(1, None, parent, map, host)?,
                embedded(2, Some(1), child, child_map, parent)?,
            ],
        );
        let mut allowance = budget();
        let mapper = MappedBinding::new(&value, &profile(), &mut allowance)?;
        let captured = mapper.capture_body(
            2,
            LocalSpan {
                start: 0,
                end: child.len(),
            },
            &mut allowance,
        )?;
        let selection = ByteSpan {
            start: 1,
            end: child.len(),
        };
        let actual = captured.focus(child, selection)?.unwrap();
        assert_eq!(
            actual,
            mapper.mapped_location(
                2,
                LocalSpan {
                    start: 1,
                    end: child.len()
                },
                &mut allowance
            )?
        );
        let DocumentLocation::Mapped {
            region: RegionLocation::Embedded { original_spans, .. },
        } = actual.0
        else {
            panic!("embedded body")
        };
        assert_eq!(
            original_spans,
            vec![
                ByteSpan {
                    start: alpha,
                    end: alpha + 6
                },
                ByteSpan {
                    start: c,
                    end: c + 1
                }
            ]
        );
        assert!(
            captured
                .focus(child, ByteSpan { start: 2, end: 3 })
                .is_err()
        );
        Ok(())
    }

    #[test]
    fn decoded_atom_focus_expands_only_to_verified_atomic_origin() -> Result<()> {
        let host = "<encoded-pair>";
        let parent = "ab";
        let map = SourceMap {
            version: 1,
            segments: vec![MapSegment {
                local: ByteSpan { start: 0, end: 2 },
                mapping: Mapping::Decoded {
                    host: ByteSpan { start: 1, end: 13 },
                },
            }],
        };
        // Splitting the immediate child into two runs must not prevent joining
        // them to cover one parent's indivisible decoded unit.
        let child_map = SourceMap {
            version: 1,
            segments: (0..2)
                .map(|start| MapSegment {
                    local: ByteSpan {
                        start,
                        end: start + 1,
                    },
                    mapping: Mapping::Verbatim {
                        host: ByteSpan {
                            start,
                            end: start + 1,
                        },
                    },
                })
                .collect(),
        };
        let value = payload(
            host.into(),
            vec![
                embedded(1, None, parent, map, host)?,
                embedded(2, Some(1), parent, child_map, parent)?,
            ],
        );
        let mut allowance = budget();
        let mapper = MappedBinding::new(&value, &profile(), &mut allowance)?;
        let captured = mapper.capture_body(2, LocalSpan { start: 0, end: 2 }, &mut allowance)?;
        assert_eq!(
            captured
                .focus(parent, ByteSpan { start: 1, end: 2 })?
                .unwrap(),
            mapper.mapped_location(2, LocalSpan { start: 0, end: 2 }, &mut allowance)?
        );
        let clipped = mapper.capture_body(2, LocalSpan { start: 1, end: 2 }, &mut allowance)?;
        assert!(clipped.focus("b", ByteSpan { start: 0, end: 1 })?.is_none());
        Ok(())
    }

    #[test]
    fn generated_bytes_are_unavailable_without_hiding_later_real_origin() -> Result<()> {
        let host = "[ab]";
        let mut region = embedded(
            1,
            None,
            "ab",
            SourceMap {
                version: 1,
                segments: vec![MapSegment {
                    local: ByteSpan { start: 0, end: 2 },
                    mapping: Mapping::Verbatim {
                        host: ByteSpan { start: 1, end: 3 },
                    },
                }],
            },
            host,
        )?;
        region.text = "a?b".into();
        region.source_map.segments = vec![
            MapSegment {
                local: ByteSpan { start: 0, end: 1 },
                mapping: Mapping::Verbatim {
                    host: ByteSpan { start: 1, end: 2 },
                },
            },
            MapSegment {
                local: ByteSpan { start: 1, end: 2 },
                mapping: Mapping::Unmapped,
            },
            MapSegment {
                local: ByteSpan { start: 2, end: 3 },
                mapping: Mapping::Verbatim {
                    host: ByteSpan { start: 2, end: 3 },
                },
            },
        ];
        region.location = RegionLocation::Embedded {
            region_id: "region-1".into(),
            local_span: ByteSpan { start: 0, end: 1 },
            original_spans: vec![ByteSpan { start: 1, end: 2 }],
        };
        let value = payload(host.into(), vec![region]);
        let mut allowance = budget();
        let mapper = MappedBinding::new(&value, &profile(), &mut allowance)?;
        let captured = mapper.capture_body(1, LocalSpan { start: 0, end: 3 }, &mut allowance)?;
        assert!(
            captured
                .focus("a?b", ByteSpan { start: 0, end: 3 })?
                .is_none()
        );
        assert!(
            captured
                .focus("a?b", ByteSpan { start: 2, end: 3 })?
                .is_some()
        );
        Ok(())
    }

    #[test]
    fn full_size_nested_verbatim_body_uses_map_work_not_character_work() -> Result<()> {
        let text = "αx\n".repeat(MAX_MAPPED_BODY_BYTES / 4);
        assert_eq!(text.len(), MAX_MAPPED_BODY_BYTES);
        let host = format!("prefix\n[{text}]");
        let mut regions = Vec::new();
        for id in 1..=4 {
            let original_start = if id == 1 { "prefix\n[".len() } else { 0 };
            regions.push(embedded(
                id,
                (id > 1).then_some(id - 1),
                &text,
                SourceMap {
                    version: 1,
                    segments: vec![MapSegment {
                        local: ByteSpan {
                            start: 0,
                            end: text.len(),
                        },
                        mapping: Mapping::Verbatim {
                            host: ByteSpan {
                                start: original_start,
                                end: original_start + text.len(),
                            },
                        },
                    }],
                },
                if id == 1 { &host } else { &text },
            )?);
        }
        let value = payload(host, regions);
        // Construction, capture through four levels, and comparison binding all
        // share this tiny node budget despite 16 KiB and 12,288 Unicode scalars.
        let mut allowance = HostBudget::new(32, 0, 16_777_216)?;
        let mapper = MappedBinding::new(&value, &profile(), &mut allowance)?;
        let captured = mapper.capture_body(
            4,
            LocalSpan {
                start: 0,
                end: text.len(),
            },
            &mut allowance,
        )?;
        assert_eq!(captured.segments.len(), 1);
        assert!(serde_json::to_vec(&captured)?.len() < 1024);
        let selected = LocalSpan {
            start: text.len() - 4,
            end: text.len() - 1,
        };
        assert_eq!(
            captured
                .focus(
                    &text,
                    ByteSpan {
                        start: selected.start,
                        end: selected.end,
                    },
                )?
                .unwrap(),
            mapper.mapped_location(4, selected, &mut allowance)?
        );
        Ok(())
    }

    #[test]
    fn capture_and_decode_enforce_body_work_and_metadata_bounds() -> Result<()> {
        let text = "a".repeat(MAX_MAPPED_BODY_BYTES + 1);
        let region = embedded(
            1,
            None,
            &text,
            SourceMap {
                version: 1,
                segments: vec![MapSegment {
                    local: ByteSpan {
                        start: 0,
                        end: text.len(),
                    },
                    mapping: Mapping::Verbatim {
                        host: ByteSpan {
                            start: 0,
                            end: text.len(),
                        },
                    },
                }],
            },
            &text,
        )?;
        let value = payload(text.clone(), vec![region]);
        let mut allowance = budget();
        let mapper = MappedBinding::new(&value, &profile(), &mut allowance)?;
        assert!(
            mapper
                .capture_body(
                    1,
                    LocalSpan {
                        start: 0,
                        end: text.len()
                    },
                    &mut allowance
                )
                .is_err()
        );
        let mut exhausted = HostBudget::new(0, 0, 0)?;
        assert!(
            mapper
                .capture_body(1, LocalSpan { start: 0, end: 1 }, &mut exhausted)
                .is_err()
        );
        let captured = mapper.capture_body(
            1,
            LocalSpan {
                start: 0,
                end: MAX_MAPPED_BODY_BYTES,
            },
            &mut allowance,
        )?;
        assert!(
            serde_json::to_vec(&captured)?.len() < 1024,
            "a long verbatim run remains compact"
        );
        let mut forged = captured.clone();
        forged.version = 2;
        assert!(forged.validate(MAX_MAPPED_BODY_BYTES).is_err());
        forged = captured;
        forged.segments[0].body.start = 1;
        assert!(forged.validate(MAX_MAPPED_BODY_BYTES).is_err());
        Ok(())
    }
}
