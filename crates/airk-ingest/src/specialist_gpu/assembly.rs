// SPDX-License-Identifier: Apache-2.0
use super::*;
pub(super) fn select<'a, 'b>(node: Node<'a>, source: &'b str, adapter: &str) -> Vec<Cue<'a, 'b>> {
    let found = match node.kind() {
        "label" | "const" => field(node, source, "name")
            .or_else(|| {
                named(node, "ident")
                    .or_else(|| named(node, "meta_ident"))
                    .map(|n| text(n, source))
            })
            .map(|name| {
                cue(
                    node,
                    if node.kind() == "label" {
                        "label"
                    } else {
                        "constant"
                    },
                    name,
                    true,
                )
            }),
        "meta" => field(node, source, "kind").map(|name| {
            let kind = match name {
                ".section" | ".text" | ".data" | ".bss" => "section",
                ".include" => "include",
                ".globl" | ".global" | ".extern" => "linkage",
                _ => "directive",
            };
            cue(node, kind, text(node, source), false)
        }),
        "instruction" => field(node, source, "kind").map(|mnemonic| {
            let kind = match adapter {
                "assembly-x86-att" | "assembly-x86-intel" => match mnemonic {
                    "call" | "callq" => "call",
                    "jmp" | "je" | "jne" | "jz" | "jnz" | "ja" | "jb" | "jge" | "jle" => "branch",
                    _ => "instruction",
                },
                "assembly-aarch64" => match mnemonic {
                    "bl" | "blr" => "call",
                    "b" | "br" | "cbz" | "cbnz" | "tbz" | "tbnz" => "branch",
                    _ => "instruction",
                },
                _ => "instruction",
            };
            cue(node, kind, text(node, source), false)
        }),
        _ => None,
    };
    found.into_iter().collect()
}
