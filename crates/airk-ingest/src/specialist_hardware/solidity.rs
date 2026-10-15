// SPDX-License-Identifier: Apache-2.0
use super::*;
pub(super) fn select<'a, 'b>(node: Node<'a>, source: &'b str) -> Vec<Cue<'a, 'b>> {
    let (kind, key, declaration) = match node.kind() {
        "contract_declaration"
        | "interface_declaration"
        | "library_declaration"
        | "struct_declaration"
        | "function_definition" => ("definition", "name", true),
        "event_definition" => ("event", "name", false),
        "error_declaration" => ("error", "name", false),
        "import_directive" => ("import", "source", false),
        "inheritance_specifier" => ("inheritance", "ancestor", false),
        "call_expression" => ("call", "function", false),
        _ => return vec![],
    };
    field(node, source, key)
        .map(|name| cue(node, kind, name, declaration))
        .into_iter()
        .collect()
}
