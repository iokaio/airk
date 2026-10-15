// SPDX-License-Identifier: Apache-2.0
use super::*;

pub(super) fn visit<'tree, 'source>(
    node: Node<'tree>,
    text: &'source str,
    owner: Option<Node<'tree>>,
    cues: &mut Vec<RepositoryCue<'tree, 'source>>,
) {
    if node.kind().ends_with("_instruction") {
        // Preserve existing instruction facts and use the full multiline AST node as evidence.
        let spelling = source(node, text);
        let instruction = spelling.split_whitespace().next().unwrap_or("");
        emit(
            cues,
            node,
            instruction,
            "directive",
            CueCategory::Directive,
            owner,
            None,
        );
    }
    match node.kind() {
        "from_instruction" => {
            if let Some(alias) = node.child_by_field_name("as") {
                emit(
                    cues,
                    node,
                    source(alias, text),
                    "stage",
                    CueCategory::Declaration,
                    owner,
                    None,
                );
            }
        }
        "image_spec" => emit(
            cues,
            node,
            source(node, text),
            "image",
            CueCategory::Reference,
            owner,
            None,
        ),
        "arg_instruction" => {
            if let Some(name) = node.child_by_field_name("name") {
                emit(
                    cues,
                    node,
                    source(name, text),
                    "argument",
                    CueCategory::Attribute,
                    owner,
                    None,
                );
            }
        }
        "env_pair" => {
            if let Some(name) = node.child_by_field_name("name") {
                emit(
                    cues,
                    node,
                    source(name, text),
                    "environment",
                    CueCategory::Attribute,
                    owner,
                    None,
                );
            }
        }
        "copy_instruction" => {
            // Keep flags as written. Only explicit --from qualifies as a stage/image candidate.
            let mut cursor = node.walk();
            for item in node.named_children(&mut cursor) {
                let value = source(item, text);
                if value.starts_with("--from=") {
                    emit(
                        cues,
                        item,
                        value,
                        "copy_from",
                        CueCategory::Reference,
                        owner,
                        None,
                    );
                }
            }
        }
        "workdir_instruction" | "expose_instruction" => emit(
            cues,
            node,
            source(node, text),
            "configuration",
            CueCategory::Value,
            owner,
            None,
        ),
        _ => {}
    }
}
