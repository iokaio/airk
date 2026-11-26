// SPDX-License-Identifier: Apache-2.0
//! Compose (step 5): group delivered citations by sub-need. Source excerpts
//! stay in the records, so these paragraphs do not carry a second copy.
use super::gather::Gathering;
use super::state::PartState;
use super::*;
use std::fmt::Write as _;

fn coverage_name(coverage: v3::Coverage) -> &'static str {
    match coverage {
        v3::Coverage::Covered => "covered",
        v3::Coverage::Partial => "partly covered",
        v3::Coverage::Missing => "missing",
    }
}

fn bounded_read(operation: v3::LoopRead) -> bool {
    matches!(
        operation,
        v3::LoopRead::ReadFile | v3::LoopRead::Context | v3::LoopRead::DocumentSections
    )
}

fn delivered_span(delivery: &v3::LoopEvidence, pointer: &str) -> Option<bool> {
    delivery.canonical_result["spans"]
        .as_array()?
        .iter()
        .find(|row| row["pointer"].as_str() == Some(pointer))
        .and_then(|row| row["truncated"].as_bool())
}

/// One paragraph per sub-need, using only citations present in this packing
/// pass. Compact paragraphs omit the location list, but still disclose losses.
pub(crate) fn part(
    gather: &Gathering,
    part: &PartState,
    delivered: &BTreeMap<Sha256Digest, &v3::LoopEvidence>,
    compact: bool,
) -> Vec<GroundedParagraph> {
    let anchor = gather
        .records
        .iter()
        .find(|record| {
            delivered.contains_key(&record.id)
                && record
                    .uses
                    .iter()
                    .any(|item| item.part == part.id && item.need.is_none())
        })
        .map(|record| record.id);
    let mut out = Vec::new();
    for state in &part.needs {
        let need = &state.need;
        let mut text = format!(
            "Part {} sub-need {} ({}, {}): {}",
            part.id,
            need.id,
            need.kind.name(),
            coverage_name(state.coverage),
            need.query
        );
        let mut evidence = Vec::new();
        let mut seen_records = BTreeSet::new();
        let mut seen_spans = BTreeSet::new();
        let mut seen_locations = BTreeSet::new();
        let mut clipped = 0;
        let mut withheld = 0;
        // Prefer the source reads that followed a lead. Preserve the ranked
        // order within each group, then include the earlier rounds' citations.
        for bounded in [true, false] {
            for span in state.spans.iter().chain(&state.history) {
                let record = &gather.records[span.record];
                if bounded_read(record.operation) != bounded
                    || !state.evidence.contains(&span.record)
                    || !seen_spans.insert((record.id, span.pointer.as_str()))
                {
                    continue;
                }
                let Some(delivery) = delivered.get(&record.id) else {
                    continue;
                };
                let Some(truncated) = delivered_span(delivery, &span.pointer) else {
                    withheld += 1;
                    continue;
                };
                clipped += usize::from(truncated);
                if seen_records.insert(record.id) {
                    evidence.push(record.id);
                }
                if compact || !seen_locations.insert(span.location()) {
                    continue;
                }
                let _ = write!(text, "\n- {} (round {}", span.location(), span.round);
                if let Some(score) = span.focus {
                    let _ = write!(text, ", Focus {score:.2}");
                }
                if truncated {
                    text.push_str(", excerpt clipped");
                }
                text.push(')');
            }
        }
        let mut omitted = 0;
        for index in &state.evidence {
            let record = &gather.records[*index];
            let Some(delivery) = delivered.get(&record.id) else {
                omitted += 1;
                continue;
            };
            if seen_records.insert(record.id) {
                evidence.push(record.id);
            }
            if !compact && delivery.canonical_result["hits_total"].as_u64() == Some(0) {
                let _ = write!(text, "\n- {} {}", record.tool, record.subject);
            }
        }
        for (count, reason) in [
            (omitted, "Evidence records omitted"),
            (withheld, "Cited excerpts withheld"),
            (clipped, "Cited excerpts clipped"),
        ] {
            if count > 0 {
                let _ = write!(text, "\n{reason} ({count}).");
            }
        }
        if !state.unrun.is_empty() {
            let unrun: Vec<String> = state
                .unrun
                .iter()
                .map(|read| format!("{:?} {} ({:?})", read.operation, read.target, read.reason))
                .collect();
            let _ = write!(text, "\nNot run: {}", unrun.join("; "));
        }
        if evidence.is_empty() {
            let Some(anchor) = anchor.filter(|_| state.evidence.is_empty()) else {
                continue;
            };
            evidence.push(anchor);
        }
        out.push(GroundedParagraph {
            text,
            evidence,
            interpretation: true,
        });
    }
    out
}
