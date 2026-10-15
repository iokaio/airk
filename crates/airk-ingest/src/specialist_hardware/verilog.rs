// SPDX-License-Identifier: Apache-2.0
use super::*;
fn identifier<'a>(node: Node<'a>) -> Option<Node<'a>> {
    named(node, "simple_identifier").or_else(|| named(node, "escaped_identifier"))
}
pub(super) fn select<'a, 'b>(node: Node<'a>, source: &'b str) -> Vec<Cue<'a, 'b>> {
    let definition = match node.kind() {
        "module_declaration" => named(node, "module_header")
            .and_then(identifier)
            .or_else(|| identifier(node)),
        "interface_declaration" => named(node, "interface_identifier").or_else(|| {
            named(node, "interface_ansi_header")
                .or_else(|| named(node, "interface_nonansi_header"))
                .and_then(|head| named(head, "interface_identifier"))
        }),
        "package_declaration" => named(node, "package_identifier"),
        "class_declaration" => named(node, "class_identifier"),
        "function_body_declaration" => named(node, "function_identifier"),
        "task_body_declaration" => named(node, "task_identifier"),
        _ => None,
    };
    if let Some(name) = definition {
        return vec![cue(node, "definition", text(name, source), true)];
    }
    match node.kind() {
        "ansi_port_declaration" => named(node, "port_identifier")
            .map(|name| cue(node, "port", text(name, source), false))
            .into_iter()
            .collect(),
        "port_identifier"
            if node
                .parent()
                .is_some_and(|parent| parent.kind() == "list_of_port_identifiers") =>
        {
            vec![cue(node, "port", text(node, source), false)]
        }
        "param_assignment" => named(node, "parameter_identifier")
            .map(|name| cue(node, "parameter", text(name, source), false))
            .into_iter()
            .collect(),
        "include_compiler_directive" => vec![cue(node, "include", text(node, source), false)],
        "module_instantiation" | "interface_instantiation" => identifier(node)
            .map(|name| cue(node, "instantiation", text(name, source), false))
            .into_iter()
            .collect(),
        "package_import_item" => vec![cue(node, "import", text(node, source), false)],
        "tf_call" => identifier(node)
            .map(|name| cue(node, "call", text(name, source), false))
            .into_iter()
            .collect(),
        _ => vec![],
    }
}
