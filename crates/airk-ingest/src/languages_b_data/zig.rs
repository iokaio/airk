// SPDX-License-Identifier: Apache-2.0
use super::*;
pub(super) fn select<'a>(node: Node<'a>, source: &str) -> Vec<Cue<'a>> {
    match node.kind() {
        "function_declaration" => field(node, source, "name")
            .map(|n| cue(node, "definition", n, true))
            .into_iter()
            .collect(),
        "variable_declaration" => {
            let Some(name) = named(node, "identifier") else {
                return vec![];
            };
            let body = [
                "struct_declaration",
                "enum_declaration",
                "union_declaration",
                "opaque_declaration",
                "error_set_declaration",
            ]
            .into_iter()
            .find_map(|k| named(node, k));
            let mut result = cue(
                node,
                if body.is_some() {
                    "definition"
                } else {
                    "variable"
                },
                text(name, source),
                body.is_some(),
            );
            result.body = body;
            vec![result]
        }
        "call_expression" => field(node, source, "function")
            .map(|n| cue(node, "call", n, false))
            .into_iter()
            .collect(),
        "builtin_function" => {
            let Some(name) = named(node, "builtin_identifier") else {
                return vec![];
            };
            let name = text(name, source);
            vec![cue(
                node,
                if name == "@import" {
                    "import"
                } else {
                    "builtin"
                },
                text(node, source),
                false,
            )]
        }
        "test_declaration" => vec![cue(node, "test", text(node, source), false)],
        _ => vec![],
    }
}
