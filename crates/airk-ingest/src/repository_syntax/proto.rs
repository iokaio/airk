// SPDX-License-Identifier: Apache-2.0
use super::*;

pub(super) fn visit<'tree, 'source>(
    node: Node<'tree>,
    text: &'source str,
    owner: Option<Node<'tree>>,
    cues: &mut Vec<RepositoryCue<'tree, 'source>>,
) {
    let declaration = match node.kind() {
        "message" => Some(("message_name", Some("message_body"))),
        "enum" => Some(("enum_name", Some("enum_body"))),
        "service" => Some(("service_name", None)),
        "rpc" => Some(("rpc_name", None)),
        "oneof" => Some(("identifier", None)),
        _ => None,
    };
    if let Some((name_kind, body_kind)) = declaration {
        if let Some(name) = child(node, &[name_kind]) {
            emit(
                cues,
                node,
                source(name, text),
                "definition",
                CueCategory::Declaration,
                owner,
                body_kind.and_then(|kind| child(node, &[kind])),
            );
        }
        return;
    }
    match node.kind() {
        "package" => emit(
            cues,
            node,
            source(node, text),
            "package",
            CueCategory::Value,
            owner,
            None,
        ),
        "import" => {
            if let Some(path) = node.child_by_field_name("path") {
                emit(
                    cues,
                    node,
                    source(path, text),
                    "import",
                    CueCategory::Import,
                    owner,
                    None,
                );
            }
        }
        // Full written field text preserves label/type/name/number/options without evaluation.
        "field" | "map_field" | "oneof_field" | "enum_field" => emit(
            cues,
            node,
            source(node, text),
            "field",
            CueCategory::Attribute,
            owner,
            None,
        ),
        "field_number" => emit(
            cues,
            node,
            source(node, text),
            "field_number",
            CueCategory::Value,
            owner,
            None,
        ),
        "message_or_enum_type" => emit(
            cues,
            node,
            source(node, text),
            "type_reference",
            CueCategory::Reference,
            owner,
            None,
        ),
        _ => {}
    }
}
