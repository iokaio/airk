// SPDX-License-Identifier: Apache-2.0
use super::*;
pub(super) fn select<'a, 'b>(node: Node<'a>, source: &'b str) -> Vec<Cue<'a, 'b>> {
    if node.kind() == "declaration" {
        let mut cursor = node.walk();
        let mut result: Vec<_> = node
            .children_by_field_name("declarator", &mut cursor)
            .filter_map(|declarator| {
                cuda::declarator_name(declarator, source)
                    .map(|name| cue(declarator, "variable", name, true))
            })
            .take(10_001)
            .collect();
        if let (Some(body), Some(name)) = (
            named(node, "field_declaration_list"),
            named(node, "identifier"),
        ) {
            let mut interface = cue(node, "interface_block", text(name, source), true);
            interface.body = Some(body);
            result.push(interface);
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor).take(10_001) {
            if matches!(child.kind(), "uniform" | "buffer" | "in" | "out" | "inout") {
                result.push(cue(child, "storage_qualifier", text(child, source), false));
            }
        }
        return result;
    }
    let found = match node.kind() {
        "function_definition" => node
            .child_by_field_name("declarator")
            .and_then(|n| cuda::declarator_name(n, source))
            .map(|name| cue(node, "function", name, true)),
        "struct_specifier" => {
            field(node, source, "name").map(|name| cue(node, "struct", name, true))
        }
        "layout_specification" => Some(cue(node, "binding", text(node, source), false)),
        "extension_storage_class" | "type_qualifier" => {
            Some(cue(node, "storage_qualifier", text(node, source), false))
        }
        "call_expression" => {
            field(node, source, "function").map(|name| cue(node, "call", name, false))
        }
        "preproc_call" | "preproc_extension" => {
            Some(cue(node, "directive", text(node, source), false))
        }
        _ => None,
    };
    found.into_iter().collect()
}
