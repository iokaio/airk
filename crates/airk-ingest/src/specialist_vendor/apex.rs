// SPDX-License-Identifier: Apache-2.0
use super::*;
use airk_core::{CodeSpan, location::ByteSpan};
use tree_sitter::{Node, Parser};

/// Bounded strong-marker selection, never a parser or suffix-only classification.
pub(crate) fn qualifies_class(bytes: &[u8]) -> bool {
    let bytes = &bytes[..bytes.len().min(4096)];
    let mut words = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at..].starts_with(b"//") {
            while at < bytes.len() && bytes[at] != b'\n' {
                at += 1;
            }
        } else if bytes[at..].starts_with(b"/*") {
            at += 2;
            while at + 1 < bytes.len() && !bytes[at..].starts_with(b"*/") {
                at += 1;
            }
            if at + 1 >= bytes.len() {
                return false;
            }
            at += 2;
        } else if matches!(bytes[at], b'\'' | b'"') {
            let quote = bytes[at];
            at += 1;
            let mut closed = false;
            while at < bytes.len() {
                if bytes[at] == b'\\' {
                    at = (at + 2).min(bytes.len());
                } else if bytes[at] == quote {
                    at += 1;
                    closed = true;
                    break;
                } else {
                    at += 1;
                }
            }
            if !closed {
                return false;
            }
            words.push(String::new());
        } else if bytes[at].is_ascii_alphabetic() || bytes[at] == b'_' {
            let start = at;
            at += 1;
            while at < bytes.len() && (bytes[at].is_ascii_alphanumeric() || bytes[at] == b'_') {
                at += 1;
            }
            words.push(String::from_utf8_lossy(&bytes[start..at]).to_ascii_lowercase());
        } else {
            if !bytes[at].is_ascii_whitespace() {
                words.push((bytes[at] as char).to_string());
            }
            at += 1;
        }
    }
    let class = words.iter().any(|word| word == "class");
    class
        && (words.windows(3).any(|w| {
            matches!(w[0].as_str(), "with" | "without" | "inherited")
                && w[1] == "sharing"
                && w[2] == "class"
        }) || words.windows(2).any(|w| w[0] == "@" && w[1] == "istest"))
}

fn text<'a>(node: Node<'_>, source: &'a str) -> &'a str {
    &source[node.byte_range()]
}

pub(super) fn analyze(path: &str, source: &str, budget: &mut HostBudget) -> Result<Analysis> {
    budget.charge(1, 0, 0, 0)?;
    let mut parser = Parser::new();
    parser.set_language(&tree_sitter_sfapex::apex::LANGUAGE.into())?;
    let tree = parser
        .parse(source, None)
        .ok_or_else(|| anyhow::anyhow!("Apex parser returned no tree"))?;
    let mut output = Facts::new(path, source, "apex")?;
    let mut pending = vec![(tree.root_node(), None)];
    while let Some((node, owner)) = pending.pop() {
        budget.charge(1, 0, 0, 0)?;
        if matches!(
            node.kind(),
            "line_comment" | "block_comment" | "comment" | "string_literal"
        ) {
            continue;
        }
        let mut next_owner = owner;
        let declaration = matches!(
            node.kind(),
            "class_declaration"
                | "interface_declaration"
                | "enum_declaration"
                | "method_declaration"
                | "constructor_declaration"
                | "trigger_declaration"
        );
        let kind = match node.kind() {
            "class_declaration" => Some("class"),
            "interface_declaration" => Some("interface"),
            "enum_declaration" => Some("enum"),
            "method_declaration" => Some("method"),
            "constructor_declaration" => Some("constructor"),
            "trigger_declaration" => Some("trigger"),
            "method_invocation" => Some("call"),
            "query_expression" => Some("query"),
            "storage_identifier" => Some("table_reference"),
            "superclass" | "interfaces" | "extends_interfaces" => Some("reference"),
            _ => None,
        };
        if let Some(kind) = kind.filter(|_| !node.has_error() && !node.is_missing()) {
            let kind = if declaration { "definition" } else { kind };
            let name = if let Some(name) = node.child_by_field_name("name") {
                if kind == "call" {
                    node.child_by_field_name("object").map_or_else(
                        || text(name, source).to_owned(),
                        |object| format!("{}.{}", text(object, source), text(name, source)),
                    )
                } else {
                    text(name, source).to_owned()
                }
            } else {
                text(node, source).to_owned()
            };
            budget.charge(0, 1, 0, 0)?;
            let index = output.facts.len();
            output.push(node.start_byte(), node.end_byte(), kind, node.kind(), &name)?;
            let mut scope = owner.map_or_else(Vec::new, |i: usize| {
                output.facts[i].code.as_ref().unwrap().scope.clone()
            });
            let owner_id = owner.map(|i| output.facts[i].code.as_ref().unwrap().symbol_id.clone());
            let truncated_owner =
                owner.is_some_and(|i| output.facts[i].code.as_ref().unwrap().details_truncated);
            if let Some(i) = owner {
                scope.push(output.facts[i].name.clone());
            }
            ensure!(scope.len() <= 128, "Apex owner depth limit");
            let body = node.child_by_field_name("body");
            let signature = body.map(|body| ByteSpan {
                start: node.start_byte(),
                end: body.start_byte(),
            });
            let mut fingerprints = if declaration {
                Some(crate::syntax::declaration_fingerprints(
                    source,
                    &name,
                    &scope.iter().map(String::as_str).collect::<Vec<_>>(),
                    signature,
                    body,
                    None,
                    budget,
                )?)
            } else {
                None
            };
            if truncated_owner && let Some(hashes) = &mut fingerprints {
                hashes.qualified_name = None;
            }
            let code = output.facts[index].code.as_mut().unwrap();
            code.details_truncated |= truncated_owner;
            code.scope = scope;
            code.owner_id = owner_id;
            code.fingerprints = fingerprints;
            code.signature_span = signature.map(|s| CodeSpan {
                start_byte: s.start,
                end_byte: s.end,
            });
            code.basis = EvidenceBasis::Syntax;
            code.producer = "airk-tree-sitter/apex".into();
            code.producer_version = "vendor-facts/1".into();
            code.coverage = "Written Apex syntax and selected query/reference candidates; Salesforce metadata, dispatch, security and query results unresolved".into();
            if matches!(kind, "call" | "reference" | "table_reference") {
                code.target = Some(name.chars().take(1024).collect());
            }
            if declaration {
                next_owner = Some(index);
            }
        }
        for i in (0..node.named_child_count()).rev() {
            ensure!(pending.len() < 500_000, "Apex pending node limit");
            if let Some(child) = node.named_child(i) {
                pending.push((child, next_owner));
            }
        }
    }
    let mut result = finish(output, tree.root_node().has_error(), "apex")?;
    if result.status != "partial-syntax" {
        result.status = "syntax".into();
    }
    Ok(result)
}
