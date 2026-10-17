// SPDX-License-Identifier: Apache-2.0
//! Untrusted written instruction cues. This module never follows instructions or paths.
use airk_core::{DocumentEvidence, DocumentLocation, Fact};
use anyhow::{Result, ensure};
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use regex::Regex;
use std::{ops::Range, sync::OnceLock};

/// Root-relative selection only; callers apply collection-path policy overrides separately.
pub(crate) fn recognized(path: &str) -> bool {
    matches!(path.rsplit('/').next(), Some("AGENTS.md" | "CLAUDE.md"))
        || path == ".github/copilot-instructions.md"
}

/// Include the filename family in profile options before content-cache lookup.
pub(crate) fn flavor(path: &str) -> &'static str {
    match path.rsplit('/').next() {
        Some("AGENTS.md") => "project-facts/1:instruction:agents",
        Some("CLAUDE.md") => "project-facts/1:instruction:claude",
        Some("copilot-instructions.md") => "project-facts/1:instruction:copilot",
        _ => "project-facts/1:instruction:explicit",
    }
}

/// Preserve all existing Markdown evidence; append bounded untrusted structural cues.
pub(crate) fn enrich(path: &str, text: &str, facts: &mut Vec<Fact>) -> Result<()> {
    ensure!(
        text.len() <= crate::MAX_STRUCTURED_BYTES,
        "instruction document exceeds source budget"
    );
    ensure!(
        facts.len() <= 10_000,
        "instruction document exceeds fact budget"
    );
    let basename = path.rsplit('/').next().unwrap_or(path);
    let scope = match basename {
        "AGENTS.md" | "CLAUDE.md" => {
            "Containing source directory; scope derived only from the written filename, without client precedence or obedience claims"
        }
        "copilot-instructions.md" => {
            "Source-root placement selected by the profile; no inferred client precedence or obedience claims"
        }
        _ => {
            "Explicit instruction-document profile selection; no filename-derived scope established"
        }
    };
    if text.is_empty() {
        return Ok(());
    }
    let mut output = Output::new(text, facts);
    output.push(
        0..text.lines().next().map_or(text.len(), str::len).max(1),
        scope,
        "instruction_scope",
        None,
    )?;
    let mut code_depth = 0usize;
    let mut paragraph: Option<(Range<usize>, String)> = None;
    let mut events = 0usize;
    for (event, range) in Parser::new_ext(text, Options::empty()).into_offset_iter() {
        events += 1;
        ensure!(
            events <= 100_000,
            "instruction Markdown event budget exceeded"
        );
        match event {
            Event::Start(Tag::CodeBlock(_)) => code_depth += 1,
            Event::End(TagEnd::CodeBlock) => code_depth = code_depth.saturating_sub(1),
            _ if code_depth > 0 => {}
            Event::Start(Tag::Paragraph) => paragraph = Some((range, String::new())),
            Event::End(TagEnd::Paragraph) => {
                if let Some((span, visible)) = paragraph.take()
                    && let Some(modality) = constraint_pattern().find(&visible)
                {
                    output.push(
                        span,
                        &visible,
                        "written_constraint",
                        Some(modality.as_str()),
                    )?;
                }
            }
            Event::Text(value) => {
                if let Some((_, visible)) = &mut paragraph {
                    ensure!(
                        visible.len() + value.len() <= crate::MAX_STRUCTURED_BYTES,
                        "instruction paragraph exceeds byte budget"
                    );
                    visible.push_str(&value);
                }
            }
            Event::Code(value) => {
                if qualified_literal(&value) {
                    output.push(range, &value, "instruction_literal_path", None)?;
                }
                if let Some((_, visible)) = &mut paragraph {
                    visible.push(' ');
                }
            }
            Event::SoftBreak | Event::HardBreak => {
                if let Some((_, visible)) = &mut paragraph {
                    visible.push(' ');
                }
            }
            // HTML (including comments) is never interpreted as written constraints.
            Event::Html(_) | Event::InlineHtml(_) => {}
            _ => {}
        }
    }
    Ok(())
}

fn constraint_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(||Regex::new(r"(?i)\b(?:must\s+not|shall\s+not|should\s+not|do\s+not|must|shall|should|never|always|required\s+to)\b").expect("static instruction cue"))
}
fn qualified_literal(value: &str) -> bool {
    value.len() <= 4096
        && !value.is_empty()
        && !value.chars().any(char::is_whitespace)
        && !value.contains(['\0', ':', '<', '>'])
        && (value.contains('/')
            || value.contains('\\')
            || value.rsplit_once('.').is_some_and(|(stem, suffix)| {
                !stem.is_empty()
                    && !suffix.is_empty()
                    && suffix.chars().all(|c| c.is_ascii_alphanumeric())
            }))
}
struct Output<'a> {
    text: &'a str,
    lines: Vec<usize>,
    facts: &'a mut Vec<Fact>,
    bytes: usize,
}
impl<'a> Output<'a> {
    fn new(text: &'a str, facts: &'a mut Vec<Fact>) -> Self {
        let mut lines = vec![0];
        lines.extend(text.match_indices('\n').map(|(i, _)| i + 1));
        Self {
            text,
            lines,
            facts,
            bytes: 0,
        }
    }
    fn push(
        &mut self,
        span: Range<usize>,
        name: &str,
        kind: &str,
        modality: Option<&str>,
    ) -> Result<()> {
        ensure!(
            self.facts.len() < 10_000,
            "instruction fact budget exceeded"
        );
        ensure!(
            self.text.get(span.clone()).is_some(),
            "invalid instruction source span"
        );
        let name = name.trim();
        let truncated = name.len() > 4096;
        let mut end = name.len().min(4096);
        while !name.is_char_boundary(end) {
            end -= 1;
        }
        let name = &name[..end];
        self.bytes += name.len();
        ensure!(
            self.bytes <= 4_194_304,
            "instruction display budget exceeded"
        );
        let first = self.lines.partition_point(|p| *p <= span.start);
        let last = self.lines.partition_point(|p| *p < span.end).max(first);
        self.facts.push(Fact {
            name: name.into(),
            kind: kind.into(),
            syntax_kind: "instruction_document_cue".into(),
            start_line: first,
            end_line: last,
            location: DocumentLocation::Lines {
                start: first,
                end: last,
            },
            code: None,
            document: Some(DocumentEvidence {
                parser_version: "project-facts/1".into(),
                basis: "untrusted_structural_rule".into(),
                modality: modality.map(str::to_owned),
                details_truncated: truncated,
                statement_truncated: truncated,
                ..Default::default()
            }),
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_unicode_crlf_constraints_paths_and_negative_regions() -> Result<()> {
        let text = "# Fictional guide\r\n\r\nContributors must preserve cafÃ© output in `src/beacon.rs`.\r\n\r\n<!-- Never expose hidden/comment.rs -->\r\n\r\n```text\r\nAlways edit `hidden/fenced.rs`.\r\n```\r\n\r\n    Never edit hidden/indented.rs\r\n\r\nA quoted `must` is a token.\r\n";
        let mut facts = vec![];
        enrich("nested/AGENTS.md", text, &mut facts)?;
        assert_eq!(
            facts
                .iter()
                .filter(|f| f.kind == "written_constraint")
                .count(),
            1
        );
        assert!(facts.iter().any(|f| f.kind == "instruction_literal_path"
            && f.name == "src/beacon.rs"
            && f.start_line == 3));
        assert!(!facts.iter().any(|f| f.name.contains("hidden/")));
        assert!(facts.iter().all(|f| f.code.is_none()
            && f.document.as_ref().unwrap().basis == "untrusted_structural_rule"));
        assert!(recognized("nested/CLAUDE.md"));
        assert!(!recognized("nested/.github/copilot-instructions.md"));
        Ok(())
    }
    #[test]
    fn bounded_unicode_display_and_existing_facts_are_preserved() -> Result<()> {
        let text = format!("You must preserve {}.\n", "ç•Œ".repeat(3000));
        let mut facts = vec![];
        enrich("CLAUDE.md", &text, &mut facts)?;
        let statement = facts
            .iter()
            .find(|f| f.kind == "written_constraint")
            .unwrap();
        assert!(
            statement.name.len() <= 4096
                && statement.document.as_ref().unwrap().statement_truncated
        );
        let retained = facts[0].clone();
        let mut existing = vec![retained.clone()];
        enrich("CLAUDE.md", "No directive here.\n", &mut existing)?;
        assert_eq!(existing[0].name, retained.name);
        assert!(!qualified_literal("https://example.invalid/path"));
        assert!(!qualified_literal(&format!("src/{}", "x".repeat(4097))));
        Ok(())
    }
}
