// SPDX-License-Identifier: Apache-2.0
//! Bounded English document cues. Classification never proves an obligation or design.
use airk_core::extraction::{IndexDetail, LocalSpan};
use airk_core::{DocumentDate, DocumentEvidence, DocumentLocation, DocumentQuantity, Fact};
use anyhow::{Result, ensure};
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use regex::Regex;
use std::{ops::Range, sync::OnceLock};

const VERSION: &str = "document-cues/1";
const MAX_FACTS: usize = 10_000;
const MAX_DETAILS: usize = 32;
pub(crate) type HeadingPath = Vec<(u8, String)>;

struct Patterns {
    date: Regex,
    quantity: Regex,
    modality: Regex,
    id: Regex,
}

fn patterns() -> &'static Patterns {
    static PATTERNS: OnceLock<Patterns> = OnceLock::new();
    PATTERNS.get_or_init(|| {
        let months = "January|February|March|April|May|June|July|August|September|October|November|December|Jan|Feb|Mar|Apr|Jun|Jul|Aug|Sep|Sept|Oct|Nov|Dec";
        Patterns {
            date: Regex::new(&format!(r"(?i)\b(?:\d{{4}}-\d{{2}}-\d{{2}}|\d{{1,2}}\s+(?:{months})\s+\d{{4}}|(?:{months})\s+\d{{1,2}}(?:st|nd|rd|th)?[,]?\s+\d{{4}}|\d{{1,2}}/\d{{1,2}}/\d{{4}})\b")).expect("static date pattern"),
            quantity: Regex::new(r"(?ix)(?P<bound>no\s+more\s+than|no\s+less\s+than|not\s+exceed|at\s+least|at\s+most|less\s+than|greater\s+than|more\s+than|up\s+to|under|below|over|above|exactly|between|<=|>=|≤|≥|<|>)?\s*(?P<value>[+-]?\d+(?:,\d{3})*(?:\.\d+)?)\s*(?:(?:-|–|to|and)\s*(?P<upper>[+-]?\d+(?:,\d{3})*(?:\.\d+)?)\s*)?(?P<unit>%|(?:milliseconds?|microseconds?|nanoseconds?|seconds?|minutes?|hours?|days?|weeks?|months?|years?|ms|us|ns|secs?|mins?|hrs?|s|KiB|MiB|GiB|TiB|KB|MB|GB|TB|bytes?|files?|documents?|users?|requests?|tokens?|threads?|cores?|connections?|pages?|slides?|fps|kHz|GHz|Hz)\b)").expect("static quantity pattern"),
            modality: Regex::new(r"(?i)\b(must\s+not|shall\s+not|should\s+not|must|shall|should|required\s+to|needs?\s+to|may)\b").expect("static modality pattern"),
            id: Regex::new(r"(?i)\b(?:REQ|NFR|FR|AC|R)[-_]?\d+(?:[._-]\d+)*\b").expect("static requirement identifier pattern"),
        }
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Unit {
    pub(crate) span: Range<usize>,
    pub(crate) visible: String,
    pub(crate) origin: &'static str,
    pub(crate) level: Option<u8>,
}

#[derive(Clone, Copy)]
enum AnalysisMode {
    Facts,
    Units,
    Selected(IndexDetail),
    #[cfg(test)]
    FactsAndUnits,
}
impl AnalysisMode {
    fn facts(self) -> bool {
        !matches!(self, Self::Units | Self::Selected(IndexDetail::Essential))
    }
    fn units(self) -> bool {
        match self {
            Self::Units => true,
            #[cfg(test)]
            Self::FactsAndUnits => true,
            _ => false,
        }
    }
    fn headings_only(self) -> bool {
        matches!(self, Self::Selected(IndexDetail::Essential))
    }
}

pub(crate) struct HeadingSeed {
    pub span: LocalSpan,
    pub depth: usize,
}

pub(crate) struct SelectedProse {
    pub facts: Vec<Fact>,
    pub headings: HeadingPath,
    pub seeds: Vec<HeadingSeed>,
}

impl SelectedProse {
    pub fn semantic_seeds(
        &self,
        offset: usize,
    ) -> Result<crate::semantic_projection::SemanticSeeds> {
        let mut seeds = crate::semantic_projection::SemanticSeeds::default();
        for heading in &self.seeds {
            seeds.heading(
                None,
                LocalSpan {
                    start: heading
                        .span
                        .start
                        .checked_add(offset)
                        .ok_or_else(|| anyhow::anyhow!("heading offset overflow"))?,
                    end: heading
                        .span
                        .end
                        .checked_add(offset)
                        .ok_or_else(|| anyhow::anyhow!("heading offset overflow"))?,
                },
                heading.depth,
            )?;
        }
        Ok(seeds)
    }
}

struct Analyzer<'a> {
    text: &'a str,
    line_starts: Vec<usize>,
    headings: HeadingPath,
    facts: Vec<Fact>,
    mode: AnalysisMode,
    units: Vec<Unit>,
    seeds: Vec<HeadingSeed>,
}

impl<'a> Analyzer<'a> {
    fn new(text: &'a str, mode: AnalysisMode) -> Self {
        let mut line_starts = Vec::new();
        if !matches!(mode, AnalysisMode::Units) {
            line_starts.push(0);
            line_starts.extend(text.match_indices('\n').map(|(offset, _)| offset + 1));
        }
        Self {
            text,
            line_starts,
            headings: Vec::new(),
            facts: Vec::new(),
            mode,
            units: Vec::new(),
            seeds: Vec::new(),
        }
    }

    fn emit(
        &mut self,
        unit: &Unit,
        name: &str,
        kind: &str,
        mut details: DocumentEvidence,
    ) -> Result<()> {
        if !self.mode.facts() {
            return Ok(());
        }
        ensure!(self.facts.len() < MAX_FACTS, "document exceeds 10000 facts");
        let start = self
            .line_starts
            .partition_point(|offset| *offset <= unit.span.start);
        let end = self
            .line_starts
            .partition_point(|offset| *offset <= unit.span.end.saturating_sub(1));
        details.parser_version = VERSION.into();
        details.heading_path = self.headings.iter().map(|(_, name)| name.clone()).collect();
        details.statement_truncated = name.len() > 1024;
        self.facts.push(Fact {
            code: None,
            name: bounded(name, 1024),
            kind: kind.into(),
            syntax_kind: unit.origin.into(),
            start_line: start.max(1),
            end_line: end.max(start).max(1),
            location: DocumentLocation::Lines {
                start: start.max(1),
                end: end.max(start).max(1),
            },
            document: Some(details),
        });
        Ok(())
    }

    fn analyze(&mut self, unit: Unit) -> Result<()> {
        if self.mode.units() {
            ensure!(
                self.units.len() < airk_core::continuity::MAX_CONTINUITY_UNITS,
                "document structural unit limit exceeded"
            );
            if !self.mode.facts() {
                self.units.push(unit);
                return Ok(());
            }
            self.units.push(unit.clone());
        }
        if self.mode.headings_only() && unit.level.is_none() {
            return Ok(());
        }
        let text = unit
            .visible
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        if text.is_empty() {
            return Ok(());
        }
        if unit.origin == "markdown-table-header" {
            return Ok(());
        }
        if let Some(level) = unit.level {
            self.headings.retain(|(old, _)| *old < level);
            self.headings.push((level, bounded(&text, 160)));
            if matches!(self.mode, AnalysisMode::Selected(_)) {
                ensure!(
                    self.seeds.len() < airk_core::extraction::MAX_SEMANTIC_BOUNDARIES,
                    "document semantic heading limit exceeded"
                );
                let start = self
                    .line_starts
                    .partition_point(|offset| *offset <= unit.span.start);
                let end = self
                    .line_starts
                    .partition_point(|offset| *offset <= unit.span.end.saturating_sub(1));
                self.seeds.push(HeadingSeed {
                    span: LocalSpan {
                        start: self.line_starts[start.saturating_sub(1)],
                        end: self
                            .line_starts
                            .get(end)
                            .copied()
                            .unwrap_or(self.text.len()),
                    },
                    depth: self.headings.len(),
                });
            }
            if !self.mode.facts() {
                return Ok(());
            }
            let mut dates = patterns()
                .date
                .find_iter(&text)
                .take(MAX_DETAILS + 1)
                .map(|found| parse_date(found.as_str()))
                .collect::<Vec<_>>();
            let details_truncated = dates.len() > MAX_DETAILS;
            dates.truncate(MAX_DETAILS);
            self.emit(
                &unit,
                &text,
                "section",
                DocumentEvidence {
                    basis: if unit.origin == "text-heading-candidate" {
                        "heuristic-heading"
                    } else {
                        "explicit-heading"
                    }
                    .into(),
                    dates: dates.clone(),
                    details_truncated,
                    ..Default::default()
                },
            )?;
            for date in dates {
                let name = date.normalized.clone().unwrap_or_else(|| date.raw.clone());
                self.emit(
                    &unit,
                    &name,
                    "date",
                    DocumentEvidence {
                        basis: "explicit-date-text".into(),
                        dates: vec![date],
                        ..Default::default()
                    },
                )?;
            }
            return Ok(());
        }
        let lower = text.to_lowercase();
        let heading = self
            .headings
            .iter()
            .map(|(_, h)| h.to_lowercase())
            .collect::<Vec<_>>()
            .join(" / ");
        let mut details = DocumentEvidence {
            basis: "heuristic-candidate".into(),
            ..Default::default()
        };
        let mut date_spans = Vec::new();
        for found in patterns().date.find_iter(&text) {
            if details.dates.len() == MAX_DETAILS {
                details.details_truncated = true;
                break;
            }
            details.dates.push(parse_date(found.as_str()));
            date_spans.push(found.range());
        }
        for found in patterns().quantity.captures_iter(&text) {
            if details.quantities.len() == MAX_DETAILS {
                details.details_truncated = true;
                break;
            }
            let raw = found.get(0).expect("match");
            // A suffix of a dotted version or identifier is not a quantity.
            let value_start = found.name("value").expect("value").start();
            if date_spans.iter().any(|date| date.contains(&value_start)) {
                continue;
            }
            if text[..value_start]
                .chars()
                .next_back()
                .is_some_and(|c| c == '.' || c.is_alphanumeric())
            {
                continue;
            }
            let bound = found
                .name("bound")
                .map(|m| m.as_str().to_lowercase())
                .unwrap_or_default();
            let upper_value = found.name("upper").map(|m| m.as_str().replace(',', ""));
            let comparator = if upper_value.is_some() {
                "range"
            } else {
                match bound
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
                    .as_str()
                {
                    "<=" | "≤" | "at most" | "no more than" | "not exceed" | "up to" => "lte",
                    ">=" | "≥" | "at least" | "no less than" => "gte",
                    "<" | "less than" | "under" | "below" => "lt",
                    ">" | "greater than" | "more than" | "over" | "above" => "gt",
                    "exactly" => "eq",
                    _ => "unspecified",
                }
            };
            details.quantities.push(DocumentQuantity {
                raw: raw.as_str().trim().into(),
                comparator: comparator.into(),
                value: found
                    .name("value")
                    .expect("value")
                    .as_str()
                    .replace(',', ""),
                upper_value,
                unit: found.name("unit").expect("unit").as_str().into(),
            });
        }
        details.requirement_id = patterns().id.find(&text).map(|m| m.as_str().into());
        let modal = patterns()
            .modality
            .find_iter(&text)
            .find(|found| {
                !date_spans
                    .iter()
                    .any(|date| date.start <= found.start() && date.end >= found.end())
            })
            .map(|m| m.as_str().to_lowercase());
        let requirement_heading = heading.contains("requirement")
            || heading.contains("constraint")
            || heading.contains("acceptance criteria");
        let labelled = patterns()
            .id
            .find(&text)
            .is_some_and(|m| m.start() < 8 && text[m.end()..].trim_start().starts_with(':'));
        let constrained = details
            .quantities
            .iter()
            .any(|q| q.comparator != "unspecified");
        let normative = modal.as_deref().is_some_and(|m| m != "may")
            || (modal.is_some() && requirement_heading);
        details.modality = if normative { modal } else { None };
        let qualitative = contains_word(
            &lower,
            &[
                "accessible",
                "intuitive",
                "usable",
                "maintainable",
                "modular",
                "extensible",
                "compatible",
                "secure",
                "reliable",
                "robust",
                "testable",
                "observable",
                "portable",
                "performant",
                "resilient",
                "scalable",
                "offline",
                "responsive",
                "simple",
                "fast",
                "efficient",
                "consistent",
                "discoverable",
                "auditable",
                "interoperable",
                "idempotent",
                "atomic",
                "deterministic",
            ],
        ) || [
            "easy to use",
            "user-friendly",
            "backward compatible",
            "loosely coupled",
            "human-readable",
        ]
        .iter()
        .any(|cue| lower.contains(cue));
        let requirement = normative || labelled || constrained || requirement_heading;
        if requirement {
            details.requirement_type = Some(
                match (!details.quantities.is_empty(), qualitative) {
                    (true, true) => "mixed",
                    (true, false) => "numeric",
                    (false, true) => "qualitative",
                    _ => "functional",
                }
                .into(),
            );
            self.emit(
                &unit,
                &source_statement(self.text, &unit.span),
                "requirement",
                details.clone(),
            )?;
        }
        for (kind, matched) in [
            (
                "architecture",
                heading.contains("architectur")
                    || contains_word(
                        &lower,
                        &[
                            "architecture",
                            "architectural",
                            "component",
                            "components",
                            "boundary",
                            "boundaries",
                            "interface",
                            "interfaces",
                            "layer",
                            "layers",
                            "pipeline",
                        ],
                    ),
            ),
            (
                "design",
                heading.contains("design")
                    || contains_word(&lower, &["design", "rationale", "tradeoff", "tradeoffs"])
                    || lower.contains("trade-off"),
            ),
            (
                "feature",
                heading.contains("feature")
                    || contains_word(
                        &lower,
                        &[
                            "feature",
                            "features",
                            "capability",
                            "capabilities",
                            "supports",
                            "allows",
                            "enables",
                            "provides",
                        ],
                    )
                    || (lower.starts_with("as a ") && lower.contains("i want")),
            ),
            (
                "decision",
                heading.contains("decision")
                    || lower.starts_with("decision:")
                    || ["we chose", "we choose", "we decided", "we will use"]
                        .iter()
                        .any(|cue| lower.contains(cue)),
            ),
            (
                "acceptance",
                heading.contains("acceptance")
                    || (contains_word(&lower, &["given"])
                        && contains_word(&lower, &["when"])
                        && contains_word(&lower, &["then"])),
            ),
        ] {
            if matched {
                self.emit(
                    &unit,
                    &source_statement(self.text, &unit.span),
                    kind,
                    details.clone(),
                )?;
            }
        }
        for date in &details.dates {
            let name = date
                .normalized
                .as_ref()
                .map(|iso| format!("{iso} — {}", date.raw))
                .unwrap_or_else(|| date.raw.clone());
            self.emit(
                &unit,
                &name,
                "date",
                DocumentEvidence {
                    basis: "explicit-date-text".into(),
                    dates: vec![date.clone()],
                    ..Default::default()
                },
            )?;
        }
        Ok(())
    }
}

#[cfg(test)]
pub(crate) fn markdown(text: &str) -> Result<Vec<Fact>> {
    Ok(markdown_analysis(text, AnalysisMode::Facts)?.facts)
}

pub(crate) fn markdown_selected(text: &str, detail: IndexDetail) -> Result<SelectedProse> {
    let analyzer = markdown_analysis(text, AnalysisMode::Selected(detail))?;
    Ok(SelectedProse {
        facts: analyzer.facts,
        headings: analyzer.headings,
        seeds: analyzer.seeds,
    })
}

fn markdown_analysis(text: &str, mode: AnalysisMode) -> Result<Analyzer<'_>> {
    let mut analyzer = Analyzer::new(text, mode);
    let mut unit: Option<Unit> = None;
    let mut in_code = false;
    for (ordinal, (event, span)) in Parser::new_ext(text, Options::ENABLE_TABLES)
        .into_offset_iter()
        .enumerate()
    {
        ensure!(ordinal < 500_000, "document exceeds 500000 Markdown events");
        match event {
            Event::Start(Tag::CodeBlock(_)) => in_code = true,
            Event::End(TagEnd::CodeBlock) => in_code = false,
            Event::Start(Tag::Heading { level, .. }) => {
                unit = Some(Unit {
                    span,
                    visible: String::new(),
                    origin: "markdown-heading",
                    level: Some(level as u8),
                })
            }
            Event::Start(Tag::Paragraph) if unit.is_none() => {
                unit = Some(Unit {
                    span,
                    visible: String::new(),
                    origin: "markdown-paragraph",
                    level: None,
                })
            }
            Event::Start(Tag::Item) => {
                if let Some(previous) = unit.take() {
                    analyzer.analyze(previous)?;
                }
                unit = Some(Unit {
                    span,
                    visible: String::new(),
                    origin: "markdown-list-item",
                    level: None,
                });
            }
            Event::Start(Tag::Table(_)) if mode.facts() => {
                let table = Unit {
                    span,
                    visible: String::new(),
                    origin: "markdown-table",
                    level: None,
                };
                analyzer.emit(
                    &table,
                    "Markdown table",
                    "table",
                    DocumentEvidence {
                        basis: "explicit-table".into(),
                        ..Default::default()
                    },
                )?;
            }
            Event::Start(Tag::TableHead) => {
                unit = Some(Unit {
                    span,
                    visible: String::new(),
                    origin: "markdown-table-header",
                    level: None,
                })
            }
            Event::Start(Tag::TableRow) => {
                unit = Some(Unit {
                    span,
                    visible: String::new(),
                    origin: "markdown-table-row",
                    level: None,
                })
            }
            Event::Text(value) if !in_code => {
                if let Some(unit) = &mut unit
                    && (!mode.headings_only() || unit.level.is_some())
                {
                    unit.visible.push_str(&value);
                }
            }
            Event::Code(value) => {
                if let Some(unit) = &mut unit
                    && (!mode.headings_only() || unit.level.is_some())
                {
                    if unit.level.is_some() {
                        unit.visible.push_str(&value);
                    } else {
                        unit.visible.push(' ');
                    }
                }
            }
            Event::SoftBreak | Event::HardBreak | Event::End(TagEnd::TableCell) => {
                if let Some(unit) = &mut unit
                    && (!mode.headings_only() || unit.level.is_some())
                {
                    unit.visible.push(' ');
                }
            }
            Event::End(
                TagEnd::Heading(_) | TagEnd::Item | TagEnd::TableHead | TagEnd::TableRow,
            ) => {
                if let Some(mut unit) = unit.take() {
                    unit.span.end = span.end;
                    analyzer.analyze(unit)?;
                }
            }
            Event::End(TagEnd::Paragraph)
                if unit
                    .as_ref()
                    .is_some_and(|u| u.origin == "markdown-paragraph") =>
            {
                if let Some(mut unit) = unit.take() {
                    unit.span.end = span.end;
                    analyzer.analyze(unit)?;
                }
            }
            _ => {}
        }
    }
    Ok(analyzer)
}

#[cfg(test)]
pub(crate) fn plain(text: &str) -> Result<Vec<Fact>> {
    Ok(plain_with_context(text, &[], None)?.0)
}

pub(crate) fn plain_with_context(
    text: &str,
    context: &[(u8, String)],
    native_level: Option<u8>,
) -> Result<(Vec<Fact>, HeadingPath)> {
    let analyzer = plain_analysis(text, context, native_level, AnalysisMode::Facts)?;
    Ok((analyzer.facts, analyzer.headings))
}

pub(crate) fn plain_selected(
    text: &str,
    context: &[(u8, String)],
    native_level: Option<u8>,
    detail: IndexDetail,
) -> Result<SelectedProse> {
    let analyzer = plain_analysis(text, context, native_level, AnalysisMode::Selected(detail))?;
    Ok(SelectedProse {
        facts: analyzer.facts,
        headings: analyzer.headings,
        seeds: analyzer.seeds,
    })
}

fn plain_analysis<'a>(
    text: &'a str,
    context: &[(u8, String)],
    native_level: Option<u8>,
    mode: AnalysisMode,
) -> Result<Analyzer<'a>> {
    let mut analyzer = Analyzer::new(text, mode);
    analyzer.headings = context.to_vec();
    if let Some(level) = native_level {
        analyzer.analyze(Unit {
            span: 0..text.len(),
            visible: text.trim().into(),
            origin: "docx-styled-heading",
            level: Some(level),
        })?;
        return Ok(analyzer);
    }
    let mut offset = 0;
    let mut fence: Option<char> = None;
    let mut paragraph: Option<Unit> = None;
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim();
        let fence_marker = if trimmed.starts_with("```") {
            Some('`')
        } else if trimmed.starts_with("~~~") {
            Some('~')
        } else {
            None
        };
        if let Some(marker) = fence_marker {
            if let Some(unit) = paragraph.take() {
                analyzer.analyze(unit)?;
            }
            if fence == Some(marker) {
                fence = None;
            } else if fence.is_none() {
                fence = Some(marker);
            }
            offset += line.len();
            continue;
        }
        if fence.is_some() {
            offset += line.len();
            continue;
        }
        let title = trimmed.trim_start_matches('#').trim().trim_end_matches(':');
        let heading = !title.is_empty()
            && title.len() <= 160
            && (trimmed.starts_with('#') || is_section_title(title));
        let list = trimmed.starts_with(['-', '*', '•'])
            || trimmed.split_once('.').is_some_and(|(prefix, suffix)| {
                prefix.chars().all(|c| c.is_ascii_digit())
                    && !prefix.is_empty()
                    && suffix.starts_with(' ')
            });
        if (trimmed.is_empty() || heading || list)
            && let Some(unit) = paragraph.take()
        {
            analyzer.analyze(unit)?;
        }
        if heading {
            analyzer.analyze(Unit {
                span: offset..offset + line.len(),
                visible: title.into(),
                origin: "text-heading-candidate",
                level: Some(1),
            })?;
        } else if !trimmed.is_empty() && !mode.headings_only() {
            if let Some(unit) = &mut paragraph {
                unit.span.end = offset + line.len();
                unit.visible.push(' ');
                unit.visible.push_str(trimmed);
            } else {
                paragraph = Some(Unit {
                    span: offset..offset + line.len(),
                    visible: trimmed.into(),
                    origin: "text-paragraph",
                    level: None,
                });
            }
        }
        offset += line.len();
    }
    if let Some(unit) = paragraph {
        analyzer.analyze(unit)?;
    }
    Ok(analyzer)
}

/// Reuse the existing bounded prose parser; unit ordinals are local locators.
pub(crate) fn continuity_units(text: &str, markdown: bool) -> Result<Vec<Unit>> {
    Ok(if markdown {
        markdown_analysis(text, AnalysisMode::Units)?
    } else {
        plain_analysis(text, &[], None, AnalysisMode::Units)?
    }
    .units)
}

fn is_section_title(title: &str) -> bool {
    let title = title
        .trim_start_matches(|c: char| c.is_ascii_digit() || matches!(c, '.' | ')' | ' '))
        .to_lowercase();
    matches!(
        title.as_str(),
        "architecture"
            | "system architecture"
            | "design"
            | "design rationale"
            | "requirements"
            | "functional requirements"
            | "non-functional requirements"
            | "features"
            | "acceptance criteria"
            | "decision"
            | "decisions"
            | "alternatives"
            | "consequences"
            | "constraints"
            | "rationale"
            | "goals"
            | "non-goals"
            | "status"
    )
}

#[cfg(test)]
mod selected_tests {
    use super::*;

    #[test]
    fn units_only_match_combined_parser_without_fact_classification() -> Result<()> {
        let text = "  # Café `API`\r\n\r\nRequirements\r\n------------\r\n\r\n- REQ-1 must use 20 ms by 2026-10-13.\r\n  - Nested **界** text.\r\n\r\n| A | B |\r\n|---|---|\r\n| 1 | shall use 10 MiB |\r\n\r\n```text\r\n# Hidden\r\n```\r\n\r\nAn inline `must` literal.\r\n";
        for markdown in [false, true] {
            let reference = if markdown {
                markdown_analysis(text, AnalysisMode::FactsAndUnits)?
            } else {
                plain_analysis(text, &[], None, AnalysisMode::FactsAndUnits)?
            };
            assert!(!reference.facts.is_empty());
            assert_eq!(continuity_units(text, markdown)?, reference.units);
            let units_only = if markdown {
                markdown_analysis(text, AnalysisMode::Units)?
            } else {
                plain_analysis(text, &[], None, AnalysisMode::Units)?
            };
            assert!(units_only.facts.is_empty());
            assert!(units_only.line_starts.is_empty());
        }
        Ok(())
    }

    #[test]
    fn selected_heading_seeds_match_full_fact_line_spans_and_depth() -> Result<()> {
        let text = "  # Café `API`\r\n\r\n### Deep 2026-10-13\r\n\r\nThe system must use 200 ms.\r\n\r\n## Sibling\r\n\r\nTail\r\n";
        for markdown in [false, true] {
            let full = if markdown {
                markdown_selected(text, IndexDetail::Detailed)?
            } else {
                plain_selected(text, &[], None, IndexDetail::Detailed)?
            };
            let essential = if markdown {
                markdown_selected(text, IndexDetail::Essential)?
            } else {
                plain_selected(text, &[], None, IndexDetail::Essential)?
            };
            assert!(essential.facts.is_empty());
            assert_eq!(essential.headings, full.headings);
            assert_eq!(essential.seeds.len(), full.seeds.len());
            let starts: Vec<_> = std::iter::once(0)
                .chain(text.match_indices('\n').map(|(i, _)| i + 1))
                .collect();
            let headings: Vec<_> = full.facts.iter().filter(|f| f.kind == "section").collect();
            assert_eq!(headings.len(), essential.seeds.len());
            for (seed, fact) in essential.seeds.iter().zip(headings) {
                assert_eq!(seed.span.start, starts[fact.start_line - 1]);
                assert_eq!(
                    seed.span.end,
                    starts.get(fact.end_line).copied().unwrap_or(text.len())
                );
                assert_eq!(
                    seed.depth,
                    fact.document.as_ref().unwrap().heading_path.len()
                );
            }
        }
        Ok(())
    }

    #[test]
    fn unit_only_has_its_own_bound_and_does_not_pay_discarded_fact_limit() -> Result<()> {
        let text = "The system must use 200 ms by 2026-10-13.\n\n".repeat(6000);
        assert!(markdown(&text).is_err());
        assert_eq!(continuity_units(&text, true)?.len(), 6000);
        assert!(
            markdown_selected(&text, IndexDetail::Essential)?
                .facts
                .is_empty()
        );
        let too_many = "Paragraph.\n\n".repeat(airk_core::continuity::MAX_CONTINUITY_UNITS + 1);
        assert!(
            continuity_units(&too_many, true)
                .unwrap_err()
                .to_string()
                .contains("structural unit limit")
        );
        Ok(())
    }

    #[test]
    #[ignore = "release-only prose timing qualification"]
    fn qualify_selected_prose_work() -> Result<()> {
        let text = format!(
            "# Requirements\n\n{}",
            "REQ-1: The system must serve at least 200 users in 20 ms by 2026-10-13.\n\n"
                .repeat(1800)
        );
        for markdown in [true, false] {
            for mode in [
                AnalysisMode::Facts,
                AnalysisMode::FactsAndUnits,
                AnalysisMode::Units,
                AnalysisMode::Selected(IndexDetail::Essential),
            ] {
                let label = match mode {
                    AnalysisMode::Facts => "full-facts",
                    AnalysisMode::FactsAndUnits => "full-facts-and-units",
                    AnalysisMode::Units => "units-only",
                    AnalysisMode::Selected(_) => "essential-headings",
                };
                std::hint::black_box(if markdown {
                    markdown_analysis(&text, mode)?
                } else {
                    plain_analysis(&text, &[], None, mode)?
                });
                let started = std::time::Instant::now();
                let mut units = 0;
                let mut facts = 0;
                let mut seeds = 0;
                for _ in 0..10 {
                    let result = if markdown {
                        markdown_analysis(&text, mode)?
                    } else {
                        plain_analysis(&text, &[], None, mode)?
                    };
                    units = result.units.len();
                    facts = result.facts.len();
                    seeds = result.seeds.len();
                    std::hint::black_box(result);
                }
                eprintln!(
                    "prose mode={label} markdown={markdown} units={units} facts={facts} seeds={seeds} bytes={} iterations=10 elapsed_us={}",
                    text.len(),
                    started.elapsed().as_micros()
                );
            }
        }
        Ok(())
    }
}

fn source_statement(text: &str, span: &Range<usize>) -> String {
    text[span.clone()]
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn contains_word(text: &str, wanted: &[&str]) -> bool {
    text.split(|c: char| !c.is_alphanumeric())
        .any(|word| wanted.contains(&word))
}

fn bounded(text: &str, limit: usize) -> String {
    let mut end = text.len().min(limit);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].into()
}

fn parse_date(raw: &str) -> DocumentDate {
    if raw.contains('/') {
        return DocumentDate {
            raw: raw.into(),
            normalized: None,
            format: "numeric-locale-unspecified".into(),
        };
    }
    let lower = raw.to_lowercase();
    let (year, month, day, format) = if raw.as_bytes().get(4) == Some(&b'-') {
        let values = raw
            .split('-')
            .map(|v| v.parse::<u32>().unwrap_or(0))
            .collect::<Vec<_>>();
        (values[0], values[1], values[2], "iso-date")
    } else {
        let values = lower.split_whitespace().collect::<Vec<_>>();
        let (month, day) = if values[0].chars().all(|c| c.is_ascii_digit()) {
            (values[1], values[0])
        } else {
            (values[0], values[1])
        };
        let month = match &month[..3] {
            "jan" => 1,
            "feb" => 2,
            "mar" => 3,
            "apr" => 4,
            "may" => 5,
            "jun" => 6,
            "jul" => 7,
            "aug" => 8,
            "sep" => 9,
            "oct" => 10,
            "nov" => 11,
            "dec" => 12,
            _ => 0,
        };
        let day = day
            .trim_end_matches(|c: char| !c.is_ascii_digit())
            .parse::<u32>()
            .unwrap_or(0);
        (
            values[2].parse::<u32>().unwrap_or(0),
            month,
            day,
            "english-date",
        )
    };
    let leap = year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
    let days = match month {
        2 if leap => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        _ => 0,
    };
    let normalized =
        (year > 0 && day > 0 && day <= days).then(|| format!("{year:04}-{month:02}-{day:02}"));
    DocumentDate {
        raw: raw.into(),
        format: if normalized.is_some() {
            format.into()
        } else {
            "invalid-calendar-date".into()
        },
        normalized,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_requirement_lists_keep_hierarchy_modality_and_quantities() -> Result<()> {
        let text = "# Platform\n\n## Requirements\n\n- REQ-001: The API must respond in less than 200 ms.\n- NFR-2: The interface should be intuitive and accessible.\n- REQ-003: The system must not retain more than 10 GB.\n";
        let facts = markdown(text)?;
        let requirements = facts
            .iter()
            .filter(|f| f.kind == "requirement")
            .collect::<Vec<_>>();
        assert_eq!(requirements.len(), 3);
        let numeric = requirements[0].document.as_ref().expect("details");
        assert_eq!(numeric.heading_path, ["Platform", "Requirements"]);
        assert_eq!(numeric.requirement_id.as_deref(), Some("REQ-001"));
        assert_eq!(numeric.modality.as_deref(), Some("must"));
        assert_eq!(numeric.requirement_type.as_deref(), Some("numeric"));
        assert_eq!(numeric.quantities[0].comparator, "lt");
        assert_eq!(numeric.quantities[0].value, "200");
        assert_eq!(numeric.quantities[0].unit, "ms");
        assert_eq!(requirements[0].start_line, 5);
        let qualitative = requirements[1].document.as_ref().expect("details");
        assert_eq!(qualitative.requirement_type.as_deref(), Some("qualitative"));
        assert_eq!(qualitative.modality.as_deref(), Some("should"));
        assert_eq!(
            requirements[2]
                .document
                .as_ref()
                .expect("details")
                .modality
                .as_deref(),
            Some("must not")
        );
        assert!(requirements[2].name.contains("must not"));
        Ok(())
    }

    #[test]
    fn dates_validate_calendar_and_never_guess_numeric_locale() -> Result<()> {
        let facts = plain(
            "Due 13 October 2026; reviewed October 14th, 2026; baseline 2024-02-29.\n\nAmbiguous 10/11/2026; invalid 2026-02-29.\n",
        )?;
        let dates = facts
            .iter()
            .filter(|f| f.kind == "date")
            .map(|f| &f.document.as_ref().expect("details").dates[0])
            .collect::<Vec<_>>();
        assert_eq!(dates.len(), 5);
        assert_eq!(dates[0].normalized.as_deref(), Some("2026-10-13"));
        assert_eq!(dates[1].normalized.as_deref(), Some("2026-10-14"));
        assert_eq!(dates[2].normalized.as_deref(), Some("2024-02-29"));
        assert!(dates[3].normalized.is_none());
        assert_eq!(dates[3].raw, "10/11/2026");
        assert!(dates[4].normalized.is_none());
        assert_eq!(dates[4].format, "invalid-calendar-date");
        Ok(())
    }

    #[test]
    fn numeric_ranges_percentages_and_written_bounds_preserve_units() -> Result<()> {
        let facts = plain(
            "The system shall serve at least 1,000 users at 99.9% availability.\n\nThe cache shall use between 5 and 10 MiB.\n\nLatency <= 250 ms.\n",
        )?;
        let requirements = facts
            .iter()
            .filter(|f| f.kind == "requirement")
            .collect::<Vec<_>>();
        assert_eq!(requirements.len(), 3);
        let quantities = &requirements[0]
            .document
            .as_ref()
            .expect("details")
            .quantities;
        assert_eq!(quantities.len(), 2);
        assert_eq!(quantities[0].value, "1000");
        assert_eq!(quantities[0].comparator, "gte");
        assert_eq!(quantities[1].value, "99.9");
        assert_eq!(quantities[1].unit, "%");
        let range = &requirements[1]
            .document
            .as_ref()
            .expect("details")
            .quantities[0];
        assert_eq!(range.comparator, "range");
        assert_eq!(range.upper_value.as_deref(), Some("10"));
        assert_eq!(range.unit, "MiB");
        let signed = plain(
            "The clock offset must be at least -5 ms.\n\nThe adjustment shall be -10 to -1 ms.",
        )?;
        let signed = signed
            .iter()
            .filter(|fact| fact.kind == "requirement")
            .collect::<Vec<_>>();
        assert_eq!(
            signed[0].document.as_ref().expect("details").quantities[0].value,
            "-5"
        );
        let range = &signed[1].document.as_ref().expect("details").quantities[0];
        assert_eq!(range.value, "-10");
        assert_eq!(range.upper_value.as_deref(), Some("-1"));
        assert_eq!(
            requirements[2]
                .document
                .as_ref()
                .expect("details")
                .quantities[0]
                .comparator,
            "lte"
        );
        Ok(())
    }

    #[test]
    fn design_decisions_features_and_acceptance_keep_context_without_obligations() -> Result<()> {
        let facts = markdown(
            "# Architecture\n\nThe storage component separates read and write interfaces.\n\n## Decision\n\nWe chose immutable generations.\n\n# Features\n\nAs a developer, I want offline retrieval so that I can work locally.\n\n## Acceptance criteria\n\nGiven an index, when the agent searches, then it receives cited evidence.\n",
        )?;
        for kind in ["architecture", "decision", "feature", "acceptance"] {
            assert!(facts.iter().any(|f| f.kind == kind), "{kind}");
        }
        let decision = facts
            .iter()
            .find(|f| f.kind == "decision")
            .expect("decision");
        assert_eq!(
            decision.document.as_ref().expect("details").heading_path,
            ["Architecture", "Decision"]
        );
        assert!(
            decision
                .document
                .as_ref()
                .expect("details")
                .modality
                .is_none()
        );
        Ok(())
    }

    #[test]
    fn code_examples_table_headers_and_inline_literals_are_not_requirements() -> Result<()> {
        let facts = markdown(
            "# Notes\n\n`must` is a literal string.\n\n```text\nThe server must be secure by 2026-10-13.\n```\n\n- Example:\n\n  ```text\n  The service shall use 10 GB.\n  ```\n\n# Requirements\n\n| ID | Requirement |\n|---|---|\n| FR-1 | The UI must be accessible. |\n",
        )?;
        let requirements = facts
            .iter()
            .filter(|f| f.kind == "requirement")
            .collect::<Vec<_>>();
        assert_eq!(requirements.len(), 1);
        assert!(requirements[0].name.contains("accessible"));
        assert!(!facts.iter().any(|f| f.kind == "date"));
        assert_eq!(facts.iter().filter(|f| f.kind == "table").count(), 1);
        Ok(())
    }

    #[test]
    fn metadata_is_bounded_unicode_safe_and_prose_facts_fail_explicitly_at_limit() -> Result<()> {
        let text = format!("The interface must be intuitive {}.\n", "界".repeat(1500));
        let facts = plain(&text)?;
        let requirement = facts
            .iter()
            .find(|f| f.kind == "requirement")
            .expect("requirement");
        assert!(requirement.name.len() <= 1024);
        assert!(
            requirement
                .document
                .as_ref()
                .expect("details")
                .statement_truncated
        );
        let text = (0..40)
            .map(|n| format!("{n} ms"))
            .collect::<Vec<_>>()
            .join(" ");
        let facts = plain(&format!("The system must process {text}."))?;
        let details = facts[0].document.as_ref().expect("details");
        assert_eq!(details.quantities.len(), MAX_DETAILS);
        assert!(details.details_truncated);
        let text = "- The UI must be accessible.\n".repeat(MAX_FACTS + 1);
        assert!(markdown(&text).is_err());
        Ok(())
    }
}
