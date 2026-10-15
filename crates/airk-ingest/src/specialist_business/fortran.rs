// SPDX-License-Identifier: Apache-2.0
use super::*;
pub(super) fn select<'a>(node: Node<'a>, source: &str) -> Vec<Cue<'a>> {
    match node.kind() {
        "program" | "module" | "subroutine" | "function" | "submodule" => {
            let header_kind = match node.kind() {
                "program" => "program_statement",
                "module" => "module_statement",
                "subroutine" => "subroutine_statement",
                "submodule" => "submodule_statement",
                _ => "function_statement",
            };
            let Some(header) = named(node, header_kind) else {
                return vec![];
            };
            let name = header
                .child_by_field_name("name")
                .or_else(|| named(header, "name"))
                .or_else(|| named(header, "module_name"));
            let Some(name) = name else { return vec![] };
            let mut c = cue(node, "definition", text(name, source), true);
            c.signature = Some(ByteSpan {
                start: header.start_byte(),
                end: header.end_byte(),
            });
            vec![c]
        }
        "use_statement" | "include_statement" | "import_statement" | "preproc_include" => {
            vec![cue(node, "import", text(node, source), false)]
        }
        "variable_declaration" => vec![cue(node, "declaration", text(node, source), false)],
        "subroutine_call" => field(node, source, "subroutine")
            .map(|n| cue(node, "call", n, false))
            .into_iter()
            .collect(),
        "call_expression" => field(node, source, "function")
            .map(|n| cue(node, "call", n, false))
            .into_iter()
            .collect(),
        _ => vec![],
    }
}
