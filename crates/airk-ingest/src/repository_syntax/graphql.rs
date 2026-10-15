// SPDX-License-Identifier: Apache-2.0
use super::*;

pub(super) fn visit<'tree, 'source>(
    node: Node<'tree>,
    text: &'source str,
    owner: Option<Node<'tree>>,
    cues: &mut Vec<RepositoryCue<'tree, 'source>>,
) {
    let declaration = matches!(
        node.kind(),
        "object_type_definition"
            | "interface_type_definition"
            | "input_object_type_definition"
            | "enum_type_definition"
            | "union_type_definition"
            | "scalar_type_definition"
            | "directive_definition"
            | "object_type_extension"
            | "interface_type_extension"
            | "input_object_type_extension"
            | "enum_type_extension"
            | "union_type_extension"
            | "scalar_type_extension"
            | "operation_definition"
            | "fragment_definition"
    );
    if declaration {
        let name = child(node, &["name", "fragment_name"]);
        // Anonymous operations have no stable declaration name: retain the written operation cue.
        let name = name.map_or_else(|| source(node, text), |name| source(name, text));
        emit(
            cues,
            node,
            name,
            if node.kind() == "operation_definition" {
                "operation"
            } else {
                "definition"
            },
            CueCategory::Declaration,
            owner,
            child(
                node,
                &[
                    "fields_definition",
                    "input_fields_definition",
                    "enum_values_definition",
                    "selection_set",
                ],
            ),
        );
        return;
    }
    match node.kind() {
        "schema_definition" | "schema_extension" | "root_operation_type_definition" => emit(
            cues,
            node,
            source(node, text),
            "schema",
            CueCategory::Value,
            owner,
            None,
        ),
        "field_definition" | "input_value_definition" | "enum_value_definition" => {
            if let Some(name) = child(node, &["name"]) {
                emit(
                    cues,
                    node,
                    source(name, text),
                    "field",
                    CueCategory::Attribute,
                    owner,
                    None,
                );
            }
        }
        "named_type" => emit(
            cues,
            node,
            source(node, text),
            "type_reference",
            CueCategory::Reference,
            owner,
            None,
        ),
        "fragment_spread" => emit(
            cues,
            node,
            source(node, text),
            "fragment_reference",
            CueCategory::Reference,
            owner,
            None,
        ),
        "argument" => emit(
            cues,
            node,
            source(node, text),
            "argument",
            CueCategory::Attribute,
            owner,
            None,
        ),
        _ => {}
    }
}
