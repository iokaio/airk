// SPDX-License-Identifier: Apache-2.0
//! Original-offset JSON/JSONC/TOML frontends for the ordered schema view.
pub(crate) use crate::structured_yaml::{Kind, Value};
use anyhow::{Result, ensure};
use std::collections::BTreeSet;
use tree_sitter::{Node, Parser};

pub(crate) struct View {
    pub documents: Vec<Value>,
    pub issues: Vec<(usize, usize, String)>,
    nodes: usize,
}
impl View {
    pub fn issue(&mut self, start: usize, end: usize, reason: &str) {
        if self.issues.len() < 128 && start < end {
            self.issues.push((start, end, reason.into()));
        }
    }
    fn convert(&mut self, node: Node<'_>, text: &str, toml: bool, depth: usize) -> Result<Value> {
        self.nodes += 1;
        ensure!(
            self.nodes <= 100_000 && depth <= 128,
            "structured document budget exceeded"
        );
        let mut cursor = node.walk();
        let children: Vec<_> = node
            .named_children(&mut cursor)
            .filter(|n| n.kind() != "comment")
            .collect();
        let raw = node.utf8_text(text.as_bytes())?;
        let kind = match node.kind() {
            "document" if !toml => {
                if children.len() == 1 {
                    return self.convert(children[0], text, toml, depth + 1);
                }
                self.issue(
                    node.start_byte(),
                    node.end_byte(),
                    "Expected one JSON value",
                );
                Kind::Unknown
            }
            "document" | "object" | "inline_table" | "table" | "table_array_element" => {
                let mut entries = Vec::new();
                let mut names = BTreeSet::new();
                for child in children {
                    if matches!(child.kind(), "bare_key" | "quoted_key" | "dotted_key") {
                        continue;
                    }
                    let mut cursor = child.walk();
                    let parts: Vec<_> = child
                        .named_children(&mut cursor)
                        .filter(|n| n.kind() != "comment")
                        .collect();
                    let pair = if child.kind() == "pair" {
                        let key = child
                            .child_by_field_name("key")
                            .or_else(|| parts.first().copied());
                        let value = child
                            .child_by_field_name("value")
                            .or_else(|| parts.get(1).copied());
                        match (key, value) {
                            (Some(k), Some(v)) => Some((
                                self.convert(k, text, toml, depth + 1)?,
                                self.convert(v, text, toml, depth + 1)?,
                            )),
                            _ => None,
                        }
                    } else if matches!(child.kind(), "table" | "table_array_element") {
                        if let Some(key) = parts.first() {
                            if child.kind() == "table_array_element" {
                                self.issue(
                                    child.start_byte(),
                                    child.end_byte(),
                                    "TOML array tables retained as separate written sections",
                                );
                            }
                            Some((
                                self.convert(*key, text, toml, depth + 1)?,
                                self.convert(child, text, toml, depth + 1)?,
                            ))
                        } else {
                            None
                        }
                    } else {
                        None
                    };
                    if let Some((key, value)) = pair {
                        if let Some(name) = key.scalar()
                            && !names.insert(name.to_owned())
                        {
                            self.issue(
                                child.start_byte(),
                                child.end_byte(),
                                "Duplicate structured key retained without choosing a winner",
                            );
                        }
                        entries.push((key, value));
                    } else {
                        self.issue(
                            child.start_byte(),
                            child.end_byte(),
                            "Malformed structured entry omitted",
                        );
                    }
                }
                Kind::Mapping(entries)
            }
            "array" => Kind::Sequence(
                children
                    .into_iter()
                    .map(|n| self.convert(n, text, toml, depth + 1))
                    .collect::<Result<_>>()?,
            ),
            "string" | "quoted_key" if raw.starts_with('"') && !raw.starts_with("\"\"\"") => {
                match serde_json::from_str::<String>(raw) {
                    Ok(value) => Kind::Scalar(value),
                    Err(_) => {
                        self.issue(
                            node.start_byte(),
                            node.end_byte(),
                            "Unsupported quoted escape; original source retained",
                        );
                        Kind::Unknown
                    }
                }
            }
            "string" | "quoted_key" if toml && raw.starts_with('\'') && !raw.starts_with("'''") => {
                Kind::Scalar(raw[1..raw.len() - 1].into())
            }
            "null" => Kind::Null,
            "bare_key" | "dotted_key" | "number" | "true" | "false" | "integer" | "float"
            | "boolean" | "local_date" | "local_time" | "local_date_time" | "offset_date_time" => {
                Kind::Scalar(raw.into())
            }
            "string" if toml => {
                self.issue(
                    node.start_byte(),
                    node.end_byte(),
                    "Multiline TOML string retained as written syntax",
                );
                Kind::Scalar(raw.into())
            }
            _ => {
                self.issue(
                    node.start_byte(),
                    node.end_byte(),
                    "Unsupported structured syntax",
                );
                Kind::Unknown
            }
        };
        Ok(Value {
            start: node.start_byte(),
            end: node.end_byte(),
            kind,
        })
    }
}

pub(crate) fn parse(path: &str, text: &str, multi_document: bool) -> Result<View> {
    ensure!(
        text.len() <= crate::MAX_FILE_BYTES,
        "structured source exceeds byte limit"
    );
    let suffix = path.rsplit('.').next().unwrap_or("");
    if !matches!(suffix, "json" | "jsonc" | "toml") {
        let view = crate::structured_yaml::parse(text)?;
        return Ok(View {
            documents: view.documents,
            issues: view
                .issues
                .into_iter()
                .filter(|(_, _, reason)| {
                    !multi_document || !reason.starts_with("Expected one YAML document")
                })
                .collect(),
            nodes: 0,
        });
    }
    let toml = suffix == "toml";
    let mut parser = Parser::new();
    let language = if toml {
        tree_sitter_toml_ng::LANGUAGE.into()
    } else {
        tree_sitter_json::LANGUAGE.into()
    };
    parser.set_language(&language)?;
    let tree = parser
        .parse(text, None)
        .ok_or_else(|| anyhow::anyhow!("structured parser returned no tree"))?;
    let mut view = View {
        documents: vec![],
        issues: vec![],
        nodes: 0,
    };
    if tree.root_node().has_error() {
        view.issue(
            0,
            text.len(),
            "Malformed structured syntax; evidence is partial",
        );
    }
    let value = view.convert(tree.root_node(), text, toml, 0)?;
    view.documents.push(value);
    Ok(view)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn duplicate_jsonc_and_toml_sections_preserve_original_offsets() -> Result<()> {
        let text = "{/* original comment */\"name\":\"first\",\"name\":\"second\"}";
        let view = parse("tsconfig.json", text, false)?;
        let entries = view.documents[0].mapping().unwrap();
        assert_eq!(entries.len(), 2);
        assert!(view.issues.iter().any(|i| i.2.contains("Duplicate")));
        assert_eq!(&text[entries[1].1.start..entries[1].1.end], "\"second\"");
        let toml = parse(
            "Cargo.toml",
            "[package]\nname='beacon'\n[dependencies]\nother='1'\n",
            false,
        )?;
        assert_eq!(toml.documents[0].mapping().unwrap().len(), 2);
        Ok(())
    }
}
