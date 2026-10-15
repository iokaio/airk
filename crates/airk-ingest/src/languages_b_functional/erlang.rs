// SPDX-License-Identifier: Apache-2.0
use super::*;
pub(super) fn select<'a>(node: Node<'a>, source: &str) -> Vec<Cue<'a>> {
    match node.kind() {
        "function_clause" => {
            let Some(name) = field(node, source, "name") else {
                return vec![];
            };
            let arity = node.child_by_field_name("args").map_or(0, |n| {
                let mut cursor = n.walk();
                n.children_by_field_name("args", &mut cursor).count()
            });
            vec![cue(node, "definition", &format!("{name}/{arity}"), true)]
        }
        "module_attribute" => field(node, source, "name")
            .map(|n| cue(node, "module", n, false))
            .into_iter()
            .collect(),
        "record_decl" | "type_alias" | "opaque" => field(node, source, "name")
            .map(|n| cue(node, "definition", n, true))
            .into_iter()
            .collect(),
        "spec" | "callback" => vec![cue(node, "signature", text(node, source), false)],
        "pp_include" | "pp_include_lib" | "import_attribute" => {
            vec![cue(node, "import", text(node, source), false)]
        }
        "export_attribute" | "export_type_attribute" => {
            vec![cue(node, "export", text(node, source), false)]
        }
        "pp_define" | "macro_lhs" => vec![cue(node, "macro", text(node, source), false)],
        "macro_call_expr" => vec![cue(node, "macro_call", text(node, source), false)],
        "call" => field(node, source, "expr")
            .map(|n| cue(node, "call", n, false))
            .into_iter()
            .collect(),
        _ => vec![],
    }
}
