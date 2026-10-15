// SPDX-License-Identifier: Apache-2.0
use super::*;

pub(super) fn visit<'tree, 'source>(
    node: Node<'tree>,
    text: &'source str,
    owner: Option<Node<'tree>>,
    cues: &mut Vec<RepositoryCue<'tree, 'source>>,
) {
    match node.kind() {
        "block" => {
            let body = child(node, &["body"]);
            // The entire written header includes block type and all labels, including Unicode.
            let end =
                child(node, &["block_start"]).map_or(node.end_byte(), |start| start.start_byte());
            let header = text
                .get(node.start_byte()..end)
                .unwrap_or("")
                .trim()
                .trim_end_matches('{')
                .trim();
            emit(
                cues,
                node,
                header,
                "definition",
                CueCategory::Declaration,
                owner,
                body,
            );
        }
        "attribute" => {
            if let Some(name) = child(node, &["identifier"]) {
                emit(
                    cues,
                    node,
                    source(name, text),
                    "key",
                    CueCategory::Attribute,
                    owner,
                    None,
                );
            }
        }
        "variable_expr" => emit(
            cues,
            node,
            source(node, text),
            "reference",
            CueCategory::Reference,
            owner,
            None,
        ),
        "get_attr" | "index" => emit(
            cues,
            node,
            source(node, text),
            "reference",
            CueCategory::Reference,
            owner,
            None,
        ),
        _ => {}
    }
}
