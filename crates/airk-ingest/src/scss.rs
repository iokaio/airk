// SPDX-License-Identifier: Apache-2.0
//! Bounded SCSS written syntax; no Sass evaluation or resolved module semantics.
use crate::{build_facts::Facts, syntax::Analysis};
use airk_core::EvidenceBasis;
use airk_core::location::HostBudget;
use anyhow::{Result, ensure};
use tree_sitter::{Node, Parser};

pub(crate) fn analyze(path: &str, text: &str) -> Result<Analysis> {
    analyze_budgeted(
        path,
        text,
        &mut HostBudget::new(500_000, 10_000, 16_777_216)?,
    )
}

/// The host owns source-text/map admission; this charges parser attempts, nodes and facts.
pub(crate) fn analyze_budgeted(
    path: &str,
    text: &str,
    budget: &mut HostBudget,
) -> Result<Analysis> {
    let mut facts = Facts::new(path, text, "scss")?;
    budget.charge(1, 0, 0, 0)?;
    let mut parser = Parser::new();
    parser.set_language(&tree_sitter_scss::language())?;
    let tree = parser
        .parse(text, None)
        .ok_or_else(|| anyhow::anyhow!("SCSS parser returned no tree"))?;
    let mut pending = vec![(tree.root_node(), None)];
    let mut visited = 0usize;
    let mut partial = tree.root_node().has_error();
    while let Some((node, owner)) = pending.pop() {
        visited += 1;
        budget.charge(1, 0, 0, 0)?;
        ensure!(visited <= 500_000, "SCSS parser node limit exceeded");
        let mut next_owner = owner;
        if !node.has_error() && !node.is_missing() {
            let cue = match node.kind() {
                "rule_set" => child(node, "selectors").map(|name| ("selector", name)),
                "mixin_statement" => node.child_by_field_name("name").map(|name| ("mixin", name)),
                "function_statement" => node
                    .child_by_field_name("name")
                    .map(|name| ("function", name)),
                "include_statement" => Some(("include", node)),
                "import_statement" => Some(("import", node)),
                "use_statement" => Some(("use", node)),
                "forward_statement" => Some(("forward", node)),
                "extend_statement" => Some(("extend", node)),
                "declaration" => node
                    .named_child(0)
                    .filter(|name| matches!(name.kind(), "variable" | "property_name"))
                    .map(|name| {
                        (
                            if name.kind() == "variable"
                                || name
                                    .utf8_text(text.as_bytes())
                                    .is_ok_and(|name| name.starts_with('$'))
                            {
                                "variable"
                            } else {
                                "property"
                            },
                            name,
                        )
                    }),
                "variable"
                    if !node.parent().is_some_and(|parent| {
                        parent.kind() == "declaration" && parent.named_child(0) == Some(node)
                    }) =>
                {
                    Some(("variable_reference", node))
                }
                "interpolation" => {
                    partial = true;
                    Some(("interpolation", node))
                }
                "at_rule" => {
                    partial = true;
                    Some(("unsupported_at_rule", node))
                }
                _ => None,
            };
            if let Some((kind, name)) = cue {
                budget.charge(0, 1, 0, 0)?;
                facts.push(
                    node.start_byte(),
                    node.end_byte(),
                    kind,
                    node.kind(),
                    name.utf8_text(text.as_bytes())?,
                )?;
                let fact = facts.facts.last_mut().expect("SCSS syntax fact");
                let code = fact.code.as_mut().expect("SCSS evidence");
                code.basis = EvidenceBasis::Syntax;
                code.producer = "airk-tree-sitter/scss".into();
                code.producer_version = "composite-facts/1".into();
                code.coverage = "Selected SCSS grammar nodes and written nesting; interpolation, variables, mixins, imports and module paths are not evaluated or resolved; declaration continuity is unsupported".into();
                code.owner_id = owner.map(|owner| symbol(path, owner));
                if matches!(
                    node.kind(),
                    "rule_set" | "mixin_statement" | "function_statement"
                ) {
                    next_owner = Some(node);
                }
            }
        }
        for index in (0..node.named_child_count()).rev() {
            ensure!(
                pending.len() < 500_000,
                "SCSS parser pending limit exceeded"
            );
            if let Some(child) = node.named_child(index) {
                pending.push((child, next_owner));
            }
        }
    }
    ensure!(
        serde_json::to_vec(&facts.facts)?.len() <= 16_777_216,
        "SCSS evidence byte limit exceeded"
    );
    let mut result = facts.finish(partial);
    if !partial {
        result.status = "syntax".into();
    }
    Ok(result)
}

fn symbol(path: &str, node: Node<'_>) -> String {
    format!(
        "syntax:{path}:{}:{}:{}",
        node.start_byte(),
        node.end_byte(),
        node.kind()
    )
}
fn child<'a>(node: Node<'a>, kind: &str) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .find(|child| child.kind() == kind)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sibling_scss_regions_share_fact_allowance() {
        let mut budget = HostBudget::new(1000, 1, 0).unwrap();
        assert_eq!(
            analyze_budgeted("a.scss", "$accent: red;", &mut budget)
                .unwrap()
                .facts
                .len(),
            1
        );
        assert!(analyze_budgeted("b.scss", "$accent: blue;", &mut budget).is_err());
    }

    #[test]
    fn written_scss_constructs_and_nested_owner_spans_are_preserved() -> Result<()> {
        let source = include_str!("../tests/fixtures/scss/original.scss")
            .replace(".café", ".cafe")
            .replace("$tone: #369;", "$tone: #369;\n$caption: 'café 🦀';")
            .replace("\r\n", "\n")
            .replace('\n', "\r\n");
        let analysis = analyze("original.scss", &source)?;
        let mut parser = Parser::new();
        parser.set_language(&tree_sitter_scss::language())?;
        assert_eq!(
            analysis.status,
            "syntax",
            "{}",
            parser.parse(&source, None).unwrap().root_node().to_sexp()
        );
        for (kind, name) in [
            ("variable", "$tone"),
            ("mixin", "padded"),
            ("function", "double"),
            ("include", "padded"),
            ("use", "palette"),
            ("forward", "shared"),
            ("import", "legacy"),
            ("selector", ".cafe"),
            ("selector", "&:hover"),
        ] {
            assert!(
                analysis
                    .facts
                    .iter()
                    .any(|fact| fact.kind == kind && fact.name.contains(name)),
                "missing {kind} {name}"
            );
        }
        let nested = analysis
            .facts
            .iter()
            .find(|fact| fact.kind == "selector" && fact.name == "&:hover")
            .unwrap();
        let owner = nested.code.as_ref().unwrap().owner_id.as_ref().unwrap();
        assert!(analysis.facts.iter().any(|fact| {
            fact.code
                .as_ref()
                .is_some_and(|code| &code.symbol_id == owner)
                && fact.name == ".cafe"
        }));
        for fact in analysis.facts {
            let code = fact.code.unwrap();
            let span = code.span.unwrap();
            assert!(source[span.start_byte..span.end_byte].contains(&fact.name));
            assert!(code.fingerprints.is_none());
            assert_eq!(code.basis, EvidenceBasis::Syntax);
        }
        Ok(())
    }

    #[test]
    fn comments_and_strings_are_not_declarations_and_interpolation_is_partial() -> Result<()> {
        let analysis = analyze(
            "negative.scss",
            "// @mixin fake() {}\n.note { content: '@include fake;'; }\n",
        )?;
        assert!(
            !analysis
                .facts
                .iter()
                .any(|fact| matches!(fact.kind.as_str(), "mixin" | "include"))
        );
        let interpolation = analyze("dynamic.scss", ".item-#{$name} { color: $tone; }\n")?;
        assert_eq!(interpolation.status, "partial-syntax");
        assert!(
            interpolation
                .facts
                .iter()
                .any(|fact| fact.kind == "interpolation")
        );
        let malformed = analyze("broken.scss", ".broken { color: ")?;
        assert_eq!(malformed.status, "partial-syntax");
        assert!(!malformed.facts.iter().any(|fact| fact.kind == "selector"));
        // The pinned grammar does not accept non-ASCII selector identifiers.
        // Keep this explicit rather than normalizing bytes into a false match.
        let unicode_selector = analyze("unicode.scss", ".café { color: red; }")?;
        assert_eq!(unicode_selector.status, "partial-syntax");
        assert!(
            !unicode_selector
                .facts
                .iter()
                .any(|fact| fact.kind == "selector")
        );
        Ok(())
    }

    #[test]
    fn source_and_fact_limits_fail_explicitly() {
        assert!(analyze("large.scss", &" ".repeat(crate::MAX_FILE_BYTES + 1)).is_err());
        assert!(
            analyze("many.scss", &"$entry: 1;\n".repeat(10_001)).is_err_and(|error| error
                .to_string()
                .contains("shared host extraction budget exhausted"))
        );
    }
}
