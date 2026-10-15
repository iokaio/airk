// SPDX-License-Identifier: Apache-2.0
//! HTML host discovery. Regions contain original verbatim bytes, never fetched resources.
use crate::build_facts::Facts;
use airk_core::{
    EvidenceBasis, Fact,
    location::{ByteSpan, HostBudget},
};
use anyhow::{Result, ensure};
use std::collections::BTreeMap;
use tree_sitter::{Node, Parser};

pub(crate) struct HtmlDiscovery {
    pub regions: Vec<HtmlRegion>,
    pub facts: Vec<Fact>,
    pub diagnostics: Vec<String>,
    pub partial: bool,
}
pub(crate) struct HtmlRegion {
    pub adapter_id: &'static str,
    pub span: ByteSpan,
    pub kind: &'static str,
}

pub(crate) fn discover(path: &str, text: &str, budget: &mut HostBudget) -> Result<HtmlDiscovery> {
    let mut facts = Facts::new(path, text, "html")?;
    let mut parser = Parser::new();
    parser.set_language(&tree_sitter_html::LANGUAGE.into())?;
    let tree = parser
        .parse(text, None)
        .ok_or_else(|| anyhow::anyhow!("HTML parser returned no tree"))?;
    let mut result = HtmlDiscovery {
        regions: vec![],
        facts: vec![],
        diagnostics: vec![],
        partial: tree.root_node().has_error(),
    };
    let mut pending = vec![tree.root_node()];
    while let Some(node) = pending.pop() {
        budget.charge(1, 0, 0, 0)?;
        if matches!(node.kind(), "script_element" | "style_element") {
            budget.enter_region(1)?;
            let kind = if node.kind() == "script_element" {
                "script"
            } else {
                "style"
            };
            if node.has_error() || node.is_missing() || child(node, "end_tag").is_none() {
                diagnostic(&mut result, node, "malformed or unclosed child block");
            } else if let Some(start) = child(node, "start_tag") {
                match select(start, text, kind) {
                    Ok(adapter_id) => {
                        if let Some(raw) = child(node, "raw_text") {
                            let span = ByteSpan {
                                start: raw.start_byte(),
                                end: raw.end_byte(),
                            };
                            span.validate(text)?;
                            budget.charge(0, 0, span.end - span.start, 1)?;
                            result.regions.push(HtmlRegion {
                                adapter_id,
                                span,
                                kind,
                            });
                        }
                    }
                    Err(reason) => diagnostic(&mut result, node, reason),
                }
            }
        }
        if matches!(node.kind(), "start_tag" | "self_closing_tag")
            && !node.has_error()
            && !node.is_missing()
            && let Some(name) = child(node, "tag_name")
        {
            let tag = name.utf8_text(text.as_bytes())?;
            if (tag.eq_ignore_ascii_case("script") || tag.eq_ignore_ascii_case("style"))
                && !node.parent().is_some_and(|parent| {
                    matches!(parent.kind(), "script_element" | "style_element")
                })
            {
                budget.enter_region(1)?;
                diagnostic(
                    &mut result,
                    node,
                    "child tag lacks a complete script/style element",
                );
            }
            push(&mut facts, budget, node, "tag", tag, EvidenceBasis::Syntax)?;
            if tag.contains('-') || tag.chars().next().is_some_and(char::is_uppercase) {
                push(
                    &mut facts,
                    budget,
                    node,
                    "component_candidate",
                    tag,
                    EvidenceBasis::StructuralRule,
                )?;
            }
            if tag.eq_ignore_ascii_case("template") {
                push(
                    &mut facts,
                    budget,
                    node,
                    "template",
                    tag,
                    EvidenceBasis::Syntax,
                )?;
            }
        }
        for index in (0..node.named_child_count()).rev() {
            ensure!(pending.len() < 500_000, "HTML pending node limit exceeded");
            if let Some(child) = node.named_child(index) {
                pending.push(child);
            }
        }
    }
    ensure!(
        serde_json::to_vec(&facts.facts)?.len() <= 16_777_216,
        "HTML host evidence byte limit exceeded"
    );
    result.facts = facts.facts;
    Ok(result)
}

fn child<'a>(node: Node<'a>, kind: &str) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .find(|child| child.kind() == kind)
}

fn push(
    facts: &mut Facts<'_>,
    budget: &mut HostBudget,
    node: Node<'_>,
    kind: &str,
    name: &str,
    basis: EvidenceBasis,
) -> Result<()> {
    budget.charge(0, 1, 0, 0)?;
    facts.push(node.start_byte(), node.end_byte(), kind, node.kind(), name)?;
    let code = facts
        .facts
        .last_mut()
        .expect("host tag")
        .code
        .as_mut()
        .expect("host evidence");
    code.basis = basis;
    code.producer = if code.basis == EvidenceBasis::Syntax {
        "airk-tree-sitter/html"
    } else {
        "airk-host-structure/html"
    }
    .into();
    code.producer_version = "composite-facts/1".into();
    code.coverage = "Written HTML tags and component spelling candidates; no DOM execution, component resolution or linked-resource fetch".into();
    Ok(())
}

fn diagnostic(result: &mut HtmlDiscovery, node: Node<'_>, reason: &str) {
    result.partial = true;
    // Region attempts are independently limited to256; this bound also holds if callers lower it.
    if result.diagnostics.len() < 256 {
        result.diagnostics.push(format!(
            "{} at bytes {}..{} omitted: {reason}",
            node.kind(),
            node.start_byte(),
            node.end_byte()
        ));
    }
}

fn select(
    start: Node<'_>,
    text: &str,
    kind: &str,
) -> std::result::Result<&'static str, &'static str> {
    let mut attrs = BTreeMap::new();
    let mut cursor = start.walk();
    for attribute in start
        .named_children(&mut cursor)
        .filter(|node| node.kind() == "attribute")
    {
        let Some(name) = child(attribute, "attribute_name") else {
            return Err("malformed attribute");
        };
        let name = name
            .utf8_text(text.as_bytes())
            .map_err(|_| "invalid attribute")?
            .to_ascii_lowercase();
        if name == "src" {
            return Err("external source attribute is not fetched");
        }
        if matches!(
            name.as_str(),
            ":lang" | ":type" | ":src" | "v-bind:lang" | "v-bind:type" | "v-bind:src"
        ) {
            return Err("dynamic child marker");
        }
        if !matches!(name.as_str(), "type" | "lang" | "language") {
            continue;
        }
        let value = child(attribute, "attribute_value").or_else(|| {
            child(attribute, "quoted_attribute_value")
                .and_then(|quoted| child(quoted, "attribute_value"))
        });
        let value = value
            .map(|value| value.utf8_text(text.as_bytes()).unwrap_or(""))
            .unwrap_or("")
            .trim();
        if value.len() > 128 || value.contains(['&', '{', '}', '<', '>']) {
            return Err("nonliteral child marker");
        }
        if attrs.insert(name, value.to_ascii_lowercase()).is_some() {
            return Err("duplicate child marker");
        }
    }
    let mut selected = None;
    for (name, value) in attrs {
        let candidate = match (kind, name.as_str(), value.as_str()) {
            ("script", "type", "" | "module") => continue,
            (
                "script",
                "type",
                "text/javascript"
                | "application/javascript"
                | "text/ecmascript"
                | "application/ecmascript",
            ) => "javascript",
            ("script", "type", "text/typescript" | "application/typescript") => "typescript",
            ("script", "lang" | "language", "js" | "javascript") => "javascript",
            ("script", "lang" | "language", "ts" | "typescript") => "typescript",
            ("style", "type", "" | "text/css") => "css",
            ("style", "type", "text/scss") => "scss",
            ("style", "lang" | "language", "css") => "css",
            ("style", "lang" | "language", "scss") => "scss",
            _ => return Err("unsupported child marker"),
        };
        if selected.is_some_and(|previous| previous != candidate) {
            return Err("conflicting child markers");
        }
        selected = Some(candidate);
    }
    Ok(selected.unwrap_or(if kind == "script" {
        "javascript"
    } else {
        "css"
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn budget() -> HostBudget {
        HostBudget::new(500_000, 10_000, 16_777_216).unwrap()
    }

    #[test]
    fn direct_regions_preserve_unicode_crlf_and_marker_selection() -> Result<()> {
        let text = "<template><My-Card /></template>\r\n<script type=\"module\">const café = 1;\r\n</script><script lang='ts'>let value: number = 2;</script><style>.café { color:red }</style><style lang=scss>$tone:red;</style>";
        let result = discover("page.html", text, &mut budget())?;
        assert!(!result.partial, "{:?}", result.diagnostics);
        assert_eq!(
            result
                .regions
                .iter()
                .map(|region| region.adapter_id)
                .collect::<Vec<_>>(),
            ["javascript", "typescript", "css", "scss"]
        );
        assert_eq!(
            &text[result.regions[0].span.start..result.regions[0].span.end],
            "const café = 1;\r\n"
        );
        assert!(
            result
                .facts
                .iter()
                .any(|fact| fact.kind == "component_candidate")
        );
        assert!(result.facts.iter().any(|fact| fact.kind == "template"));
        Ok(())
    }

    #[test]
    fn comments_external_unknown_conflicting_and_duplicate_markers_are_not_children() -> Result<()>
    {
        let text = "<!-- <script>fake()</script> --><script src='remote.js'>ignored()</script><script type='application/ld+json'>{}</script><script type='text/javascript' lang='ts'>ignored()</script><style lang=css lang=scss>ignored</style>";
        let result = discover("page.html", text, &mut budget())?;
        assert!(result.partial);
        assert!(result.regions.is_empty());
        assert_eq!(result.diagnostics.len(), 4);
        assert!(!result.facts.iter().any(|fact| fact.name.contains("fake")));
        Ok(())
    }

    #[test]
    fn missing_closer_is_partial_and_empty_or_unsupported_attempts_consume_budget() -> Result<()> {
        let result = discover("page.html", "<script>const broken = 1;", &mut budget())?;
        assert!(result.partial);
        assert!(result.regions.is_empty());
        for tag in ["<script></script>", "<script type=unknown></script>"] {
            let result = discover("page.html", &tag.repeat(257), &mut budget());
            assert!(result.is_err_and(|error| error.to_string().contains("region budget")));
        }
        Ok(())
    }

    #[test]
    fn host_and_child_mapping_work_use_the_supplied_budget() {
        let mut nodes = HostBudget::new(0, 10_000, 16_777_216).unwrap();
        assert!(discover("page.html", "<p>one</p>", &mut nodes).is_err());
        let mut text = HostBudget::new(500_000, 10_000, 2).unwrap();
        assert!(discover("page.html", "<script>four</script>", &mut text).is_err());
        let mut maps = budget();
        maps.charge(0, 0, 0, 100_000).unwrap();
        assert!(discover("page.html", "<script>x</script>", &mut maps).is_err());
    }
}
