// SPDX-License-Identifier: Apache-2.0
//! Union (step 6): the parts' results in one reply. Evidence found by more
//! than one part is one record with every part's provenance; conflicts are
//! noted, never resolved; parts share the reply frame, evidence first.
//!
//! Each record is sent as compact spans (location, content hash and the
//! excerpt read) instead of the whole canonical search result with its
//! metadata; the intact canonical result stays in custody beside the reply,
//! bound by the record id. To fit the frame Union omits, in this order:
//! decisions (oldest deterministic first, disclosed by the log summary), hits
//! no sub-need cited, excerpt bytes, the paragraphs' location lists, then
//! whole records by keep rank.
use super::decide::Ledger;
use super::gather::{Gathering, Record};
use super::state::PartState;
use super::*;
use std::collections::BTreeMap;

pub(crate) const PROJECTION_SCHEMA: &str = "airk.arbiter.loop-evidence/1";
/// Excerpt bytes kept per span at each trimming level.
const EXCERPT_BYTES: [usize; 5] = [usize::MAX, usize::MAX, 768, 384, 384];
const ROWS: [usize; 5] = [24, 24, 12, 6, 6];
/// From this level on, paragraphs keep only each sub-need's heading.
const COMPACT_PARAGRAPHS: usize = 4;

/// Everything Union needs from the finished loop.
pub(crate) struct Assembly<'a> {
    pub(crate) question: &'a str,
    pub(crate) scope: &'a ScopeBinding,
    pub(crate) parts: &'a [PartState],
    pub(crate) gather: &'a Gathering,
    pub(crate) ledger: &'a Ledger,
    pub(crate) degradations: Vec<v3::Degradation>,
    pub(crate) application: IndexApplication,
    pub(crate) elapsed_ms: u64,
    /// The bytes the reply may take, after the transport's reserve.
    pub(crate) limit: usize,
}

fn truncate(text: &str, limit: usize) -> (&str, bool) {
    if text.len() <= limit {
        return (text, false);
    }
    let mut end = limit;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    (&text[..end], true)
}

fn revision(body: &Value) -> Value {
    body.get("publication")
        .and_then(|publication| publication.get("token"))
        .or_else(|| {
            body.get("read")
                .and_then(|read| read.get("publication"))
                .and_then(|publication| publication.get("token"))
        })
        .cloned()
        .unwrap_or(Value::Null)
}

/// A record's compact span projection at a trimming level.
fn project(record: &Record, cited: &BTreeMap<String, Option<f32>>, level: usize) -> Value {
    let bytes = serde_json::to_vec(&record.body).unwrap_or_default();
    let hits = super::coverage::record_hits(record);
    let mut spans = Vec::new();
    for (pointer, hit) in &hits {
        let focus = cited.get(pointer);
        if level >= 1 && focus.is_none() {
            continue;
        }
        let (text, cut) = truncate(&hit.text, EXCERPT_BYTES[level]);
        let mut span = json!({"pointer":pointer,"path":hit.citation.path,"start_line":hit.citation.start_line,
            "end_line":hit.citation.end_line,"content_hash":hit.citation.content_hash,"text":text,
            "truncated":hit.truncated || cut});
        if let Some(Some(score)) = focus {
            span["focus"] = json!(score);
        }
        spans.push(span);
    }
    let mut projection = json!({"projection":PROJECTION_SCHEMA,"revision":revision(&record.body),
        "full_bytes":record.bytes,"full_digest":framed_digest(b"airk.arbiter.loop-record-body/1", &[&bytes]).to_string(),
        "hits_total":hits.len(),"spans":spans});
    if hits.is_empty() {
        let rows = crate::workbench::locations(&record.body).unwrap_or_default();
        let shown: Vec<Value> = rows
            .iter()
            .take(ROWS[level])
            .map(|row| {
                let citation = &row["citation"];
                let name = row["text"].as_str().unwrap_or("");
                json!({"path":citation["path"],"start_line":citation["start_line"],"end_line":citation["end_line"],
                    "text":truncate(name, 160).0})
            })
            .collect();
        projection["rows_total"] = json!(rows.len());
        projection["rows"] = json!(shown);
    }
    if record.failure {
        let text = record.body.to_string();
        projection["failure"] = json!(truncate(&text, 512).0);
    }
    projection
}

/// Records omitted first: uncited before cited, then failure receipts and
/// the least useful kinds, newest first. A part's own search is never
/// omitted.
fn omission_order(gather: &Gathering, cited: &BTreeSet<usize>) -> Vec<usize> {
    let mut order: Vec<usize> = (0..gather.records.len())
        .filter(|index| {
            let record = &gather.records[*index];
            !record
                .uses
                .iter()
                .any(|item| item.cause == v3::UseCause::PartSearch)
        })
        .collect();
    order.sort_by_key(|index| {
        let record = &gather.records[*index];
        let rank = if record.failure {
            0
        } else {
            record.kind.keep_rank() + 1
        };
        (cited.contains(index), rank, std::cmp::Reverse(record.seq))
    });
    order
}

/// Parts and records that found one definition location.
type Found = (BTreeSet<u8>, BTreeSet<Sha256Digest>);

fn conflicts(gather: &Gathering, parts: &[PartState]) -> Vec<v3::ConflictNote> {
    let mut notes = Vec::new();
    // Same definition subject, different definition locations across parts.
    let mut definitions: BTreeMap<String, BTreeMap<String, Found>> = BTreeMap::new();
    for record in &gather.records {
        if record.failure || record.kind != software::FollowUp::Excerpt {
            continue;
        }
        let Some(path) = record.arguments.get("path").and_then(Value::as_str) else {
            continue;
        };
        let entry = definitions
            .entry(record.subject.clone())
            .or_default()
            .entry(path.to_owned())
            .or_default();
        entry.0.extend(record.uses.iter().map(|item| item.part));
        entry.1.insert(record.id);
    }
    for (subject, locations) in definitions {
        let parts: BTreeSet<u8> = locations
            .values()
            .flat_map(|(parts, _)| parts.iter().copied())
            .collect();
        if locations.len() >= 2 && parts.len() >= 2 {
            notes.push(v3::ConflictNote {
                kind: v3::ConflictKind::DivergentDefinition,
                parts: parts.into_iter().collect(),
                evidence: locations.values().flat_map(|(_, ids)| ids.iter().copied()).collect(),
                note: truncate(
                    &format!("Parts found different definitions of {subject}; both are kept and none is preferred."),
                    v3::MAX_TEXT_BYTES,
                )
                .0
                .to_owned(),
            });
        }
    }
    // The same evidence judged covered by one part and not by another.
    for (left, first) in parts.iter().enumerate() {
        for second in &parts[left + 1..] {
            for one in &first.needs {
                for other in &second.needs {
                    if one.need.kind != other.need.kind
                        || one.need.subject.is_none()
                        || one.need.subject != other.need.subject
                        || one.covered() == other.covered()
                    {
                        continue;
                    }
                    let shared: Vec<Sha256Digest> = one
                        .evidence
                        .iter()
                        .filter(|index| other.evidence.contains(index))
                        .map(|index| gather.records[*index].id)
                        .collect();
                    if !shared.is_empty() && notes.len() < v3::MAX_CONFLICTS {
                        notes.push(v3::ConflictNote {
                            kind: v3::ConflictKind::DivergentVerdict,
                            parts: vec![first.id, second.id],
                            evidence: shared,
                            note: truncate(
                                &format!(
                                    "Part {} judged {} covered and part {} did not, on shared evidence.",
                                    first.id,
                                    one.need.subject.as_deref().unwrap_or(""),
                                    second.id
                                ),
                                v3::MAX_TEXT_BYTES,
                            )
                            .0
                            .to_owned(),
                        });
                    }
                }
            }
        }
    }
    notes.truncate(v3::MAX_CONFLICTS);
    notes
}

fn stop_reason(stop: v3::PartStop) -> Option<v3::PartialReason> {
    match stop {
        v3::PartStop::Budget => Some(v3::PartialReason::Budget),
        v3::PartStop::Deadline => Some(v3::PartialReason::Deadline),
        v3::PartStop::Stale => Some(v3::PartialReason::Stale),
        v3::PartStop::Refused => Some(v3::PartialReason::Refused),
        _ => None,
    }
}

/// The decisions a reply includes when `included` of them fit: the
/// oldest deterministic ones are omitted first, then the oldest scored.
fn included(ledger: &Ledger, count: usize) -> Vec<v3::DecisionRecord> {
    let total = ledger.entries.len();
    let (deterministic, scored): (Vec<_>, Vec<_>) = ledger
        .entries
        .iter()
        .map(|entry| &entry.decision)
        .partition(|decision| !decision.scored_by_model());
    let omitted: BTreeSet<u8> = deterministic
        .iter()
        .chain(&scored)
        .take(total - count)
        .map(|decision| decision.ordinal)
        .collect();
    ledger
        .entries
        .iter()
        .map(|entry| entry.decision.clone())
        .filter(|decision| !omitted.contains(&decision.ordinal))
        .collect()
}

/// The reply and its custody log, packed into the frame.
pub(crate) fn assemble(
    assembly: &Assembly<'_>,
) -> Result<(v3::ContextResponseV3, v3::DecisionLog)> {
    let gather = assembly.gather;
    let log = assembly.ledger.log(assembly.question, assembly.scope);
    let total = log.entries.len();
    // Pointers each record's spans were cited at, with their best Focus.
    let mut cited_spans: BTreeMap<usize, BTreeMap<String, Option<f32>>> = BTreeMap::new();
    let mut cited: BTreeSet<usize> = BTreeSet::new();
    for part in assembly.parts {
        for need in &part.needs {
            cited.extend(need.evidence.iter().copied());
            for span in &need.history {
                if need.evidence.contains(&span.record) {
                    let slot = cited_spans
                        .entry(span.record)
                        .or_default()
                        .entry(span.pointer.clone())
                        .or_insert(None);
                    if let Some(score) = span.focus {
                        *slot = Some(slot.map_or(score, |old: f32| old.max(score)));
                    }
                }
            }
        }
    }
    let order = omission_order(gather, &cited);
    let mut decisions = total;
    let mut level = 0usize;
    let mut omitted = 0usize;
    loop {
        let removed: BTreeSet<usize> = order.iter().take(omitted).copied().collect();
        let response = build(
            assembly,
            &log,
            &cited_spans,
            &cited,
            decisions,
            level,
            &removed,
        )?;
        if serde_json::to_vec(&response)?.len() <= assembly.limit {
            return Ok((response, log));
        }
        if decisions > 0 {
            decisions -= 1;
        } else if level + 1 < EXCERPT_BYTES.len() {
            level += 1;
        } else if omitted < order.len() {
            omitted += 1;
        } else {
            anyhow::bail!(
                "Answer mandatory part searches and their metadata exceed the reply frame"
            );
        }
    }
}

#[allow(clippy::too_many_lines)]
fn build(
    assembly: &Assembly<'_>,
    log: &v3::DecisionLog,
    cited_spans: &BTreeMap<usize, BTreeMap<String, Option<f32>>>,
    cited: &BTreeSet<usize>,
    decisions: usize,
    level: usize,
    removed: &BTreeSet<usize>,
) -> Result<v3::ContextResponseV3> {
    let gather = assembly.gather;
    let empty = BTreeMap::new();
    let records: Vec<v3::LoopEvidence> = gather
        .records
        .iter()
        .enumerate()
        .filter(|(index, _)| !removed.contains(index))
        .map(|(index, record)| v3::LoopEvidence {
            id: record.id,
            operation: record.operation,
            arguments: record.arguments.clone().into_iter().collect(),
            role: EvidenceRole::Unknown,
            canonical_result: project(record, cited_spans.get(&index).unwrap_or(&empty), level),
        })
        .collect();
    let removed_ids: BTreeSet<Sha256Digest> = removed
        .iter()
        .map(|index| gather.records[*index].id)
        .collect();
    let mut record_uses = Vec::new();
    for (index, record) in gather.records.iter().enumerate() {
        if removed.contains(&index) {
            continue;
        }
        for item in &record.uses {
            record_uses.push(v3::RecordUse {
                record: record.id,
                part: item.part,
                need: item.need,
                round: item.round,
                cause: item.cause,
            });
        }
    }
    record_uses.sort();
    record_uses.dedup_by(|left, right| {
        (left.record, left.part, left.need, left.round)
            == (right.record, right.part, right.need, right.round)
    });
    let mut dedup_count = 0usize;
    for record in &gather.records {
        let parts: BTreeSet<u8> = record.uses.iter().map(|item| item.part).collect();
        dedup_count += parts.len().saturating_sub(1);
    }
    let summary = log.summary(decisions)?;
    let mut parts = Vec::new();
    let mut paragraphs = Vec::new();
    for part in assembly.parts {
        let shortened = part.shortened
            || removed.iter().any(|index| {
                gather.records[*index]
                    .uses
                    .iter()
                    .any(|item| item.part == part.id)
            });
        let stop = part.stop.unwrap_or(v3::PartStop::NoProgress);
        let sub_needs = part
            .needs
            .iter()
            .map(|state| {
                let evidence: Vec<Sha256Digest> = if state.coverage == v3::Coverage::Missing {
                    vec![]
                } else {
                    state
                        .evidence
                        .iter()
                        .map(|index| gather.records[*index].id)
                        .collect()
                };
                let evidence_omitted = evidence
                    .iter()
                    .filter(|id| removed_ids.contains(id))
                    .count();
                v3::SubNeed {
                    id: state.need.id,
                    text: state.need.query.clone(),
                    kind: state.need.kind.wire(),
                    coverage: state.coverage,
                    evidence,
                    evidence_omitted: evidence_omitted as u16,
                    unrun_reads: if state.covered() {
                        vec![]
                    } else {
                        state.unrun.clone()
                    },
                }
            })
            .collect();
        let rounds = part
            .rounds
            .iter()
            .enumerate()
            .map(|(index, round)| v3::RoundProvenance {
                round: index as u8 + 1,
                calls: round.calls,
                fanout: round.fanout,
                input_tokens: summary
                    .attribution
                    .iter()
                    .filter(|charge| {
                        charge.part == part.id && usize::from(charge.round) == index + 1
                    })
                    .map(|charge| charge.tokens)
                    .sum(),
                new_evidence: round.new_evidence,
                follow_ups_run: round.follow_ups_run.clone(),
                sub_needs_covered: round.covered,
            })
            .collect();
        parts.push(v3::AnswerPart {
            id: part.id,
            span: v3::QuestionSpan {
                utf8_start: part.span.0,
                utf8_end: part.span.1,
            },
            deadline_share_ms: part.share_ms,
            sub_needs,
            gaps: part.gaps(),
            rounds,
            stop,
            shortened,
        });
        for paragraph in super::compose::part(gather, part, level >= COMPACT_PARAGRAPHS) {
            let evidence: Vec<Sha256Digest> = paragraph
                .evidence
                .iter()
                .copied()
                .filter(|id| !removed_ids.contains(id))
                .collect();
            if !evidence.is_empty() {
                paragraphs.push(GroundedParagraph {
                    evidence,
                    ..paragraph
                });
            }
        }
    }
    let retained: BTreeSet<Sha256Digest> = records.iter().map(|record| record.id).collect();
    let conflicts = conflicts(gather, assembly.parts)
        .into_iter()
        .filter_map(|mut note| {
            note.evidence.retain(|id| retained.contains(id));
            let minimum = match note.kind {
                v3::ConflictKind::DivergentDefinition => 2,
                v3::ConflictKind::DivergentVerdict => 1,
            };
            (note.evidence.len() >= minimum).then_some(note)
        })
        .collect();
    let omitted_count = gather.omitted + removed.len();
    let omitted_evidence: Vec<Sha256Digest> = removed
        .iter()
        .filter(|index| cited.contains(index))
        .map(|index| gather.records[*index].id)
        .collect();
    let early = assembly
        .parts
        .iter()
        .find_map(|part| part.stop.and_then(stop_reason).map(|reason| (part, reason)));
    let partial = match early {
        Some((part, reason)) => Some(v3::PartialMarker {
            stage: part.stopped_in.unwrap_or(v3::LoopStage::Retrieve),
            reason,
            part: Some(part.id),
            continuation: None,
        }),
        None if omitted_count > 0 => Some(v3::PartialMarker {
            stage: v3::LoopStage::Union,
            reason: v3::PartialReason::Bytes,
            part: None,
            continuation: None,
        }),
        None => None,
    };
    let mut work = Work {
        calls: gather.calls,
        rounds: assembly
            .parts
            .iter()
            .map(|part| part.rounds.len())
            .max()
            .unwrap_or(0),
        fanout: assembly
            .parts
            .iter()
            .flat_map(|part| part.rounds.iter().map(|round| round.fanout))
            .max()
            .unwrap_or(0),
        candidates: gather.candidates,
        bytes: gather.bytes,
        input_tokens: summary.attribution.iter().map(|charge| charge.tokens).sum(),
        ..Work::default()
    };
    work.queue_us = summary.timings.queue_us;
    work.compile_us = summary.timings.compile_us;
    work.inference_us = summary.timings.inference_us;
    work.policy_us = summary.timings.policy_us;
    let model_work = v3::LoopModelWork {
        context_calls: summary.card_calls,
        focus_calls: summary.focus_calls,
        focus_pairs: summary.focus_pairs,
    };
    Ok(v3::ContextResponseV3 {
        schema: v3::CONTEXT_SCHEMA_V3.into(),
        packet_schema: v3::PACKET_SCHEMA_V2.into(),
        question: assembly.question.to_owned(),
        scope: assembly.scope.clone(),
        parts,
        records,
        omitted_evidence,
        record_uses,
        paragraphs,
        union: v3::UnionSummary {
            dedup_count,
            conflicts,
        },
        partial,
        ticket: None,
        degradations: assembly.degradations.clone(),
        decisions: included(assembly.ledger, decisions),
        decision_log: summary,
        work,
        model_work,
        elapsed_ms: assembly.elapsed_ms,
        candidate_count: gather.candidates,
        omitted_count,
        applications: vec![assembly.application.clone()],
    })
}

#[cfg(test)]
mod tests;
