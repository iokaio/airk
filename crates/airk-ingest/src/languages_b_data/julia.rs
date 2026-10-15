// SPDX-License-Identifier: Apache-2.0
use super::*;
pub(super) fn select<'a>(node: Node<'a>, source: &str) -> Vec<Cue<'a>> {
    match node.kind() {
        "module_definition" => field(node, source, "name")
            .map(|n| cue(node, "definition", n, true))
            .into_iter()
            .collect(),
        "function_definition"
        | "macro_definition"
        | "struct_definition"
        | "abstract_definition"
        | "primitive_definition" => {
            let head = named(node, "signature")
                .or_else(|| named(node, "type_head"))
                .or_else(|| named(node, "identifier"));
            let Some(head) = head else { return vec![] };
            let Some(name) = header_name(head, source) else {
                return vec![];
            };
            let mut selected = cue(node, "definition", name, true);
            selected.signature = Some(ByteSpan {
                start: node.start_byte(),
                end: head.end_byte(),
            });
            // Block bodies have several sibling expressions: unknown is safer than hashing one.
            vec![selected]
        }
        "assignment"
            if node
                .named_child(0)
                .is_some_and(|n| n.kind() == "call_expression") =>
        {
            let head = node.named_child(0).unwrap();
            let Some(name) = header_name(head, source) else {
                return vec![];
            };
            let mut selected = cue(node, "definition", name, true);
            selected.signature = Some(ByteSpan {
                start: node.start_byte(),
                end: head.end_byte(),
            });
            selected.body = node.named_child(1);
            vec![selected]
        }
        "using_statement" | "import_statement" => {
            vec![cue(node, "import", text(node, source), false)]
        }
        "export_statement" => vec![cue(node, "export", text(node, source), false)],
        "call_expression" | "broadcast_call_expression"
            if !node.parent().is_some_and(|p| {
                p.kind() == "signature"
                    || (p.kind() == "assignment" && p.named_child(0) == Some(node))
            }) =>
        {
            node.named_child(0)
                .map(|n| cue(node, "call", text(n, source), false))
                .into_iter()
                .collect()
        }
        "macrocall_expression" => named(node, "macro_identifier")
            .map(|n| cue(node, "macro_call", text(n, source), false))
            .into_iter()
            .collect(),
        _ => vec![],
    }
}
fn header_name<'a>(mut node: Node<'_>, source: &'a str) -> Option<&'a str> {
    for _ in 0..8 {
        if matches!(
            node.kind(),
            "identifier" | "scoped_identifier" | "field_expression"
        ) {
            return Some(text(node, source));
        }
        node = node.named_child(0)?;
    }
    None
}
