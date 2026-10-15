// SPDX-License-Identifier: Apache-2.0
use super::*;
pub(super) fn select<'a, 'b>(node: Node<'a>, source: &'b str) -> Vec<Cue<'a, 'b>> {
    let definition = match node.kind() {
        "entity_declaration" => Some(("entity", "entity_body")),
        "architecture_definition" => Some(("architecture", "concurrent_block")),
        "package_declaration" => Some(("package", "package_declaration_body")),
        "package_definition" => Some(("package", "package_definition_body")),
        "component_declaration" => Some(("component", "component_body")),
        _ => None,
    };
    if let Some((key, body)) = definition {
        return field(node, source, key)
            .map(|name| {
                let mut result = cue(node, "definition", name, true);
                result.body = named(node, body);
                result
            })
            .into_iter()
            .collect();
    }
    match node.kind() {
        "signal_declaration" | "interface_signal_declaration" | "interface_declaration" => {
            let Some(list) = named(node, "identifier_list") else {
                return vec![];
            };
            let mut cursor = list.walk();
            list.named_children(&mut cursor)
                .take(10_001)
                .map(|name| {
                    cue(
                        name,
                        if node.kind() == "signal_declaration" {
                            "signal"
                        } else {
                            "port"
                        },
                        text(name, source),
                        false,
                    )
                })
                .collect()
        }
        "process_statement" => named(node, "label_declaration")
            .map(|name| {
                let mut result = cue(
                    node,
                    "definition",
                    text(name, source).trim_end_matches(':').trim(),
                    true,
                );
                result.body = named(node, "sequential_block");
                result
            })
            .into_iter()
            .collect(),
        "library_clause" => named(node, "logical_name_list")
            .map(|name| cue(node, "library", text(name, source), false))
            .into_iter()
            .collect(),
        "use_clause" => named(node, "selected_name_list")
            .map(|name| cue(node, "use", text(name, source), false))
            .into_iter()
            .collect(),
        "instantiated_unit" => ["entity", "component", "configuration"]
            .iter()
            .find_map(|key| field(node, source, key))
            .map(|name| cue(node, "instantiation", name, false))
            .into_iter()
            .collect(),
        "component_instantiation_statement" => field(node, source, "component")
            .map(|name| cue(node, "instantiation", name, false))
            .into_iter()
            .collect(),
        _ => vec![],
    }
}
