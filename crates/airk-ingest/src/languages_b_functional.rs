// SPDX-License-Identifier: Apache-2.0
//! Original-source OCaml, Haskell, Erlang and F# evidence; no evaluation or resolution.
mod erlang;
mod fsharp;
mod haskell;
pub(crate) mod literate;
mod ocaml;
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
    signature: Option<ByteSpan>,
    declaration: bool,
    structural: bool,
}
fn cue<'a>(node: Node<'a>, kind: &'static str, name: &str, declaration: bool) -> Cue<'a> {
    let mut cursor = node.walk();
    let mut bodies = node.children_by_field_name("body", &mut cursor);
    let mut body = bodies.next();
    if bodies.next().is_some() {
        body = None;
    }
    Cue {
        node,
        kind,
        name: name.into(),
        body,
        signature: None,
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
        "ocaml" => tree_sitter_ocaml::LANGUAGE_OCAML.into(),
        "ocaml-interface" => tree_sitter_ocaml::LANGUAGE_OCAML_INTERFACE.into(),
        "haskell" => tree_sitter_haskell::LANGUAGE.into(),
        "erlang" => tree_sitter_erlang::LANGUAGE.into(),
        "fsharp" => tree_sitter_fsharp::LANGUAGE_FSHARP.into(),
        "fsharp-signature" => tree_sitter_fsharp::LANGUAGE_SIGNATURE.into(),
        _ => anyhow::bail!("unsupported functional language adapter"),
    };
    budget.charge(1, 0, 0, 0)?;
    let mut parser = Parser::new();
    parser.set_language(&grammar)?;
    let tree = parser
        .parse(source, None)
        .ok_or_else(|| anyhow::anyhow!("language parser returned no tree"))?;
    let mut output = Facts::new(path, source, adapter)?;
    let mut pending = vec![(tree.root_node(), None)];
    while let Some((node, owner)) = pending.pop() {
        budget.charge(1, 0, 0, 0)?;
        let mut next_owner = owner;
        let broken_tail = node.next_named_sibling().is_some_and(|next| {
            next.is_error() && source[node.end_byte()..next.start_byte()].trim().is_empty()
        });
        if !node.has_error() && !node.is_missing() && !broken_tail {
            let cues = match adapter {
                "ocaml" | "ocaml-interface" => ocaml::select(node, source),
                "haskell" => haskell::select(node, source),
                "erlang" => erlang::select(node, source),
                _ => fsharp::select(node, source, adapter == "fsharp-signature"),
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
                let signature = selected.signature.or_else(|| {
                    selected.body.map(|body| ByteSpan {
                        start: selected.node.start_byte(),
                        end: body.start_byte(),
                    })
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
                code.coverage="Selected written syntax only; imports, preprocessor directives and calls are not evaluated or resolved; documentation continuity unknown".into();
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
            "comment"
                | "line_comment"
                | "block_comment"
                | "string_literal"
                | "string_lit"
                | "string_expression"
                | "indented_string_expression"
                | "prefixed_string_literal"
                | "quote_expression"
                | "quote_statement"
                | "quoting_lit"
                | "syn_quoting_lit"
                | "dis_expr"
                | "evaling_lit"
                | "tagged_or_ctor_lit"
        ) {
            continue;
        }
        for index in (0..node.named_child_count()).rev() {
            ensure!(pending.len() < 500_000, "language pending node limit");
            if let Some(child) = node.named_child(index) {
                pending.push((child, next_owner));
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
    #[test]
    fn required_exceptions_instances_and_members_are_written_evidence() -> Result<()> {
        let ocaml = include_str!("../tests/fixtures/languages-b-functional/required.ml");
        for (kind, name) in [
            ("exception", "Missing"),
            ("definition", "SOURCE"),
            ("definition", "Archive"),
            ("definition", "render"),
            ("import", "include"),
        ] {
            check("ocaml", ocaml, kind, name)?;
        }
        let haskell = include_str!("../tests/fixtures/languages-b-functional/required.hs");
        for (kind, name) in [
            ("definition", "Display"),
            ("instance", "Display Record"),
            ("definition", "Label"),
            ("definition", "Record"),
            ("signature", "display"),
        ] {
            check("haskell", haskell, kind, name)?;
        }
        let fsharp = include_str!("../tests/fixtures/languages-b-functional/required.fs");
        for (kind, name) in [
            ("module", "Archive"),
            ("definition", "Record"),
            ("member", "Render"),
            ("import", "System"),
            ("definition", "render"),
        ] {
            check("fsharp", fsharp, kind, name)?;
        }
        Ok(())
    }
    fn check(adapter: &str, source: &str, kind: &str, name: &str) -> Result<()> {
        let source = source.replace("\r\n", "\n").replace('\n', "\r\n");
        let result = analyze("original", &source, adapter)?;
        assert!(
            result
                .facts
                .iter()
                .any(|f| f.kind == kind && f.name.contains(name)),
            "{adapter}: {:?}",
            result.facts
        );
        for fact in result.facts {
            let span = fact.code.unwrap().span.unwrap();
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
        Ok(())
    }
    #[test]
    fn ocaml_implementation_and_interface() -> Result<()> {
        check(
            "ocaml",
            include_str!("../tests/fixtures/languages-b-functional/original.ml"),
            "definition",
            "render",
        )?;
        check(
            "ocaml-interface",
            include_str!("../tests/fixtures/languages-b-functional/original.mli"),
            "signature",
            "render",
        )
    }
    #[test]
    fn haskell_definitions_and_imports() -> Result<()> {
        let text = include_str!("../tests/fixtures/languages-b-functional/original.hs");
        check("haskell", text, "definition", "render")?;
        check("haskell", text, "import", "Data.Char")
    }
    #[test]
    fn erlang_arity_specs_and_header_macros() -> Result<()> {
        let text = include_str!("../tests/fixtures/languages-b-functional/original.erl");
        check("erlang", text, "definition", "render/1")?;
        check("erlang", text, "signature", "render")?;
        check(
            "erlang",
            include_str!("../tests/fixtures/languages-b-functional/original.hrl"),
            "macro",
            "OFFSET",
        )
    }
    #[test]
    fn fsharp_implementation_signature_and_script() -> Result<()> {
        check(
            "fsharp",
            include_str!("../tests/fixtures/languages-b-functional/original.fs"),
            "definition",
            "render",
        )?;
        check(
            "fsharp-signature",
            include_str!("../tests/fixtures/languages-b-functional/original.fsi"),
            "signature",
            "render",
        )?;
        check(
            "fsharp",
            include_str!("../tests/fixtures/languages-b-functional/original.fsx"),
            "directive",
            "#load",
        )
    }
    #[test]
    fn literate_bird_prefixes_and_latex_map_exact_original_crlf() -> Result<()> {
        let source = include_str!("../tests/fixtures/languages-b-functional/original.lhs")
            .replace("\r\n", "\n")
            .replace('\n', "\r\n");
        let found = literate::discover(
            "original.lhs",
            &source,
            &mut HostBudget::new(500_000, 10_000, 16_777_216)?,
        )?;
        assert_eq!(found.regions.len(), 2);
        for region in &found.regions {
            region.source_map.validate(&region.text, &source)?;
            assert!(region.text.contains("\r\n"));
            assert!(!region.text.starts_with('>'));
        }
        assert_eq!(found.regions[0].source_map.segments.len(), 2);
        let prose = source.find("Outside code").unwrap();
        assert!(
            !found
                .exclusions
                .iter()
                .any(|s| s.start <= prose && prose < s.end)
        );
        Ok(())
    }
    #[test]
    fn malformed_empty_and_region_attempts_are_bounded() -> Result<()> {
        let mut budget = HostBudget::new(500_000, 10_000, 16_777_216)?;
        let found = literate::discover("bad.lhs", "\\begin{code}\nphantom = 1\n", &mut budget)?;
        assert!(found.partial && found.regions.is_empty());
        assert!(
            literate::discover(
                "many.lhs",
                &">\n\n".repeat(257),
                &mut HostBudget::new(500_000, 10_000, 16_777_216)?
            )
            .is_err()
        );
        let mut budget = HostBudget::new(1000, 1, 0)?;
        analyze_budgeted("one.ml", "let one = 1", "ocaml", &mut budget)?;
        assert!(analyze_budgeted("two.ml", "let two = 2", "ocaml", &mut budget).is_err());
        Ok(())
    }
}
