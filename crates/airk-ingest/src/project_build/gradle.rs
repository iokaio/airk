// SPDX-License-Identifier: Apache-2.0
use super::*;

fn callee<'a>(mut node: Node<'_>, source: &'a str) -> Option<&'a str> {
    for _ in 0..8 {
        match node.kind() {
            "call_expression" | "expression" => node = node.named_child(0)?,
            "identifier" | "simple_identifier" | "navigation_expression" => {
                return Some(text(node, source));
            }
            _ => return None,
        }
    }
    None
}

pub(super) fn visit(
    node: Node<'_>,
    source: &str,
    context: Context,
    facts: &mut Facts<'_>,
) -> Result<Context> {
    if node.kind() != "call_expression" {
        return Ok(context);
    }
    let Some(callee) = callee(node, source) else {
        return Ok(context);
    };
    let next = match callee {
        "plugins" => Context::Plugins,
        "dependencies" => Context::Dependencies,
        "tasks" => Context::Tasks,
        "configurations" => Context::Configurations,
        _ => context,
    };
    let kind = if callee == "include" || callee == "includeBuild" || callee == "project" {
        Some("module")
    } else if callee.starts_with("tasks.") || context == Context::Tasks {
        Some("task")
    } else if callee.starts_with("configurations.") || context == Context::Configurations {
        Some("configuration")
    } else if context == Context::Plugins && matches!(callee, "id" | "kotlin" | "alias") {
        Some("plugin")
    } else if context == Context::Dependencies
        && !matches!(callee, "platform" | "enforcedPlatform" | "project")
    {
        Some("dependency")
    } else {
        None
    };
    if let Some(kind) = kind {
        // Arguments remain expressions; interpolation and version-catalog aliases stay unexpanded.
        emit(facts, node, kind, text(node, source))?;
    }
    if matches!(
        callee,
        "plugins" | "dependencies" | "tasks" | "configurations"
    ) && named(node, "annotated_lambda").is_some()
    {
        emit(facts, node, "build_section", callee)?;
    }
    Ok(
        if matches!(
            callee,
            "plugins" | "dependencies" | "tasks" | "configurations"
        ) {
            next
        } else {
            Context::None
        },
    )
}
