// SPDX-License-Identifier: Apache-2.0
use super::*;
pub(super) fn select<'a>(node: Node<'a>, source: &str) -> Vec<Cue<'a>> {
    let selected = match node.kind() {
        "package_statement" => {
            field(node, source, "name").map(|name| cue(node, "package", name, false))
        }
        "subroutine_declaration_statement" | "method_declaration_statement" => {
            field(node, source, "name").map(|name| cue(node, "definition", name, true))
        }
        "use_statement" => {
            field(node, source, "module").map(|name| cue(node, "import", name, false))
        }
        "require_expression" => node
            .named_child(0)
            .filter(|n| {
                matches!(
                    n.kind(),
                    "bareword"
                        | "package"
                        | "string_literal"
                        | "string"
                        | "interpolated_string_literal"
                )
            })
            .map(|name| cue(node, "require", text(name, source), false)),
        "function_call_expression"
        | "ambiguous_function_call_expression"
        | "func0op_call_expression"
        | "func1op_call_expression" => {
            field(node, source, "function").map(|name| cue(node, "call", name, false))
        }
        "method_call_expression" => node
            .child_by_field_name("arguments")
            .map(|args| {
                cue(
                    node,
                    "call",
                    &source[node.start_byte()..args.start_byte()],
                    false,
                )
            })
            .or_else(|| Some(cue(node, "call", text(node, source), false))),
        _ => None,
    };
    selected.into_iter().collect()
}
