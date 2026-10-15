// SPDX-License-Identifier: Apache-2.0
use super::*;
mod free;
pub(super) use free::analyze as analyze_free;
pub(super) fn select<'a>(node: Node<'a>, source: &str) -> Vec<Cue<'a>> {
    match node.kind() {
        "program_name" => vec![cue(node, "program", text(node, source), false)],
        "identification_division"
        | "environment_division"
        | "data_division"
        | "procedure_division" => vec![cue(node, "division", node.kind(), false)],
        "working_storage_section"
        | "file_section"
        | "linkage_section"
        | "local_storage_section" => vec![cue(node, "section", node.kind(), false)],
        "paragraph_header" | "section_header" => field(node, source, "name")
            .map(|n| {
                cue(
                    node,
                    if node.kind() == "paragraph_header" {
                        "paragraph"
                    } else {
                        "section"
                    },
                    n,
                    false,
                )
            })
            .into_iter()
            .collect(),
        "data_description" => named(node, "entry_name")
            .map(|n| cue(node, "declaration", text(n, source), false))
            .into_iter()
            .collect(),
        "copy_statement" => field(node, source, "book")
            .map(|n| cue(node, "import", n, false))
            .into_iter()
            .collect(),
        "call_statement" => field(node, source, "x")
            .map(|n| cue(node, "call", n, false))
            .into_iter()
            .collect(),
        _ => vec![],
    }
}
