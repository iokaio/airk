// SPDX-License-Identifier: Apache-2.0
//! Bounded Astro/MDX source islands, not a framework or MDX compiler.
use crate::{
    build_facts::Facts,
    host_html::{HtmlDiscovery, HtmlRegion},
};
use airk_core::{
    DocumentLocation, EvidenceBasis,
    location::{ByteSpan, HostBudget},
};
use anyhow::{Result, ensure};
use pulldown_cmark::{Event, Options, Parser as Markdown, Tag, TagEnd};
use tree_sitter::{Node, Parser};

pub(crate) struct ProseDiscovery {
    pub host: HtmlDiscovery,
    /// Exact source bytes omitted from the host prose pass, including frontmatter delimiters.
    pub exclusions: Vec<ByteSpan>,
}

pub(crate) fn discover(
    path: &str,
    text: &str,
    adapter: &str,
    budget: &mut HostBudget,
) -> Result<ProseDiscovery> {
    ensure!(
        text.len() <= crate::MAX_STRUCTURED_BYTES,
        "prose host source limit exceeded"
    );
    match adapter {
        "astro" => astro(path, text, budget),
        "mdx" => mdx(path, text, budget),
        _ => anyhow::bail!("unknown prose host adapter"),
    }
}

fn empty() -> ProseDiscovery {
    ProseDiscovery {
        host: HtmlDiscovery {
            regions: vec![],
            facts: vec![],
            diagnostics: vec![],
            partial: false,
        },
        exclusions: vec![],
    }
}

fn astro(path: &str, text: &str, budget: &mut HostBudget) -> Result<ProseDiscovery> {
    let mut result = empty();
    let mut suffix = 0;
    let initial = usize::from(text.starts_with('\u{feff}')) * '\u{feff}'.len_utf8();
    let first_end = text[initial..]
        .find('\n')
        .map_or(text.len(), |offset| initial + offset + 1);
    if text[initial..first_end].trim_end_matches(['\r', '\n']) == "---" {
        budget.enter_region(1)?;
        let mut offset = first_end;
        let mut close = None;
        for line in text[first_end..].split_inclusive('\n') {
            budget.charge(1, 0, 0, 0)?;
            if line.trim_end_matches(['\r', '\n']) == "---" {
                close = Some((offset, offset + line.len()));
                break;
            }
            offset += line.len();
        }
        if let Some((end, after)) = close {
            suffix = after;
            result.exclusions.push(ByteSpan {
                start: initial,
                end: after,
            });
            if end > first_end {
                let span = ByteSpan {
                    start: first_end,
                    end,
                };
                budget.charge(0, 0, end - first_end, 1)?;
                result.host.regions.push(HtmlRegion {
                    adapter_id: "typescript",
                    span,
                    kind: "frontmatter",
                });
            }
        } else {
            result.exclusions.push(ByteSpan {
                start: initial,
                end: text.len(),
            });
            omission(
                &mut result.host,
                initial,
                text.len(),
                "unclosed Astro frontmatter",
            );
            return Ok(result);
        }
    }
    // Parsing an actual source suffix keeps every returned span a direct translation.
    let mut html = crate::host_html::discover(path, &text[suffix..], budget)?;
    let line_offset = text[..suffix].bytes().filter(|byte| *byte == b'\n').count();
    for region in &mut html.regions {
        region.span.start += suffix;
        region.span.end += suffix;
    }
    for fact in &mut html.facts {
        fact.start_line += line_offset;
        fact.end_line += line_offset;
        if let DocumentLocation::Lines { start, end } = &mut fact.location {
            *start += line_offset;
            *end += line_offset;
        }
        if let Some(code) = fact.code.as_mut()
            && let Some(span) = code.span.as_mut()
        {
            span.start_byte += suffix;
            span.end_byte += suffix;
            code.symbol_id = format!(
                "syntax:{path}:{}:{}:{}",
                span.start_byte, span.end_byte, fact.syntax_kind
            );
        }
    }
    result.host.regions.extend(html.regions);
    result.host.facts.extend(html.facts);
    result.host.partial |= html.partial;
    result.host.diagnostics.extend(
        html.diagnostics
            .into_iter()
            .map(|diagnostic| format!("template suffix offset {suffix}: {diagnostic}")),
    );
    // Astro expressions are not valid standalone HTML. Leave them explicit, not invented regions.
    let mut cursor = suffix;
    let mut expression = false;
    for region in result
        .host
        .regions
        .iter()
        .filter(|region| region.span.start >= suffix)
    {
        expression |= text[cursor..region.span.start].contains('{');
        cursor = region.span.end;
    }
    expression |= text[cursor..].contains('{');
    if expression {
        omission(
            &mut result.host,
            suffix,
            text.len(),
            "Astro template expressions remain unparsed; only frontmatter and explicit script/style regions are enabled",
        );
    }
    Ok(result)
}

fn mdx(path: &str, text: &str, budget: &mut HostBudget) -> Result<ProseDiscovery> {
    let mut result = empty();
    let mut ignored = Vec::new();
    let mut block_start = None;
    for (event, range) in Markdown::new_ext(text, Options::all()).into_offset_iter() {
        budget.charge(1, 0, 0, 0)?;
        match event {
            Event::Start(Tag::CodeBlock(_)) => block_start = Some(range.start),
            Event::End(TagEnd::CodeBlock) => {
                if let Some(start) = block_start.take() {
                    ignored.push(ByteSpan {
                        start,
                        end: range.end,
                    });
                }
            }
            Event::Code(_) => ignored.push(ByteSpan {
                start: range.start,
                end: range.end,
            }),
            Event::Html(html) | Event::InlineHtml(html)
                if html.trim_start().starts_with("<!--") =>
            {
                ignored.push(ByteSpan {
                    start: range.start,
                    // Pulldown emits a multi-line HTML comment as several
                    // events. Suppress its entire original extent, including
                    // ESM-looking lines across blank Markdown paragraphs.
                    end: text[range.start..]
                        .find("-->")
                        .map_or(text.len(), |offset| range.start + offset + 3),
                })
            }
            _ => {}
        }
    }
    if let Some(start) = block_start {
        ignored.push(ByteSpan {
            start,
            end: text.len(),
        });
    }
    let mut ignored = merge_spans(ignored);
    // Explicit JSX comments can span blank paragraphs; never parse their contents.
    let mut comments = Vec::new();
    let mut comment_end = 0;
    for (start, _) in text.match_indices("{/*") {
        budget.charge(1, 0, 0, 0)?;
        let index = ignored.partition_point(|span| span.end <= start);
        if start < comment_end || ignored.get(index).is_some_and(|span| span.start <= start) {
            continue;
        }
        comment_end = text[start + 3..]
            .find("*/}")
            .map_or(text.len(), |offset| start + 3 + offset + 3);
        comments.push(ByteSpan {
            start,
            end: comment_end,
        });
    }
    ignored.extend(comments);
    let ignored = merge_spans(ignored);
    let mut output = Facts::new(path, text, "mdx")?;
    let mut at = 0;
    while at < text.len() {
        budget.charge(1, 0, 0, 0)?;
        let end = line_end(text, at);
        let line = text[at..end].trim_end_matches(['\r', '\n']);
        let trim = line.trim_start_matches(' ');
        let candidate_start = at + line.len() - trim.len();
        let kind = if trim.starts_with("import ") || trim.starts_with("export ") {
            Some("esm")
        } else if trim.starts_with('<') && !trim.starts_with("<!--") {
            Some("jsx")
        } else if trim.starts_with('{') {
            Some("expression")
        } else {
            None
        };
        let blocked = ignored.partition_point(|span| span.end <= candidate_start);
        if kind.is_none()
            || candidate_start - at > 3
            || ignored
                .get(blocked)
                .is_some_and(|span| span.start <= candidate_start)
        {
            if trim.contains("<") || trim.contains('{') {
                // Fenced/inline examples remain ordinary Markdown examples, not failed child attempts.
                if !ignored.get(blocked).is_some_and(|span| span.start < end) {
                    omission(
                        &mut result.host,
                        at,
                        end,
                        "mixed inline MDX remains unsupported",
                    );
                }
            }
            at = end;
            continue;
        }
        budget.enter_region(1)?;
        let kind = kind.unwrap();
        let mut finish = end;
        while finish < text.len() && finish - candidate_start <= 65_536 {
            let next = line_end(text, finish);
            let ignored_next = ignored.partition_point(|span| span.end <= finish);
            if ignored
                .get(ignored_next)
                .is_some_and(|span| span.start < next)
            {
                break;
            }
            if text[finish..next].trim().is_empty() {
                break;
            }
            finish = next;
        }
        if finish - candidate_start > 65_536 {
            omission(
                &mut result.host,
                candidate_start,
                finish,
                "MDX island exceeds 64 KiB qualification limit",
            );
            at = finish;
            continue;
        }
        let candidate = &text[candidate_start..finish];
        let valid = validate_island(candidate, kind, budget)?;
        result.exclusions.push(ByteSpan {
            start: candidate_start,
            end: finish,
        });
        if valid {
            budget.charge(0, 1, candidate.len(), 1)?;
            output.push(
                candidate_start,
                finish,
                match kind {
                    "esm" => "esm_candidate",
                    "jsx" => "component_candidate",
                    _ => "expression_candidate",
                },
                "mdx_source_island",
                candidate,
            )?;
            let code = output.facts.last_mut().unwrap().code.as_mut().unwrap();
            code.basis = EvidenceBasis::StructuralRule;
            code.producer = "airk-host-structure/prose".into();
            code.producer_version = "composite-facts/1".into();
            code.coverage = "Bounded standalone MDX source island validated as written TSX syntax; not an MDX compiler or evaluated component binding".into();
            result.host.regions.push(HtmlRegion {
                adapter_id: "tsx",
                span: ByteSpan {
                    start: candidate_start,
                    end: finish,
                },
                kind,
            });
        } else {
            omission(
                &mut result.host,
                candidate_start,
                finish,
                "unsupported or malformed MDX source island",
            );
        }
        if kind == "expression" {
            omission(
                &mut result.host,
                candidate_start,
                finish,
                "standalone braced expression is syntax-only; full MDX expression semantics remain unsupported",
            );
        }
        at = finish;
    }
    result.host.facts = output.facts;
    Ok(result)
}

fn line_end(text: &str, start: usize) -> usize {
    text[start..]
        .find('\n')
        .map_or(text.len(), |offset| start + offset + 1)
}
fn omission(host: &mut HtmlDiscovery, start: usize, end: usize, message: &str) {
    host.partial = true;
    if host.diagnostics.len() < 256 {
        host.diagnostics
            .push(format!("prose host bytes {start}..{end}: {message}"));
    }
}
fn validate_island(text: &str, kind: &str, budget: &mut HostBudget) -> Result<bool> {
    let mut parser = Parser::new();
    parser.set_language(&tree_sitter_typescript::LANGUAGE_TSX.into())?;
    let tree = parser
        .parse(text, None)
        .ok_or_else(|| anyhow::anyhow!("MDX island parser returned no tree"))?;
    let root = tree.root_node();
    let mut pending = vec![root];
    while let Some(node) = pending.pop() {
        budget.charge(1, 0, 0, 0)?;
        for index in (0..node.named_child_count()).rev() {
            ensure!(pending.len() < 500_000, "MDX pending node limit exceeded");
            if let Some(child) = node.named_child(index) {
                pending.push(child);
            }
        }
    }
    if root.has_error() {
        return Ok(false);
    }
    let mut cursor = root.walk();
    let nodes: Vec<Node<'_>> = root
        .named_children(&mut cursor)
        .filter(|node| node.kind() != "comment")
        .collect();
    Ok(match kind {
        "esm" => {
            !nodes.is_empty()
                && nodes
                    .iter()
                    .all(|node| matches!(node.kind(), "import_statement" | "export_statement"))
        }
        "jsx" => {
            nodes.len() == 1
                && nodes[0].kind() == "expression_statement"
                && nodes[0].named_child(0).is_some_and(|child| {
                    matches!(child.kind(), "jsx_element" | "jsx_self_closing_element")
                })
        }
        "expression" => {
            nodes.len() == 1
                && nodes[0].kind() == "statement_block"
                && nodes[0].named_child_count() == 1
                && nodes[0]
                    .named_child(0)
                    .is_some_and(|child| child.kind() == "expression_statement")
        }
        _ => false,
    })
}

fn merge_spans(mut spans: Vec<ByteSpan>) -> Vec<ByteSpan> {
    spans.sort_by_key(|span| span.start);
    let mut merged: Vec<ByteSpan> = Vec::new();
    for span in spans {
        if let Some(previous) = merged.last_mut()
            && span.start <= previous.end
        {
            previous.end = previous.end.max(span.end);
        } else {
            merged.push(span);
        }
    }
    merged
}

#[cfg(test)]
mod tests {
    use super::*;
    fn budget() -> HostBudget {
        HostBudget::new(500_000, 10_000, 16_777_216).unwrap()
    }

    #[test]
    fn astro_frontmatter_and_template_children_are_verbatim_with_no_string_injection() -> Result<()>
    {
        let text = include_str!("../tests/fixtures/prose-hosts/Original.astro")
            .replace("\r\n", "\n")
            .replace('\n', "\r\n");
        let result = discover("Original.astro", &text, "astro", &mut budget())?;
        assert_eq!(result.host.regions.len(), 3);
        assert_eq!(result.host.regions[0].kind, "frontmatter");
        assert!(
            text[result.host.regions[0].span.start..result.host.regions[0].span.end]
                .contains("café")
        );
        assert_eq!(result.exclusions.len(), 1);
        assert!(text[result.exclusions[0].start..result.exclusions[0].end].starts_with("---\r\n"));
        assert!(
            result
                .host
                .regions
                .iter()
                .all(|region| region.kind == "frontmatter"
                    || !text[region.span.start..region.span.end].contains("not_a_child"))
        );
        for fact in result.host.facts {
            let span = fact.code.unwrap().span.unwrap();
            assert!(text[span.start_byte..span.end_byte].contains(&fact.name));
        }
        assert!(
            result.host.partial,
            "template expression is explicitly unsupported"
        );
        Ok(())
    }

    #[test]
    fn mdx_real_islands_exclude_fences_inline_code_and_preserve_surrounding_prose() -> Result<()> {
        let text = include_str!("../tests/fixtures/prose-hosts/Original.mdx")
            .replace("\r\n", "\n")
            .replace('\n', "\r\n");
        let result = discover("Original.mdx", &text, "mdx", &mut budget())?;
        assert_eq!(result.host.regions.len(), 4);
        assert!(result.host.regions.iter().all(|region| {
            !text[region.span.start..region.span.end].contains("Fake")
                && !text[region.span.start..region.span.end].contains("fake")
        }));
        let requirement = text.find("REQ-884").unwrap();
        assert!(
            !result
                .exclusions
                .iter()
                .any(|span| span.start <= requirement && requirement < span.end)
        );
        for region in &result.host.regions {
            assert!(result.exclusions.contains(&region.span));
        }
        Ok(())
    }

    #[test]
    fn malformed_frontmatter_comments_and_mixed_inline_mdx_do_not_enable_children() -> Result<()> {
        let astro = discover(
            "x.astro",
            "---\nconst missing = 1;\n<script>fake()</script>",
            "astro",
            &mut budget(),
        )?;
        assert!(astro.host.partial && astro.host.regions.is_empty());
        let mdx = discover(
            "x.mdx",
            "<!--\nimport Hidden from 'hidden';\n\n-->\n\nThe inline <Card /> stays prose.\n",
            "mdx",
            &mut budget(),
        )?;
        assert!(
            mdx.host.partial && mdx.host.regions.is_empty(),
            "partial={} regions={:?} diagnostics={:?}",
            mdx.host.partial,
            mdx.host
                .regions
                .iter()
                .map(|r| (r.kind, r.span))
                .collect::<Vec<_>>(),
            mdx.host.diagnostics
        );
        let comment = discover(
            "x.mdx",
            "{/*\n\nimport Hidden from 'hidden';\n\n*/}\n",
            "mdx",
            &mut budget(),
        )?;
        assert!(comment.host.regions.is_empty());
        let invalid = discover("x.mdx", "export const broken = ;\n", "mdx", &mut budget())?;
        assert!(invalid.host.partial && invalid.host.regions.is_empty());
        Ok(())
    }

    #[test]
    fn failed_attempts_and_validation_nodes_share_host_budget() {
        let source = "export const broken = ;\n\n".repeat(257);
        assert!(discover("x.mdx", &source, "mdx", &mut budget()).is_err());
        let mut small = HostBudget::new(1, 10_000, 16_777_216).unwrap();
        assert!(discover("x.mdx", "export const value = 1;\n", "mdx", &mut small).is_err());
    }
}
