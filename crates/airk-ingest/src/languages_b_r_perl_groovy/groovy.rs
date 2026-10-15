// SPDX-License-Identifier: Apache-2.0
use super::*;
#[derive(Clone, Copy)]
pub(super) enum Context {
    None,
    Plugins,
    Dependencies,
    Tasks,
    Configurations,
}
pub(super) fn select<'a>(
    node: Node<'a>,
    source: &str,
    gradle: bool,
    context: Context,
) -> (Vec<Cue<'a>>, Context) {
    let mut cues = vec![];
    let mut next = context;
    let native = match node.kind() {
        "class_declaration"
        | "interface_declaration"
        | "trait_declaration"
        | "enum_declaration"
        | "method_declaration"
        | "constructor_declaration" => {
            field(node, source, "name").map(|name| cue(node, "definition", name, true))
        }
        "package_declaration" => {
            field(node, source, "name").map(|name| cue(node, "package", name, false))
        }
        "import_declaration" => {
            field(node, source, "name").map(|name| cue(node, "import", name, false))
        }
        "local_variable_declaration" => node
            .child_by_field_name("value")
            .filter(|value| value.kind() == "closure")
            .and_then(|body| {
                field(node, source, "name").map(|name| {
                    let mut c = cue(node, "definition", name, true);
                    c.body = Some(body);
                    c
                })
            }),
        "closure" => Some(cue(node, "closure", text(node, source), false)),
        "method_invocation" => {
            field(node, source, "function").map(|name| cue(node, "call", name, false))
        }
        "command_chain" => {
            field(node, source, "receiver").map(|name| cue(node, "call", name, false))
        }
        _ => None,
    };
    if let Some(c) = native {
        cues.push(c);
    }
    if gradle && matches!(node.kind(), "method_invocation" | "command_chain") {
        let target = field(node, source, "function")
            .or_else(|| field(node, source, "receiver"))
            .unwrap_or("");
        let kind = match target {
            "plugins" => {
                next = Context::Plugins;
                Some("build_section")
            }
            "dependencies" => {
                next = Context::Dependencies;
                Some("build_section")
            }
            "tasks" => {
                next = Context::Tasks;
                Some("build_section")
            }
            "configurations" => {
                next = Context::Configurations;
                Some("build_section")
            }
            "include" | "includeBuild" | "project" => Some("module"),
            _ if target.starts_with("tasks.") || target == "task" => Some("task"),
            _ if target.starts_with("configurations.") => Some("configuration"),
            _ => match context {
                Context::Plugins if matches!(target, "id" | "alias") => Some("plugin"),
                Context::Dependencies if !matches!(target, "platform" | "enforcedPlatform") => {
                    Some("dependency")
                }
                Context::Tasks => Some("task"),
                Context::Configurations => Some("configuration"),
                _ => None,
            },
        };
        if let Some(kind) = kind {
            let mut selected = cue(node, kind, text(node, source), false);
            selected.structural = true;
            cues.push(selected);
        }
        if !matches!(
            target,
            "plugins" | "dependencies" | "tasks" | "configurations"
        ) {
            next = Context::None;
        }
    }
    (cues, next)
}
