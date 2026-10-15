// SPDX-License-Identifier: Apache-2.0
//! Bounded written hardware/contract syntax, without execution or elaboration.
mod solidity;
mod verilog;
mod vhdl;
use crate::{build_facts::Facts, syntax::Analysis};
use airk_core::{
    CodeSpan, EvidenceBasis,
    location::{ByteSpan, HostBudget},
};
use anyhow::{Result, ensure};
use tree_sitter::{Language, Node, Parser};

struct Cue<'tree, 'source> {
    node: Node<'tree>,
    kind: &'static str,
    name: &'source str,
    declaration: bool,
    body: Option<Node<'tree>>,
}
fn cue<'a, 'b>(
    node: Node<'a>,
    kind: &'static str,
    name: &'b str,
    declaration: bool,
) -> Cue<'a, 'b> {
    Cue {
        node,
        kind,
        name,
        declaration,
        body: node.child_by_field_name("body"),
    }
}
fn text<'a>(node: Node<'_>, source: &'a str) -> &'a str {
    source.get(node.byte_range()).unwrap_or("")
}
fn named<'a>(node: Node<'a>, kind: &str) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .find(|child| child.kind() == kind)
}
fn field<'a>(node: Node<'_>, source: &'a str, key: &str) -> Option<&'a str> {
    node.child_by_field_name(key).map(|node| text(node, source))
}

/// Only an unambiguous leading declaration after whitespace/comments qualifies
/// ambiguous `.v`; an incomplete comment or header retains the generic fallback.
pub(crate) fn recognize_verilog(prefix: &[u8]) -> bool {
    if prefix.len() > 4096 {
        return false;
    }
    let Ok(mut text) = std::str::from_utf8(prefix) else {
        return false;
    };
    loop {
        text = text.trim_start();
        if let Some(rest) = text.strip_prefix("//") {
            let Some((_, tail)) = rest.split_once('\n') else {
                return false;
            };
            text = tail;
        } else if let Some(rest) = text.strip_prefix("/*") {
            let Some((_, tail)) = rest.split_once("*/") else {
                return false;
            };
            text = tail;
        } else {
            break;
        }
    }
    let Some(keyword_end) = text.find(char::is_whitespace) else {
        return false;
    };
    if !matches!(&text[..keyword_end], "module" | "macromodule" | "primitive") {
        return false;
    }
    let name = text[keyword_end..].trim_start();
    let end = if name.starts_with('\\') {
        name.find(char::is_whitespace).unwrap_or(name.len())
    } else {
        if !name
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_alphabetic() || ch == '_')
        {
            return false;
        }
        name.find(|ch: char| !(ch.is_ascii_alphanumeric() || matches!(ch, '_' | '$')))
            .unwrap_or(name.len())
    };
    let tail = name[end..].trim_start();
    matches!(tail.chars().next(), Some('(' | '#' | ';')) && tail.contains(';')
}

pub(crate) fn analyze(path: &str, source: &str, adapter: &str) -> Result<Analysis> {
    let mut budget = HostBudget::new(500_000, 10_000, 16_777_216)?;
    budget.charge(1, 0, source.len(), 0)?;
    analyze_budgeted(path, source, adapter, &mut budget)
}
/// Caller admits source/map bytes once; siblings share this work/fact/hash budget.
pub(crate) fn analyze_budgeted(
    path: &str,
    source: &str,
    adapter: &str,
    budget: &mut HostBudget,
) -> Result<Analysis> {
    ensure!(
        source.len() <= crate::MAX_FILE_BYTES,
        "hardware source byte limit"
    );
    let grammar: Language = match adapter {
        "solidity" => tree_sitter_solidity::LANGUAGE.into(),
        "vhdl" => tree_sitter_vhdl::LANGUAGE.into(),
        "verilog" | "systemverilog" => tree_sitter_verilog::LANGUAGE.into(),
        _ => anyhow::bail!("unsupported hardware adapter"),
    };
    budget.charge(1, 0, 0, 0)?;
    let mut parser = Parser::new();
    parser.set_language(&grammar)?;
    let tree = parser
        .parse(source, None)
        .ok_or_else(|| anyhow::anyhow!("hardware parser returned no tree"))?;
    let mut output = Facts::new(path, source, adapter)?;
    let mut pending = vec![(tree.root_node(), None)];
    let mut partial = tree.root_node().has_error();
    while let Some((node, owner)) = pending.pop() {
        budget.charge(1, 0, 0, 0)?;
        let mut next_owner = owner;
        let kind = node.kind();
        if kind.contains("compiler_directive")
            || kind.contains("macro")
            || kind.contains("generate")
            || kind.starts_with("yul_")
        {
            partial = true;
        }
        // Do not discover declarations inside comments, strings or macro bodies.
        if matches!(
            kind,
            "comment"
                | "line_comment"
                | "block_comment"
                | "string"
                | "string_literal"
                | "double_quoted_string"
                | "text_macro_definition"
                | "text_macro_usage"
        ) {
            continue;
        }
        let broken_tail = node.next_named_sibling().is_some_and(|next| {
            next.is_error() && source[node.end_byte()..next.start_byte()].trim().is_empty()
        });
        if !node.has_error() && !node.is_missing() && !broken_tail {
            let selected = match adapter {
                "solidity" => solidity::select(node, source),
                "vhdl" => vhdl::select(node, source),
                _ => verilog::select(node, source),
            };
            for selected in selected {
                if selected.name.trim().is_empty() {
                    continue;
                }
                budget.charge(0, 1, 0, 0)?;
                let index = output.facts.len();
                output.push(
                    selected.node.start_byte(),
                    selected.node.end_byte(),
                    selected.kind,
                    selected.node.kind(),
                    selected.name,
                )?;
                let owner_id =
                    owner.map(|i: usize| output.facts[i].code.as_ref().unwrap().symbol_id.clone());
                let mut scope = owner.map_or_else(Vec::new, |i| {
                    output.facts[i].code.as_ref().unwrap().scope.clone()
                });
                let truncated =
                    owner.is_some_and(|i| output.facts[i].code.as_ref().unwrap().details_truncated);
                if let Some(i) = owner {
                    scope.push(output.facts[i].name.clone());
                }
                ensure!(scope.len() <= 128, "hardware owner depth limit");
                let signature = selected
                    .body
                    .filter(|body| body.start_byte() > selected.node.start_byte())
                    .map(|body| ByteSpan {
                        start: selected.node.start_byte(),
                        end: body.start_byte(),
                    });
                let fingerprints = if selected.declaration {
                    let mut values = crate::syntax::declaration_fingerprints(
                        source,
                        selected.name,
                        &scope.iter().map(String::as_str).collect::<Vec<_>>(),
                        signature,
                        selected.body,
                        None,
                        budget,
                    )?;
                    if truncated {
                        values.qualified_name = None;
                    }
                    Some(values)
                } else {
                    None
                };
                let code = output.facts[index].code.as_mut().unwrap();
                code.owner_id = owner_id;
                code.scope = scope;
                code.details_truncated |= truncated;
                code.fingerprints = fingerprints;
                code.signature_span = signature.map(|span| CodeSpan {
                    start_byte: span.start,
                    end_byte: span.end,
                });
                code.basis = EvidenceBasis::Syntax;
                code.producer = format!("airk-tree-sitter/{adapter}");
                code.producer_version = "specialist-hardware-facts/1".into();
                code.coverage="Selected written declarations and references only; no execution, security analysis, elaboration, simulation, preprocessor evaluation or resolved bindings; unsupported body/documentation dimensions remain unknown".into();
                if matches!(
                    selected.kind,
                    "call"
                        | "import"
                        | "inheritance"
                        | "instantiation"
                        | "include"
                        | "use"
                        | "library"
                ) {
                    code.target = Some(selected.name.chars().take(1024).collect());
                }
                if selected.declaration {
                    next_owner = Some(index);
                }
            }
        }
        for i in (0..node.named_child_count()).rev() {
            ensure!(pending.len() < 500_000, "hardware pending node limit");
            if let Some(child) = node.named_child(i) {
                pending.push((child, next_owner));
            }
        }
    }
    ensure!(
        serde_json::to_vec(&output.facts)?.len() <= 16_777_216,
        "hardware evidence byte limit"
    );
    let mut result = output.finish(partial);
    if !partial {
        result.status = "syntax".into();
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_hardware_rows_and_crlf_nonbmp_locations() -> Result<()> {
        for (adapter, source, expected) in [
            (
                "solidity",
                include_str!("../tests/fixtures/specialist-hardware/original.sol"),
                vec![
                    "Ledger",
                    "IReceipt",
                    "MathLedger",
                    "Paid",
                    "InvalidAmount",
                    "Receipt",
                    "record",
                ],
            ),
            (
                "vhdl",
                include_str!("../tests/fixtures/specialist-hardware/original.vhdl"),
                vec![
                    "ledger",
                    "rtl",
                    "receipt_types",
                    "worker",
                    "clk",
                    "pending",
                    "capture",
                ],
            ),
            (
                "systemverilog",
                include_str!("../tests/fixtures/specialist-hardware/original.sv"),
                vec![
                    "ledger",
                    "receipt_bus",
                    "receipt_types",
                    "Receipt",
                    "WIDTH",
                    "clk",
                    "record",
                    "clear",
                ],
            ),
        ] {
            let source = source.replace("\r\n", "\n").replace('\n', "\r\n");
            let result = analyze("original", &source, adapter)?;
            for name in expected {
                assert!(
                    result.facts.iter().any(|fact| fact.name == name),
                    "{adapter} missing {name}: {:?}",
                    result.facts
                );
            }
            for fact in &result.facts {
                let span = fact.code.as_ref().unwrap().span.as_ref().unwrap();
                assert!(source.get(span.start_byte..span.end_byte).is_some());
                assert_eq!(
                    fact.start_line,
                    source[..span.start_byte]
                        .bytes()
                        .filter(|b| *b == b'\n')
                        .count()
                        + 1
                );
            }
        }
        Ok(())
    }
    #[test]
    fn comments_literals_macros_and_malformed_names_are_not_promoted() -> Result<()> {
        for (adapter, source) in [
            (
                "solidity",
                "// contract Phantom {}\ncontract Real { string constant value = \"contract Phantom {}\"; function broken( { }",
            ),
            (
                "vhdl",
                "-- entity Phantom is end;\nentity Real is port (broken : ); end Real;",
            ),
            (
                "systemverilog",
                "// module Phantom; endmodule\n`define MAKE module Phantom; endmodule\nmodule Real; string value = \"module Phantom; endmodule\"; endmodule",
            ),
        ] {
            let result = analyze("original", source, adapter)?;
            assert!(!result.facts.iter().any(|fact| fact.name == "Phantom"));
            assert_eq!(result.status, "partial-syntax");
        }
        Ok(())
    }
    #[test]
    fn ambiguous_suffix_and_shared_fact_budget_are_explicit() -> Result<()> {
        assert!(recognize_verilog(
            b"/* original */ module ledger(input clk);"
        ));
        for source in [
            b"The module ledger is prose;".as_slice(),
            b"module ledger",
            b"/* module fake;",
            b"fn module() {}",
        ] {
            assert!(!recognize_verilog(source));
        }
        let mut budget = HostBudget::new(5000, 1, 0)?;
        analyze_budgeted("one.sol", "contract One {}", "solidity", &mut budget)?;
        assert!(analyze_budgeted("two.sol", "contract Two {}", "solidity", &mut budget).is_err());
        Ok(())
    }
}
