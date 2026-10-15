// SPDX-License-Identifier: Apache-2.0
use super::*;
pub(super) fn select<'a>(node: Node<'a>, source: &str) -> Vec<Cue<'a>> {
    match node.kind() {
        "instance" => {
            let Some(name) = field(node, source, "name") else {
                return vec![];
            };
            let patterns = field(node, source, "patterns").unwrap_or("");
            let mut selected = cue(node, "instance", &format!("{name} {patterns}"), true);
            selected.body = node.child_by_field_name("declarations");
            vec![selected]
        }
        "function" | "bind" => {
            let Some(name) = field(node, source, "name") else {
                return vec![];
            };
            let mut c = cue(node, "definition", name, true);
            let mut cursor = node.walk();
            let mut bodies = node.children_by_field_name("match", &mut cursor);
            c.body = bodies.next();
            if bodies.next().is_some() {
                c.body = None;
            }
            vec![c]
        }
        "data_type" | "newtype" | "type_synomym" | "type_synonym" | "class" => {
            field(node, source, "name")
                .map(|n| cue(node, "definition", n, true))
                .into_iter()
                .collect()
        }
        "signature" => {
            let Some(name) = field(node, source, "name").or_else(|| field(node, source, "names"))
            else {
                return vec![];
            };
            let mut c = cue(node, "signature", name, false);
            c.signature = Some(ByteSpan {
                start: node.start_byte(),
                end: node.end_byte(),
            });
            vec![c]
        }
        "import" => vec![cue(node, "import", text(node, source), false)],
        "header" => vec![cue(node, "module", text(node, source), false)],
        "apply" => field(node, source, "function")
            .map(|n| cue(node, "call", n, false))
            .into_iter()
            .collect(),
        _ => vec![],
    }
}
