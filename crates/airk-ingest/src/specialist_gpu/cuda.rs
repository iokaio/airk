// SPDX-License-Identifier: Apache-2.0
use super::*;

pub(super) fn declarator_name<'a>(mut node: Node<'_>, source: &'a str) -> Option<&'a str> {
    for _ in 0..32 {
        if matches!(
            node.kind(),
            "identifier" | "field_identifier" | "type_identifier" | "qualified_identifier"
        ) {
            return Some(text(node, source));
        }
        node = node.child_by_field_name("declarator")?;
    }
    None
}
pub(super) fn select<'a, 'b>(node: Node<'a>, source: &'b str) -> Vec<Cue<'a, 'b>> {
    let found = match node.kind() {
        "function_definition" => node
            .child_by_field_name("declarator")
            .and_then(|n| declarator_name(n, source))
            .map(|name| {
                let mut cursor = node.walk();
                let kernel = node
                    .children(&mut cursor)
                    .any(|child| text(child, source) == "__global__");
                cue(node, if kernel { "kernel" } else { "function" }, name, true)
            }),
        "struct_specifier" | "class_specifier" | "namespace_definition" => {
            field(node, source, "name").map(|name| cue(node, "type", name, true))
        }
        "call_expression" => field(node, source, "function").map(|name| {
            cue(
                node,
                if named(node, "kernel_call_syntax").is_some() {
                    "kernel_launch"
                } else {
                    "call"
                },
                name,
                false,
            )
        }),
        "type_qualifier" | "storage_class_specifier"
            if matches!(
                text(node, source),
                "__global__" | "__device__" | "__host__" | "__shared__" | "__constant__"
            ) =>
        {
            Some(cue(node, "cuda_qualifier", text(node, source), false))
        }
        "launch_bounds" => Some(cue(node, "launch_bounds", text(node, source), false)),
        "attribute" | "attribute_declaration" => {
            Some(cue(node, "attribute", text(node, source), false))
        }
        "preproc_include" => {
            field(node, source, "path").map(|name| cue(node, "include", name, false))
        }
        _ => None,
    };
    found.into_iter().collect()
}
