// SPDX-License-Identifier: Apache-2.0
//! Original-source R, Perl and Groovy evidence; no evaluation or resolution.
mod groovy;
mod perl;
mod r;
pub(crate) mod rmd;
use crate::{build_facts::Facts, syntax::Analysis};
use airk_core::{
    CodeSpan, EvidenceBasis,
    location::{ByteSpan, HostBudget},
};
use anyhow::{Result, ensure};
use tree_sitter::{Language, Node, Parser};

struct Cue<'a> {
    node: Node<'a>,
    kind: &'static str,
    name: String,
    body: Option<Node<'a>>,
    declaration: bool,
    structural: bool,
}
fn cue<'a>(node: Node<'a>, kind: &'static str, name: &str, declaration: bool) -> Cue<'a> {
    Cue {
        node,
        kind,
        name: name.into(),
        body: node.child_by_field_name("body"),
        declaration,
        structural: false,
    }
}
fn text<'a>(node: Node<'_>, source: &'a str) -> &'a str {
    source.get(node.byte_range()).unwrap_or("")
}
fn field<'a>(node: Node<'_>, source: &'a str, name: &str) -> Option<&'a str> {
    node.child_by_field_name(name)
        .map(|node| text(node, source))
}
fn named<'a>(node: Node<'a>, kind: &str) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .find(|node| node.kind() == kind)
}

pub(crate) fn analyze(path: &str, source: &str, adapter: &str) -> Result<Analysis> {
    let mut budget = HostBudget::new(500_000, 10_000, 16_777_216)?;
    budget.charge(0, 0, source.len(), 0)?;
    analyze_budgeted(path, source, adapter, &mut budget)
}

/// Caller admits source/map bytes once. Sibling literate chunks share parser/fact/hash work.
pub(crate) fn analyze_budgeted(
    path: &str,
    source: &str,
    adapter: &str,
    budget: &mut HostBudget,
) -> Result<Analysis> {
    ensure!(
        source.len() <= crate::MAX_STRUCTURED_BYTES,
        "language source byte limit"
    );
    let grammar: Language = match adapter {
        "r" => tree_sitter_r::LANGUAGE.into(),
        "perl" => tree_sitter_perl::LANGUAGE.into(),
        "groovy" | "gradle-groovy" => tree_sitter_groovy::LANGUAGE.into(),
        _ => anyhow::bail!("unsupported first languages B adapter"),
    };
    budget.charge(1, 0, 0, 0)?;
    let mut parser = Parser::new();
    parser.set_language(&grammar)?;
    let tree = parser
        .parse(source, None)
        .ok_or_else(|| anyhow::anyhow!("language parser returned no tree"))?;
    let mut output = Facts::new(path, source, adapter)?;
    let mut pending = vec![(tree.root_node(), None, groovy::Context::None)];
    while let Some((node, owner, context)) = pending.pop() {
        budget.charge(1, 0, 0, 0)?;
        let mut next_owner = owner;
        let mut next_context = context;
        let broken_tail = node.next_named_sibling().is_some_and(|next| {
            next.is_error() && source[node.end_byte()..next.start_byte()].trim().is_empty()
        });
        if !node.has_error() && !node.is_missing() && !broken_tail {
            let cues = match adapter {
                "r" => r::select(node, source),
                "perl" => perl::select(node, source),
                _ => {
                    let (cues, next) =
                        groovy::select(node, source, adapter == "gradle-groovy", context);
                    next_context = next;
                    cues
                }
            };
            for selected in cues {
                budget.charge(0, 1, 0, 0)?;
                let index = output.facts.len();
                output.push(
                    selected.node.start_byte(),
                    selected.node.end_byte(),
                    selected.kind,
                    selected.node.kind(),
                    &selected.name,
                )?;
                if index == output.facts.len() {
                    continue;
                }
                let owner_id =
                    owner.map(|i: usize| output.facts[i].code.as_ref().unwrap().symbol_id.clone());
                let mut scope = owner.map_or_else(Vec::new, |i| {
                    output.facts[i].code.as_ref().unwrap().scope.clone()
                });
                let truncated_owner =
                    owner.is_some_and(|i| output.facts[i].code.as_ref().unwrap().details_truncated);
                if let Some(i) = owner {
                    scope.push(output.facts[i].name.clone());
                }
                ensure!(scope.len() <= 128, "language owner depth limit");
                let signature = selected.body.map(|body| ByteSpan {
                    start: selected.node.start_byte(),
                    end: body.start_byte(),
                });
                let fingerprints = if selected.declaration && !selected.structural {
                    let mut hashes = crate::syntax::declaration_fingerprints(
                        source,
                        &selected.name,
                        &scope.iter().map(String::as_str).collect::<Vec<_>>(),
                        signature,
                        selected.body,
                        None,
                        budget,
                    )?;
                    if truncated_owner {
                        hashes.qualified_name = None;
                    }
                    Some(hashes)
                } else {
                    None
                };
                let code = output.facts[index].code.as_mut().unwrap();
                code.owner_id = owner_id;
                code.scope = scope;
                code.details_truncated |= truncated_owner;
                code.fingerprints = fingerprints;
                code.signature_span = signature.map(|s| CodeSpan {
                    start_byte: s.start,
                    end_byte: s.end,
                });
                code.basis = if selected.structural {
                    EvidenceBasis::StructuralRule
                } else {
                    EvidenceBasis::Syntax
                };
                code.producer = if selected.structural {
                    format!("airk-language-structure/{adapter}")
                } else {
                    format!("airk-tree-sitter/{adapter}")
                };
                code.producer_version = "general-b-facts/1".into();
                code.coverage="Selected written syntax; package loads, build instructions, calls and closures are not executed or resolved; documentation continuity unknown".into();
                if matches!(
                    selected.kind,
                    "call" | "import" | "package_load" | "require"
                ) {
                    code.target = Some(selected.name.chars().take(1024).collect());
                }
                if selected.declaration {
                    next_owner = Some(index);
                }
            }
        }
        // Literal contents are evidence values, never new executable declarations.
        if matches!(
            node.kind(),
            "comment" | "pod" | "string" | "string_literal" | "command_string" | "heredoc"
        ) {
            continue;
        }
        for index in (0..node.named_child_count()).rev() {
            ensure!(pending.len() < 500_000, "language pending node limit");
            if let Some(child) = node.named_child(index) {
                pending.push((child, next_owner, next_context));
            }
        }
    }
    ensure!(
        serde_json::to_vec(&output.facts)?.len() <= 16_777_216,
        "language evidence byte limit"
    );
    let mut result = output.finish(tree.root_node().has_error());
    if result.status != "partial-syntax" {
        result.status = "syntax".into();
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn has(result: &Analysis, kind: &str, name: &str) {
        assert!(
            result
                .facts
                .iter()
                .any(|f| f.kind == kind && f.name.contains(name)),
            "missing {kind} {name}: {:?}",
            result
                .facts
                .iter()
                .map(|f| (&f.kind, &f.name, &f.syntax_kind))
                .collect::<Vec<_>>()
        );
    }
    #[test]
    fn r_assignments_functions_package_loads_calls_and_original_crlf() -> Result<()> {
        let source = include_str!("../tests/fixtures/languages-b-first/original.R")
            .replace("\r\n", "\n")
            .replace('\n', "\r\n");
        let result = analyze("original.R", &source, "r")?;
        assert_eq!(result.status, "syntax");
        for (kind, name) in [
            ("definition", "render"),
            ("assignment", "caption"),
            ("package_load", "stats"),
            ("package_load", "utils"),
            ("call", "paste"),
        ] {
            has(&result, kind, name);
        }
        verify(&source, &result);
        Ok(())
    }
    #[test]
    fn perl_packages_subroutines_imports_requires_and_calls() -> Result<()> {
        let source = include_str!("../tests/fixtures/languages-b-first/original.pl");
        let result = analyze("original.pl", source, "perl")?;
        assert_eq!(result.status, "syntax");
        for (kind, name) in [
            ("package", "Archive"),
            ("definition", "render"),
            ("import", "strict"),
            ("require", "Archive::Support"),
            ("call", "render"),
        ] {
            has(&result, kind, name);
        }
        verify(source, &result);
        Ok(())
    }
    #[test]
    fn groovy_native_and_gradle_structural_overlay_are_distinct() -> Result<()> {
        let source = include_str!("../tests/fixtures/languages-b-first/original.groovy");
        let result = analyze("original.groovy", source, "groovy")?;
        assert_eq!(result.status, "syntax");
        for (kind, name) in [
            ("package", "archive"),
            ("import", "java.util.List"),
            ("definition", "Archive"),
            ("definition", "render"),
            ("closure", "trim"),
            ("call", "println"),
        ] {
            has(&result, kind, name);
        }
        verify(source, &result);
        let build = include_str!("../tests/fixtures/languages-b-first/original.gradle");
        let plain = analyze("original.gradle", build, "groovy")?;
        let overlay = analyze("original.gradle", build, "gradle-groovy")?;
        for (kind, name) in [
            ("plugin", "java-library"),
            ("dependency", "example:archive"),
            ("task", "render"),
            ("module", "archive-core"),
        ] {
            has(&overlay, kind, name);
            assert!(!plain.facts.iter().any(|f| f.kind == kind));
        }
        assert!(
            overlay
                .facts
                .iter()
                .filter(|f| matches!(f.kind.as_str(), "plugin" | "dependency" | "task"))
                .all(|f| f.code.as_ref().unwrap().basis == EvidenceBasis::StructuralRule)
        );
        Ok(())
    }
    #[test]
    fn quoted_comment_examples_and_shared_sibling_allowances() -> Result<()> {
        for (adapter, source) in [
            ("r", "# phantom <- function() {}\ncaption <- 'phantom()'\n"),
            ("perl", "# sub phantom {}\nmy $caption = 'phantom()';\n"),
            ("groovy", "// class Phantom {}\ndef caption = 'phantom()'\n"),
        ] {
            let result = analyze("negative", source, adapter)?;
            assert!(
                !result
                    .facts
                    .iter()
                    .any(|f| f.kind == "definition" || f.kind == "call")
            );
        }
        let mut budget = HostBudget::new(1000, 1, 0)?;
        analyze_budgeted("one.R", "a <- 1", "r", &mut budget)?;
        assert!(analyze_budgeted("two.R", "b <- 2", "r", &mut budget).is_err());
        assert!(analyze("huge.R", &" ".repeat(crate::MAX_STRUCTURED_BYTES + 1), "r").is_err());
        Ok(())
    }
    #[test]
    fn rmd_chunks_preserve_original_bytes_and_ignore_ordinary_examples() -> Result<()> {
        let source = include_str!("../tests/fixtures/languages-b-first/original.Rmd")
            .replace("\r\n", "\n")
            .replace('\n', "\r\n");
        let mut budget = HostBudget::new(500_000, 10_000, 16_777_216)?;
        let found = rmd::discover("original.Rmd", &source, &mut budget)?;
        assert!(!found.host.partial);
        assert_eq!(found.host.regions.len(), 2);
        assert_eq!(found.host.regions[0].adapter_id, "r");
        assert_eq!(found.host.regions[1].adapter_id, "python");
        for region in &found.host.regions {
            let bytes = &source[region.span.start..region.span.end];
            assert!(bytes.contains("\r\n"));
            assert!(!bytes.contains("phantom"));
        }
        let requirement = source.find("REQ-920").unwrap();
        assert!(
            !found
                .exclusions
                .iter()
                .any(|s| s.start <= requirement && s.end > requirement)
        );
        Ok(())
    }
    #[test]
    fn rmd_failed_empty_and_unsupported_attempts_are_bounded() -> Result<()> {
        let mut budget = HostBudget::new(500_000, 10_000, 16_777_216)?;
        let found = rmd::discover(
            "negative.Rmd",
            "```{unknown hidden}\nphantom()\n```\n\n```{r empty}\n```\n",
            &mut budget,
        )?;
        assert!(found.host.partial);
        assert!(found.host.regions.is_empty());
        assert!(
            rmd::discover(
                "many.Rmd",
                &"```{unknown}\n```\n".repeat(257),
                &mut HostBudget::new(500_000, 10_000, 16_777_216)?
            )
            .is_err()
        );
        Ok(())
    }
    fn verify(source: &str, result: &Analysis) {
        for fact in &result.facts {
            let span = fact.code.as_ref().unwrap().span.as_ref().unwrap();
            assert!(source.get(span.start_byte..span.end_byte).is_some());
            assert_eq!(
                fact.start_line,
                source[..span.start_byte]
                    .bytes()
                    .filter(|b| *b == b'\n')
                    .count()
                    + 1
            );
        }
    }
}
