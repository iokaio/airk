// SPDX-License-Identifier: Apache-2.0
use super::*;

pub(super) fn select(node: Node<'_>, source: &str) -> Option<Cue> {
    match node.kind() {
        "class_declaration"
        | "mixin_declaration"
        | "extension_declaration"
        | "extension_type_declaration"
        | "enum_declaration"
        | "enum_constant" => cue("definition", field(node, source, "name")?, true),
        "function_declaration"
        | "method_declaration"
        | "getter_declaration"
        | "setter_declaration"
        | "external_function_declaration" => {
            let signature = node.child_by_field_name("signature")?;
            let inner = if signature.kind() == "method_signature" {
                signature.named_child(0)?
            } else {
                signature
            };
            cue("definition", field(inner, source, "name")?, true)
        }
        "initialized_variable_definition" | "static_final_declaration" => {
            cue("field", field(node, source, "name")?, false)
        }
        "initialized_identifier" => cue(
            "field",
            field(node, source, "name")
                .or_else(|| named(node, "identifier").map(|n| text(n, source)))?,
            false,
        ),
        "library_name" => cue(
            "library",
            field(node, source, "uri")
                .or_else(|| named(node, "dotted_identifier_list").map(|n| text(n, source)))?,
            false,
        ),
        "import_specification" | "library_export" => {
            cue("import", field(node, source, "uri")?, false)
        }
        "part_directive" | "part_of_directive" => cue(
            "part",
            field(node, source, "uri").or_else(|| {
                named(node, "uri")
                    .or_else(|| named(node, "dotted_identifier_list"))
                    .map(|n| text(n, source))
            })?,
            false,
        ),
        "call_expression" | "cascade_call_expression" => cue(
            "call",
            field(node, source, "function").or_else(|| field(node, source, "property"))?,
            false,
        ),
        _ => None,
    }
}
