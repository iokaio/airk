// SPDX-License-Identifier: Apache-2.0
use super::*;
use std::ops::Range;

pub(super) fn island(node: Node<'_>) -> bool {
    matches!(
        node.kind(),
        "class_interface"
            | "class_implementation"
            | "protocol_declaration"
            | "message_expression"
            | "selector_expression"
    )
}

pub(super) fn select(node: Node<'_>, source: &str) -> Option<Cue> {
    match node.kind() {
        "class_interface" | "class_implementation" | "protocol_declaration" => {
            let name = text(named(node, "identifier")?, source);
            let mut result = cue("definition", name, true)?;
            if let Some(category) = field(node, source, "category") {
                result.name = format!("{name}({category})");
            }
            Some(result)
        }
        "method_declaration" | "method_definition" => {
            let mut selector = String::new();
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                if child.kind() == "identifier"
                    && child
                        .next_named_sibling()
                        .is_some_and(|next| next.kind() == "method_parameter")
                {
                    selector.push_str(text(child, source));
                    selector.push(':');
                }
                if child.kind() == "keyword_declarator"
                    && let Some(identifier) = named(child, "identifier")
                {
                    selector.push_str(text(identifier, source));
                    selector.push(':');
                }
            }
            if selector.is_empty() {
                selector = text(named(node, "identifier")?, source).into();
            }
            cue("definition", &selector, true)
        }
        "property_declaration" => cue("property", text(node, source), false),
        "message_expression" => cue("message", text(node, source), false),
        "call_expression" => cue("call", field(node, source, "function")?, false),
        "selector_expression" => cue("selector", text(node, source), false),
        "module_import" => cue("import", field(node, source, "path")?, false),
        "preproc_include" => cue(
            "import",
            field(node, source, "path").unwrap_or_else(|| text(node, source)),
            false,
        ),
        "preproc_call" if field(node, source, "directive") == Some("#import") => {
            cue("import", field(node, source, "argument")?, false)
        }
        _ => None,
    }
}

pub(super) fn cpp_pass(
    source: &str,
    output: &mut Facts<'_>,
    budget: &mut HostBudget,
    mut islands: Vec<Range<usize>>,
) -> Result<()> {
    islands.sort_by_key(|span| (span.start, span.end));
    let mut merged: Vec<Range<usize>> = Vec::new();
    for span in islands {
        if let Some(last) = merged.last_mut()
            && span.start <= last.end
        {
            last.end = last.end.max(span.end);
        } else {
            merged.push(span);
        }
    }
    budget.charge(1, 0, 0, 0)?;
    let mut parser = Parser::new();
    parser.set_language(&tree_sitter_cpp::LANGUAGE.into())?;
    let tree = parser
        .parse(source, None)
        .ok_or_else(|| anyhow::anyhow!("C++ parser returned no tree"))?;
    let mut pending = vec![(tree.root_node(), None)];
    while let Some((node, owner)) = pending.pop() {
        budget.charge(1, 0, 0, 0)?;
        let at = merged.partition_point(|span| span.end <= node.start_byte());
        let overlaps = merged
            .get(at)
            .is_some_and(|span| span.start < node.end_byte());
        let mut next_owner = owner;
        if !overlaps && !node.has_error() && !node.is_missing() {
            let selected = match node.kind() {
                "namespace_definition" | "class_specifier" | "struct_specifier" => {
                    field(node, source, "name").and_then(|name| cue("definition", name, true))
                }
                "template_declaration" => cue("template", text(node, source), true),
                "function_definition" => {
                    field(node, source, "declarator").and_then(|name| cue("definition", name, true))
                }
                "call_expression" => {
                    field(node, source, "function").and_then(|name| cue("call", name, false))
                }
                _ => None,
            };
            if let Some(cue) = selected {
                next_owner = emit(output, node, cue, owner, "objcpp", source, budget)?;
            }
        }
        for index in (0..node.named_child_count()).rev() {
            ensure!(pending.len() < 500_000, "Objective-C++ pending node limit");
            if let Some(child) = node.named_child(index) {
                pending.push((child, next_owner));
            }
        }
    }
    Ok(())
}
