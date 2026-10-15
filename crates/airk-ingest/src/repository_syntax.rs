// SPDX-License-Identifier: Apache-2.0
//! Bounded, format-specific written-syntax cues. Parsing and evidence binding live in syntax.
mod dockerfile;
mod graphql;
mod hcl;
mod proto;

use anyhow::{Result, ensure};
use tree_sitter::Node;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RepositoryFormat {
    Hcl,
    Dockerfile,
    Proto,
    Graphql,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CueCategory {
    Declaration,
    Attribute,
    Import,
    Reference,
    Directive,
    Value,
}

/// Names borrow complete source spelling: the binder must fingerprint before display truncation.
/// `body` is a syntactic declaration body only; absent bodies/dimensions remain unknown.
pub(crate) struct RepositoryCue<'tree, 'source> {
    pub node: Node<'tree>,
    pub name: &'source str,
    pub kind: &'static str,
    pub category: CueCategory,
    pub owner: Option<Node<'tree>>,
    pub body: Option<Node<'tree>>,
}

pub(crate) fn extract<'tree, 'source>(
    root: Node<'tree>,
    text: &'source str,
    format: RepositoryFormat,
) -> Result<Vec<RepositoryCue<'tree, 'source>>> {
    ensure!(root.end_byte() <= text.len(), "syntax tree exceeds source");
    let mut pending = vec![(root, None)];
    let mut visited = 0usize;
    let mut cues = Vec::new();
    let mut docker_stage = None;
    while let Some((node, inherited_owner)) = pending.pop() {
        if format == RepositoryFormat::Dockerfile && node.kind() == "from_instruction" {
            docker_stage = None;
        }
        let owner = inherited_owner.or(docker_stage);
        visited += 1;
        ensure!(visited <= 500_000, "file exceeds parser node limit");
        let mut next_owner = owner;
        if !node.has_error() && !node.is_missing() {
            let start = cues.len();
            match format {
                RepositoryFormat::Hcl => hcl::visit(node, text, owner, &mut cues),
                RepositoryFormat::Dockerfile => dockerfile::visit(node, text, owner, &mut cues),
                RepositoryFormat::Proto => proto::visit(node, text, owner, &mut cues),
                RepositoryFormat::Graphql => graphql::visit(node, text, owner, &mut cues),
            }
            ensure!(cues.len() <= 10_000, "file exceeds syntax fact limit");
            if cues[start..]
                .iter()
                .any(|cue| cue.category == CueCategory::Declaration)
            {
                next_owner = Some(node);
                if format == RepositoryFormat::Dockerfile && node.kind() == "from_instruction" {
                    docker_stage = Some(node);
                }
            }
        }
        // Reverse named traversal preserves source order, and admission bounds pending storage.
        for index in (0..node.named_child_count()).rev() {
            ensure!(pending.len() < 500_000, "file exceeds parser pending limit");
            if let Some(child) = node.named_child(index) {
                pending.push((child, next_owner));
            }
        }
    }
    Ok(cues)
}

fn child<'a>(node: Node<'a>, kinds: &[&str]) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .find(|child| kinds.contains(&child.kind()))
}

fn source<'a>(node: Node<'_>, text: &'a str) -> &'a str {
    text.get(node.byte_range()).unwrap_or("").trim()
}

fn emit<'tree, 'source>(
    cues: &mut Vec<RepositoryCue<'tree, 'source>>,
    node: Node<'tree>,
    name: &'source str,
    kind: &'static str,
    category: CueCategory,
    owner: Option<Node<'tree>>,
    body: Option<Node<'tree>>,
) {
    if !name.is_empty() {
        cues.push(RepositoryCue {
            node,
            name,
            kind,
            category,
            owner,
            body,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::{Language, Parser, Tree};

    fn parse(format: RepositoryFormat, text: &str) -> Tree {
        let language: Language = match format {
            RepositoryFormat::Hcl => tree_sitter_hcl::LANGUAGE.into(),
            RepositoryFormat::Dockerfile => tree_sitter_dockerfile_updated::language(),
            RepositoryFormat::Proto => tree_sitter_proto::LANGUAGE.into(),
            RepositoryFormat::Graphql => tree_sitter_graphql::LANGUAGE.into(),
        };
        let mut parser = Parser::new();
        parser.set_language(&language).unwrap();
        parser.parse(text, None).unwrap()
    }

    #[test]
    fn hcl_headers_empty_blocks_and_references_preserve_written_structure() -> Result<()> {
        let text = "resource \"fictional\" \"café\" {}\r\nresource \"fictional\" \"other\" {\r\n name = var.input\r\n note = \"var.not_a_reference\"\r\n}\r\n";
        let tree = parse(RepositoryFormat::Hcl, text);
        assert!(
            !tree.root_node().has_error(),
            "{}",
            tree.root_node().to_sexp()
        );
        let cues = extract(tree.root_node(), text, RepositoryFormat::Hcl)?;
        let empty = cues
            .iter()
            .find(|cue| cue.name == "resource \"fictional\" \"café\"")
            .expect("empty block labels");
        assert_eq!(
            source(empty.node, text),
            "resource \"fictional\" \"café\" {}"
        );
        assert!(empty.body.is_none());
        let key = cues
            .iter()
            .find(|cue| cue.kind == "key" && cue.name == "name")
            .unwrap();
        assert_eq!(key.owner.unwrap().kind(), "block");
        assert!(
            cues.iter()
                .any(|cue| cue.kind == "reference" && cue.name == "var")
        );
        assert!(
            !cues
                .iter()
                .any(|cue| cue.kind == "reference" && cue.name.contains("not_a_reference"))
        );
        Ok(())
    }

    #[test]
    fn docker_stages_reset_and_multiline_spans_are_not_line_fragments() -> Result<()> {
        let text = "FROM scratch AS first\r\nRUN echo café \\\n  && echo second\nFROM scratch AS final\r\nCOPY --from=first /one /two\r\nFROM scratch\r\nENV LAST=plain\r\n";
        let tree = parse(RepositoryFormat::Dockerfile, text);
        assert!(
            !tree.root_node().has_error(),
            "{}",
            tree.root_node().to_sexp()
        );
        let cues = extract(tree.root_node(), text, RepositoryFormat::Dockerfile)?;
        let run = cues
            .iter()
            .find(|cue| cue.kind == "directive" && cue.name == "RUN")
            .unwrap();
        assert!(source(run.node, text).contains("\n"));
        assert!(source(run.owner.unwrap(), text).ends_with("first"));
        let copy = cues.iter().find(|cue| cue.kind == "copy_from").unwrap();
        assert_eq!(copy.name, "--from=first");
        assert!(source(copy.owner.unwrap(), text).ends_with("final"));
        assert!(
            cues.iter()
                .find(|cue| cue.kind == "environment")
                .unwrap()
                .owner
                .is_none()
        );
        Ok(())
    }

    #[test]
    fn docker_crlf_continuations_remain_partial_without_offset_normalization() -> Result<()> {
        // The pinned grammar requires backslash+LF for shell continuation.
        let text = "FROM scratch AS first\r\nRUN echo café \\\r\n && echo second\r\n";
        let tree = parse(RepositoryFormat::Dockerfile, text);
        assert!(tree.root_node().has_error());
        let analysis = crate::syntax::analyze_selected("Dockerfile", text, "dockerfile")?;
        assert_eq!(analysis.status, "partial-syntax");
        for fact in &analysis.facts {
            let span = fact
                .code
                .as_ref()
                .and_then(|code| code.span.as_ref())
                .expect("source span");
            let original = text
                .get(span.start_byte..span.end_byte)
                .expect("original UTF-8 offsets");
            assert!(original.contains(&fact.name));
        }
        assert!(text.contains("\\\r\n"));
        Ok(())
    }

    #[test]
    fn proto_nested_owners_maps_and_stream_rpc_references_are_explicit() -> Result<()> {
        let text = include_str!("../tests/fixtures/repository-syntax/archive.proto");
        let tree = parse(RepositoryFormat::Proto, text);
        assert!(
            !tree.root_node().has_error(),
            "{}",
            tree.root_node().to_sexp()
        );
        let cues = extract(tree.root_node(), text, RepositoryFormat::Proto)?;
        let nested = cues
            .iter()
            .find(|cue| cue.kind == "definition" && cue.name == "Entry")
            .unwrap();
        assert_eq!(nested.owner.unwrap().kind(), "message");
        let rpc = cues.iter().find(|cue| cue.name == "Watch").unwrap();
        assert_eq!(rpc.owner.unwrap().kind(), "service");
        assert!(source(rpc.node, text).contains("stream Envelope"));
        assert!(
            cues.iter()
                .any(|cue| cue.node.kind() == "map_field" && cue.name.contains("entries = 1"))
        );
        assert_eq!(
            cues.iter()
                .filter(|cue| cue.kind == "type_reference" && cue.owner == Some(rpc.node))
                .count(),
            2
        );
        Ok(())
    }

    #[test]
    fn graphql_extensions_arguments_and_spreads_are_not_resolved() -> Result<()> {
        let text = include_str!("../tests/fixtures/repository-syntax/archive.graphql");
        let tree = parse(RepositoryFormat::Graphql, text);
        assert!(
            !tree.root_node().has_error(),
            "{}",
            tree.root_node().to_sexp()
        );
        let cues = extract(tree.root_node(), text, RepositoryFormat::Graphql)?;
        assert_eq!(
            cues.iter()
                .filter(|cue| cue.kind == "definition" && cue.name == "Entry")
                .count(),
            2,
            "definition and extension remain separate written nodes"
        );
        let spread = cues
            .iter()
            .find(|cue| cue.kind == "fragment_reference")
            .unwrap();
        assert_eq!(spread.name, "...EntryFields");
        assert_eq!(spread.owner.unwrap().kind(), "operation_definition");
        assert!(
            cues.iter()
                .any(|cue| cue.kind == "argument" && cue.name == "filter: $filter")
        );
        assert!(
            cues.iter()
                .any(|cue| cue.kind == "schema" && cue.name == "query: Archive")
        );
        Ok(())
    }

    #[test]
    fn malformed_nodes_never_become_authoritative_declarations() -> Result<()> {
        for (format, text) in [
            (
                RepositoryFormat::Hcl,
                "resource \"bad\" { value = }\nvariable \"good\" {}\n",
            ),
            (RepositoryFormat::Dockerfile, "FROM scratch AS good\nFROM\n"),
            (
                RepositoryFormat::Proto,
                "syntax = \"proto3\"; message Bad { string = ; } message Good {}",
            ),
            (
                RepositoryFormat::Graphql,
                "type Bad { broken: }\ntype Good { okay: String }",
            ),
        ] {
            let tree = parse(format, text);
            assert!(
                tree.root_node().has_error(),
                "negative fixture must exercise recovery: {format:?}"
            );
            let cues = extract(tree.root_node(), text, format)?;
            assert!(
                cues.iter()
                    .all(|cue| !cue.node.has_error() && !cue.node.is_missing())
            );
            assert!(
                cues.iter()
                    .any(|cue| cue.category == CueCategory::Declaration),
                "valid recovered sibling should remain available: {format:?}"
            );
        }
        Ok(())
    }

    #[test]
    fn excessive_fact_count_fails_instead_of_silently_dropping_tail() {
        let text = (0..10_001)
            .map(|n| format!("key_{n} = 1\n"))
            .collect::<String>();
        let tree = parse(RepositoryFormat::Hcl, &text);
        assert!(!tree.root_node().has_error());
        let result = extract(tree.root_node(), &text, RepositoryFormat::Hcl);
        assert!(result.is_err_and(|error| error.to_string().contains("syntax fact limit")));
    }
}
