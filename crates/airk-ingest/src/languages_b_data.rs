// SPDX-License-Identifier: Apache-2.0
//! Original-source Nix, Zig, Julia and Clojure/EDN evidence; no evaluation or resolution.
mod clojure;
mod julia;
mod nix;
mod zig;
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
    Cue {
        node,
        kind,
        name: name.into(),
        body: node.child_by_field_name("body"),
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
        "nix" => tree_sitter_nix::LANGUAGE.into(),
        "zig" => tree_sitter_zig::LANGUAGE.into(),
        "julia" => tree_sitter_julia::LANGUAGE.into(),
        "clojure" | "edn" => tree_sitter_clojure::LANGUAGE.into(),
        _ => anyhow::bail!("unsupported data languages B adapter"),
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
        if matches!(adapter, "clojure" | "edn") && node.kind() == "list_lit" {
            let mut cursor = node.walk();
            let first = node.children_by_field_name("value", &mut cursor).next();
            if adapter == "edn"
                || first.is_some_and(|n| matches!(text(n, source), "comment" | "quote"))
            {
                continue;
            }
        }
        let broken_tail = node.next_named_sibling().is_some_and(|next| {
            next.is_error() && source[node.end_byte()..next.start_byte()].trim().is_empty()
        });
        if !node.has_error() && !node.is_missing() && !broken_tail {
            let cues = match adapter {
                "nix" => nix::select(node, source),
                "zig" => zig::select(node, source),
                "julia" => julia::select(node, source),
                _ => clojure::select(node, source, adapter == "edn"),
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
                if matches!(adapter, "clojure" | "edn") {
                    scope.extend(clojure::reader_context(selected.node, source, budget)?);
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
                code.coverage="Selected written syntax only; imports, builtins, macro forms and reader branches are not evaluated or resolved; documentation continuity unknown".into();
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
    let mut result =
        output.finish(tree.root_node().has_error() || matches!(adapter, "clojure" | "edn"));
    if result.status != "partial-syntax" {
        result.status = "syntax".into();
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_language_rows_preserve_locations() -> Result<()> {
        for (adapter, source, expected) in [
            (
                "nix",
                include_str!("../tests/fixtures/languages-b-data/original.nix"),
                "render",
            ),
            (
                "zig",
                include_str!("../tests/fixtures/languages-b-data/original.zig"),
                "render",
            ),
            (
                "julia",
                include_str!("../tests/fixtures/languages-b-data/original.jl"),
                "render",
            ),
            (
                "clojure",
                include_str!("../tests/fixtures/languages-b-data/original.cljc"),
                "render",
            ),
            (
                "edn",
                include_str!("../tests/fixtures/languages-b-data/original.edn"),
                "dependencies",
            ),
        ] {
            let source = source.replace("\r\n", "\n").replace('\n', "\r\n");
            let result = analyze("original", &source, adapter)?;
            assert!(
                result.facts.iter().any(|f| f.name.contains(expected)),
                "{adapter}: {:?}",
                result.facts
            );
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
        Ok(())
    }
    #[test]
    fn data_and_quoted_macro_examples_are_not_declarations() -> Result<()> {
        let data = analyze("original.edn", "{:example (defn phantom [] nil)}", "edn")?;
        assert!(
            !data
                .facts
                .iter()
                .any(|f| f.kind == "written_definition" || f.kind == "call")
        );
        let forms = analyze(
            "original.clj",
            "(comment (defn phantom [] nil)) '(defn quoted [] nil) (defn real [] 1)",
            "clojure",
        )?;
        assert!(
            !forms
                .facts
                .iter()
                .any(|f| f.name == "phantom" || f.name == "quoted")
        );
        assert!(forms.facts.iter().any(|f| f.name == "real"));
        assert!(
            forms
                .facts
                .iter()
                .all(|f| f.code.as_ref().unwrap().fingerprints.is_none())
        );
        Ok(())
    }
    #[test]
    fn sibling_facts_share_budget() -> Result<()> {
        let mut budget = HostBudget::new(1000, 1, 0)?;
        analyze_budgeted("one.nix", "{ a = 1; }", "nix", &mut budget)?;
        assert!(analyze_budgeted("two.nix", "{ b = 2; }", "nix", &mut budget).is_err());
        Ok(())
    }
    #[test]
    fn reader_branches_keep_written_feature_context_without_selection() -> Result<()> {
        let result = analyze(
            "original.cljc",
            "#?(:clj (def platform :jvm) :cljs (def platform :js))",
            "clojure",
        )?;
        let records = result
            .facts
            .iter()
            .filter(|f| f.kind == "written_definition")
            .collect::<Vec<_>>();
        assert_eq!(records.len(), 2);
        for feature in ["reader::clj", "reader::cljs"] {
            assert!(
                records
                    .iter()
                    .any(|f| f.code.as_ref().unwrap().scope.iter().any(|s| s == feature))
            );
        }
        assert_eq!(result.status, "partial-syntax");
        Ok(())
    }
    #[test]
    fn julia_short_methods_have_name_and_independent_signature() -> Result<()> {
        let result = analyze("original.jl", "render(value) = value + 41\n", "julia")?;
        let fact = result
            .facts
            .iter()
            .find(|f| f.kind == "definition" && f.name == "render")
            .unwrap();
        let hashes = fact.code.as_ref().unwrap().fingerprints.as_ref().unwrap();
        assert!(hashes.signature.is_some());
        assert!(hashes.body.is_some());
        Ok(())
    }
}
