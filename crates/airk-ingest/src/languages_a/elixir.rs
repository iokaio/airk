// SPDX-License-Identifier: Apache-2.0
use super::*;

pub(super) fn quoted(node: Node<'_>, source: &str) -> bool {
    node.kind() == "call" && field(node, source, "target") == Some("quote")
}

pub(super) fn select(node: Node<'_>, source: &str) -> Option<Cue> {
    if node.kind() == "binary_operator" && field(node, source, "operator") == Some("when") {
        let args = node.parent()?;
        let declaration = args.parent()?;
        if args.kind() == "arguments"
            && declaration.kind() == "call"
            && field(declaration, source, "target")
                .is_some_and(|name| matches!(name, "def" | "defp" | "defmacro" | "defmacrop"))
        {
            let mut selected = cue("guard", field(node, source, "right")?, false)?;
            selected.structural = true;
            return Some(selected);
        }
    }
    if node.kind() != "call" {
        return None;
    }
    let target = node.child_by_field_name("target")?;
    let written = text(target, source);
    if target.kind() == "identifier"
        && matches!(
            written,
            "defmodule"
                | "def"
                | "defp"
                | "defmacro"
                | "defmacrop"
                | "alias"
                | "import"
                | "use"
                | "require"
        )
    {
        let args = named(node, "arguments")?;
        let mut head = args.named_child(0)?;
        if head.kind() == "binary_operator" && field(head, source, "operator") == Some("when") {
            head = head.child_by_field_name("left")?;
        }
        let declaration = matches!(
            written,
            "defmodule" | "def" | "defp" | "defmacro" | "defmacrop"
        );
        let kind = if declaration {
            "definition"
        } else {
            match written {
                "alias" => "alias",
                "use" => "use",
                "require" => "require",
                _ => "import",
            }
        };
        let name = if matches!(written, "def" | "defp" | "defmacro" | "defmacrop") {
            if head.kind() == "call" {
                let function = head.child_by_field_name("target")?;
                if function.kind() != "identifier" {
                    return None;
                }
                let arity = named(head, "arguments").map_or(0, |args| args.named_child_count());
                format!("{}/{}", text(function, source), arity)
            } else if head.kind() == "identifier" {
                format!("{}/0", text(head, source))
            } else {
                return None;
            }
        } else {
            if head.kind() != "alias" {
                return None;
            }
            text(head, source).into()
        };
        let mut selected = cue(kind, &name, declaration)?;
        selected.structural = true;
        Some(selected)
    } else {
        // Function heads are not invocations. Their containing def fact carries arity.
        let mut ancestor = node.parent();
        for _ in 0..3 {
            let Some(parent) = ancestor else {
                break;
            };
            if parent.kind() == "do_block" {
                break;
            }
            if parent.kind() == "call"
                && field(parent, source, "target")
                    .is_some_and(|name| matches!(name, "def" | "defp" | "defmacro" | "defmacrop"))
            {
                return None;
            }
            ancestor = parent.parent();
        }
        cue("call", written, false)
    }
}
