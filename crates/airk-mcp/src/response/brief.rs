// SPDX-License-Identifier: Apache-2.0
//! The brief reply of the public Gather Context tool (user decision, 27
//! November 2026). In terse mode an Answer reply is projected to a short map:
//! for each sub-need where it is answered, the few best passages the kernel
//! delivered inside a byte allowance, what was not found and what to ask
//! next. `response_mode=full` returns the kernel's `airk.arbiter.context/3`
//! reply unchanged. Only data the kernel reply records is used; a location's
//! reason names the read that fetched it, never an inferred relation.
use airk_core::arbiter::answer_loop::{self as v3, ContextResponseV3};
use rmcp::schemars;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};

#[cfg(test)]
mod tests;

/// The brief's own schema name.
pub const BRIEF_SCHEMA: &str = airk_core::combined_context::GATHER_CONTEXT_BRIEF_SCHEMA;
/// The most bytes one serialized brief takes: about what a reference agent
/// read for itself on a task (strategy of 27 November 2026, about 25 KB).
pub const MAX_BRIEF_BYTES: usize = 24_000;
/// The passage text one brief carries, about two thirds of the ceiling; the
/// rest is the map, which every need gets whatever the passages take.
pub const BRIEF_PASSAGE_BYTES: usize = 16_000;
/// Locations listed under one sub-need. The kernel keeps up to eight
/// passages a sub-need; four are enough to choose what to open.
pub const WHERE_PER_NEED: usize = 4;
/// The task text the header repeats.
pub const MAX_TASK_BYTES: usize = 512;
/// The note that says where the complete reply is.
pub const FULL_NOTE: &str = "response_mode=full returns the complete reply.";
const ASK: &str = "Ask again with narrower needs for what is still open, the same anchors, and known ranges for what you already hold.";
const ASK_TICKET: &str = "The job is queued or running: send the same request again after ticket.retry_after_ms; the ticket keeps its place.";
const MAX_LABEL_BYTES: usize = 80;
const MAX_KEY_BYTES: usize = 60;
const MAX_NEED_TEXT_BYTES: usize = 80;
const MAX_NEXT: usize = 12;
const MAX_OMITTED: usize = 48;
const MAX_SEARCHED: usize = 4;
const MAX_DIFFERING: usize = 4;
/// Top-level fields the transport added beside the kernel reply.
const ADDED: [&str; 12] = [
    "request_id",
    "error",
    "delivery",
    "freshness",
    "discovery_readiness",
    "semantic_readiness",
    "discovery_observation",
    "indexing",
    "scope_observation",
    "read_scope",
    "guidance",
    "source_verification",
];
/// Of those, the ones a brief carries unchanged: the request identity, a
/// required-verification error, a job's delivery record and the status and
/// scope observations the session's status contract owns.
const CARRIED: [&str; 10] = [
    "request_id",
    "error",
    "delivery",
    "freshness",
    "discovery_readiness",
    "semantic_readiness",
    "discovery_observation",
    "indexing",
    "scope_observation",
    "read_scope",
];

/// The reply's publication, once: the token every cited location is bound to.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BriefPublication {
    pub token: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BriefResult {
    Complete,
    Partial,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BriefOrigin {
    Caller,
    Standing,
    Derived,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub enum BriefStatus {
    #[serde(rename = "covered")]
    Covered,
    #[serde(rename = "partly covered")]
    PartlyCovered,
    #[serde(rename = "open")]
    Open,
}

/// One place a sub-need is answered: `path:start-end` and why it is there.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BriefLocation {
    pub at: String,
    /// The passage's first line, the read that fetched it and its cause.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub why: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BriefNeed {
    /// Numbered from 1 in the reply's order; passages name needs by it.
    pub need: usize,
    pub text: String,
    pub origin: BriefOrigin,
    pub status: BriefStatus,
    #[serde(rename = "where", default, skip_serializing_if = "Vec::is_empty")]
    pub locations: Vec<BriefLocation>,
}

/// One delivered passage, byte for byte as the kernel delivered it. A
/// passage the kernel cut is kept to its last whole line (`clipped`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BriefPassage {
    pub at: String,
    pub needs: Vec<usize>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub clipped: bool,
    pub text: String,
}

/// What to ask for next.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BriefNext {
    /// Reads the kernel chose or proposed and did not run: `operation target`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub names: Vec<String>,
    /// Files the evidence names that no listed location opens.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub paths: Vec<String>,
    /// Kinds of evidence a part still lacks.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub gaps: Vec<String>,
    pub ask: String,
}

/// The terse public Gather Context reply.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct BriefReply {
    pub schema: String,
    pub task: String,
    pub publication: BriefPublication,
    pub result: BriefResult,
    /// Why the result is partial.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub why: Option<String>,
    /// The parts' stop reasons.
    pub stop: String,
    pub full: String,
    /// The kernel's ticket for a queued or running job, unchanged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ticket: Option<Value>,
    /// The kinds of the kernel's degradations, for example `context_absent`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub degradations: Vec<String>,
    pub needs: Vec<BriefNeed>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub passages: Vec<BriefPassage>,
    /// Plain statements for sub-needs with no evidence.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub not_found: Vec<String>,
    /// Cited locations whose text this brief does not carry.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub omitted: Vec<String>,
    /// How many more cited locations `omitted` leaves out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub omitted_more: Option<usize>,
    pub next: BriefNext,
    /// One line per gathering stage.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub gathering: Vec<String>,
    pub elapsed_ms: u64,
    /// One line summarizing the saved-source verification receipt.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verification: Option<String>,
    /// The reply's delivery bindings, unchanged (Core `combined_context`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub delivery_bindings: Vec<Value>,
    /// The per-read status, when the caller asked for status.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read_status: Option<Value>,
    /// Transport fields carried unchanged (`CARRIED`).
    #[serde(flatten)]
    pub carried: Map<String, Value>,
}

/// Replace a rendered Answer /3 reply with its brief. Any other value, and a
/// reply this projection cannot read, is left unchanged.
pub(crate) fn project(display: &mut Value) {
    if let Some(brief) = brief_of(display)
        && let Ok(value) = serde_json::to_value(&brief)
    {
        *display = value;
    }
}

/// The brief of a rendered Answer /3 reply.
pub fn brief_of(display: &Value) -> Option<BriefReply> {
    let object = display.as_object()?;
    if object.get("schema").and_then(Value::as_str) != Some(v3::CONTEXT_SCHEMA_V3) {
        return None;
    }
    let mut reply = Map::new();
    let mut carried = Map::new();
    for (key, value) in object {
        if CARRIED.contains(&key.as_str()) {
            carried.insert(key.clone(), value.clone());
        } else if !ADDED.contains(&key.as_str()) {
            reply.insert(key.clone(), value.clone());
        }
    }
    let reply: ContextResponseV3 = serde_json::from_value(Value::Object(reply)).ok()?;
    let bindings = airk_core::combined_context::reply_delivery_bindings(&reply)
        .ok()?
        .iter()
        .map(serde_json::to_value)
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    let verification = object.get("source_verification").map(verification_line);
    let view = View::new(&reply);
    let mut keep = view.selected.len();
    let mut level = 0;
    loop {
        let mut brief = view.compose(keep, level);
        brief.verification = verification.clone();
        brief.delivery_bindings = bindings.clone();
        brief.read_status = reply
            .read_status
            .as_ref()
            .map(|status| Value::Object(status.clone()));
        brief.carried = carried.clone();
        let bytes = serde_json::to_vec(&brief).map_or(usize::MAX, |bytes| bytes.len());
        if bytes <= MAX_BRIEF_BYTES {
            return Some(brief);
        }
        if keep > 0 {
            keep -= 1;
        } else if level < LAST_LEVEL {
            level += 1;
        } else {
            // Only carried status remains above the ceiling; it is never
            // dropped (its session acknowledged it), and the map is minimal.
            return Some(brief);
        }
    }
}

/// Map trimming levels once no passage is left: fewer locations per need,
/// fewer omitted and next entries, then bare locations and shorter texts.
const LAST_LEVEL: usize = 3;
const WHERE_AT: [usize; LAST_LEVEL + 1] = [WHERE_PER_NEED, 2, 1, 1];
const OMITTED_AT: [usize; LAST_LEVEL + 1] = [MAX_OMITTED, 16, 8, 4];
const NEXT_AT: [usize; LAST_LEVEL + 1] = [MAX_NEXT, 6, 3, 0];

/// One span row of a record's projection.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct RowId {
    record: usize,
    span: usize,
}

/// The text a location's passage would carry.
#[derive(Clone, Debug)]
struct Source {
    carrier: RowId,
    at: String,
    text: String,
    clipped: bool,
}

/// One sub-need as the brief reads it.
struct Need {
    part: u8,
    id: u8,
    text: String,
    origin: BriefOrigin,
    status: BriefStatus,
    evidence: BTreeSet<usize>,
    /// Cited locations, best first, with the row that names each.
    locations: Vec<(String, Option<RowId>)>,
    unrun: Vec<String>,
    omitted: u16,
}

struct View<'a> {
    reply: &'a ContextResponseV3,
    needs: Vec<Need>,
    /// Passages chosen inside the allowance, in choosing order: the source
    /// and the needs it serves.
    selected: Vec<(Source, Vec<usize>)>,
}

fn location_of(row: &Value) -> Option<String> {
    Some(format!(
        "{}:{}-{}",
        row.get("path")?.as_str()?,
        row.get("start_line")?.as_u64()?,
        row.get("end_line")?.as_u64()?
    ))
}

fn cut(text: &str, limit: usize) -> String {
    if text.len() <= limit {
        return text.to_owned();
    }
    let mut end = limit;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &text[..end])
}

fn name_of<T: Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_default()
}

impl<'a> View<'a> {
    fn new(reply: &'a ContextResponseV3) -> Self {
        let mut view = Self {
            reply,
            needs: Vec::new(),
            selected: Vec::new(),
        };
        let ids: BTreeMap<String, usize> = reply
            .records
            .iter()
            .enumerate()
            .map(|(index, record)| (record.id.to_string(), index))
            .collect();
        let mut by_location: BTreeMap<String, Vec<RowId>> = BTreeMap::new();
        for record in 0..reply.records.len() {
            for (span, value) in view.spans(record).iter().enumerate() {
                if let Some(at) = location_of(value) {
                    by_location
                        .entry(at)
                        .or_default()
                        .push(RowId { record, span });
                }
            }
        }
        for part in &reply.parts {
            for need in &part.sub_needs {
                let evidence: BTreeSet<usize> = need
                    .evidence
                    .iter()
                    .filter_map(|id| ids.get(&id.to_string()).copied())
                    .collect();
                let mut locations: Vec<(String, Option<RowId>)> = Vec::new();
                let mut seen = BTreeSet::new();
                let row_for = |at: &str| {
                    let rows = by_location.get(at)?;
                    rows.iter()
                        .find(|row| evidence.contains(&row.record))
                        .or_else(|| rows.first())
                        .copied()
                };
                // The kernel's own list for this sub-need, best first.
                for at in paragraph_locations(reply, part.id, need.id) {
                    if seen.insert(at.clone()) {
                        let row = row_for(&at);
                        locations.push((at, row));
                    }
                }
                // Without one (a compact reply), the spans of its records.
                if locations.is_empty() {
                    for record in need
                        .evidence
                        .iter()
                        .filter_map(|id| ids.get(&id.to_string()).copied())
                    {
                        for (span, value) in view.spans(record).iter().enumerate() {
                            if let Some(at) = location_of(value)
                                && seen.insert(at.clone())
                            {
                                locations.push((at, Some(RowId { record, span })));
                            }
                        }
                    }
                }
                view.needs.push(Need {
                    part: part.id,
                    id: need.id,
                    text: need.text.clone(),
                    origin: match need.origin {
                        v3::SubNeedOrigin::Caller => BriefOrigin::Caller,
                        v3::SubNeedOrigin::Standing => BriefOrigin::Standing,
                        v3::SubNeedOrigin::Derived => BriefOrigin::Derived,
                    },
                    status: match need.coverage {
                        v3::Coverage::Covered => BriefStatus::Covered,
                        v3::Coverage::Partial => BriefStatus::PartlyCovered,
                        v3::Coverage::Missing => BriefStatus::Open,
                    },
                    evidence,
                    locations,
                    unrun: need
                        .unrun_reads
                        .iter()
                        .map(|read| format!("{} {}", name_of(&read.operation), read.target))
                        .collect(),
                    omitted: need.evidence_omitted,
                });
            }
        }
        view.select();
        view
    }

    fn spans(&self, record: usize) -> &'a [Value] {
        self.reply.records[record].canonical_result["spans"]
            .as_array()
            .map_or(&[], Vec::as_slice)
    }

    fn row(&self, id: RowId) -> &'a Value {
        &self.spans(id.record)[id.span]
    }

    /// The row that carries a row's text: itself, or the row its `same_as`
    /// names (followed a bounded number of times).
    fn carrier(&self, mut id: RowId) -> Option<RowId> {
        for _ in 0..8 {
            let row = self.row(id);
            let Some(name) = row.get("same_as") else {
                return Some(id);
            };
            let record = self.reply.records.iter().position(|record| {
                Some(record.id.to_string().as_str()) == name["record"].as_str()
            })?;
            let span = self
                .spans(record)
                .iter()
                .position(|row| row["pointer"] == name["pointer"])?;
            id = RowId { record, span };
        }
        None
    }

    /// The passage a row stands for: its carrier's whole delivered text. A
    /// carrier trimmed of an overlap does not hold its own lines whole, so it
    /// is no passage; a cut text keeps its whole lines only.
    fn source(&self, id: RowId) -> Option<Source> {
        let carrier = self.carrier(id)?;
        let row = self.row(carrier);
        if row
            .get("overlaps")
            .and_then(Value::as_array)
            .is_some_and(|rows| !rows.is_empty())
        {
            return None;
        }
        let text = row.get("text")?.as_str()?;
        let path = row.get("path")?.as_str()?;
        let start = row.get("start_line")?.as_u64()?;
        let end = row.get("end_line")?.as_u64()?;
        if text.trim().is_empty() {
            return None;
        }
        if row.get("truncated").and_then(Value::as_bool) != Some(true) {
            return Some(Source {
                carrier,
                at: format!("{path}:{start}-{end}"),
                text: text.to_owned(),
                clipped: false,
            });
        }
        let whole = &text[..=text.rfind('\n')?];
        let lines = u64::try_from(whole.matches('\n').count()).ok()?;
        Some(Source {
            carrier,
            at: format!("{path}:{start}-{}", (start + lines - 1).min(end)),
            text: whole.to_owned(),
            clipped: true,
        })
    }

    /// Passages inside the allowance: each sub-need's best location first,
    /// then each one's second, and so on through its listed locations. A
    /// passage two needs share is sent once; one that does not fit is
    /// omitted and the next is tried.
    fn select(&mut self) {
        let lists: Vec<Vec<Source>> = self
            .needs
            .iter()
            .map(|need| {
                need.locations
                    .iter()
                    .take(WHERE_PER_NEED)
                    .filter_map(|(_, row)| self.source((*row)?))
                    .collect()
            })
            .collect();
        let deepest = lists.iter().map(Vec::len).max().unwrap_or(0);
        let mut used = 0usize;
        let mut skipped = BTreeSet::new();
        let mut selected: Vec<(Source, Vec<usize>)> = Vec::new();
        for rank in 0..deepest {
            for (index, list) in lists.iter().enumerate() {
                let Some(source) = list.get(rank) else {
                    continue;
                };
                let number = index + 1;
                if let Some((_, needs)) = selected
                    .iter_mut()
                    .find(|(chosen, _)| chosen.carrier == source.carrier)
                {
                    if !needs.contains(&number) {
                        needs.push(number);
                    }
                    continue;
                }
                if skipped.contains(&source.carrier)
                    || used + source.text.len() > BRIEF_PASSAGE_BYTES
                {
                    skipped.insert(source.carrier);
                    continue;
                }
                used += source.text.len();
                selected.push((source.clone(), vec![number]));
            }
        }
        for (_, needs) in &mut selected {
            needs.sort_unstable();
        }
        self.selected = selected;
    }

    /// Why a record is cited: the read that fetched it, its cause for this
    /// sub-need and its role, as the reply records them.
    fn reason(&self, record: usize, need: &Need) -> String {
        let evidence = &self.reply.records[record];
        let mut text = name_of(&evidence.operation);
        let arguments = &evidence.arguments;
        let key = arguments
            .get("query")
            .and_then(Value::as_str)
            .map(|query| format!("\"{}\"", cut(query, MAX_KEY_BYTES)))
            .or_else(|| {
                ["name", "symbol"]
                    .iter()
                    .find_map(|key| arguments.get(*key).and_then(Value::as_str))
                    .map(|name| cut(name, MAX_KEY_BYTES))
            })
            .or_else(|| {
                matches!(
                    evidence.operation,
                    v3::LoopRead::Outline | v3::LoopRead::Imports | v3::LoopRead::DocumentSections
                )
                .then(|| arguments.get("path").and_then(Value::as_str))
                .flatten()
                .map(|path| cut(path, MAX_KEY_BYTES))
            });
        if let Some(key) = key {
            text.push(' ');
            text.push_str(&key);
        }
        if let Some(kind) = arguments.get("kind").and_then(Value::as_str) {
            text.push_str(&format!(" ({kind})"));
        }
        let uses: Vec<&v3::RecordUse> = self
            .reply
            .record_uses
            .iter()
            .filter(|item| item.record == evidence.id && item.part == need.part)
            .collect();
        let cause = uses
            .iter()
            .find(|item| item.need == Some(need.id))
            .or_else(|| uses.first())
            .map(|item| match item.cause {
                v3::UseCause::PartSearch => "task search",
                v3::UseCause::NeedSearch => "need search",
                v3::UseCause::StructuralRead => "structure read",
                v3::UseCause::FollowUp => "follow-up read",
                v3::UseCause::SharedRead => "shared read",
            });
        if let Some(cause) = cause {
            text.push_str(", ");
            text.push_str(cause);
        }
        let role = name_of(&evidence.role);
        if !role.is_empty() && role != "unknown" {
            text.push_str(", ");
            text.push_str(&role);
        }
        text
    }

    fn location(&self, need: &Need, at: &str, row: Option<RowId>, level: usize) -> BriefLocation {
        let mut why = String::new();
        if level < LAST_LEVEL
            && let Some(row) = row
        {
            let label = self.source(row).map(|source| source.text).and_then(|text| {
                text.lines()
                    .map(str::trim)
                    .find(|line| !line.is_empty())
                    .map(str::to_owned)
            });
            if let Some(label) = label {
                why.push_str(&cut(&label, MAX_LABEL_BYTES));
                why.push_str(" | ");
            }
            why.push_str(&self.reason(row.record, need));
        }
        BriefLocation { at: at.into(), why }
    }

    /// What the reply records as searched for a sub-need.
    fn searched(&self, need: &Need) -> Vec<String> {
        let mut out = Vec::new();
        for (index, record) in self.reply.records.iter().enumerate() {
            if out.len() >= MAX_SEARCHED {
                break;
            }
            let used = need.evidence.contains(&index)
                || self.reply.record_uses.iter().any(|item| {
                    item.record == record.id && item.part == need.part && item.need == Some(need.id)
                });
            if !used {
                continue;
            }
            let mut text = self.reason(index, need);
            if record.canonical_result["hits_total"].as_u64() == Some(0)
                && record.canonical_result["rows_total"].as_u64().unwrap_or(0) == 0
            {
                text.push_str(", no hits");
            }
            if !out.contains(&text) {
                out.push(text);
            }
        }
        out
    }

    fn compose(&self, keep: usize, level: usize) -> BriefReply {
        let reply = self.reply;
        let selected = &self.selected[..keep.min(self.selected.len())];
        let carried: BTreeSet<RowId> = selected.iter().map(|(source, _)| source.carrier).collect();
        let carries = |row: Option<RowId>| {
            row.and_then(|row| self.carrier(row))
                .is_some_and(|carrier| carried.contains(&carrier))
        };
        let needs: Vec<BriefNeed> = self
            .needs
            .iter()
            .enumerate()
            .map(|(index, need)| BriefNeed {
                need: index + 1,
                text: if level >= LAST_LEVEL {
                    cut(&need.text, 160)
                } else {
                    need.text.clone()
                },
                origin: need.origin,
                status: need.status,
                locations: need
                    .locations
                    .iter()
                    .take(WHERE_AT[level])
                    .map(|(at, row)| self.location(need, at, *row, level))
                    .collect(),
            })
            .collect();
        let passages = selected
            .iter()
            .map(|(source, needs)| BriefPassage {
                at: source.at.clone(),
                needs: needs.clone(),
                clipped: source.clipped,
                text: source.text.clone(),
            })
            .collect();
        let mut not_found = Vec::new();
        for (index, need) in self.needs.iter().enumerate() {
            if !need.locations.is_empty() {
                continue;
            }
            let mut text = format!(
                "need {}: nothing found for \"{}\"",
                index + 1,
                cut(&need.text, MAX_NEED_TEXT_BYTES)
            );
            let searched = self.searched(need);
            if searched.is_empty() {
                text.push_str("; the reply records no read for it");
            } else {
                text.push_str("; searched: ");
                text.push_str(&searched.join("; "));
            }
            if need.omitted > 0 {
                text.push_str(&format!(
                    "; {} cited records were left out of the reply frame",
                    need.omitted
                ));
            }
            not_found.push(text);
        }
        let mut omitted = Vec::new();
        let mut listed = BTreeSet::new();
        for need in &self.needs {
            for (at, row) in &need.locations {
                if !carries(*row) && listed.insert(at.clone()) {
                    omitted.push(at.clone());
                }
            }
        }
        let omitted_more = omitted
            .len()
            .checked_sub(OMITTED_AT[level])
            .filter(|more| *more > 0);
        omitted.truncate(OMITTED_AT[level]);
        let opened: BTreeSet<&str> = self
            .needs
            .iter()
            .flat_map(|need| &need.locations)
            .filter_map(|(at, _)| at.rsplit_once(':').map(|(path, _)| path))
            .collect();
        let mut names = Vec::new();
        for need in &self.needs {
            for read in &need.unrun {
                if !names.contains(read) {
                    names.push(read.clone());
                }
            }
        }
        names.truncate(NEXT_AT[level]);
        let mut paths: Vec<String> = Vec::new();
        for record in &reply.records {
            for row in record.canonical_result["rows"]
                .as_array()
                .into_iter()
                .flatten()
            {
                if let Some(path) = row["path"].as_str()
                    && !opened.contains(path)
                    && !paths.iter().any(|known| known == path)
                {
                    paths.push(path.into());
                }
            }
        }
        paths.truncate(NEXT_AT[level]);
        let several = reply.parts.len() > 1;
        let mut gaps = Vec::new();
        for part in &reply.parts {
            for gap in &part.gaps {
                let words = match gap {
                    airk_core::arbiter::Gap::Sufficient => continue,
                    airk_core::arbiter::Gap::Definition => "a definition",
                    airk_core::arbiter::Gap::Constraint => "a constraint or rule",
                    airk_core::arbiter::Gap::Example => "an example",
                    airk_core::arbiter::Gap::Test => "a test",
                    airk_core::arbiter::Gap::Counterevidence => "counterevidence",
                    airk_core::arbiter::Gap::Ambiguous => "the task is ambiguous",
                    airk_core::arbiter::Gap::Unsupported => "unsupported by the index",
                };
                gaps.push(if several {
                    format!("part {}: {words}", part.id)
                } else {
                    words.into()
                });
            }
        }
        let (result, why) = outcome(reply);
        let mut stops: Vec<String> = Vec::new();
        for part in &reply.parts {
            let stop = name_of(&part.stop);
            if !stops.contains(&stop) {
                stops.push(stop);
            }
        }
        let stop = if stops.is_empty() {
            if reply.ticket.is_some() {
                "queued"
            } else {
                "none"
            }
            .to_owned()
        } else {
            stops.join(", ")
        };
        let mut degradations: Vec<String> = Vec::new();
        for degradation in &reply.degradations {
            if let Some(kind) = serde_json::to_value(degradation)
                .ok()
                .and_then(|value| value["kind"].as_str().map(str::to_owned))
                && !degradations.contains(&kind)
            {
                degradations.push(kind);
            }
        }
        let gathering = reply
            .gathering
            .iter()
            .flat_map(|report| &report.stages)
            .map(|stage| {
                format!(
                    "{}: {} reads, {} delivered, {} ms",
                    name_of(&stage.stage),
                    stage.reads.iter().map(|reads| reads.count).sum::<usize>(),
                    stage.passages_delivered,
                    stage.elapsed_ms
                )
            })
            .collect();
        BriefReply {
            schema: BRIEF_SCHEMA.into(),
            task: cut(&reply.question, MAX_TASK_BYTES),
            publication: BriefPublication {
                token: reply.scope.publication.token.to_string(),
            },
            result,
            why,
            stop,
            full: FULL_NOTE.into(),
            ticket: reply
                .ticket
                .as_ref()
                .and_then(|ticket| serde_json::to_value(ticket).ok()),
            degradations,
            needs,
            passages,
            not_found,
            omitted,
            omitted_more,
            next: BriefNext {
                names,
                paths,
                gaps,
                ask: if reply.ticket.is_some() {
                    ASK_TICKET
                } else {
                    ASK
                }
                .into(),
            },
            gathering,
            elapsed_ms: reply.elapsed_ms,
            verification: None,
            delivery_bindings: Vec::new(),
            read_status: None,
            carried: Map::new(),
        }
    }
}

/// Whether the reply is complete, and why not.
fn outcome(reply: &ContextResponseV3) -> (BriefResult, Option<String>) {
    if let Some(marker) = &reply.partial {
        let mut why = format!("{} at {}", name_of(&marker.reason), name_of(&marker.stage));
        if let Some(part) = marker.part {
            why.push_str(&format!(" (part {part})"));
        }
        return (BriefResult::Partial, Some(why));
    }
    let cut: Vec<String> = reply
        .parts
        .iter()
        .filter(|part| {
            matches!(
                part.stop,
                v3::PartStop::Budget
                    | v3::PartStop::Deadline
                    | v3::PartStop::Stale
                    | v3::PartStop::Refused
                    | v3::PartStop::Unsupported
            )
        })
        .map(|part| format!("part {} stopped: {}", part.id, name_of(&part.stop)))
        .collect();
    if !cut.is_empty() {
        return (BriefResult::Partial, Some(cut.join("; ")));
    }
    if !reply.omitted_evidence.is_empty() {
        return (
            BriefResult::Partial,
            Some(format!(
                "the reply frame left out {} cited records",
                reply.omitted_evidence.len()
            )),
        );
    }
    (BriefResult::Complete, None)
}

/// The locations a sub-need's paragraph lists (`- path:start-end (round …`),
/// in the kernel's order.
fn paragraph_locations(reply: &ContextResponseV3, part: u8, need: u8) -> Vec<String> {
    let heading = format!("Part {part} sub-need {need} (");
    let Some(paragraph) = reply
        .paragraphs
        .iter()
        .find(|paragraph| paragraph.text.starts_with(&heading))
    else {
        return Vec::new();
    };
    paragraph
        .text
        .lines()
        .skip(1)
        .filter_map(|line| {
            let rest = line.strip_prefix("- ")?;
            let (at, _) = rest.split_once(" (round ")?;
            let (path, lines) = at.rsplit_once(':')?;
            let (start, end) = lines.split_once('-')?;
            (!path.is_empty() && start.parse::<u64>().is_ok() && end.parse::<u64>().is_ok())
                .then(|| at.to_owned())
        })
        .collect()
}

/// One line for a saved-source verification receipt: its status, sources
/// checked, matched, differing and not checked, and the differing paths.
fn verification_line(receipt: &Value) -> String {
    let counts = &receipt["counts"];
    let number = |key: &str| counts[key].as_u64().unwrap_or(0);
    let checked = number("checked_sources");
    let unchecked = counts["unchecked_sources"].as_u64().unwrap_or_else(|| {
        counts["requested_sources"]
            .as_u64()
            .or_else(|| counts["requested_sources_lower_bound"].as_u64())
            .unwrap_or(0)
            .saturating_sub(checked)
    });
    let mut line = format!(
        "{}: {checked} sources checked, {} matched, {} differ, {unchecked} not checked",
        receipt["status"].as_str().unwrap_or("unknown"),
        number("matched_sources"),
        number("mismatched_sources"),
    );
    let differing: Vec<&str> = receipt["sources"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|source| {
            matches!(
                source["state"].as_str(),
                Some("mismatch" | "missing" | "changed_during_check")
            )
        })
        .filter_map(|source| source["path"].as_str())
        .take(MAX_DIFFERING)
        .collect();
    if !differing.is_empty() {
        line.push_str(&format!(" (differ: {})", differing.join(", ")));
    }
    line
}
