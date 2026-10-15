// SPDX-License-Identifier: Apache-2.0
use super::*;
pub(super) fn select<'a>(node: Node<'a>, source: &str) -> Vec<Cue<'a>> {
    match node.kind() {
        "exception_definition" => named(node, "constructor_declaration")
            .and_then(|n| named(n, "constructor_name"))
            .map(|name| cue(node, "exception", text(name, source), true))
            .into_iter()
            .collect(),
        "let_binding" => node
            .child_by_field_name("pattern")
            .filter(|n| matches!(n.kind(), "value_name" | "value_pattern"))
            .or_else(|| named(node, "value_name"))
            .map(|n| cue(node, "definition", text(n, source), true))
            .into_iter()
            .collect(),
        "module_binding" | "module_type_definition" => named(node, "module_name")
            .or_else(|| named(node, "module_type_name"))
            .map(|n| cue(node, "definition", text(n, source), true))
            .into_iter()
            .collect(),
        "type_binding" => field(node, source, "name")
            .map(|n| cue(node, "definition", n, true))
            .into_iter()
            .collect(),
        "value_specification" | "external_declaration" => {
            let Some(name) = named(node, "value_name") else {
                return vec![];
            };
            let mut c = cue(node, "signature", text(name, source), true);
            c.signature = Some(ByteSpan {
                start: node.start_byte(),
                end: node.end_byte(),
            });
            vec![c]
        }
        "open_module" | "open_module_signature" | "include_module" | "include_module_type" => {
            vec![cue(node, "import", text(node, source), false)]
        }
        "application_expression" => node
            .child_by_field_name("function")
            .or_else(|| node.named_child(0))
            .map(|n| cue(node, "call", text(n, source), false))
            .into_iter()
            .collect(),
        _ => vec![],
    }
}
