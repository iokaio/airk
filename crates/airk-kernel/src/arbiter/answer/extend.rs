// SPDX-License-Identifier: Apache-2.0
//! Rules 3 and 4 of the gathering rules (kernel CONTRACT "Gathering rules"):
//! once round one's searches and recipes have run, the loop reads the
//! project's own guidance files, the files the evidence names by path, and
//! the code structure around each sub-need's best passages. Each phase plans
//! from retained records only, its private lookups are charged as calls, and
//! every passage it adds ends in an ordinary hash-bound read on the same
//! publication, so it is citable like any other.
use super::coverage::Span;
use super::gather::{Cause, Step};
use super::*;
use software::FollowUp;
use std::collections::BTreeMap;
use v3::GatheringStageKind as Kind;

/// Files a project keeps its own working rules in.
pub(crate) const GUIDANCE: [&str; 4] = ["CLAUDE.md", "AGENTS.md", "CONTRIBUTING.md", "README.md"];
/// Folders whose guidance files are looked for: the root, top-level folders
/// and the folders of the best passages, in that order.
const GUIDANCE_FOLDERS: usize = 6;
/// Guidance files read in one Answer.
const GUIDANCE_FILES: usize = 8;
/// Existence probes (`file_info`) of guidance names in one Answer: each
/// name in each guidance folder.
const GUIDANCE_PROBES: usize = GUIDANCE_FOLDERS * GUIDANCE.len();
/// Sections read of one guidance file, and of all of them in one Answer.
const SECTIONS_PER_FILE: usize = 3;
const GUIDANCE_SECTIONS: usize = 12;
/// Section facts one guidance file's lookup returns, and their bytes.
const SECTION_ROWS: usize = 64;
const SECTION_FACT_BYTES: usize = 4096;
/// One guidance section is read with `context` from its heading, at most
/// `HEAD_AFTER` lines after it.
const SECTION_BYTES: usize = 4096;
/// Paths one code-fact page returns.
const LISTING_LIMIT: usize = 200;
const LISTING_BYTES: usize = 16_384;
/// A guidance or cited file is read from its first line: `context` after 100
/// lines, the canonical radius bound.
const HEAD_AFTER: usize = airk_core::native_reads::CONTEXT_RADIUS_MAX;
const HEAD_BYTES: usize = 6_144;
/// Files the evidence names that one Answer reads.
const CITED_FILES: usize = 6;
/// Best passages per sub-need the guidance folders and cited paths come from.
const BEST_PASSAGES: usize = 3;
/// Best code passages per sub-need whose structure is followed.
const STRUCTURE_PASSAGES: usize = 2;
/// Files whose structure one Answer follows.
const STRUCTURE_FILES: usize = 6;
/// Private structure lookups in one Answer.
const STRUCTURE_LOOKUPS: usize = 24;
/// A definition longer than this is not read whole.
const FUNCTION_LINES: usize = 200;
/// Ranges of one file closer than this are read together.
const MERGE_GAP: usize = 20;
/// One structure read spans at most this many lines.
const READ_LINES: usize = 240;
const READ_BYTES: usize = 8_192;
/// Callee and caller names followed per sub-need.
const CALLEES: usize = 3;
const CALLERS: usize = 2;
/// Call sites read per caller name, lines around each.
const CALL_SITES: usize = 3;
const CALL_MARGIN: usize = 12;
/// Inherited type names per file and their users read, lines around each.
const BASES: usize = 2;
const USERS: usize = 3;
const USER_MARGIN: usize = 15;
/// Retention bytes left for later rounds after the gathering reads.
const RETENTION_RESERVE_SHARE: usize = 4;
/// Round one's recipe reads (whole chunks, outlines, excerpts) leave
/// `max_bytes / ROUND_ONE_KEEPS` free for these rules and the later rounds
/// (`Gathering::recipe_floor`); its searches are never held back.
pub(super) const ROUND_ONE_KEEPS: usize = 2;
/// The same reads leave these rules `min(calls, candidates) / 4` (at most
/// `GATHERING_READS_PER_PART`) beside one follow-up read per sub-need and
/// later round.
const GATHERING_CALL_SHARE: usize = 4;

/// Calls and candidates round one's recipe reads and nominations leave for
/// rules 3 and 4 and the later rounds' follow-up reads.
pub(super) fn reserved_calls(request: &v3::ContextRequestV3, parts: &[PartState]) -> usize {
    let budget = &request.budget;
    let needs: usize = parts.iter().map(|part| part.needs.len()).sum();
    let follow_ups =
        needs * budget.rounds.saturating_sub(1) * v3::FOLLOW_UP_READS_PER_SUB_NEED_ROUND;
    let gathering = v3::GATHERING_READS_PER_PART
        .min(budget.calls.min(budget.candidates) / GATHERING_CALL_SHARE);
    follow_ups + gathering
}

/// The share of the bytes above the later rounds' reserve a stage may
/// plan, of what is left when it begins: a third for guidance, half the
/// rest for cited files, and all that remains for structure, so no stage
/// can spend another's.
fn stage_share(kind: Kind, available: usize) -> usize {
    match kind {
        Kind::Guidance => available / 3,
        Kind::Cited => available / 2,
        _ => available,
    }
}

/// One code fact of a lookup: its name, kind and citable lines.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Fact {
    pub(crate) name: String,
    pub(crate) kind: String,
    pub(crate) path: String,
    pub(crate) start: usize,
    pub(crate) end: usize,
    pub(crate) hash: Option<String>,
}

/// The facts of a code-fact page, minimal or rich. A minimal row names its
/// file by index (`file`, absent for a single exact file); a rich one carries
/// its own citation.
pub(crate) fn facts(body: &Value) -> Vec<Fact> {
    let page = body
        .get("minimal")
        .filter(|page| page.is_object())
        .unwrap_or(body);
    let files = page.get("files").and_then(Value::as_array);
    let rows = page
        .get("items")
        .or_else(|| page.get("facts"))
        .and_then(Value::as_array);
    let mut out = Vec::new();
    for row in rows.into_iter().flatten() {
        let name = row["name"].as_str().unwrap_or("").to_owned();
        let kind = row["kind"].as_str().unwrap_or("").to_owned();
        let fact = if let Some(lines) = row.get("lines").and_then(Value::as_array) {
            let file = files.and_then(|files| {
                files.get(row.get("file").and_then(Value::as_u64).unwrap_or(0) as usize)
            });
            Fact {
                name,
                kind,
                path: file
                    .and_then(|file| file["path"].as_str())
                    .unwrap_or("")
                    .to_owned(),
                start: lines.first().and_then(Value::as_u64).unwrap_or(0) as usize,
                end: lines.get(1).and_then(Value::as_u64).unwrap_or(0) as usize,
                hash: file
                    .and_then(|file| file["content_hash"].as_str())
                    .map(str::to_owned),
            }
        } else {
            let citation = &row["citation"];
            Fact {
                name: if name.is_empty() {
                    row["text"].as_str().unwrap_or("").to_owned()
                } else {
                    name
                },
                kind: if kind.is_empty() {
                    row["fact"]["kind"].as_str().unwrap_or("").to_owned()
                } else {
                    kind
                },
                path: citation["path"].as_str().unwrap_or("").to_owned(),
                start: citation["start_line"].as_u64().unwrap_or(0) as usize,
                end: citation["end_line"].as_u64().unwrap_or(0) as usize,
                hash: citation["content_hash"].as_str().map(str::to_owned),
            }
        };
        if crate::workbench::safe_path(&fact.path) && fact.start > 0 && fact.end >= fact.start {
            out.push(fact);
        }
    }
    out
}

/// The name a call fact calls: its last path or member segment, without
/// generic arguments or a call's parentheses.
pub(crate) fn terminal(name: &str) -> &str {
    let name = name.split('(').next().unwrap_or(name);
    let name = name.split('<').next().unwrap_or(name);
    name.rsplit(['.', ':', '>', '?'])
        .find(|segment| !segment.trim().is_empty())
        .unwrap_or(name)
        .trim()
}

fn followable(name: &str) -> bool {
    name.len() >= 4
        && name.len() <= 128
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        && name
            .bytes()
            .next()
            .is_some_and(|byte| !byte.is_ascii_digit())
}

/// Repository paths a text names: tokens of path characters with at least
/// one folder and a file extension, safe and relative.
pub(crate) fn cited_paths(texts: &[&str], limit: usize) -> Vec<String> {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for text in texts {
        for token in text
            .split(|ch: char| !(ch.is_ascii_alphanumeric() || matches!(ch, '_' | '.' | '/' | '-')))
        {
            // A sentence's full stop is not part of the path; a leading dot
            // folder (`.github/`) is.
            let token = token.trim_end_matches('.');
            let token = token.strip_prefix("./").unwrap_or(token);
            let Some((folders, file)) = token.rsplit_once('/') else {
                continue;
            };
            let Some((stem, extension)) = file.rsplit_once('.') else {
                continue;
            };
            if folders.is_empty()
                || folders.starts_with('/')
                || folders
                    .split('/')
                    .any(|part| part.is_empty() || part == "..")
                || stem.is_empty()
                || !(1..=8).contains(&extension.len())
                || !extension.bytes().all(|byte| byte.is_ascii_alphanumeric())
                || !crate::workbench::safe_path(token)
                || airk_core::index_catalog::relative_path(token, false).is_err()
            {
                continue;
            }
            *counts.entry(token.to_owned()).or_default() += 1;
        }
    }
    let mut ranked: Vec<(String, usize)> = counts.into_iter().collect();
    ranked.sort_by(|left, right| right.1.cmp(&left.1).then(left.0.cmp(&right.0)));
    ranked
        .into_iter()
        .take(limit)
        .map(|(path, _)| path)
        .collect()
}

fn folder(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(parent, _)| parent)
}

/// The guidance folders of a set of passage paths: the root, their
/// top-level folders and their own folders, in that order, at most `limit`.
pub(crate) fn guidance_folders(paths: &[&str], limit: usize) -> Vec<String> {
    let mut out = vec![String::new()];
    for path in paths {
        let top = path.split('/').next().unwrap_or("");
        if top != *path && !out.iter().any(|known| known == top) {
            out.push(top.to_owned());
        }
    }
    for path in paths {
        let own = folder(path);
        if !own.is_empty() && !out.iter().any(|known| known == own) {
            out.push(own.to_owned());
        }
    }
    out.truncate(limit);
    out
}

/// Words that say little about which section holds a rule.
const COMMON: [&str; 16] = [
    "that", "this", "with", "what", "which", "where", "from", "into", "they", "their", "there",
    "when", "each", "have", "does", "done",
];

/// The distinct lower-case words of four or more letters of a text.
pub(crate) fn words(text: &str) -> BTreeSet<String> {
    text.split(|ch: char| !ch.is_alphanumeric())
        .filter(|word| word.chars().count() >= 4)
        .map(str::to_lowercase)
        .filter(|word| !COMMON.contains(&word.as_str()))
        .collect()
}

/// The sections of a guidance file of `lines` lines, from its section facts:
/// each runs from its heading to the line before the next (the last to the
/// file's end), and lines before the first heading are a section without
/// one. Without facts the file is one section.
pub(crate) fn section_spans(
    path: &str,
    facts: &[Fact],
    lines: usize,
) -> Vec<(usize, usize, String)> {
    let mut headings: Vec<(usize, String)> = facts
        .iter()
        .filter(|fact| {
            fact.path == path
                && fact.start <= lines
                && matches!(fact.kind.as_str(), "section" | "heading" | "")
        })
        .map(|fact| (fact.start, fact.name.clone()))
        .collect();
    headings.sort();
    headings.dedup_by_key(|(start, _)| *start);
    if headings.first().is_none_or(|(start, _)| *start > 1) {
        headings.insert(0, (1, String::new()));
    }
    let ends: Vec<usize> = headings
        .iter()
        .skip(1)
        .map(|(start, _)| start - 1)
        .chain([lines.max(1)])
        .collect();
    headings
        .into_iter()
        .zip(ends)
        .map(|((start, name), end)| (start, end.max(start), name))
        .collect()
}

/// Up to `SECTIONS_PER_FILE` sections to read, each with the sub-need it
/// is for: first those whose heading shares most words with a sub-need
/// (that sub-need), then the rest in document order (`owner`).
pub(crate) fn choose_sections(
    spans: &[(usize, usize, String)],
    needs: &[(u8, BTreeSet<String>)],
    owner: u8,
) -> Vec<(usize, usize, u8)> {
    let mut scored: Vec<(usize, usize, usize, u8)> = spans
        .iter()
        .filter_map(|(start, end, name)| {
            let heading = words(name);
            needs
                .iter()
                .map(|(need, wanted)| (heading.intersection(wanted).count(), *need))
                .filter(|(score, _)| *score > 0)
                .max_by(|left, right| left.0.cmp(&right.0).then(right.1.cmp(&left.1)))
                .map(|(score, need)| (score, *start, *end, need))
        })
        .collect();
    scored.sort_by(|left, right| right.0.cmp(&left.0).then(left.1.cmp(&right.1)));
    let mut out: Vec<(usize, usize, u8)> = scored
        .into_iter()
        .take(SECTIONS_PER_FILE)
        .map(|(_, start, end, need)| (start, end, need))
        .collect();
    for (start, end, _) in spans {
        if out.len() >= SECTIONS_PER_FILE {
            break;
        }
        if !out.iter().any(|(known, _, _)| known == start) {
            out.push((*start, *end, owner));
        }
    }
    out
}

/// Line ranges of one file merged where closer than `MERGE_GAP`, each at
/// most `READ_LINES` long; the owning sub-need is the first range's.
fn merge(mut ranges: Vec<(usize, usize, u8)>) -> Vec<(usize, usize, u8)> {
    ranges.sort();
    let mut out: Vec<(usize, usize, u8)> = Vec::new();
    for (start, end, need) in ranges {
        let end = end.min(start + READ_LINES - 1);
        if let Some(last) = out.last_mut()
            && start <= last.1 + MERGE_GAP
            && end.max(last.1) + 1 - last.0 <= READ_LINES
        {
            last.1 = last.1.max(end);
            continue;
        }
        out.push((start, end, need));
    }
    out
}

/// What one sub-need's structure following will read.
#[derive(Default)]
struct Wanted {
    /// Definition ranges, by file: start, end, sub-need.
    ranges: BTreeMap<String, Vec<(usize, usize, u8)>>,
    hashes: BTreeMap<String, String>,
    /// Places read with lines around them: path, line, after, sub-need.
    around: Vec<(String, usize, usize, usize, u8)>,
}

impl Loop<'_> {
    /// The judgments taken so far: Focus pairs dispatched and card decisions
    /// a model scored.
    pub(super) fn judgments(&self) -> usize {
        self.focus_pairs
            + self
                .ledger
                .entries
                .iter()
                .filter(|entry| entry.decision.scored_by_model())
                .count()
    }

    pub(super) fn begin_stage(&mut self, kind: Kind) {
        let (now, judgments) = (self.now(), self.judgments());
        self.report.begin(kind, now, judgments);
        self.gather.stage = kind;
    }

    /// Reads and lookups a part may still spend on gathering: within its
    /// share and the Answer's calls and candidates, after one follow-up read
    /// per sub-need and later round, and within `GATHERING_READS_PER_PART`.
    pub(super) fn gathering_room(&self, part: usize) -> usize {
        let id = self.parts[part].id;
        let share = self.shares.parts[part]
            .calls
            .saturating_sub(self.used_calls[part])
            .saturating_sub(self.gather.queued_for(id));
        let reserve = self.parts[part].needs.len()
            * self.request.budget.rounds.saturating_sub(1)
            * v3::FOLLOW_UP_READS_PER_SUB_NEED_ROUND;
        share
            .min(self.gather.room())
            .saturating_sub(reserve)
            .min(v3::GATHERING_READS_PER_PART.saturating_sub(self.gathered[part]))
    }

    /// Whether `bytes` more can be planned within the stage's own allowance,
    /// which always leaves later rounds a quarter of the retention budget.
    /// A refusal is noted as the stage's reason if it reads nothing.
    fn bytes_fit(&mut self, planned: usize, bytes: usize) -> bool {
        let floor = self
            .stage_floor
            .max(self.request.budget.max_bytes / RETENTION_RESERVE_SHARE);
        let fits = self.gather.remaining_bytes() >= planned + bytes + floor;
        if !fits {
            self.starved.get_or_insert(v3::GatheringSkip::NoBytes);
        }
        fits
    }

    /// The part's gathering room; none left is noted as the stage's reason
    /// if it reads nothing.
    fn room_left(&mut self, part: usize) -> bool {
        let left = self.gathering_room(part) > 0;
        if !left {
            self.starved.get_or_insert(v3::GatheringSkip::NoCalls);
        }
        left
    }

    /// One private lookup through the canonical reader, charged as a call of
    /// the part and its round and kept in custody beside the nominations. A
    /// lookup chooses reads; it is never evidence.
    fn lookup(
        &mut self,
        part: usize,
        basis: Sha256Digest,
        tool: &'static str,
        arguments: Value,
    ) -> Result<Option<Value>> {
        let arguments = arguments
            .as_object()
            .cloned()
            .context("lookup arguments are not an object")?;
        let key = gather::read_key(tool, &arguments, &self.gather.revision)?;
        if let Some(known) = self.lookups.get(&key) {
            return Ok(known.outcome.get("body").cloned());
        }
        if self.gather.stop.is_some() || self.stop_due() || !self.room_left(part) {
            return Ok(None);
        }
        let max_bytes = arguments
            .get("max_bytes")
            .and_then(Value::as_u64)
            .unwrap_or(4096) as usize;
        if !self.bytes_fit(0, max_bytes) {
            return Ok(None);
        }
        if !self.gather.reserve_nomination() {
            self.starved.get_or_insert(v3::GatheringSkip::NoCalls);
            return Ok(None);
        }
        self.used_calls[part] += 1;
        self.gathered[part] += 1;
        let round = self.parts[part].current();
        round.fanout = round.fanout.max(1);
        round.calls += 1;
        self.report.lookup(self.gather.stage);
        let read = nominate::Read::Canonical {
            tool,
            arguments: arguments.clone(),
        };
        let outcome = self.run_nomination(&read, tool, &arguments, max_bytes)?;
        let ordinal = self.nominations.len() + self.lookups.len() + 1;
        let body = outcome.get("body").cloned();
        self.lookups.insert(
            key,
            NominationObservation {
                ordinal,
                tool,
                arguments,
                basis: vec![basis],
                outcome,
            },
        );
        Ok(body)
    }

    /// Offer a gathering read outside its sub-need's round-one recipe cap,
    /// counted against the part's gathering room instead.
    fn offer_gathered(&mut self, part: usize, dispatch: gather::Dispatch) -> Result<bool> {
        if self.gather.known(&dispatch.key) || !self.room_left(part) {
            return Ok(false);
        }
        self.gathered[part] += 1;
        self.offer_as(part, dispatch, false)?;
        Ok(true)
    }

    /// Each sub-need's best located passages in this part, by the
    /// deterministic delivery score of its own and part-level reads.
    fn best_passages(&self, part: usize, limit: usize) -> Vec<(u8, Vec<Span>)> {
        let id = self.parts[part].id;
        self.parts[part]
            .needs
            .iter()
            .map(|state| {
                let ranked = deliver::candidates(
                    &self.gather,
                    id,
                    state.need.id,
                    &state.need.query,
                    &[],
                    false,
                );
                (
                    state.need.id,
                    ranked
                        .into_iter()
                        .take(limit)
                        .map(|candidate| candidate.span)
                        .collect(),
                )
            })
            .collect()
    }

    /// The sub-need a part-wide read belongs to: the standing rules need when
    /// there is one, otherwise the first.
    fn rules_need(&self, part: usize) -> u8 {
        let needs = &self.parts[part].needs;
        needs
            .iter()
            .find(|state| state.standing == Some(rules::Standing::Rules))
            .or_else(|| needs.first())
            .map_or(1, |state| state.need.id)
    }

    /// A head read of a file from its first line, at most `HEAD_AFTER` lines.
    fn head(&mut self, part: usize, need: u8, path: &str) -> Result<bool> {
        let id = self.parts[part].id;
        let read = self.gather.dispatch(
            ("context", v3::LoopRead::Context),
            json!({"path":path,"line":1,"before":0,"after":HEAD_AFTER,"max_bytes":HEAD_BYTES}),
            (id, Some(need), 1),
            Cause::Structural,
            FollowUp::Governing,
            Step::Leaf,
            path,
        )?;
        self.offer_gathered(part, read)
    }

    /// Round one's queue is empty: run the next gathering phase that has
    /// work. True when a phase queued a read.
    pub(super) fn extend(&mut self) -> Result<bool> {
        // A request the rules do not apply to has no such stage at all.
        if self.rules == GatheringRules::none() {
            return Ok(false);
        }
        while self.extension < 3 {
            let phase = self.extension;
            self.extension += 1;
            if self.gather.stop.is_some() || self.stop_due() {
                return Ok(false);
            }
            let (kind, on) = match phase {
                0 => (Kind::Guidance, self.rules.guidance),
                1 => (Kind::Cited, self.rules.cited),
                _ => (Kind::Structure, self.rules.structure),
            };
            if !on {
                self.report.skip(kind, v3::GatheringSkip::RuleOff);
                continue;
            }
            self.begin_stage(kind);
            // The stage's own share of the bytes above the later rounds'
            // reserve, taken from what is left as it begins.
            let remaining = self.gather.remaining_bytes();
            let later = self.request.budget.max_bytes / RETENTION_RESERVE_SHARE;
            self.stage_floor = remaining - stage_share(kind, remaining.saturating_sub(later));
            self.starved = None;
            self.idle = v3::GatheringSkip::NothingFound;
            let queued = match kind {
                Kind::Guidance => self.guidance()?,
                Kind::Cited => self.cited()?,
                _ => self.structure()?,
            };
            if queued {
                return Ok(true);
            }
            let reason = self.starved.unwrap_or(self.idle);
            self.report.skip(kind, reason);
        }
        Ok(false)
    }

    /// At delivery: each stage of rules 3 and 4 that never began says why,
    /// for a request the rules apply to.
    pub(super) fn report_unbegun(&mut self) {
        if self.rules == GatheringRules::none() {
            return;
        }
        for (kind, on) in [
            (Kind::Guidance, self.rules.guidance),
            (Kind::Cited, self.rules.cited),
            (Kind::Structure, self.rules.structure),
        ] {
            if !self.report.has(kind) {
                let reason = if on {
                    v3::GatheringSkip::Stopped
                } else {
                    v3::GatheringSkip::RuleOff
                };
                self.report.skip(kind, reason);
            }
        }
    }

    /// The indexed lines of `path` at this publication, by the same
    /// uncharged `file_info` probe that governing documents take. A missing
    /// or unindexed file is an ordinary `None`, never a failure.
    fn indexed_lines(&mut self, path: &str) -> Option<usize> {
        let arguments = json!({"path":path}).as_object().cloned()?;
        let info = (self.env.reader)("file_info", &arguments, Some(&self.gather.revision)).ok()?;
        let lines = info.get("indexed_lines").and_then(Value::as_u64)? as usize;
        (token(&info) == Some(self.gather.revision.as_str()) && lines > 0).then_some(lines)
    }

    /// Rule 3a: an existence probe of each guidance name in each guidance
    /// folder, then up to three sections of each file found, read from their
    /// headings.
    fn guidance(&mut self) -> Result<bool> {
        let mut queued = false;
        let mut read = 0usize;
        let mut sections = 0usize;
        let mut probes = 0usize;
        for part in 0..self.parts.len() {
            if !self.parts[part].active() {
                continue;
            }
            let best = self.best_passages(part, BEST_PASSAGES);
            let mut owners: BTreeMap<String, u8> = BTreeMap::new();
            let mut paths: Vec<&str> = Vec::new();
            for (need, spans) in &best {
                for span in spans {
                    let path = span.hit.citation.path.as_str();
                    paths.push(path);
                    owners.entry(folder(path).to_owned()).or_insert(*need);
                }
            }
            let folders = guidance_folders(&paths, GUIDANCE_FOLDERS);
            let basis = best
                .iter()
                .flat_map(|(_, spans)| spans.first())
                .map(|span| self.gather.records[span.record].id)
                .next()
                .unwrap_or_else(|| Sha256Digest::from_bytes([0; 32]));
            let rules_need = self.rules_need(part);
            let mut found: Vec<(String, u8, usize)> = Vec::new();
            for folder in folders {
                let prefix = if folder.is_empty() {
                    String::new()
                } else {
                    format!("{folder}/")
                };
                let owner = owners.get(&folder).copied().unwrap_or(rules_need);
                for name in GUIDANCE {
                    let path = format!("{prefix}{name}");
                    if probes >= GUIDANCE_PROBES || found.iter().any(|(known, _, _)| *known == path)
                    {
                        continue;
                    }
                    probes += 1;
                    if let Some(lines) = self.indexed_lines(&path) {
                        found.push((path, owner, lines));
                    }
                }
            }
            let words: Vec<(u8, BTreeSet<String>)> = self.parts[part]
                .needs
                .iter()
                .map(|state| (state.need.id, words(&state.need.query)))
                .collect();
            let id = self.parts[part].id;
            let mut planned = 0usize;
            for (path, owner, lines) in found {
                if read >= GUIDANCE_FILES || sections >= GUIDANCE_SECTIONS {
                    break;
                }
                read += 1;
                let facts = self
                    .lookup(
                        part,
                        basis,
                        "document_facts",
                        json!({"path":path,"kind":"section","compact":false,
                            "limit":SECTION_ROWS,"max_bytes":SECTION_FACT_BYTES}),
                    )?
                    .map(|body| facts(&body))
                    .unwrap_or_default();
                let chosen = choose_sections(&section_spans(&path, &facts, lines), &words, owner);
                for (start, end, need) in chosen {
                    if sections >= GUIDANCE_SECTIONS || !self.bytes_fit(planned, SECTION_BYTES) {
                        break;
                    }
                    let after = (end - start).min(HEAD_AFTER);
                    let dispatch = self.gather.dispatch(
                        ("context", v3::LoopRead::Context),
                        json!({"path":path,"line":start,"before":0,"after":after,
                            "max_bytes":SECTION_BYTES}),
                        (id, Some(need), 1),
                        Cause::Structural,
                        FollowUp::Governing,
                        Step::Leaf,
                        &path,
                    )?;
                    if self.offer_gathered(part, dispatch)? {
                        planned += SECTION_BYTES;
                        sections += 1;
                        queued = true;
                    }
                }
            }
        }
        Ok(queued)
    }

    /// Rule 3b: files the retained guidance and best passages name by path,
    /// most cited first, each read from its first line.
    fn cited(&mut self) -> Result<bool> {
        let mut queued = false;
        for part in 0..self.parts.len() {
            if !self.parts[part].active() {
                continue;
            }
            let id = self.parts[part].id;
            let rules_need = self.rules_need(part);
            let mut texts: Vec<(String, u8)> = self
                .gather
                .records
                .iter()
                .filter(|record| {
                    !record.failure
                        && record.stage == Kind::Guidance
                        && record.uses.iter().any(|item| item.part == id)
                })
                .flat_map(super::coverage::record_hits)
                .map(|(_, hit)| (hit.text, rules_need))
                .collect();
            for (need, spans) in self.best_passages(part, BEST_PASSAGES) {
                texts.extend(spans.into_iter().map(|span| (span.hit.text, need)));
            }
            if texts.is_empty() {
                self.idle = v3::GatheringSkip::NoPassages;
            }
            let read: BTreeSet<String> = self
                .gather
                .records
                .iter()
                .filter_map(|record| record.arguments.get("path").and_then(Value::as_str))
                .map(str::to_owned)
                .collect();
            let all: Vec<&str> = texts.iter().map(|(text, _)| text.as_str()).collect();
            let mut planned = 0usize;
            for path in cited_paths(&all, CITED_FILES * 2)
                .into_iter()
                .filter(|path| !read.contains(path))
                .take(CITED_FILES)
            {
                if !self.bytes_fit(planned, HEAD_BYTES) {
                    break;
                }
                let need = texts
                    .iter()
                    .find(|(text, _)| text.contains(path.as_str()))
                    .map_or(rules_need, |(_, need)| *need);
                if self.head(part, need, &path)? {
                    planned += HEAD_BYTES;
                    queued = true;
                }
            }
        }
        Ok(queued)
    }

    /// Rule 4: from each sub-need's best code passages, through the index's
    /// code facts: the enclosing definitions, the definitions of what they
    /// call (same file first, then by name), the places that call them, and
    /// the users of a type a file inherits from outside its folder. One read
    /// per file covers nearby ranges.
    fn structure(&mut self) -> Result<bool> {
        let mut queued = false;
        let mut lookups = 0usize;
        for part in 0..self.parts.len() {
            if !self.parts[part].active() {
                continue;
            }
            let best: Vec<(u8, Vec<Span>)> = self
                .best_passages(part, BEST_PASSAGES * 2)
                .into_iter()
                .map(|(need, spans)| {
                    (
                        need,
                        spans
                            .into_iter()
                            .filter(|span| !software::is_document_path(&span.hit.citation.path))
                            .take(STRUCTURE_PASSAGES)
                            .collect(),
                    )
                })
                .collect();
            let mut files: Vec<String> = Vec::new();
            for (_, spans) in &best {
                for span in spans {
                    let path = &span.hit.citation.path;
                    if !files.contains(path) && files.len() < STRUCTURE_FILES {
                        files.push(path.clone());
                    }
                }
            }
            if files.is_empty() {
                self.idle = v3::GatheringSkip::NoPassages;
            }
            let mut wanted = Wanted::default();
            let mut definitions: BTreeMap<String, Vec<Fact>> = BTreeMap::new();
            let mut bases: BTreeMap<String, Vec<String>> = BTreeMap::new();
            let mut callees: BTreeMap<u8, Vec<String>> = BTreeMap::new();
            let mut callers: BTreeMap<u8, Vec<String>> = BTreeMap::new();
            let mut followed: BTreeSet<(String, usize, usize)> = BTreeSet::new();
            for (need, spans) in &best {
                for span in spans {
                    let path = span.hit.citation.path.clone();
                    if !files.contains(&path) {
                        continue;
                    }
                    let basis = self.gather.records[span.record].id;
                    wanted
                        .hashes
                        .entry(path.clone())
                        .or_insert_with(|| span.hit.citation.content_hash.clone());
                    if !definitions.contains_key(&path) {
                        if lookups >= STRUCTURE_LOOKUPS {
                            continue;
                        }
                        lookups += 2;
                        let found = self
                            .lookup(
                                part,
                                basis,
                                "code_facts",
                                json!({"path":path,"kind":"definition","minimal":true,
                                    "limit":LISTING_LIMIT,"max_bytes":LISTING_BYTES}),
                            )?
                            .map(|body| facts(&body))
                            .unwrap_or_default();
                        // The index's own file hash binds the reads.
                        if let Some(hash) = found.iter().find_map(|fact| fact.hash.clone()) {
                            wanted.hashes.insert(path.clone(), hash);
                        }
                        definitions.insert(path.clone(), found);
                        let inherited = self
                            .lookup(
                                part,
                                basis,
                                "code_facts",
                                json!({"path":path,"kind":"inherits","minimal":true,
                                    "limit":8,"max_bytes":2048}),
                            )?
                            .map(|body| facts(&body))
                            .unwrap_or_default();
                        let mut names: Vec<String> = Vec::new();
                        for fact in inherited {
                            let name = terminal(&fact.name).to_owned();
                            if followable(&name) && !names.contains(&name) && names.len() < BASES {
                                names.push(name);
                            }
                        }
                        bases.insert(path.clone(), names);
                    }
                    let (start, end) = (span.hit.citation.start_line, span.hit.citation.end_line);
                    let file_definitions = definitions.get(&path).cloned().unwrap_or_default();
                    // The innermost bounded definitions the passage sits in.
                    let mut enclosing: Vec<&Fact> = file_definitions
                        .iter()
                        .filter(|fact| {
                            fact.start <= end
                                && fact.end >= start
                                && fact.end - fact.start < FUNCTION_LINES
                        })
                        .collect();
                    enclosing.sort_by_key(|fact| fact.end - fact.start);
                    enclosing.truncate(2);
                    for definition in enclosing {
                        if !followed.insert((path.clone(), definition.start, definition.end)) {
                            continue;
                        }
                        wanted.ranges.entry(path.clone()).or_default().push((
                            definition.start,
                            definition.end,
                            *need,
                        ));
                        let owner = terminal(&definition.name).to_owned();
                        let known = callers.entry(*need).or_default();
                        if followable(&owner) && !known.contains(&owner) && known.len() < CALLERS {
                            known.push(owner.clone());
                        }
                        if lookups >= STRUCTURE_LOOKUPS {
                            continue;
                        }
                        lookups += 1;
                        let calls = self
                            .lookup(
                                part,
                                basis,
                                "code_facts",
                                json!({"path":path,"kind":"call","minimal":true,
                                    "start_line":definition.start,"end_line":definition.end,
                                    "limit":60,"max_bytes":8192}),
                            )?
                            .map(|body| facts(&body))
                            .unwrap_or_default();
                        for call in calls {
                            let name = terminal(&call.name).to_owned();
                            if name == owner || !followable(&name) {
                                continue;
                            }
                            match file_definitions.iter().find(|fact| {
                                terminal(&fact.name) == name
                                    && fact.end - fact.start < FUNCTION_LINES
                            }) {
                                Some(local) => {
                                    if followed.insert((path.clone(), local.start, local.end)) {
                                        wanted.ranges.entry(path.clone()).or_default().push((
                                            local.start,
                                            local.end,
                                            *need,
                                        ));
                                    }
                                }
                                None => {
                                    let known = callees.entry(*need).or_default();
                                    if !known.contains(&name) && known.len() < CALLEES {
                                        known.push(name);
                                    }
                                }
                            }
                        }
                    }
                }
            }
            let basis = |this: &Self, need: u8| {
                best.iter()
                    .find(|(id, _)| *id == need)
                    .and_then(|(_, spans)| spans.first())
                    .map_or_else(
                        || Sha256Digest::from_bytes([0; 32]),
                        |span| this.gather.records[span.record].id,
                    )
            };
            // Callees defined in other files, by name.
            for (need, names) in &callees {
                for name in names {
                    if lookups >= STRUCTURE_LOOKUPS {
                        break;
                    }
                    lookups += 1;
                    let found = self
                        .lookup(
                            part,
                            basis(self, *need),
                            "code_facts",
                            json!({"name":name,"kind":"definition","minimal":true,
                                "limit":3,"max_bytes":2048}),
                        )?
                        .map(|body| facts(&body))
                        .unwrap_or_default();
                    for fact in found
                        .into_iter()
                        .filter(|fact| fact.end - fact.start < FUNCTION_LINES)
                        .take(2)
                    {
                        if followed.insert((fact.path.clone(), fact.start, fact.end)) {
                            if let Some(hash) = &fact.hash {
                                wanted
                                    .hashes
                                    .entry(fact.path.clone())
                                    .or_insert_with(|| hash.clone());
                            }
                            wanted
                                .ranges
                                .entry(fact.path.clone())
                                .or_default()
                                .push((fact.start, fact.end, *need));
                        }
                    }
                }
            }
            // The places that call each enclosing definition.
            for (need, names) in &callers {
                for name in names {
                    if lookups >= STRUCTURE_LOOKUPS {
                        break;
                    }
                    lookups += 1;
                    let found = self
                        .lookup(
                            part,
                            basis(self, *need),
                            "code_facts",
                            json!({"name":name,"kind":"call","minimal":true,
                                "limit":CALL_SITES + 1,"max_bytes":2048}),
                        )?
                        .map(|body| facts(&body))
                        .unwrap_or_default();
                    for fact in found.into_iter().take(CALL_SITES) {
                        let after = (fact.end - fact.start + CALL_MARGIN).min(HEAD_AFTER);
                        wanted
                            .around
                            .push((fact.path, fact.start, CALL_MARGIN, after, *need));
                    }
                }
            }
            // Who uses an inherited type outside the implementing folder.
            for (path, names) in &bases {
                let need = best
                    .iter()
                    .find(|(_, spans)| spans.iter().any(|span| span.hit.citation.path == *path))
                    .map_or(1, |(need, _)| *need);
                let outside = format!("{}/", folder(path));
                for name in names {
                    if lookups >= STRUCTURE_LOOKUPS {
                        break;
                    }
                    lookups += 1;
                    let Some(body) = self.lookup(
                        part,
                        basis(self, need),
                        "references",
                        json!({"name":name,"exclude_path_prefix":outside,"order":"code_first",
                            "limit":20,"max_bytes":8192}),
                    )?
                    else {
                        continue;
                    };
                    let mut seen = BTreeSet::new();
                    for row in crate::workbench::locations(&body).unwrap_or_default() {
                        let citation = &row["citation"];
                        let (Some(file), Some(line), Some(last)) = (
                            citation["path"].as_str(),
                            citation["start_line"].as_u64(),
                            citation["end_line"].as_u64(),
                        ) else {
                            continue;
                        };
                        if seen.len() >= USERS || !seen.insert(file.to_owned()) {
                            continue;
                        }
                        let (line, last) = (line as usize, last as usize);
                        let after = (last.saturating_sub(line) + USER_MARGIN).min(HEAD_AFTER);
                        wanted
                            .around
                            .push((file.to_owned(), line, USER_MARGIN, after, need));
                    }
                }
            }
            queued |= self.structure_reads(part, wanted)?;
        }
        Ok(queued)
    }

    /// The structure phase's hash-bound reads: one `read_file` per merged
    /// range of a file, and a `context` read around each call site or user.
    fn structure_reads(&mut self, part: usize, wanted: Wanted) -> Result<bool> {
        let id = self.parts[part].id;
        let mut queued = false;
        let mut planned = 0usize;
        for (path, ranges) in wanted.ranges {
            for (start, end, need) in merge(ranges) {
                if !self.bytes_fit(planned, READ_BYTES) {
                    return Ok(queued);
                }
                let mut arguments = json!({"path":path,"start_line":start,"end_line":end,
                    "max_bytes":READ_BYTES});
                if let Some(hash) = wanted.hashes.get(&path) {
                    arguments["known_hash"] = json!(hash);
                }
                let read = self.gather.dispatch(
                    ("read_file", v3::LoopRead::ReadFile),
                    arguments,
                    (id, Some(need), 1),
                    Cause::Structural,
                    FollowUp::Definition,
                    Step::Leaf,
                    &path,
                )?;
                if self.offer_gathered(part, read)? {
                    planned += READ_BYTES;
                    queued = true;
                }
            }
        }
        let mut seen = BTreeSet::new();
        for (path, line, before, after, need) in wanted.around {
            if !seen.insert((path.clone(), line)) {
                continue;
            }
            if !self.bytes_fit(planned, HEAD_BYTES) {
                break;
            }
            let read = self.gather.dispatch(
                ("context", v3::LoopRead::Context),
                json!({"path":path,"line":line,"before":before,"after":after,"max_bytes":HEAD_BYTES}),
                (id, Some(need), 1),
                Cause::Structural,
                FollowUp::Callers,
                Step::Leaf,
                &path,
            )?;
            if self.offer_gathered(part, read)? {
                planned += HEAD_BYTES;
                queued = true;
            }
        }
        Ok(queued)
    }
}

#[cfg(test)]
mod tests;
