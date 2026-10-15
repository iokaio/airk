// SPDX-License-Identifier: Apache-2.0
//! Vue/Svelte spelling candidates over HTML structure, never template compilation.
use crate::{build_facts::Facts, host_html::HtmlDiscovery};
use airk_core::{EvidenceBasis, location::HostBudget};
use anyhow::{Result, ensure};
use std::collections::BTreeMap;
use tree_sitter::{Node, Parser};

pub(crate) fn discover(
    path: &str,
    text: &str,
    adapter_id: &str,
    budget: &mut HostBudget,
) -> Result<HtmlDiscovery> {
    ensure!(
        matches!(adapter_id, "vue" | "svelte"),
        "unknown component host"
    );
    let mut result = crate::host_html::discover(path, text, budget)?;
    let mut facts = Facts::new(path, text, adapter_id)?;
    let mut parser = Parser::new();
    parser.set_language(&tree_sitter_html::LANGUAGE.into())?;
    let tree = parser
        .parse(text, None)
        .ok_or_else(|| anyhow::anyhow!("component host parser returned no tree"))?;
    let mut pending = vec![tree.root_node()];
    while let Some(node) = pending.pop() {
        // This is a distinct overlay walk. Base tags/regions were charged exactly once by HTML.
        budget.charge(1, 0, 0, 0)?;
        if matches!(node.kind(), "start_tag" | "self_closing_tag")
            && !node.has_error()
            && !node.is_missing()
            && let Some(tag_node) = child(node, "tag_name")
        {
            let tag = raw(tag_node, text);
            let mut markers = BTreeMap::new();
            let mut invalid_marker = false;
            let mut cursor = node.walk();
            for attribute in node
                .named_children(&mut cursor)
                .filter(|child| child.kind() == "attribute")
            {
                let Some(name_node) = child(attribute, "attribute_name") else {
                    continue;
                };
                let name = raw(name_node, text);
                if matches!(name, "setup" | "context" | "module" | "scoped")
                    && markers.insert(name, value(attribute, text)).is_some()
                {
                    invalid_marker = true;
                }
                let kind = match adapter_id {
                    "vue" if name.starts_with('@') || name.starts_with("v-on:") => {
                        Some("event_candidate")
                    }
                    "vue" if name.starts_with(':') || name.starts_with("v-bind:") => {
                        Some("prop_candidate")
                    }
                    "vue" if name.starts_with('#') || name.starts_with("v-slot") => {
                        Some("slot_candidate")
                    }
                    "vue" if name.starts_with("v-") => Some("directive_candidate"),
                    "svelte" if name.starts_with("on:") => Some("event_candidate"),
                    "svelte" if name.starts_with("bind:") => Some("prop_candidate"),
                    "svelte"
                        if [
                            "use:",
                            "class:",
                            "style:",
                            "transition:",
                            "in:",
                            "out:",
                            "animate:",
                            "let:",
                        ]
                        .iter()
                        .any(|prefix| name.starts_with(prefix)) =>
                    {
                        Some("directive_candidate")
                    }
                    _ if name == "slot" => Some("slot_candidate"),
                    _ => None,
                };
                if let Some(kind) = kind {
                    push(&mut facts, budget, attribute, kind, raw(attribute, text))?;
                }
            }
            if tag == "slot" {
                push(&mut facts, budget, node, "slot_candidate", raw(node, text))?;
            }
            if adapter_id == "svelte" && tag.starts_with("svelte:") {
                push(&mut facts, budget, node, "component_candidate", tag)?;
            }
            if tag.eq_ignore_ascii_case("script") {
                let module = markers.get("module");
                let context = markers.get("context");
                let setup = markers.get("setup");
                if adapter_id == "svelte" {
                    invalid_marker |= setup.is_some()
                        || (module.is_some() && context.is_some())
                        || module.is_some_and(|value| !value.is_empty())
                        || context.is_some_and(|value| *value != "module");
                    if !invalid_marker && (module.is_some() || context.is_some()) {
                        push(&mut facts, budget, node, "script_module", raw(node, text))?;
                    }
                } else {
                    invalid_marker |= context.is_some()
                        || module.is_some()
                        || setup.is_some_and(|value| !value.is_empty());
                    if !invalid_marker && setup.is_some() {
                        push(&mut facts, budget, node, "script_setup", raw(node, text))?;
                    }
                }
                if invalid_marker {
                    diagnostic(
                        &mut result,
                        node,
                        "conflicting, duplicate or unsupported script context/setup markers",
                    );
                    if let Some(parent) = node.parent() {
                        result.regions.retain(|region| {
                            !(region.span.start >= parent.start_byte()
                                && region.span.end <= parent.end_byte())
                        });
                    }
                }
            }
            if tag.eq_ignore_ascii_case("style")
                && let Some(scoped) = markers.get("scoped")
            {
                if adapter_id == "vue" && scoped.is_empty() && !invalid_marker {
                    push(&mut facts, budget, node, "scoped_style", raw(node, text))?;
                } else {
                    diagnostic(
                        &mut result,
                        node,
                        "unsupported or duplicate scoped-style marker",
                    );
                }
            }
        }
        if node.kind() == "text" {
            let source = raw(node, text);
            if (adapter_id == "vue" && source.contains("{{"))
                || (adapter_id == "svelte" && source.contains('{'))
            {
                push(
                    &mut facts,
                    budget,
                    node,
                    "template_expression_candidate",
                    source,
                )?;
                diagnostic(
                    &mut result,
                    node,
                    "template expressions/control blocks remain unparsed written text",
                );
            }
        }
        // HTML raw script/style and comments cannot fabricate host directives.
        if !matches!(node.kind(), "raw_text" | "comment") {
            for index in (0..node.named_child_count()).rev() {
                ensure!(
                    pending.len() < 500_000,
                    "component host pending limit exceeded"
                );
                if let Some(child) = node.named_child(index) {
                    pending.push(child);
                }
            }
        }
    }
    ensure!(
        result.facts.len() + facts.facts.len() <= 10_000,
        "component host fact limit exceeded"
    );
    result.facts.extend(facts.facts);
    ensure!(
        serde_json::to_vec(&result.facts)?.len() <= 16_777_216,
        "component evidence byte limit exceeded"
    );
    Ok(result)
}

fn child<'a>(node: Node<'a>, kind: &str) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .find(|child| child.kind() == kind)
}
fn raw<'a>(node: Node<'_>, text: &'a str) -> &'a str {
    text.get(node.byte_range()).unwrap_or("")
}
fn value<'a>(attribute: Node<'_>, text: &'a str) -> &'a str {
    child(attribute, "attribute_value")
        .or_else(|| {
            child(attribute, "quoted_attribute_value")
                .and_then(|quoted| child(quoted, "attribute_value"))
        })
        .map(|node| raw(node, text))
        .unwrap_or("")
}
fn push(
    facts: &mut Facts<'_>,
    budget: &mut HostBudget,
    node: Node<'_>,
    kind: &str,
    name: &str,
) -> Result<()> {
    budget.charge(0, 1, 0, 0)?;
    facts.push(node.start_byte(), node.end_byte(), kind, node.kind(), name)?;
    let code = facts
        .facts
        .last_mut()
        .expect("component fact")
        .code
        .as_mut()
        .expect("component evidence");
    code.basis = EvidenceBasis::StructuralRule;
    code.producer = "airk-host-structure/components".into();
    code.producer_version = "composite-facts/1".into();
    code.coverage = "Written Vue/Svelte template, attribute and block-marker candidates; expressions, component bindings and framework behavior are not compiled, evaluated or resolved".into();
    Ok(())
}
fn diagnostic(result: &mut HtmlDiscovery, node: Node<'_>, message: &str) {
    result.partial = true;
    if result.diagnostics.len() < 256 {
        result.diagnostics.push(format!(
            "component bytes {}..{}: {message}",
            node.start_byte(),
            node.end_byte()
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn budget() -> HostBudget {
        HostBudget::new(500_000, 10_000, 16_777_216).unwrap()
    }

    #[test]
    fn vue_setup_scoped_style_and_template_candidates_keep_source_spans() -> Result<()> {
        let text = include_str!("../tests/fixtures/components/Original.vue")
            .replace("\r\n", "\n")
            .replace('\n', "\r\n");
        let result = discover("Original.vue", &text, "vue", &mut budget())?;
        assert_eq!(
            result
                .regions
                .iter()
                .map(|region| region.adapter_id)
                .collect::<Vec<_>>(),
            ["typescript", "scss"]
        );
        for kind in [
            "script_setup",
            "scoped_style",
            "event_candidate",
            "prop_candidate",
            "slot_candidate",
            "template_expression_candidate",
        ] {
            assert!(
                result.facts.iter().any(|fact| fact.kind == kind),
                "missing {kind}"
            );
        }
        assert!(result.partial, "unparsed interpolation remains partial");
        for fact in result.facts {
            let span = fact.code.unwrap().span.unwrap();
            assert!(text[span.start_byte..span.end_byte].contains(&fact.name));
        }
        Ok(())
    }

    #[test]
    fn svelte_module_events_slots_and_bindings_are_written_candidates() -> Result<()> {
        let text = include_str!("../tests/fixtures/components/Original.svelte");
        let result = discover("Original.svelte", text, "svelte", &mut budget())?;
        assert_eq!(result.regions.len(), 2);
        for kind in [
            "script_module",
            "event_candidate",
            "prop_candidate",
            "slot_candidate",
        ] {
            assert!(
                result.facts.iter().any(|fact| fact.kind == kind),
                "missing {kind}"
            );
        }
        Ok(())
    }

    #[test]
    fn conflicts_comments_and_script_literals_cannot_enable_false_host_facts() -> Result<()> {
        let result = discover(
            "x.svelte",
            "<!-- <button on:click='fake'/> --><script module context='module'>let x=1;</script>",
            "svelte",
            &mut budget(),
        )?;
        assert!(result.partial && result.regions.is_empty());
        assert!(
            !result
                .facts
                .iter()
                .any(|fact| fact.kind == "event_candidate")
        );
        let result = discover(
            "x.vue",
            "<script setup>const text = '<button @click=fake />';</script>",
            "vue",
            &mut budget(),
        )?;
        assert!(
            !result
                .facts
                .iter()
                .any(|fact| fact.kind == "event_candidate")
        );
        assert_eq!(result.regions.len(), 1);
        Ok(())
    }
}
