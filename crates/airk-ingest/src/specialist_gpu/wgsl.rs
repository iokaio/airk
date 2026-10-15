// SPDX-License-Identifier: Apache-2.0
use super::*;
pub(super) fn select<'a, 'b>(node: Node<'a>, source: &'b str) -> Vec<Cue<'a, 'b>> {
    let found = match node.kind() {
        "function_declaration" => node
            .child_by_field_name("name")
            .filter(|name| name.kind() == "identifier")
            .map(|name| cue(node, "function", text(name, source), true)),
        "struct_declaration" => {
            field(node, source, "name").map(|name| cue(node, "struct", name, true))
        }
        "variable_identifier_declaration" => {
            field(node, source, "name").map(|name| cue(node, "variable", name, true))
        }
        "attribute" => named(node, "identifier").map(|name| {
            let kind = match text(name, source) {
                "vertex" | "fragment" | "compute" => "stage",
                "group" | "binding" | "location" | "builtin" => "binding",
                _ => "attribute",
            };
            cue(node, kind, text(node, source), false)
        }),
        "type_constructor_or_function_call_expression" => {
            named(node, "identifier").map(|name| cue(node, "call", text(name, source), false))
        }
        _ => None,
    };
    found.into_iter().collect()
}
