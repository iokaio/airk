// SPDX-License-Identifier: Apache-2.0
//! Bounded semantic inputs with exact, independent citation membership.
use crate::model::{EmbeddingModel, PlanningTokens};
use airk_core::{
    DocumentLocation,
    extraction::{ExtractionPayload, LocalChunk, LocalSpan, SemanticBoundary},
    location::{ByteSpan, RegionLocation},
};
use anyhow::{Context, Result, ensure};
use std::collections::BTreeMap;

pub const VECTOR_UNIT_SPECIFICATION: &str =
    "semantic-units/target256-context32-quarter-cap/no-overlap/members64/input16k/1";
pub const MAX_VECTOR_UNIT_INPUT_BYTES: usize = crate::model::MAX_INPUT_BYTES;
pub const MAX_VECTOR_UNIT_MEMBERS: usize = 64;
pub const MAX_VECTOR_UNITS_PER_ARTIFACT: usize = airk_core::extraction::MAX_EXTRACTION_CHUNKS;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VectorMemberSpan {
    pub chunk_index: u32,
    /// Absolute byte positions within the chunk's host or mapped region.
    pub span: LocalSpan,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedVectorUnit {
    /// Exact embedding-cache input, including any repeated written context.
    pub input: String,
    /// Body evidence only; repeated context does not create duplicate citations.
    pub members: Vec<VectorMemberSpan>,
}

#[derive(Clone, Debug, Default)]
pub struct VectorUnitStats {
    pub units: u64,
    pub body_bytes: u64,
    pub planning_us: u64,
}

#[derive(Clone, Copy, Debug)]
pub struct VectorUnitOptions {
    pub target_tokens: usize,
    pub context_tokens: usize,
}

impl Default for VectorUnitOptions {
    fn default() -> Self {
        Self {
            target_tokens: 256,
            context_tokens: 32,
        }
    }
}

trait PlannerModel {
    fn context_tokens(&self) -> Option<usize>;
    fn tokens(&self, text: &str) -> Result<PlanningTokens>;
}

impl PlannerModel for EmbeddingModel {
    fn context_tokens(&self) -> Option<usize> {
        self.planning_context_tokens()
    }
    fn tokens(&self, text: &str) -> Result<PlanningTokens> {
        self.planning_tokens(text)
    }
}

type ChunkRef<'a> = (u32, &'a LocalChunk);

struct Pending<'a> {
    group: u32,
    region: Option<u32>,
    location: &'a DocumentLocation,
    context: String,
    body: String,
    span: LocalSpan,
    members: Vec<VectorMemberSpan>,
}

/// Lines are exact citation partitions, not semantic boundaries. Native document
/// coordinates and mapped cell/region identities remain hard boundaries.
fn compatible(left: &DocumentLocation, right: &DocumentLocation) -> bool {
    match (left, right) {
        (DocumentLocation::Lines { .. }, DocumentLocation::Lines { .. }) => true,
        (DocumentLocation::Mapped { region: a }, DocumentLocation::Mapped { region: b }) => {
            match (a, b) {
                (
                    RegionLocation::Embedded { region_id: a, .. },
                    RegionLocation::Embedded { region_id: b, .. },
                ) => a == b,
                (
                    RegionLocation::NotebookCell {
                        cell_id: a,
                        position: ap,
                        ..
                    },
                    RegionLocation::NotebookCell {
                        cell_id: b,
                        position: bp,
                        ..
                    },
                ) => a == b && ap == bp,
                _ => false,
            }
        }
        _ => left == right,
    }
}

fn local_text(payload: &ExtractionPayload, region: Option<u32>) -> Result<&str> {
    match region {
        None => Ok(&payload.text),
        Some(id) => payload
            .regions
            .iter()
            .find(|r| r.id == id)
            .map(|r| r.text.as_str())
            .context("vector boundary region is missing"),
    }
}

fn validate_span(span: &LocalSpan, text: &str) -> Result<()> {
    ByteSpan {
        start: span.start,
        end: span.end,
    }
    .validate(text)
}

/// The two partitions must cover exactly the same bytes. This deliberately does
/// not reconstruct semantic boundaries from selected citation chunks.
fn validate_coverage(chunks: &[ChunkRef<'_>], boundaries: &[&SemanticBoundary]) -> Result<()> {
    let mut chunk = 0;
    let mut boundary = 0;
    let mut chunk_cursor = chunks.first().map(|c| c.1.span.start);
    let mut boundary_cursor = boundaries.first().map(|b| b.span.start);
    while let (Some(c), Some(b)) = (chunk_cursor, boundary_cursor) {
        ensure!(
            c == b,
            "semantic projection does not cover citation text exactly; rebuild the index with airk index --full"
        );
        let end = chunks[chunk].1.span.end.min(boundaries[boundary].span.end);
        chunk_cursor = if end == chunks[chunk].1.span.end {
            chunk += 1;
            chunks.get(chunk).map(|c| c.1.span.start)
        } else {
            Some(end)
        };
        boundary_cursor = if end == boundaries[boundary].span.end {
            boundary += 1;
            boundaries.get(boundary).map(|b| b.span.start)
        } else {
            Some(end)
        };
    }
    ensure!(
        chunk_cursor.is_none() && boundary_cursor.is_none(),
        "semantic projection is incomplete; rebuild the index with airk index --full"
    );
    Ok(())
}

fn prefix_end(text: &str, maximum: usize) -> usize {
    let mut end = maximum.min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    if end < text.len()
        && text.as_bytes().get(end) == Some(&b'\n')
        && text.as_bytes().get(end.wrapping_sub(1)) == Some(&b'\r')
    {
        end -= 1;
    }
    end
}

fn checked_tokens(model: &impl PlannerModel, text: &str) -> Result<PlanningTokens> {
    ensure!(
        text.len() <= MAX_VECTOR_UNIT_INPUT_BYTES,
        "vector planning slice exceeds byte limit"
    );
    let tokens = model.tokens(text)?;
    ensure!(
        tokens.offsets.len() <= 65_536,
        "vector planning token budget exceeded"
    );
    let mut previous = 0;
    for &(start, end) in &tokens.offsets {
        ensure!(
            previous <= start
                && start <= end
                && end <= text.len()
                && text.is_char_boundary(start)
                && text.is_char_boundary(end),
            "invalid planning token offsets"
        );
        previous = start;
    }
    Ok(tokens)
}

fn compose(context: &str, body: &str) -> String {
    if context.is_empty() {
        body.to_owned()
    } else {
        format!("{context}\n{body}")
    }
}

/// Every tokenization call is bounded; candidate reduction is explicit and the
/// final composed input is checked again before emission. No model truncation
/// is used to establish source coverage.
fn fit_body(
    text: &str,
    mut context: String,
    model: &impl PlannerModel,
    target: usize,
) -> Result<(usize, String)> {
    let mut end = prefix_end(
        text,
        MAX_VECTOR_UNIT_INPUT_BYTES
            .saturating_sub(context.len() + usize::from(!context.is_empty())),
    );
    for _ in 0..64 {
        if end == 0 {
            ensure!(
                !context.is_empty(),
                "one source character exceeds vector input budget"
            );
            context.clear();
            end = prefix_end(text, MAX_VECTOR_UNIT_INPUT_BYTES);
        }
        let input = compose(&context, &text[..end]);
        let tokens = checked_tokens(model, &input)?;
        if tokens.len() <= target {
            return Ok((end, context));
        }
        let allowance = target.saturating_sub(tokens.special_tokens);
        let proposed = tokens
            .offsets
            .get(allowance)
            .map(|offset| offset.0)
            .unwrap_or(0);
        let context_bytes = context.len() + usize::from(!context.is_empty());
        let reduced = prefix_end(
            text,
            proposed
                .saturating_sub(context_bytes)
                .min(end.saturating_sub(1)),
        );
        if reduced > 0 {
            end = reduced;
        } else if !context.is_empty() {
            context.clear();
            end = prefix_end(text, MAX_VECTOR_UNIT_INPUT_BYTES);
        } else {
            let first = text.chars().next().context("empty vector body")?.len_utf8();
            let half = prefix_end(text, end / 2);
            ensure!(
                end > first,
                "one source character cannot fit the model context without truncation"
            );
            end = half.max(first);
            if text[..end].ends_with('\r') && text[end..].starts_with('\n') {
                end += 1;
            }
        }
    }
    anyhow::bail!("vector planning reduction budget exceeded")
}

fn context_for(
    boundary: &SemanticBoundary,
    body: &LocalSpan,
    text: &str,
    chunks: &[ChunkRef<'_>],
    location: &DocumentLocation,
    model: &impl PlannerModel,
    context_budget: usize,
) -> Result<String> {
    let mut context = String::new();
    for span in boundary
        .split_context
        .iter()
        .filter(|_| body.start > boundary.span.start)
        .chain(boundary.context.iter())
    {
        if span.start >= body.start && span.end <= body.end {
            continue;
        }
        validate_span(span, text)?;
        let remaining = (MAX_VECTOR_UNIT_INPUT_BYTES / 4)
            .saturating_sub(context.len() + usize::from(!context.is_empty()));
        let end = prefix_end(&text[span.start..span.end], remaining);
        if end == 0 {
            continue;
        }
        let first = chunks.partition_point(|(_, c)| c.span.end <= span.start);
        let last = chunks.partition_point(|(_, c)| c.span.start < span.start + end);
        for (_, chunk) in &chunks[first..last] {
            ensure!(
                compatible(location, &chunk.location),
                "vector context crosses a native document boundary"
            );
        }
        if !context.is_empty() {
            context.push('\n');
        }
        context.push_str(&text[span.start..span.start + end]);
    }
    if context.is_empty() || context_budget == 0 {
        return Ok(String::new());
    }
    let specials = checked_tokens(model, "")?.special_tokens;
    let (end, _) = fit_body(&context, String::new(), model, context_budget + specials)?;
    context.truncate(end);
    Ok(context)
}

fn member_spans(chunks: &[ChunkRef<'_>], span: &LocalSpan) -> Vec<VectorMemberSpan> {
    chunks
        .iter()
        .filter(|(_, c)| c.span.start < span.end && c.span.end > span.start)
        .map(|(index, c)| VectorMemberSpan {
            chunk_index: *index,
            span: LocalSpan {
                start: span.start.max(c.span.start),
                end: span.end.min(c.span.end),
            },
        })
        .collect()
}

fn joined_members(left: &[VectorMemberSpan], right: &[VectorMemberSpan]) -> Vec<VectorMemberSpan> {
    let mut result = left.to_vec();
    for member in right {
        if let Some(last) = result.last_mut()
            && last.chunk_index == member.chunk_index
            && last.span.end == member.span.start
        {
            last.span.end = member.span.end;
        } else {
            result.push(member.clone());
        }
    }
    result
}

fn emit(
    pending: Pending<'_>,
    stats: &mut VectorUnitStats,
    visitor_time: &mut std::time::Duration,
    visitor: &mut impl FnMut(PreparedVectorUnit) -> Result<()>,
) -> Result<()> {
    stats.units += 1;
    stats.body_bytes += pending.body.len() as u64;
    ensure!(
        stats.units <= MAX_VECTOR_UNITS_PER_ARTIFACT as u64,
        "vector unit count exceeds artifact budget"
    );
    let unit = PreparedVectorUnit {
        input: compose(&pending.context, &pending.body),
        members: pending.members,
    };
    let started = std::time::Instant::now();
    let result = visitor(unit);
    *visitor_time += started.elapsed();
    result
}

pub fn visit_vector_units(
    payload: &ExtractionPayload,
    model: &EmbeddingModel,
    visitor: impl FnMut(PreparedVectorUnit) -> Result<()>,
) -> Result<VectorUnitStats> {
    visit_vector_units_with_options(payload, model, VectorUnitOptions::default(), visitor)
}

/// Explicit candidates for local qualification. Published indexes bind their
/// selected policy through the retrieval specification, not user-supplied text.
pub fn visit_vector_units_with_options(
    payload: &ExtractionPayload,
    model: &EmbeddingModel,
    options: VectorUnitOptions,
    visitor: impl FnMut(PreparedVectorUnit) -> Result<()>,
) -> Result<VectorUnitStats> {
    plan(payload, model, options, visitor)
}

fn plan(
    payload: &ExtractionPayload,
    model: &impl PlannerModel,
    options: VectorUnitOptions,
    mut visitor: impl FnMut(PreparedVectorUnit) -> Result<()>,
) -> Result<VectorUnitStats> {
    let started = std::time::Instant::now();
    ensure!(
        payload.semantic_projection.version == 1,
        "unsupported semantic projection; rebuild the index with airk index --full"
    );
    ensure!(
        (8..=512).contains(&options.target_tokens) && options.context_tokens <= 128,
        "invalid vector unit planning options"
    );
    ensure!(
        payload.chunks.len() <= airk_core::extraction::MAX_EXTRACTION_CHUNKS
            && payload.semantic_projection.boundaries.len()
                <= airk_core::extraction::MAX_SEMANTIC_BOUNDARIES,
        "vector planning metadata budget exceeded"
    );
    ensure!(
        payload.regions.len() <= airk_core::location::MAX_REGIONS,
        "vector planning region budget exceeded"
    );
    let decoded_bytes = payload
        .regions
        .iter()
        .try_fold(payload.text.len(), |bytes, region| {
            bytes
                .checked_add(region.text.len())
                .context("vector planning text size overflow")
        })?;
    ensure!(
        decoded_bytes <= airk_core::extraction::MAX_TEXT_BYTES,
        "vector planning text budget exceeded"
    );
    let target = options
        .target_tokens
        .min(model.context_tokens().unwrap_or(options.target_tokens));
    let specials = checked_tokens(model, "")?.special_tokens;
    ensure!(
        specials < target,
        "model context cannot fit a semantic body token"
    );
    let context_budget = options.context_tokens.min((target - specials) / 4);
    let mut regions = BTreeMap::<Option<u32>, Vec<ChunkRef<'_>>>::new();
    for (index, chunk) in payload.chunks.iter().enumerate() {
        validate_span(&chunk.span, local_text(payload, chunk.region)?)?;
        if chunk.span.start < chunk.span.end {
            regions
                .entry(chunk.region)
                .or_default()
                .push((u32::try_from(index)?, chunk));
        }
    }
    let mut boundaries = BTreeMap::<Option<u32>, Vec<&SemanticBoundary>>::new();
    for boundary in &payload.semantic_projection.boundaries {
        let text = local_text(payload, boundary.region)?;
        validate_span(&boundary.span, text)?;
        ensure!(
            boundary.span.start < boundary.span.end,
            "empty semantic boundary"
        );
        for context in boundary.context.iter().chain(&boundary.split_context) {
            validate_span(context, text)?;
        }
        boundaries
            .entry(boundary.region)
            .or_default()
            .push(boundary);
    }
    ensure!(
        regions.keys().eq(boundaries.keys()),
        "semantic projection regions do not match citation text; rebuild the index with airk index --full"
    );
    let mut stats = VectorUnitStats::default();
    let mut visitor_time = std::time::Duration::ZERO;
    for (region, chunks) in &mut regions {
        chunks.sort_by_key(|(_, c)| (c.span.start, c.span.end));
        ensure!(
            chunks
                .windows(2)
                .all(|p| p[0].1.span.end <= p[1].1.span.start),
            "overlapping citation spans cannot define vector membership"
        );
        let boundaries = boundaries
            .get_mut(region)
            .context("semantic region missing")?;
        boundaries.sort_by_key(|b| (b.span.start, b.span.end));
        ensure!(
            boundaries
                .windows(2)
                .all(|p| p[0].span.end <= p[1].span.start),
            "overlapping semantic boundaries"
        );
        validate_coverage(chunks, boundaries)?;
        let text = local_text(payload, *region)?;
        let mut pending: Option<Pending<'_>> = None;
        let mut chunk_cursor = 0;
        for boundary in boundaries {
            let mut cursor = boundary.span.start;
            while cursor < boundary.span.end {
                while chunks[chunk_cursor].1.span.end <= cursor {
                    chunk_cursor += 1;
                }
                let location = &chunks[chunk_cursor].1.location;
                let mut slab_end = chunks[chunk_cursor].1.span.end.min(boundary.span.end);
                let mut next = chunk_cursor + 1;
                while slab_end < boundary.span.end
                    && next < chunks.len()
                    && next - chunk_cursor < MAX_VECTOR_UNIT_MEMBERS
                    && chunks[next].1.span.start == slab_end
                    && compatible(location, &chunks[next].1.location)
                {
                    slab_end = chunks[next].1.span.end.min(boundary.span.end);
                    next += 1;
                }
                let context = context_for(
                    boundary,
                    &LocalSpan {
                        start: cursor,
                        end: slab_end,
                    },
                    text,
                    chunks,
                    location,
                    model,
                    context_budget,
                )?;
                let (bytes, context) = fit_body(&text[cursor..slab_end], context, model, target)?;
                let span = LocalSpan {
                    start: cursor,
                    end: cursor + bytes,
                };
                let members = member_spans(&chunks[chunk_cursor..next], &span);
                ensure!(
                    !members.is_empty() && members.len() <= MAX_VECTOR_UNIT_MEMBERS,
                    "invalid vector membership count"
                );
                let current = Pending {
                    group: boundary.group,
                    region: *region,
                    location,
                    context,
                    body: text[span.start..span.end].to_owned(),
                    span,
                    members,
                };
                if let Some(prior) = pending.as_mut()
                    && prior.group == current.group
                    && prior.region == current.region
                    && compatible(prior.location, current.location)
                    && prior.context == current.context
                    && prior.span.end == current.span.start
                {
                    let members = joined_members(&prior.members, &current.members);
                    let body = format!("{}{}", prior.body, current.body);
                    let input = compose(&prior.context, &body);
                    if members.len() <= MAX_VECTOR_UNIT_MEMBERS
                        && input.len() <= MAX_VECTOR_UNIT_INPUT_BYTES
                        && checked_tokens(model, &input)?.len() <= target
                    {
                        prior.body = body;
                        prior.span.end = current.span.end;
                        prior.members = members;
                        cursor = current.span.end;
                        continue;
                    }
                }
                if let Some(prior) = pending.take() {
                    emit(prior, &mut stats, &mut visitor_time, &mut visitor)?;
                }
                cursor = current.span.end;
                pending = Some(current);
            }
        }
        if let Some(prior) = pending {
            emit(prior, &mut stats, &mut visitor_time, &mut visitor)?;
        }
    }
    stats.planning_us = started
        .elapsed()
        .saturating_sub(visitor_time)
        .as_micros()
        .min(u128::from(u64::MAX)) as u64;
    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;
    use airk_core::extraction::SemanticProjection;
    use std::cell::Cell;

    /// A bounded tokenizer fixture, not a simulated embedding engine.
    struct Words {
        calls: Cell<usize>,
        largest_slice: Cell<usize>,
        maximum: usize,
        specials: usize,
    }
    impl Words {
        fn new(maximum: usize) -> Self {
            Self {
                calls: Cell::new(0),
                largest_slice: Cell::new(0),
                maximum,
                specials: 2,
            }
        }
    }
    impl PlannerModel for Words {
        fn context_tokens(&self) -> Option<usize> {
            Some(self.maximum)
        }
        fn tokens(&self, text: &str) -> Result<PlanningTokens> {
            ensure!(
                text.len() <= MAX_VECTOR_UNIT_INPUT_BYTES,
                "unbounded tokenizer fixture input"
            );
            self.calls.set(self.calls.get() + 1);
            self.largest_slice
                .set(self.largest_slice.get().max(text.len()));
            let mut offsets = Vec::new();
            let mut start = None;
            for (index, character) in text.char_indices() {
                if character.is_whitespace() {
                    if let Some(start) = start.take() {
                        offsets.push((start, index));
                    }
                } else {
                    start.get_or_insert(index);
                }
            }
            if let Some(start) = start {
                offsets.push((start, text.len()));
            }
            Ok(PlanningTokens {
                offsets,
                special_tokens: self.specials,
            })
        }
    }
    fn payload(text: String, cuts: &[usize]) -> ExtractionPayload {
        let mut starts = vec![0];
        starts.extend_from_slice(cuts);
        starts.push(text.len());
        let chunks = starts
            .windows(2)
            .map(|part| LocalChunk {
                span: LocalSpan {
                    start: part[0],
                    end: part[1],
                },
                start_line: 1,
                end_line: 1,
                location: DocumentLocation::Lines { start: 1, end: 1 },
                region: None,
            })
            .collect();
        ExtractionPayload {
            contract_version: airk_core::extraction::EXTRACTION_CONTRACT_VERSION,
            extraction_method: "utf8".into(),
            parse_status: "plain".into(),
            provenance: vec![],
            semantic_projection: SemanticProjection {
                version: 1,
                boundaries: vec![SemanticBoundary {
                    span: LocalSpan {
                        start: 0,
                        end: text.len(),
                    },
                    region: None,
                    group: 0,
                    context: None,
                    split_context: None,
                }],
            },
            text,
            chunks,
            facts: vec![],
            syntax_identities: vec![],
            regions: vec![],
            relationships: vec![],
            continuity: vec![],
            continuity_coverage: BTreeMap::new(),
            diagnostics: vec![],
            partial: false,
        }
    }
    fn collect(
        payload: &ExtractionPayload,
        model: &impl PlannerModel,
        target: usize,
    ) -> Result<Vec<PreparedVectorUnit>> {
        let mut result = Vec::new();
        plan(
            payload,
            model,
            VectorUnitOptions {
                target_tokens: target,
                context_tokens: 32,
            },
            |unit| {
                result.push(unit);
                Ok(())
            },
        )?;
        Ok(result)
    }
    fn assert_coverage(payload: &ExtractionPayload, units: &[PreparedVectorUnit]) {
        let mut spans = vec![Vec::new(); payload.chunks.len()];
        for unit in units {
            assert!(unit.input.len() <= MAX_VECTOR_UNIT_INPUT_BYTES);
            assert!(!unit.members.is_empty() && unit.members.len() <= MAX_VECTOR_UNIT_MEMBERS);
            for member in &unit.members {
                spans[member.chunk_index as usize].push(member.span.clone());
            }
        }
        for (chunk, spans) in payload.chunks.iter().zip(spans) {
            let mut cursor = chunk.span.start;
            for span in spans {
                assert_eq!(span.start, cursor);
                assert!(span.end > span.start && span.end <= chunk.span.end);
                cursor = span.end;
            }
            assert_eq!(cursor, chunk.span.end);
        }
    }

    #[test]
    fn long_utf8_crlf_bodies_preserve_every_tail_and_bound_tokenizer_slices() -> Result<()> {
        let text = format!(
            "{}last_unique_tail_marker\r\n",
            "café 🦀 word\r\n".repeat(6000)
        );
        let p = payload(text, &[]);
        let model = Words::new(256);
        let units = collect(&p, &model, 128)?;
        assert!(units.len() > 100);
        assert_coverage(&p, &units);
        assert_eq!(
            units.iter().map(|u| u.input.as_str()).collect::<String>(),
            p.text
        );
        assert!(
            units
                .last()
                .unwrap()
                .input
                .contains("last_unique_tail_marker")
        );
        assert!(model.largest_slice.get() <= MAX_VECTOR_UNIT_INPUT_BYTES);
        assert!(model.calls.get() < units.len() * 8);
        for unit in units {
            assert!(model.tokens(&unit.input)?.len() <= 128);
            for member in unit.members {
                assert!(
                    !(p.text[..member.span.end].ends_with('\r')
                        && p.text[member.span.end..].starts_with('\n'))
                );
            }
        }
        Ok(())
    }

    #[test]
    fn context_and_special_tokens_never_displace_body_or_exceed_model_context() -> Result<()> {
        let heading = "This is a written long heading with several qualifier words\n";
        let signature = "fn restore_account_secret(original_key: Key) {\n";
        let text = format!(
            "{heading}{signature}{}closing_tail_marker\n}}",
            "bodyword ".repeat(200)
        );
        let start = heading.len();
        let mut p = payload(text, &[start]);
        p.semantic_projection.boundaries = vec![
            SemanticBoundary {
                region: None,
                span: LocalSpan {
                    start: 0,
                    end: start,
                },
                group: 0,
                context: None,
                split_context: None,
            },
            SemanticBoundary {
                region: None,
                span: LocalSpan {
                    start,
                    end: p.text.len(),
                },
                group: 1,
                context: Some(LocalSpan {
                    start: 0,
                    end: start,
                }),
                split_context: Some(LocalSpan {
                    start,
                    end: start + signature.len(),
                }),
            },
        ];
        let model = Words::new(16);
        let units = collect(&p, &model, 256)?;
        assert_coverage(&p, &units);
        assert!(units.last().unwrap().input.contains("closing_tail_marker"));
        assert!(
            units
                .iter()
                .all(|u| model.tokens(&u.input).unwrap().len() <= 16)
        );
        assert!(
            units
                .iter()
                .skip(1)
                .any(|u| u.input.starts_with("This is a"))
        );
        Ok(())
    }

    #[test]
    fn sibling_coalescing_is_deterministic_and_independent_of_line_partition() -> Result<()> {
        let text = "fn alpha() {}\nfn beta() {}\nfn gamma() {}\n";
        let a = text.find("fn beta").unwrap();
        let b = text.find("fn gamma").unwrap();
        let mut p = payload(text.into(), &[a, b]);
        p.semantic_projection.boundaries = [0, a, b, text.len()]
            .windows(2)
            .map(|w| SemanticBoundary {
                region: None,
                span: LocalSpan {
                    start: w[0],
                    end: w[1],
                },
                group: 7,
                context: None,
                split_context: None,
            })
            .collect();
        p.chunks[1].location = DocumentLocation::Lines { start: 17, end: 19 };
        let model = Words::new(256);
        let first = collect(&p, &model, 128)?;
        assert_eq!(first, collect(&p, &model, 128)?);
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].members.len(), 3);
        assert_eq!(first[0].input, text);
        assert_coverage(&p, &first);
        p.semantic_projection.boundaries[1].group = 8;
        assert_eq!(collect(&p, &model, 128)?.len(), 3);
        Ok(())
    }

    #[test]
    fn native_coordinates_and_member_fanout_bound_even_one_large_semantic_group() -> Result<()> {
        let text = "word ".repeat(140);
        let cuts: Vec<_> = (1..140).map(|i| i * 5).collect();
        let mut p = payload(text, &cuts);
        let model = Words::new(512);
        let units = collect(&p, &model, 512)?;
        assert_eq!(units.len(), 3);
        assert_coverage(&p, &units);
        let coarse = payload(p.text.clone(), &[]);
        let coarse_units = collect(&coarse, &model, 512)?;
        assert_eq!(coarse_units.len(), 1);
        assert_coverage(&coarse, &coarse_units);
        assert_eq!(
            units.iter().map(|u| u.input.as_str()).collect::<String>(),
            coarse_units[0].input
        );
        for (index, chunk) in p.chunks.iter_mut().enumerate() {
            chunk.location = DocumentLocation::Page {
                number: index / 10 + 1,
            };
        }
        let pages = collect(&p, &model, 512)?;
        assert_eq!(pages.len(), 14);
        assert!(pages.iter().all(|u| u.members.len() == 10));
        assert_coverage(&p, &pages);
        Ok(())
    }

    #[test]
    fn missing_overlapping_or_cross_native_context_projection_fails_explicitly() -> Result<()> {
        let mut p = payload("first page\nsecond page\n".into(), &[11]);
        let model = Words::new(256);
        p.semantic_projection.boundaries.clear();
        assert!(collect(&p, &model, 128).is_err());
        p.semantic_projection.boundaries = vec![SemanticBoundary {
            region: None,
            span: LocalSpan {
                start: 0,
                end: p.text.len() - 1,
            },
            group: 0,
            context: None,
            split_context: None,
        }];
        assert!(collect(&p, &model, 128).is_err());
        p.semantic_projection.boundaries[0].span.end = p.text.len();
        p.semantic_projection
            .boundaries
            .push(p.semantic_projection.boundaries[0].clone());
        assert!(collect(&p, &model, 128).is_err());
        p.semantic_projection.boundaries = vec![
            SemanticBoundary {
                region: None,
                span: LocalSpan { start: 0, end: 11 },
                group: 0,
                context: None,
                split_context: None,
            },
            SemanticBoundary {
                region: None,
                span: LocalSpan {
                    start: 11,
                    end: p.text.len(),
                },
                group: 1,
                context: Some(LocalSpan { start: 0, end: 10 }),
                split_context: None,
            },
        ];
        p.chunks[0].location = DocumentLocation::Page { number: 1 };
        p.chunks[1].location = DocumentLocation::Page { number: 2 };
        assert!(collect(&p, &model, 128).is_err());
        Ok(())
    }

    #[test]
    fn byte_cap_preserves_crlf_even_when_all_text_has_zero_body_tokens() -> Result<()> {
        let text = format!(
            "{}\r\n{}",
            " ".repeat(MAX_VECTOR_UNIT_INPUT_BYTES - 1),
            "\u{2003}".repeat(7000)
        );
        let p = payload(text, &[]);
        let model = Words::new(256);
        let units = collect(&p, &model, 128)?;
        assert!(units.len() >= 3);
        assert_coverage(&p, &units);
        assert_eq!(
            units.iter().map(|u| u.input.as_str()).collect::<String>(),
            p.text
        );
        for unit in units {
            let end = unit.members.last().unwrap().span.end;
            assert!(!(p.text[..end].ends_with('\r') && p.text[end..].starts_with('\n')));
        }
        Ok(())
    }

    #[test]
    fn mapped_cells_and_regions_are_hard_boundaries_independent_of_local_lines() {
        let cell = |position, line| DocumentLocation::Mapped {
            region: RegionLocation::NotebookCell {
                cell_id: Some(format!("cell-{position}")),
                position,
                local_start_line: line,
                local_end_line: line,
                original_spans: vec![ByteSpan { start: 0, end: 4 }],
            },
        };
        assert!(compatible(&cell(1, 1), &cell(1, 4)));
        assert!(!compatible(&cell(1, 1), &cell(2, 1)));
        let region = |id: &str, start| DocumentLocation::Mapped {
            region: RegionLocation::Embedded {
                region_id: id.into(),
                local_span: ByteSpan {
                    start,
                    end: start + 1,
                },
                original_spans: vec![ByteSpan {
                    start,
                    end: start + 1,
                }],
            },
        };
        assert!(compatible(&region("script-a", 0), &region("script-a", 5)));
        assert!(!compatible(&region("script-a", 0), &region("script-b", 0)));
        assert!(!compatible(&cell(1, 1), &region("script-a", 0)));
    }

    #[test]
    fn planning_statistics_count_body_and_exclude_visitor_work() -> Result<()> {
        let p = payload("written body words".into(), &[]);
        let model = Words::new(256);
        let mut visitor_elapsed = std::time::Duration::ZERO;
        let started = std::time::Instant::now();
        let stats = plan(&p, &model, VectorUnitOptions::default(), |_| {
            let callback = std::time::Instant::now();
            std::thread::sleep(std::time::Duration::from_millis(20));
            visitor_elapsed += callback.elapsed();
            Ok(())
        })?;
        let total = started.elapsed().as_micros();
        assert_eq!((stats.units, stats.body_bytes), (1, p.text.len() as u64));
        assert!(total.saturating_sub(u128::from(stats.planning_us)) >= visitor_elapsed.as_micros());
        Ok(())
    }

    #[test]
    #[ignore = "requires AIRK_TEST_MODEL_DIR with verified local ONNX assets"]
    fn real_model_vector_unit_candidates_preserve_tail_retrieval() -> Result<()> {
        let model =
            EmbeddingModel::open(std::path::Path::new(&std::env::var("AIRK_TEST_MODEL_DIR")?))?;
        ensure!(
            matches!(model, EmbeddingModel::Onnx(_)),
            "native transformer model required"
        );
        let unicode = payload("café 中文 русский العربية 🦀\r\n".repeat(800), &[]);
        let mut unicode_units = Vec::new();
        visit_vector_units_with_options(
            &unicode,
            &model,
            VectorUnitOptions {
                target_tokens: 128,
                context_tokens: 32,
            },
            |unit| {
                unicode_units.push(unit);
                Ok(())
            },
        )?;
        assert_coverage(&unicode, &unicode_units);
        assert_eq!(
            unicode_units
                .iter()
                .map(|u| u.input.as_str())
                .collect::<String>(),
            unicode.text
        );
        assert!(
            unicode_units
                .iter()
                .all(|u| model.planning_tokens(&u.input).unwrap().len() <= 128)
        );
        let cases = [
            (
                "A forgotten login secret is recovered by issuing a single-use reset link through a verified contact channel. Expire the recovery token after fifteen minutes.",
                "How do we restore access when a user forgets their password?",
            ),
            (
                "Restore a damaged database from the latest verified backup, then replay committed transaction records until the requested recovery point.",
                "How can a corrupted database be recovered from backup and transaction logs?",
            ),
            (
                "Transient connection failures use capped exponential backoff and randomized delay so retrying clients do not synchronize their requests.",
                "What prevents network retries from overwhelming the service at the same time?",
            ),
            (
                "Before writing index objects, reserve the required storage quota. Reject the pending update when space is insufficient and preserve the published state.",
                "What happens if the disk budget cannot accommodate a new index update?",
            ),
            (
                "Encrypt the archive using a newly generated data key and wrap that key with the recipient public key. Keep the private decryption key outside the archive.",
                "How are encrypted archives shared without including the private key?",
            ),
            (
                "Worker cancellation is cooperative: signal the cancellation flag, stop admitting new jobs, join running workers, and release their reservations.",
                "How do cancelled workers stop safely and release their resources?",
            ),
        ];
        let prefix =
            "Routine operational guidance preserves local evidence and records ordinary checks. "
                .repeat(65);
        let mut fixtures = Vec::new();
        let mut tail_starts = Vec::new();
        for (tail, _) in cases {
            let mut text = prefix.clone();
            tail_starts.push(text.len());
            text.push_str(tail);
            fixtures.push(payload(text, &[]));
        }
        let functions = [
            "fn restore_account() { /* Send a one-use password recovery link to the verified user. */ }\n",
            "fn recover_database() { /* Replay committed transactions after loading a verified backup. */ }\n",
            "fn retry_connection() { /* Use randomized exponential backoff for transient failures. */ }\n",
            "fn stop_workers() { /* Signal cancellation, join workers, and release reserved capacity. */ }\n",
        ];
        let text = functions.concat();
        let cuts: Vec<_> = functions
            .iter()
            .scan(0usize, |end, s| {
                *end += s.len();
                Some(*end)
            })
            .take(3)
            .collect();
        let mut siblings = payload(text, &cuts);
        siblings.semantic_projection.boundaries = siblings
            .chunks
            .iter()
            .map(|chunk| SemanticBoundary {
                region: None,
                span: chunk.span.clone(),
                group: 77,
                context: None,
                split_context: None,
            })
            .collect();
        let queries: Vec<_> = cases
            .iter()
            .map(|(_, query)| model.embed_text(query).map(|v| v.unwrap()))
            .collect::<Result<_>>()?;
        let baseline: Vec<_> = fixtures
            .iter()
            .map(|p| model.embed_text(&p.text).map(|v| v.unwrap()))
            .collect::<Result<_>>()?;
        let score = |left: &[f32], right: &[f32]| {
            left.iter()
                .zip(right)
                .map(|(a, b)| f64::from(*a) * f64::from(*b))
                .sum::<f64>()
        };
        let baseline_correct = queries
            .iter()
            .enumerate()
            .filter(|(expected, query)| {
                let best = baseline
                    .iter()
                    .enumerate()
                    .max_by(|(ia, a), (ib, b)| {
                        score(query, a)
                            .total_cmp(&score(query, b))
                            .then_with(|| ib.cmp(ia))
                    })
                    .unwrap()
                    .0;
                best == *expected
            })
            .count();
        let descriptor = model.cache_descriptor()?;
        let mut observations = Vec::new();
        for target_tokens in [128, 192, 256] {
            let options = VectorUnitOptions {
                target_tokens,
                context_tokens: 32,
            };
            let started = std::time::Instant::now();
            let mut units = Vec::new();
            for (source, p) in fixtures.iter().enumerate() {
                let mut prepared = Vec::new();
                visit_vector_units_with_options(p, &model, options, |unit| {
                    prepared.push(unit);
                    Ok(())
                })?;
                assert_coverage(p, &prepared);
                for unit in prepared {
                    ensure!(
                        model.planning_tokens(&unit.input)?.len() <= target_tokens,
                        "planned input would truncate"
                    );
                    units.push((source, unit));
                }
            }
            let planning_us = crate::embedding_batch::elapsed_us(started);
            let mut vectors = Vec::new();
            let mut stats = crate::model::EmbeddingCacheStats::default();
            for batch in units.chunks(crate::embedding_batch::MAX_BATCH_CHUNKS) {
                let texts: Vec<_> = batch.iter().map(|(_, unit)| unit.input.as_str()).collect();
                vectors.extend(crate::embedding_batch::embed(
                    &model,
                    &texts,
                    &descriptor,
                    None,
                    false,
                    &mut stats,
                    4,
                    1 << 30,
                )?);
            }
            assert_eq!(stats.truncated_inputs, 0);
            let mut warm_us = Vec::new();
            for _ in 0..3 {
                let mut repeated = Vec::new();
                let mut warm = crate::model::EmbeddingCacheStats::default();
                for batch in units.chunks(crate::embedding_batch::MAX_BATCH_CHUNKS) {
                    let texts: Vec<_> = batch.iter().map(|(_, unit)| unit.input.as_str()).collect();
                    repeated.extend(crate::embedding_batch::embed(
                        &model,
                        &texts,
                        &descriptor,
                        None,
                        false,
                        &mut warm,
                        4,
                        1 << 30,
                    )?);
                }
                assert_eq!(repeated, vectors);
                assert_eq!((warm.truncated_inputs, warm.plan_compilations), (0, 0));
                warm_us.push(warm.inference_us);
            }
            warm_us.sort_unstable();
            let mut correct = 0;
            let mut ranks = Vec::new();
            for (expected, query) in queries.iter().enumerate() {
                let mut order: Vec<_> = vectors
                    .iter()
                    .enumerate()
                    .map(|(index, v)| (index, score(query, v.as_ref().unwrap())))
                    .collect();
                order.sort_by(|(ia, a), (ib, b)| b.total_cmp(a).then_with(|| ia.cmp(ib)));
                let rank = order
                    .iter()
                    .position(|(index, _)| {
                        units[*index].0 == expected
                            && units[*index]
                                .1
                                .members
                                .iter()
                                .any(|member| member.span.end > tail_starts[expected])
                    })
                    .unwrap()
                    + 1;
                correct += usize::from(rank == 1);
                ranks.push(rank);
            }
            let mut sibling_units = Vec::new();
            visit_vector_units_with_options(&siblings, &model, options, |unit| {
                sibling_units.push(unit);
                Ok(())
            })?;
            assert_coverage(&siblings, &sibling_units);
            ensure!(
                sibling_units.len() < functions.len(),
                "short sibling units were not reduced"
            );
            ensure!(
                ranks.iter().all(|rank| *rank <= 3),
                "tail evidence fell outside top three candidate units"
            );
            observations.push(serde_json::json!({"target_tokens":target_tokens,"tail_queries":cases.len(),"tail_top1":correct,
                "tail_ranks":ranks,"units":units.len(),"sibling_units":sibling_units.len(),"planning_us":planning_us,
                "warm_inference_us":warm_us,"warm_inference_median_us":warm_us[1],"execution":stats}));
        }
        println!(
            "{}",
            serde_json::json!({"native_vector_unit_qualification":{
                "baseline_source_top1":baseline_correct,"queries":cases.len(),"candidates":observations,
                "scope":"Original adversarial shared-prefix tails and short written functions; not a production relevance benchmark"
            }})
        );
        Ok(())
    }
}
