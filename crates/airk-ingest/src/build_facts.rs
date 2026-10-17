// SPDX-License-Identifier: Apache-2.0
//! Bounded original-source evidence shared by build adapters.
use airk_core::{CodeEvidence, CodeSpan, DocumentLocation, EvidenceBasis, Fact};
use anyhow::{Result, ensure};
use tree_sitter::Node;

pub(crate) struct Facts<'a> {
    path: &'a str,
    text: &'a str,
    language: &'a str,
    lines: Vec<usize>,
    detail_bytes: usize,
    pub(crate) facts: Vec<Fact>,
}

impl<'a> Facts<'a> {
    pub(crate) fn new(path: &'a str, text: &'a str, language: &'a str) -> Result<Self> {
        ensure!(
            text.len() <= crate::MAX_STRUCTURED_BYTES,
            "build source exceeds byte limit"
        );
        let mut lines = vec![0];
        lines.extend(text.match_indices('\n').map(|(i, _)| i + 1));
        Ok(Self {
            path,
            text,
            language,
            lines,
            detail_bytes: 0,
            facts: Vec::new(),
        })
    }

    pub(crate) fn push(
        &mut self,
        start: usize,
        end: usize,
        kind: &str,
        syntax: &str,
        name: &str,
    ) -> Result<()> {
        ensure!(self.facts.len() < 10_000, "build source exceeds fact limit");
        ensure!(
            start < end && self.text.get(start..end).is_some(),
            "invalid build source span"
        );
        let name = name.trim();
        if name.is_empty() {
            return Ok(());
        }
        let details_truncated = name.len() > 4096;
        let mut display_end = name.len().min(4096);
        while !name.is_char_boundary(display_end) {
            display_end -= 1;
        }
        let name = &name[..display_end];
        self.detail_bytes += name.len();
        ensure!(
            self.detail_bytes <= 4_194_304,
            "build fact details exceed byte limit"
        );
        let first = self.lines.partition_point(|offset| *offset <= start);
        let last = self.lines.partition_point(|offset| *offset < end);
        self.facts.push(Fact {
            name: name.into(), kind: kind.into(), syntax_kind: syntax.into(),
            start_line: first, end_line: last.max(first),
            location: DocumentLocation::Lines { start: first, end: last.max(first) }, document: None,
            code: Some(CodeEvidence {
                fingerprints: None,
                symbol_id: format!("syntax:{}:{start}:{end}:{syntax}", self.path),
                qualified_name: name.into(), scope: vec![], owner_id: None,
                signature: None, span: Some(CodeSpan { start_byte: start, end_byte: end }),
                signature_span: None, parameters: None, return_type: None,
                modifiers: vec![], documentation: None, target: None,
                basis: EvidenceBasis::StructuralRule,
                producer: format!("airk-build-structure/{}", self.language),
                producer_version: "1".into(),
                coverage: "Written build structure; expressions, conditions, commands and references are not evaluated or resolved".into(),
                details_truncated,
            }),
        });
        Ok(())
    }

    pub(crate) fn node(&mut self, node: Node<'_>, kind: &str) -> Result<()> {
        self.push(
            node.start_byte(),
            node.end_byte(),
            kind,
            node.kind(),
            node.utf8_text(self.text.as_bytes())?,
        )
    }

    pub(crate) fn finish(self, partial: bool) -> crate::syntax::Analysis {
        crate::syntax::Analysis {
            language: self.language.into(),
            status: if partial {
                "partial-syntax"
            } else {
                "structural-build"
            }
            .into(),
            facts: self.facts,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_unicode_command_keeps_complete_span_and_bounded_display() -> Result<()> {
        let command = format!("echo {}終\n", "界".repeat(3000));
        let mut output = Facts::new("Makefile", &command, "make")?;
        output.push(0, command.len(), "recipe", "recipe", &command)?;
        let fact = &output.facts[0];
        assert!(fact.name.len() <= 4096);
        assert!(command.starts_with(&fact.name));
        let code = fact.code.as_ref().unwrap();
        assert!(code.details_truncated);
        assert_eq!(code.qualified_name, fact.name);
        assert!(code.fingerprints.is_none());
        let span = code.span.as_ref().unwrap();
        assert_eq!(&command[span.start_byte..span.end_byte], command);
        assert_eq!(output.detail_bytes, fact.name.len());
        Ok(())
    }

    #[test]
    fn display_limit_does_not_mark_exact_size_text_truncated() -> Result<()> {
        let command = "x".repeat(4096);
        let mut output = Facts::new("Makefile", &command, "make")?;
        output.push(0, command.len(), "recipe", "recipe", &command)?;
        assert!(!output.facts[0].code.as_ref().unwrap().details_truncated);
        Ok(())
    }
}
