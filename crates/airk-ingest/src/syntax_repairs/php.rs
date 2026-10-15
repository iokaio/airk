// SPDX-License-Identifier: Apache-2.0
use super::*;

pub(super) fn visit(node: Node<'_>, source: &str, output: &mut Facts<'_>) -> Result<()> {
    match node.kind() {
        "namespace_definition" => {
            if let Some(name) = node.child_by_field_name("name") {
                emit(output, node, "namespace", text(name, source))?;
            }
        }
        "namespace_use_clause" | "namespace_use_declaration" => {
            emit(output, node, "import", text(node, source))?
        }
        "use_declaration" | "use_as_clause" | "use_instead_of_clause" => {
            emit(output, node, "mixin", text(node, source))?
        }
        "function_call_expression"
        | "member_call_expression"
        | "nullsafe_member_call_expression"
        | "scoped_call_expression" => {
            let end = node
                .child_by_field_name("arguments")
                .map(|arguments| arguments.start_byte());
            if let Some(end) = end {
                let callee = source.get(node.start_byte()..end).unwrap_or("").trim_end();
                let target = node
                    .child_by_field_name("name")
                    .or_else(|| node.child_by_field_name("function"));
                let literal = target.is_some_and(|target| {
                    matches!(target.kind(), "name" | "qualified_name" | "relative_name")
                });
                emit(
                    output,
                    node,
                    if literal { "call" } else { "dynamic_call" },
                    callee,
                )?;
            }
        }
        _ => {}
    }
    Ok(())
}
