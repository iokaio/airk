// SPDX-License-Identifier: Apache-2.0
use super::*;
pub(super) fn select<'a>(node: Node<'a>, source: &str) -> Vec<Cue<'a>> {
    let mut cues = vec![];
    if node.kind() == "binary_operator"
        && let Some(operator) = field(node, source, "operator")
    {
        let (name, value) = match operator {
            "<-" | "<<-" | "=" => (
                node.child_by_field_name("lhs"),
                node.child_by_field_name("rhs"),
            ),
            "->" | "->>" => (
                node.child_by_field_name("rhs"),
                node.child_by_field_name("lhs"),
            ),
            _ => (None, None),
        };
        if let (Some(name), Some(value)) = (name, value)
            && name.kind() == "identifier"
        {
            let declaration = value.kind() == "function_definition";
            let mut selected = cue(
                node,
                if declaration {
                    "definition"
                } else {
                    "assignment"
                },
                text(name, source),
                declaration,
            );
            if declaration {
                selected.body = value.child_by_field_name("body");
            }
            cues.push(selected);
        }
    }
    if node.kind() == "call"
        && let Some(function) = node.child_by_field_name("function")
    {
        cues.push(cue(node, "call", text(function, source), false));
        if matches!(
            text(function, source),
            "library" | "require" | "requireNamespace"
        ) && let Some(args) = node.child_by_field_name("arguments")
            && let Some(argument) = named(args, "argument")
            && let Some(value) = argument.child_by_field_name("value")
            && matches!(value.kind(), "identifier" | "string")
        {
            let mut selected = cue(node, "package_load", text(value, source), false);
            selected.structural = true;
            cues.push(selected);
        }
    }
    cues
}
