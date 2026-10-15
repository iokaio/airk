// SPDX-License-Identifier: Apache-2.0
use super::*;
fn first_name<'a>(mut node: Node<'_>, source: &'a str) -> Option<&'a str> {
    for _ in 0..8 {
        if matches!(
            node.kind(),
            "identifier" | "long_identifier" | "long_identifier_or_op"
        ) {
            return Some(text(node, source));
        }
        node = node.named_child(0)?;
    }
    None
}
pub(super) fn select<'a>(node: Node<'a>, source: &str, signature: bool) -> Vec<Cue<'a>> {
    match node.kind() {
        "module_defn" => named(node, "identifier")
            .or_else(|| named(node, "long_identifier"))
            .map(|name| cue(node, "module", text(name, source), true))
            .into_iter()
            .collect(),
        "method_or_prop_defn" => field(node, source, "name")
            .map(|name| cue(node, "member", name, true))
            .into_iter()
            .collect(),
        "named_module" | "namespace" => field(node, source, "name")
            .map(|n| cue(node, "module", n, false))
            .into_iter()
            .collect(),
        "function_or_value_defn" | "value_definition" | "type_definition" if !signature => {
            first_name(node, source)
                .map(|n| cue(node, "definition", n, true))
                .into_iter()
                .collect()
        }
        "value_declaration" | "member_signature" | "value_definition" if signature => {
            first_name(node, source)
                .map(|n| {
                    let mut c = cue(node, "signature", n, true);
                    c.signature = Some(ByteSpan {
                        start: node.start_byte(),
                        end: node.end_byte(),
                    });
                    c
                })
                .into_iter()
                .collect()
        }
        "import_decl" => vec![cue(node, "import", text(node, source), false)],
        "fsi_directive_decl" | "compiler_directive_decl" => {
            vec![cue(node, "directive", text(node, source), false)]
        }
        "application_expression" => node
            .named_child(0)
            .map(|n| cue(node, "call", text(n, source), false))
            .into_iter()
            .collect(),
        _ => vec![],
    }
}
