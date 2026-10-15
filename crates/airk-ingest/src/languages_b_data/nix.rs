// SPDX-License-Identifier: Apache-2.0
use super::*;
pub(super) fn select<'a>(node: Node<'a>, source: &str) -> Vec<Cue<'a>> {
    match node.kind() {
        "binding" => {
            let Some(name) = field(node, source, "attrpath") else {
                return vec![];
            };
            let value = node.child_by_field_name("expression");
            let function = value.filter(|n| n.kind() == "function_expression");
            let mut result = cue(
                node,
                if function.is_some() {
                    "definition"
                } else {
                    "binding"
                },
                name,
                function.is_some(),
            );
            result.body = function.and_then(|n| n.child_by_field_name("body"));
            vec![result]
        }
        "apply_expression" => {
            let Some(function) = node.child_by_field_name("function") else {
                return vec![];
            };
            let name = text(function, source);
            if name == "import" {
                return node
                    .child_by_field_name("argument")
                    .map(|arg| cue(node, "import", text(arg, source), false))
                    .into_iter()
                    .collect();
            }
            vec![cue(node, "call", name, false)]
        }
        "inherit" | "inherit_from" => vec![cue(node, "inherit", text(node, source), false)],
        _ => vec![],
    }
}
