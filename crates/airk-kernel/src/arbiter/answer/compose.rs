// SPDX-License-Identifier: Apache-2.0
//! Compose (step 5): a part's evidence grouped by sub-need, with locations,
//! the round and Focus score of each span, the sub-need's coverage, and the
//! reads it did not run. The excerpts themselves are the records' spans;
//! these paragraphs index them, so the text is never carried twice.
use super::gather::Gathering;
use super::state::PartState;
use super::*;

fn coverage_name(coverage: v3::Coverage) -> &'static str {
    match coverage {
        v3::Coverage::Covered => "covered",
        v3::Coverage::Partial => "partly covered",
        v3::Coverage::Missing => "missing",
    }
}

/// One paragraph per sub-need of the part, grounded in the records it
/// cites. A missing sub-need cites the part's own search. `compact` keeps
/// only the heading and the reads not run; the spans stay in the records.
pub(crate) fn part(gather: &Gathering, part: &PartState, compact: bool) -> Vec<GroundedParagraph> {
    let anchor = gather
        .records
        .iter()
        .find(|record| {
            record
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
        let mut seen = BTreeSet::new();
        for span in &state.history {
            if compact || !state.evidence.contains(&span.record) || !seen.insert(span.location()) {
                continue;
            }
            text.push_str(&format!("\n- {} (round {}", span.location(), span.round));
            if let Some(score) = span.focus {
                text.push_str(&format!(", Focus {score:.2}"));
            }
            text.push(')');
        }
        for index in &state.evidence {
            let record = &gather.records[*index];
            if !compact && record_hits_empty(record) {
                text.push_str(&format!("\n- {} {}", record.tool, record.subject));
            }
        }
        if !state.unrun.is_empty() {
            let unrun: Vec<String> = state
                .unrun
                .iter()
                .map(|read| format!("{:?} {}", read.operation, read.target))
                .collect();
            text.push_str(&format!("\nNot run: {}", unrun.join("; ")));
        }
        let evidence: Vec<Sha256Digest> = state
            .evidence
            .iter()
            .map(|index| gather.records[*index].id)
            .collect();
        let evidence = if evidence.is_empty() {
            match anchor {
                Some(anchor) => vec![anchor],
                None => continue,
            }
        } else {
            evidence
        };
        out.push(GroundedParagraph {
            text,
            evidence,
            interpretation: true,
        });
    }
    out
}

fn record_hits_empty(record: &super::gather::Record) -> bool {
    super::coverage::record_hits(record).is_empty()
}
