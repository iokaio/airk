// SPDX-License-Identifier: Apache-2.0
use super::*;

pub(super) fn select(node: Node<'_>, source: &str) -> Option<Cue> {
    match node.kind() {
        "class_definition"
        | "object_definition"
        | "trait_definition"
        | "enum_definition"
        | "function_definition"
        | "function_declaration"
        | "type_definition"
        | "given_definition" => cue("definition", field(node, source, "name")?, true),
        "val_definition" | "var_definition" => {
            let pattern = node.child_by_field_name("pattern")?;
            (pattern.kind() == "identifier")
                .then(|| cue("field", text(pattern, source), false))
                .flatten()
        }
        "val_declaration" | "var_declaration" => cue("field", field(node, source, "name")?, false),
        "package_clause" | "package_object" => cue(
            "package",
            field(node, source, "name")?,
            node.kind() == "package_object",
        ),
        "import_declaration" | "export_declaration" => cue("import", text(node, source), false),
        "call_expression" => cue("call", field(node, source, "function")?, false),
        "extension_definition" => cue("extension", field(node, source, "parameters")?, true),
        _ => None,
    }
}
