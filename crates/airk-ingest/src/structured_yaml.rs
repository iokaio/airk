// SPDX-License-Identifier: Apache-2.0
//! Ordered YAML syntax view. No alias expansion, key overwriting or interpolation.
use anyhow::{Result, ensure};
use std::collections::BTreeSet;
use tree_sitter::{Node, Parser};

#[derive(Debug)]
pub(crate) struct Value {
    pub start: usize,
    pub end: usize,
    pub kind: Kind,
}
#[derive(Debug)]
pub(crate) enum Kind {
    Scalar(String),
    Mapping(Vec<(Value, Value)>),
    Sequence(Vec<Value>),
    Null,
    Unknown,
}
impl Value {
    pub fn scalar(&self) -> Option<&str> {
        if let Kind::Scalar(s) = &self.kind {
            Some(s)
        } else {
            None
        }
    }
    pub fn mapping(&self) -> Option<&[(Value, Value)]> {
        if let Kind::Mapping(v) = &self.kind {
            Some(v)
        } else {
            None
        }
    }
    pub fn sequence(&self) -> Option<&[Value]> {
        if let Kind::Sequence(v) = &self.kind {
            Some(v)
        } else {
            None
        }
    }
}
pub(crate) struct View {
    pub documents: Vec<Value>,
    pub issues: Vec<(usize, usize, String)>,
    nodes: usize,
}
impl View {
    fn issue(&mut self, node: Node<'_>, reason: &str) {
        if self.issues.len() < 128 && node.end_byte() > node.start_byte() {
            self.issues
                .push((node.start_byte(), node.end_byte(), reason.into()));
        }
    }
    fn value(&mut self, node: Node<'_>, text: &str, depth: usize) -> Result<Value> {
        self.nodes += 1;
        ensure!(
            self.nodes <= 100_000 && depth <= 128,
            "YAML structure budget exceeded"
        );
        let mut cursor = node.walk();
        let children: Vec<_> = node
            .named_children(&mut cursor)
            .filter(|n| n.kind() != "comment")
            .collect();
        let kind = if node.has_error() || node.is_missing() {
            self.issue(node, "Malformed YAML structure omitted");
            Kind::Unknown
        } else {
            match node.kind() {
                "document" | "block_node" | "flow_node" | "block_sequence_item" => {
                    if children
                        .iter()
                        .any(|n| matches!(n.kind(), "anchor" | "alias" | "tag"))
                    {
                        self.issue(node, "YAML anchors, aliases and tags are not expanded");
                        Kind::Unknown
                    } else if children.len() == 1 {
                        return self.value(children[0], text, depth + 1);
                    } else {
                        self.issue(node, "Unsupported YAML wrapper shape");
                        Kind::Unknown
                    }
                }
                "block_mapping" | "flow_mapping" => {
                    let mut pairs = Vec::new();
                    let mut seen = BTreeSet::new();
                    for pair in children {
                        self.nodes += 1;
                        ensure!(self.nodes <= 100_000, "YAML structure budget exceeded");
                        let Some(key) = pair.child_by_field_name("key") else {
                            self.issue(pair, "YAML mapping entry lacks an explicit key");
                            continue;
                        };
                        let key = self.value(key, text, depth + 1)?;
                        let value = if let Some(value) = pair.child_by_field_name("value") {
                            self.value(value, text, depth + 1)?
                        } else {
                            Value {
                                start: pair.start_byte(),
                                end: pair.end_byte(),
                                kind: Kind::Null,
                            }
                        };
                        if let Some(name) = key.scalar() {
                            if !seen.insert(name.to_owned()) {
                                self.issue(
                                    pair,
                                    "Duplicate YAML key retained without selecting a winner",
                                );
                            }
                            if name == "<<" {
                                self.issue(pair, "YAML merge keys are not expanded");
                            }
                        } else {
                            self.issue(pair, "Non-scalar YAML key is unsupported");
                        }
                        pairs.push((key, value));
                    }
                    Kind::Mapping(pairs)
                }
                "block_sequence" | "flow_sequence" => Kind::Sequence(
                    children
                        .into_iter()
                        .map(|n| self.value(n, text, depth + 1))
                        .collect::<Result<_>>()?,
                ),
                "plain_scalar" | "block_scalar" => {
                    Kind::Scalar(node.utf8_text(text.as_bytes())?.into())
                }
                "single_quote_scalar" => {
                    let raw = node.utf8_text(text.as_bytes())?;
                    Kind::Scalar(raw[1..raw.len() - 1].replace("''", "'"))
                }
                "double_quote_scalar" => {
                    match serde_json::from_str::<String>(node.utf8_text(text.as_bytes())?) {
                        Ok(value) => Kind::Scalar(value),
                        Err(_) => {
                            self.issue(node, "YAML-specific quoted escapes are unsupported");
                            Kind::Unknown
                        }
                    }
                }
                _ => {
                    self.issue(node, "Unsupported YAML syntax retained only as source");
                    Kind::Unknown
                }
            }
        };
        Ok(Value {
            start: node.start_byte(),
            end: node.end_byte(),
            kind,
        })
    }
    pub fn diagnostics(&self, facts: &mut crate::build_facts::Facts<'_>) -> Result<()> {
        for (start, end, reason) in &self.issues {
            facts.push(
                *start,
                *end,
                "extraction_diagnostic",
                "yaml_diagnostic",
                reason,
            )?;
        }
        Ok(())
    }
}

pub(crate) fn parse(text: &str) -> Result<View> {
    ensure!(
        text.len() <= crate::MAX_STRUCTURED_BYTES,
        "YAML source exceeds byte limit"
    );
    let mut parser = Parser::new();
    parser.set_language(&tree_sitter_yaml::LANGUAGE.into())?;
    let tree = parser
        .parse(text, None)
        .ok_or_else(|| anyhow::anyhow!("YAML parser returned no tree"))?;
    let mut view = View {
        documents: vec![],
        issues: vec![],
        nodes: 0,
    };
    let root = tree.root_node();
    if root.has_error() {
        view.issue(root, "Malformed YAML; structural evidence is partial");
    }
    let mut cursor = root.walk();
    for node in root
        .named_children(&mut cursor)
        .filter(|n| n.kind() == "document")
    {
        ensure!(view.documents.len() < 64, "YAML document count exceeded");
        let value = view.value(node, text, 0)?;
        view.documents.push(value);
    }
    if view.documents.len() != 1 {
        view.issue(root, "Expected one YAML document; documents are not merged");
    }
    Ok(view)
}

pub(crate) fn emit(
    facts: &mut crate::build_facts::Facts<'_>,
    value: &Value,
    kind: &str,
) -> Result<bool> {
    if let Some(name) = value.scalar() {
        facts.push(value.start, value.end, kind, "yaml_scalar", name)?;
        Ok(true)
    } else {
        facts.push(
            value.start,
            value.end,
            "extraction_diagnostic",
            "yaml_shape",
            &format!("Expected scalar for {kind}"),
        )?;
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn malformed_and_deep_original_yaml_have_explicit_outcomes() -> Result<()> {
        assert!(!parse("jobs: [unterminated\n")?.issues.is_empty());
        let deep = format!("{}original{}", "[".repeat(130), "]".repeat(130));
        assert!(parse(&deep).is_err());
        let merged = parse("base: &original {image: example/v1}\nservice: {<<: *original}\n")?;
        assert!(merged.issues.iter().any(|issue| issue.2.contains("merge")));
        Ok(())
    }
    #[test]
    fn duplicates_aliases_and_multiple_documents_remain_explicit() -> Result<()> {
        let view = parse("name: first\nname: second\ncopy: *unknown\n---\nother: value\n")?;
        assert_eq!(view.documents.len(), 2);
        assert_eq!(view.documents[0].mapping().unwrap().len(), 3);
        assert!(view.issues.iter().any(|i| i.2.contains("Duplicate")));
        assert!(view.issues.iter().any(|i| i.2.contains("aliases")));
        assert!(view.issues.iter().any(|i| i.2.contains("one YAML")));
        Ok(())
    }
}
