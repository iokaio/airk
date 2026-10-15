// SPDX-License-Identifier: Apache-2.0
use super::*;

pub(super) fn visit(node: Node<'_>, source: &str, output: &mut Facts<'_>) -> Result<()> {
    if node.kind() != "call" {
        return Ok(());
    }
    let Some(method) = node.child_by_field_name("method") else {
        return Ok(());
    };
    let method_name = text(method, source);
    let name = source
        .get(node.start_byte()..method.end_byte())
        .unwrap_or("");
    let dynamic = matches!(
        method_name,
        "send" | "public_send" | "__send__" | "eval" | "instance_eval" | "class_eval"
    );
    emit(
        output,
        node,
        if dynamic { "dynamic_call" } else { "call" },
        name,
    )?;
    // Qualified calls may be user methods; only bare written DSL forms get these cues.
    if node.child_by_field_name("receiver").is_some() {
        return Ok(());
    }
    let Some(arguments) = node.child_by_field_name("arguments") else {
        return Ok(());
    };
    let mut cursor = arguments.walk();
    for argument in arguments.named_children(&mut cursor) {
        if matches!(method_name, "require" | "require_relative") && argument.kind() == "string" {
            let mut cursor = argument.walk();
            if !argument
                .named_children(&mut cursor)
                .any(|child| child.kind() == "interpolation")
            {
                emit(output, argument, "import", text(argument, source))?;
            }
        } else if matches!(method_name, "include" | "extend" | "prepend")
            && matches!(argument.kind(), "constant" | "scope_resolution")
        {
            emit(output, argument, "mixin", text(argument, source))?;
        }
    }
    Ok(())
}
