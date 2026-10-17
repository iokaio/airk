// SPDX-License-Identifier: Apache-2.0
//! Written project build cues; no evaluation, package resolution, or build graph.
mod bazel;
mod gradle;

use crate::build_facts::Facts;
use anyhow::{Result, ensure};
use tree_sitter::{Node, Parser};

pub(crate) fn analyze(path: &str, text: &str, adapter_id: &str) -> Result<crate::syntax::Analysis> {
    ensure!(
        text.len() <= crate::MAX_STRUCTURED_BYTES,
        "project source exceeds byte limit"
    );
    let mut parser = Parser::new();
    let language = match adapter_id {
        "starlark" => tree_sitter_python::LANGUAGE.into(),
        "gradle-kotlin" => tree_sitter_kotlin_ng::LANGUAGE.into(),
        _ => anyhow::bail!("unknown project build adapter"),
    };
    parser.set_language(&language)?;
    let tree = parser
        .parse(text, None)
        .ok_or_else(|| anyhow::anyhow!("project parser returned no tree"))?;
    let mut facts = Facts::new(path, text, adapter_id)?;
    let mut pending = vec![(tree.root_node(), Context::None)];
    let mut visited = 0usize;
    while let Some((node, context)) = pending.pop() {
        visited += 1;
        ensure!(visited <= 500_000, "project parser node limit exceeded");
        let next = if !node.has_error() && !node.is_missing() {
            match adapter_id {
                "starlark" => bazel::visit(node, text, context, &mut facts)?,
                _ => gradle::visit(node, text, context, &mut facts)?,
            }
        } else {
            context
        };
        for index in (0..node.named_child_count()).rev() {
            ensure!(
                pending.len() < 500_000,
                "project parser pending limit exceeded"
            );
            if let Some(child) = node.named_child(index) {
                pending.push((child, next));
            }
        }
    }
    for fact in &mut facts.facts {
        fact.code
            .as_mut()
            .expect("structural evidence")
            .producer_version = "project-facts/1".into();
    }
    let mut overlay = facts.finish(tree.root_node().has_error());
    if adapter_id == "gradle-kotlin" {
        let mut native = crate::syntax::analyze_selected(path, text, "kotlin")?;
        ensure!(
            native.facts.len() + overlay.facts.len() <= 10_000,
            "combined project fact limit exceeded"
        );
        if native.status == "partial-syntax" {
            overlay.status = native.status;
        }
        native.facts.append(&mut overlay.facts);
        overlay.facts = native.facts;
    }
    ensure!(
        serde_json::to_vec(&overlay.facts)?.len() <= 16_777_216,
        "project evidence byte limit exceeded"
    );
    Ok(overlay)
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum Context {
    None,
    Dependencies,
    Plugins,
    Tasks,
    Configurations,
    Load,
}

fn text<'a>(node: Node<'_>, source: &'a str) -> &'a str {
    source.get(node.byte_range()).unwrap_or("")
}

fn named<'a>(node: Node<'a>, kind: &str) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .find(|child| child.kind() == kind)
}

fn emit(facts: &mut Facts<'_>, node: Node<'_>, kind: &str, name: &str) -> Result<()> {
    facts.push(node.start_byte(), node.end_byte(), kind, node.kind(), name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bazel_loads_targets_labels_and_macros_keep_original_spans() -> Result<()> {
        let source = include_str!("../tests/fixtures/project-build/BUILD.bazel")
            .replace("\r\n", "\n")
            .replace('\n', "\r\n");
        let result = analyze("BUILD.bazel", &source, "starlark")?;
        for (kind, name) in [
            ("load", "rules.bzl"),
            ("build_target", "café"),
            ("label", "@external//archive:reader"),
            ("build_function", "publish_local"),
        ] {
            assert!(
                result
                    .facts
                    .iter()
                    .any(|fact| fact.kind == kind && fact.name.contains(name)),
                "missing {kind} {name}"
            );
        }
        for fact in result.facts {
            let code = fact.code.unwrap();
            let span = code.span.unwrap();
            assert!(source[span.start_byte..span.end_byte].contains(&fact.name));
            assert!(code.fingerprints.is_none());
            assert_eq!(code.producer_version, "project-facts/1");
        }
        Ok(())
    }

    #[test]
    fn gradle_structural_overlay_preserves_generic_kotlin_definitions() -> Result<()> {
        let source = include_str!("../tests/fixtures/project-build/build.gradle.kts");
        let result = analyze("build.gradle.kts", source, "gradle-kotlin")?;
        for (kind, name) in [
            ("plugin", "fictional.local"),
            ("dependency", "fictional:archive:1.2"),
            ("task", "inspectCafé"),
            ("configuration", "archiveTools"),
            ("module", ":shared"),
            ("definition", "localCount"),
        ] {
            assert!(
                result
                    .facts
                    .iter()
                    .any(|fact| fact.kind == kind && fact.name.contains(name)),
                "missing {kind} {name}"
            );
        }
        assert!(
            result
                .facts
                .iter()
                .filter(|fact| fact.kind == "task")
                .all(|fact| !fact.name.starts_with("println"))
        );
        assert!(
            result
                .facts
                .iter()
                .filter_map(|fact| fact.code.as_ref())
                .filter(|code| code.basis == airk_core::EvidenceBasis::StructuralRule)
                .all(|code| code.fingerprints.is_none())
        );
        Ok(())
    }

    #[test]
    fn strings_comments_and_malformed_calls_do_not_become_targets() -> Result<()> {
        let bazel = analyze(
            "BUILD",
            "# fictional_library(name = 'fake')\nnote = \"load('fake')\"\nfictional_library(name = )\n",
            "starlark",
        )?;
        assert_eq!(bazel.status, "partial-syntax");
        assert!(
            !bazel
                .facts
                .iter()
                .any(|fact| matches!(fact.kind.as_str(), "load" | "build_target"))
        );
        let kotlin = analyze(
            "build.gradle.kts",
            "// tasks.register(\"fake\")\nval note = \"dependencies { implementation(fake) }\"\n",
            "gradle-kotlin",
        )?;
        assert!(
            !kotlin
                .facts
                .iter()
                .any(|fact| matches!(fact.kind.as_str(), "task" | "dependency" | "plugin"))
        );
        Ok(())
    }

    #[test]
    fn target_overflow_fails_explicitly() {
        let text = "fictional_library(name = 'entry')\n".repeat(5001);
        assert!(
            analyze("BUILD", &text, "starlark")
                .is_err_and(|error| error.to_string().contains("fact limit"))
        );
    }
}
