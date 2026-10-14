// SPDX-License-Identifier: Apache-2.0
//! Versioned byte maps for decoded/embedded evidence; no parser or filesystem state.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

pub const SOURCE_MAP_VERSION: u32 = 1;
pub const MAX_REGION_DEPTH: usize = 4;
pub const MAX_REGIONS: usize = 256;
pub const MAX_MAP_SEGMENTS: usize = 100_000;

/// Half-open UTF-8 byte span. Empty spans are valid boundaries.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ByteSpan {
    pub start: usize,
    pub end: usize,
}

impl ByteSpan {
    pub fn validate(&self, text: &str) -> Result<()> {
        ensure!(
            self.start <= self.end && self.end <= text.len(),
            "span out of bounds"
        );
        ensure!(
            text.is_char_boundary(self.start) && text.is_char_boundary(self.end),
            "span splits UTF-8"
        );
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Mapping {
    /// Byte-identical contiguous source, such as a direct script block.
    Verbatim { host: ByteSpan },
    /// An entire decoded unit maps to this original encoded span. Interior
    /// positions have no exact mapping (for example the inside of a JSON escape).
    Decoded { host: ByteSpan },
    /// Generated separators or otherwise unsupported origin.
    Unmapped,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MapSegment {
    pub local: ByteSpan,
    pub mapping: Mapping,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceMap {
    pub version: u32,
    pub segments: Vec<MapSegment>,
}

/// Additional location metadata for a host artifact. This does not alter the
/// legacy DocumentLocation wire format before the storage migration gate.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum RegionLocation {
    Embedded {
        region_id: String,
        local_span: ByteSpan,
        original_spans: Vec<ByteSpan>,
    },
    NotebookCell {
        cell_id: Option<String>,
        /// Current one-based position, never a durable identity.
        position: usize,
        local_start_line: usize,
        local_end_line: usize,
        original_spans: Vec<ByteSpan>,
    },
}

impl RegionLocation {
    /// Validate coordinates against the supplied immediate host mapping.
    /// Nested adapters must compose each parent map before emitting a public
    /// original-source citation; this method does not skip that binding step.
    pub fn validate(&self, local: &str, host: &str, map: &SourceMap) -> Result<()> {
        let (span, spans) = match self {
            Self::Embedded {
                region_id,
                local_span,
                original_spans,
            } => {
                ensure!(
                    !region_id.is_empty() && region_id.len() <= 4096 && !region_id.contains('\0'),
                    "invalid region locator"
                );
                (*local_span, original_spans)
            }
            Self::NotebookCell {
                cell_id,
                position,
                local_start_line,
                local_end_line,
                original_spans,
            } => {
                ensure!(
                    *position > 0 && *position <= 10_000,
                    "invalid notebook cell position"
                );
                ensure!(
                    cell_id
                        .as_ref()
                        .is_none_or(|id| !id.is_empty() && id.len() <= 4096 && !id.contains('\0')),
                    "invalid notebook cell ID"
                );
                ensure!(
                    *local_start_line > 0 && local_start_line <= local_end_line,
                    "invalid notebook line range"
                );
                let mut line = 1;
                let mut start = (*local_start_line == 1).then_some(0);
                let mut end = None;
                for (offset, byte) in local.bytes().enumerate() {
                    if byte == b'\n' && offset + 1 < local.len() {
                        line += 1;
                        if line == *local_start_line {
                            start = Some(offset + 1);
                        }
                        if line - 1 == *local_end_line {
                            end = Some(offset + 1);
                        }
                    }
                }
                ensure!(
                    *local_end_line <= line,
                    "notebook line range exceeds decoded text"
                );
                let start = start
                    .ok_or_else(|| anyhow::anyhow!("notebook start line exceeds decoded text"))?;
                let end = end.unwrap_or(local.len());
                (ByteSpan { start, end }, original_spans)
            }
        };
        ensure!(
            spans.len() <= MAX_MAP_SEGMENTS,
            "region citation segment limit"
        );
        let mapped = map.original_spans(span, local, host)?.unwrap_or_default();
        ensure!(
            *spans == mapped,
            "region citation disagrees with source map"
        );
        Ok(())
    }
}

impl SourceMap {
    /// Maps must cover the decoded text, including explicitly unmappable bytes.
    /// Host ranges are monotonic and nonoverlapping; gaps preserve punctuation
    /// between JSON array elements instead of pretending it is decoded text.
    pub fn validate(&self, local: &str, host: &str) -> Result<()> {
        ensure!(
            self.version == SOURCE_MAP_VERSION,
            "unsupported source map version"
        );
        ensure!(
            self.segments.len() <= MAX_MAP_SEGMENTS,
            "source map segment limit"
        );
        let mut local_end = 0;
        let mut host_end = 0;
        for segment in &self.segments {
            segment.local.validate(local)?;
            ensure!(
                segment.local.start == local_end && segment.local.end > local_end,
                "source map gap or overlap"
            );
            match segment.mapping {
                Mapping::Verbatim { host: span } | Mapping::Decoded { host: span } => {
                    span.validate(host)?;
                    ensure!(
                        span.start >= host_end && span.end > span.start,
                        "host map overlap or empty span"
                    );
                    if matches!(segment.mapping, Mapping::Verbatim { .. }) {
                        ensure!(
                            local[segment.local.start..segment.local.end]
                                == host[span.start..span.end],
                            "verbatim source map bytes differ"
                        );
                    }
                    host_end = span.end;
                }
                Mapping::Unmapped => {}
            }
            local_end = segment.local.end;
        }
        ensure!(local_end == local.len(), "source map incomplete");
        Ok(())
    }

    /// Returns exact original pieces, not a fabricated contiguous range.
    /// Empty selectors and partial decoded units conservatively have no exact map.
    pub fn original_spans(
        &self,
        span: ByteSpan,
        local: &str,
        host: &str,
    ) -> Result<Option<Vec<ByteSpan>>> {
        self.validate(local, host)?;
        span.validate(local)?;
        if span.start == span.end {
            return Ok(None);
        }
        let mut result = Vec::new();
        for segment in &self.segments {
            let start = span.start.max(segment.local.start);
            let end = span.end.min(segment.local.end);
            if start >= end {
                continue;
            }
            let mapped = match segment.mapping {
                Mapping::Verbatim { host } => ByteSpan {
                    start: host.start + start - segment.local.start,
                    end: host.start + end - segment.local.start,
                },
                Mapping::Decoded { host }
                    if start == segment.local.start && end == segment.local.end =>
                {
                    host
                }
                Mapping::Decoded { .. } | Mapping::Unmapped => return Ok(None),
            };
            mapped.validate(host)?;
            result.push(mapped);
        }
        Ok(Some(result))
    }
}

/// One host owns the allowance. Failed child attempts are charged as work;
/// callers must not create a fresh allowance for each region or nested adapter.
#[derive(Debug)]
pub struct HostBudget {
    remaining_regions: usize,
    remaining_segments: usize,
    remaining_nodes: usize,
    remaining_facts: usize,
    remaining_text_bytes: usize,
}

impl HostBudget {
    pub fn new(nodes: usize, facts: usize, text_bytes: usize) -> Result<Self> {
        ensure!(
            nodes <= 500_000 && facts <= 10_000 && text_bytes <= 16 * 1024 * 1024,
            "host budget exceeds validated ceiling"
        );
        Ok(Self {
            remaining_regions: MAX_REGIONS,
            remaining_segments: MAX_MAP_SEGMENTS,
            remaining_nodes: nodes,
            remaining_facts: facts,
            remaining_text_bytes: text_bytes,
        })
    }

    pub fn enter_region(&mut self, depth: usize) -> Result<()> {
        // Count attempts even when nesting is unsupported.
        self.remaining_regions = self
            .remaining_regions
            .checked_sub(1)
            .ok_or_else(|| anyhow::anyhow!("host region budget exhausted"))?;
        ensure!(
            (1..=MAX_REGION_DEPTH).contains(&depth),
            "region nesting limit"
        );
        Ok(())
    }

    pub fn charge(
        &mut self,
        nodes: usize,
        facts: usize,
        text_bytes: usize,
        segments: usize,
    ) -> Result<()> {
        // Saturation consumes the remaining budget on failed work as well.
        let fits = nodes <= self.remaining_nodes
            && facts <= self.remaining_facts
            && text_bytes <= self.remaining_text_bytes
            && segments <= self.remaining_segments;
        self.remaining_nodes = self.remaining_nodes.saturating_sub(nodes);
        self.remaining_facts = self.remaining_facts.saturating_sub(facts);
        self.remaining_text_bytes = self.remaining_text_bytes.saturating_sub(text_bytes);
        self.remaining_segments = self.remaining_segments.saturating_sub(segments);
        ensure!(fits, "shared host extraction budget exhausted");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escaped_json_and_discontiguous_arrays_preserve_original_pieces() {
        let local = "a\nb";
        let host = "[\"a\\n\",\"b\"]";
        let map = SourceMap {
            version: 1,
            segments: vec![
                MapSegment {
                    local: ByteSpan { start: 0, end: 1 },
                    mapping: Mapping::Verbatim {
                        host: ByteSpan { start: 2, end: 3 },
                    },
                },
                MapSegment {
                    local: ByteSpan { start: 1, end: 2 },
                    mapping: Mapping::Decoded {
                        host: ByteSpan { start: 3, end: 5 },
                    },
                },
                MapSegment {
                    local: ByteSpan { start: 2, end: 3 },
                    mapping: Mapping::Verbatim {
                        host: ByteSpan { start: 8, end: 9 },
                    },
                },
            ],
        };
        assert_eq!(
            map.original_spans(ByteSpan { start: 0, end: 3 }, local, host)
                .unwrap()
                .unwrap(),
            vec![
                ByteSpan { start: 2, end: 3 },
                ByteSpan { start: 3, end: 5 },
                ByteSpan { start: 8, end: 9 }
            ]
        );
    }

    #[test]
    fn invalid_utf8_boundaries_overlaps_and_fabricated_verbatim_fail() {
        let mut map = SourceMap {
            version: 1,
            segments: vec![MapSegment {
                local: ByteSpan { start: 0, end: 2 },
                mapping: Mapping::Verbatim {
                    host: ByteSpan { start: 0, end: 2 },
                },
            }],
        };
        assert!(map.validate("é", "é").is_ok());
        assert!(
            map.original_spans(ByteSpan { start: 0, end: 1 }, "é", "é")
                .is_err()
        );
        assert!(map.validate("é", "ab").is_err());
        map.segments.push(map.segments[0].clone());
        assert!(map.validate("é", "é").is_err());
    }

    #[test]
    fn synthetic_or_partial_decoding_never_claims_exact_coordinates() {
        let mut map = SourceMap {
            version: 1,
            segments: vec![MapSegment {
                local: ByteSpan { start: 0, end: 2 },
                mapping: Mapping::Decoded {
                    host: ByteSpan { start: 0, end: 6 },
                },
            }],
        };
        assert_eq!(
            map.original_spans(ByteSpan { start: 0, end: 1 }, "ab", "&pair;")
                .unwrap(),
            None
        );
        map.segments[0].mapping = Mapping::Unmapped;
        assert_eq!(
            map.original_spans(ByteSpan { start: 0, end: 2 }, "ab", "")
                .unwrap(),
            None
        );
    }

    #[test]
    fn region_metadata_cannot_override_verified_map_or_cell_lines() {
        let local = "é\r\nx";
        let host = format!("[{local}]");
        let map = SourceMap {
            version: 1,
            segments: vec![MapSegment {
                local: ByteSpan {
                    start: 0,
                    end: local.len(),
                },
                mapping: Mapping::Verbatim {
                    host: ByteSpan {
                        start: 1,
                        end: 1 + local.len(),
                    },
                },
            }],
        };
        let mut cell = RegionLocation::NotebookCell {
            cell_id: Some("written-id".into()),
            position: 1,
            local_start_line: 2,
            local_end_line: 2,
            original_spans: vec![ByteSpan { start: 5, end: 6 }],
        };
        cell.validate(local, &host, &map).unwrap();
        if let RegionLocation::NotebookCell { position, .. } = &mut cell {
            *position = 0;
        }
        assert!(cell.validate(local, &host, &map).is_err());
        if let RegionLocation::NotebookCell {
            position,
            local_end_line,
            ..
        } = &mut cell
        {
            *position = 1;
            *local_end_line = 3;
        }
        assert!(cell.validate(local, &host, &map).is_err());
        let forged = RegionLocation::Embedded {
            region_id: "script".into(),
            local_span: ByteSpan { start: 0, end: 2 },
            original_spans: vec![ByteSpan { start: 0, end: 2 }],
        };
        assert!(forged.validate(local, &host, &map).is_err());
    }

    #[test]
    fn child_failures_consume_shared_host_allowances() {
        let mut budget = HostBudget::new(3, 2, 8).unwrap();
        budget.enter_region(1).unwrap();
        budget.charge(2, 1, 4, 1).unwrap();
        assert!(budget.charge(2, 0, 0, 0).is_err());
        assert!(budget.charge(1, 0, 0, 0).is_err());
        for _ in 1..MAX_REGIONS {
            assert!(budget.enter_region(5).is_err());
        }
        assert!(budget.enter_region(1).is_err());
    }
}
