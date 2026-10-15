// SPDX-License-Identifier: Apache-2.0
use super::*;

pub(super) fn visit(
    node: Node<'_>,
    source: &str,
    context: Context,
    facts: &mut Facts<'_>,
) -> Result<Context> {
    match node.kind() {
        "function_definition" => {
            if let Some(name) = node.child_by_field_name("name") {
                emit(facts, node, "build_function", text(name, source))?;
            }
        }
        "call" => {
            if let Some(function) = node.child_by_field_name("function") {
                let callee = text(function, source);
                if callee == "load" {
                    emit(facts, node, "load", text(node, source))?;
                    return Ok(Context::Load);
                }
                if let Some(arguments) = node.child_by_field_name("arguments") {
                    let mut cursor = arguments.walk();
                    for argument in arguments.named_children(&mut cursor) {
                        if argument.kind() == "keyword_argument"
                            && argument
                                .child_by_field_name("name")
                                .is_some_and(|name| text(name, source) == "name")
                            && let Some(value) = argument.child_by_field_name("value")
                        {
                            // A written name argument is a target candidate, including custom macros.
                            emit(facts, node, "build_target", text(value, source))?;
                            emit(facts, function, "build_rule", callee)?;
                        }
                    }
                }
            }
        }
        "keyword_argument" => {
            let name = node
                .child_by_field_name("name")
                .map(|name| text(name, source));
            if matches!(
                name,
                Some("deps" | "exports" | "runtime_deps" | "implementation_deps" | "data")
            ) {
                if let Some(value) = node.child_by_field_name("value") {
                    emit(facts, value, "dependencies", text(value, source))?;
                }
                return Ok(Context::Dependencies);
            }
        }
        "string" if matches!(context, Context::Dependencies | Context::Load) => {
            let raw = text(node, source);
            // Keep literal spelling. Interpolation, escaping and repository labels are not resolved.
            if !raw.starts_with(['f', 'F']) {
                emit(
                    facts,
                    node,
                    if context == Context::Load {
                        "load_symbol"
                    } else {
                        "label"
                    },
                    raw,
                )?;
            }
        }
        _ => {}
    }
    Ok(context)
}
