// SPDX-License-Identifier: Apache-2.0
//! Written Razor structure and contiguous C# member regions; no template execution.
use crate::{adapters::registry::AdapterRegistry, build_facts::Facts};
use airk_core::{
    Fact,
    extraction::LocalRegion,
    location::{ByteSpan, HostBudget, MapSegment, Mapping, RegionLocation, SourceMap},
};
use anyhow::{Context, Result, ensure};
use tree_sitter::{Node, Parser};

pub struct RazorDiscovery {
    pub regions: Vec<LocalRegion>,
    pub facts: Vec<Fact>,
    pub diagnostics: Vec<String>,
    pub partial: bool,
}
impl RazorDiscovery {
    fn diagnostic(&mut self, message: &str) {
        self.partial = true;
        if self.diagnostics.len() < 256 {
            self.diagnostics.push(message.into());
        }
    }
}

pub fn discover(path: &str, text: &str, budget: &mut HostBudget) -> Result<RazorDiscovery> {
    ensure!(text.len() <= 16_777_216, "Razor source exceeds byte limit");
    let mut parser = Parser::new();
    parser.set_language(&tree_sitter_razor::LANGUAGE.into())?;
    let tree = parser
        .parse(text, None)
        .context("Razor parser returned no tree")?;
    let mut output = RazorDiscovery {
        regions: vec![],
        facts: vec![],
        diagnostics: vec![],
        partial: false,
    };
    if tree.root_node().has_error() {
        output.diagnostic(
            "Malformed Razor syntax; only independently valid written nodes are retained",
        );
    }
    let mut facts = Facts::new(path, text, "razor")?;
    let registry = AdapterRegistry::baseline();
    let mut pending = vec![tree.root_node()];
    while let Some(node) = pending.pop() {
        budget.charge(1, 0, 0, 0)?;
        let kind = node.kind();
        if kind == "razor_comment" || kind == "html_comment" {
            continue;
        }
        if kind == "razor_block" {
            budget.enter_region(1)?;
            if let Some(span) = member_body(node, text, budget)? {
                if span.start < span.end {
                    budget.charge(0, 0, span.end - span.start, 1)?;
                    let local = ByteSpan {
                        start: 0,
                        end: span.end - span.start,
                    };
                    let id = u32::try_from(node.start_byte())?
                        .checked_add(1)
                        .context("Razor region ID overflow")?;
                    let source_map = SourceMap {
                        version: 1,
                        segments: vec![MapSegment {
                            local,
                            mapping: Mapping::Verbatim { host: span },
                        }],
                    };
                    let body = text[span.start..span.end].to_owned();
                    source_map.validate(&body, text)?;
                    output.regions.push(LocalRegion {
                        id,
                        parent: None,
                        profile: registry.effective_profile(path, "csharp")?,
                        text: body,
                        source_map,
                        location: RegionLocation::Embedded {
                            region_id: format!("razor-{id}"),
                            local_span: local,
                            original_spans: vec![span],
                        },
                    });
                }
            } else {
                output.diagnostic("Razor block is anonymous, malformed or mixed with template syntax; no contiguous C# region admitted");
            }
            continue;
        }
        if kind.ends_with("_directive") && kind.starts_with("razor_") && !node.has_error() {
            let fact_kind = match kind {
                "razor_page_directive" => "razor_route",
                "razor_inject_directive" => "razor_injection",
                _ => "razor_directive",
            };
            budget.charge(0, 1, node.byte_range().len().min(4096), 0)?;
            facts.node(node, fact_kind)?;
            continue;
        }
        if kind == "element" {
            // The grammar hides _tag_name. Inspect only its literal opening token,
            // anchored to an actual element node, never an arbitrary '<' in C#.
            if let Some((start, end)) = component_name(node, text) {
                budget.charge(0, 1, end - start, 0)?;
                facts.push(start, end, "razor_component", "element", &text[start..end])?;
            }
        } else if kind.starts_with("razor_") && !kind.ends_with("_directive") {
            output.diagnostic("Dynamic Razor expression or control flow is retained as host text only; no evaluation or resolved binding");
        }
        let mut cursor = node.walk();
        // Bound the queue as well as visited nodes.
        for child in node.named_children(&mut cursor) {
            ensure!(
                pending.len() < 500_000,
                "Razor traversal queue exceeds limit"
            );
            pending.push(child);
        }
    }
    for fact in &mut facts.facts {
        if let Some(code) = &mut fact.code {
            code.producer = "airk-razor-structure".into();
            code.producer_version = "composite-facts/1".into();
            code.coverage = "Written Razor directives and literal component spellings; no compiler, route, dependency-injection or component resolution".into();
        }
    }
    output.facts = facts.facts;
    Ok(output)
}

fn member_body(node: Node<'_>, text: &str, budget: &mut HostBudget) -> Result<Option<ByteSpan>> {
    if node.has_error() {
        return Ok(None);
    }
    let mut cursor = node.walk();
    let Some(open) = node.children(&mut cursor).find(|n| n.kind() == "{") else {
        return Ok(None);
    };
    // The grammar aliases a sequence; '@' and the keyword can be separate
    // children. Qualify the written prefix against the actual brace token.
    if !matches!(
        text[node.start_byte()..open.start_byte()].trim(),
        "@code" | "@functions"
    ) {
        return Ok(None);
    }
    let Some(close) = node
        .child(node.child_count().saturating_sub(1))
        .filter(|n| n.kind() == "}")
    else {
        return Ok(None);
    };
    let mut pending = vec![node];
    while let Some(child) = pending.pop() {
        budget.charge(1, 0, 0, 0)?;
        if child.id() != node.id()
            && (child.kind() == "element"
                || child.kind() == "html_comment"
                || child.kind().starts_with("razor_"))
        {
            return Ok(None);
        }
        let mut cursor = child.walk();
        pending.extend(child.named_children(&mut cursor));
        ensure!(
            pending.len() <= 500_000,
            "Razor member traversal queue exceeds limit"
        );
    }
    Ok(Some(ByteSpan {
        start: open.end_byte(),
        end: close.start_byte(),
    }))
}

fn component_name(node: Node<'_>, text: &str) -> Option<(usize, usize)> {
    let start = node.start_byte().checked_add(1)?;
    let source = text.get(start..node.end_byte())?;
    let first = source.chars().next()?;
    if !first.is_ascii_uppercase() {
        return None;
    }
    let length = source
        .bytes()
        .take(4097)
        .take_while(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.'))
        .count();
    if length == 0 || length > 4096 {
        return None;
    }
    let end = start + length;
    if !text
        .as_bytes()
        .get(end)
        .is_some_and(|b| b.is_ascii_whitespace() || matches!(b, b'/' | b'>'))
    {
        return None;
    }
    Some((start, end))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn budget() -> HostBudget {
        HostBudget::new(500_000, 10_000, 16_777_216).unwrap()
    }

    #[test]
    fn original_directives_and_member_bytes_keep_crlf_and_non_bmp() -> Result<()> {
        let text = "@page \"/beacon\"\r\n@inject IService BeaconService\r\n<BeaconCard />\r\n@code {\r\nprivate string BeaconText = \"🛰\";\r\n}\r\n";
        let result = discover("Beacon.razor", text, &mut budget())?;
        assert!(result.facts.iter().any(|f| f.kind == "razor_route"));
        assert!(result.facts.iter().any(|f| f.kind == "razor_injection"));
        assert!(
            result
                .facts
                .iter()
                .any(|f| f.kind == "razor_component" && f.name == "BeaconCard")
        );
        assert_eq!(result.regions.len(), 1);
        let region = &result.regions[0];
        assert_eq!(region.text, "\r\nprivate string BeaconText = \"🛰\";\r\n");
        region.source_map.validate(&region.text, text)?;
        region
            .location
            .validate(&region.text, text, &region.source_map)?;
        Ok(())
    }

    #[test]
    fn comments_mixed_and_malformed_blocks_do_not_invent_member_regions() -> Result<()> {
        let commented = discover(
            "View.cshtml",
            "@* @inject Fake Hidden *@\n<div>plain</div>",
            &mut budget(),
        )?;
        assert!(!commented.facts.iter().any(|f| f.kind == "razor_injection"));
        for text in [
            "@{ <span>hello</span> }",
            "@code { <span>hello</span> }",
            "@code { private void Broken(",
        ] {
            let result = discover("View.razor", text, &mut budget())?;
            assert!(result.regions.is_empty(), "{text}");
            assert!(result.partial, "{text}");
        }
        Ok(())
    }

    #[test]
    fn attempts_and_shared_work_are_bounded() -> Result<()> {
        assert!(discover("View.razor", &"@code {}\n".repeat(257), &mut budget()).is_err());
        assert!(
            discover(
                "View.razor",
                "@code { int Beacon = 1; }",
                &mut HostBudget::new(1, 1, 32)?
            )
            .is_err()
        );
        Ok(())
    }

    #[test]
    fn functions_are_distinct_from_dynamic_template_expressions() -> Result<()> {
        let text = "@functions { private int BeaconValue() { return 7; } }\n<p>@BeaconValue()</p>";
        let result = discover("View.cshtml", text, &mut budget())?;
        assert_eq!(result.regions.len(), 1);
        assert_eq!(
            result.regions[0].text,
            " private int BeaconValue() { return 7; } "
        );
        assert!(result.partial);
        Ok(())
    }
}
