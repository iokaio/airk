// SPDX-License-Identifier: Apache-2.0
//! Conservative grammar repairs over the same parsed source as native extraction.
mod php;
mod ruby;
pub(crate) mod sql;

use crate::{build_facts::Facts, syntax::Analysis};
use airk_core::{EvidenceBasis, Fact};
use anyhow::{Result, ensure};
use std::collections::BTreeSet;
use tree_sitter::Node;

pub(crate) fn enrich(
    mut base: Analysis,
    path: &str,
    text: &str,
    root: Node<'_>,
    language: &str,
) -> Result<Analysis> {
    ensure!(root.end_byte() <= text.len(), "repair tree exceeds source");
    let mut output = Facts::new(path, text, language)?;
    let mut pending = vec![root];
    let mut visited = 0usize;
    while let Some(node) = pending.pop() {
        visited += 1;
        ensure!(visited <= 500_000, "repair parser node limit exceeded");
        if !node.has_error() && !node.is_missing() {
            match language {
                "php" => php::visit(node, text, &mut output)?,
                "ruby" => ruby::visit(node, text, &mut output)?,
                "sql" | "sql-postgresql" | "sql-mysql" => sql::visit(node, text, &mut output)?,
                _ => anyhow::bail!("unsupported syntax repair language"),
            }
        }
        for index in (0..node.named_child_count()).rev() {
            ensure!(pending.len() < 500_000, "repair pending limit exceeded");
            if let Some(child) = node.named_child(index) {
                pending.push(child);
            }
        }
    }
    // Correct callee spelling while retaining existing enclosing declaration metadata.
    let previous_calls: std::collections::BTreeMap<_, _> = base
        .facts
        .iter()
        .filter(|fact| fact.kind == "call")
        .filter_map(|fact| span(fact).map(|span| (span, fact)))
        .collect();
    for fact in &mut output.facts {
        if matches!(fact.kind.as_str(), "call" | "dynamic_call")
            && let Some(previous) = span(fact).and_then(|span| previous_calls.get(&span))
            && let Some(prior) = previous.code.as_ref()
        {
            let code = fact.code.as_mut().expect("call evidence");
            code.owner_id = prior.owner_id.clone();
            code.scope = prior.scope.clone();
        }
    }
    let replacement_spans: BTreeSet<_> = output
        .facts
        .iter()
        .filter(|fact| matches!(fact.kind.as_str(), "call" | "dynamic_call"))
        .filter_map(span)
        .collect();
    base.facts.retain(|fact| {
        fact.kind != "call" || span(fact).is_none_or(|span| !replacement_spans.contains(&span))
    });
    let mut existing: BTreeSet<_> = base
        .facts
        .iter()
        .filter_map(|fact| span(fact).map(|span| (span, fact.kind.clone())))
        .collect();
    for mut fact in output.facts {
        let code = fact.code.as_mut().expect("repair syntax evidence");
        code.basis = EvidenceBasis::Syntax;
        code.producer = format!("airk-tree-sitter/{language}");
        code.producer_version = "repair-facts/1".into();
        code.coverage = "Written grammar syntax; receiver dispatch, mixins, imports, SQL dialect semantics and database objects are not resolved".into();
        if fact.kind == "dynamic_call" {
            code.coverage.push_str("; computed callee remains unknown");
        }
        if let Some(span) = span(&fact)
            && existing.insert((span, fact.kind.clone()))
        {
            base.facts.push(fact);
        }
    }
    if matches!(language, "sql-postgresql" | "sql-mysql") {
        base.facts.extend(sql::procedures(path, text, language)?);
    }
    ensure!(
        base.facts.len() <= 10_000,
        "combined repair fact limit exceeded"
    );
    ensure!(
        serde_json::to_vec(&base.facts)?.len() <= 16_777_216,
        "repair evidence byte limit exceeded"
    );
    if root.has_error() {
        base.status = "partial-syntax".into();
    }
    Ok(base)
}

fn span(fact: &Fact) -> Option<(usize, usize)> {
    fact.code
        .as_ref()?
        .span
        .as_ref()
        .map(|span| (span.start_byte, span.end_byte))
}
fn text<'a>(node: Node<'_>, source: &'a str) -> &'a str {
    source.get(node.byte_range()).unwrap_or("")
}
fn child<'a>(node: Node<'a>, kind: &str) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .find(|child| child.kind() == kind)
}
fn emit(output: &mut Facts<'_>, node: Node<'_>, kind: &str, name: &str) -> Result<()> {
    output.push(node.start_byte(), node.end_byte(), kind, node.kind(), name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::{Language, Parser};

    fn repaired(language: &str, source: &str) -> Result<Analysis> {
        let grammar: Language = match language {
            "php" => tree_sitter_php::LANGUAGE_PHP.into(),
            "ruby" => tree_sitter_ruby::LANGUAGE.into(),
            _ => tree_sitter_sequel::LANGUAGE.into(),
        };
        let mut parser = Parser::new();
        parser.set_language(&grammar)?;
        let tree = parser.parse(source, None).unwrap();
        enrich(
            Analysis {
                language: language.into(),
                status: "syntax".into(),
                facts: vec![],
            },
            "original",
            source,
            tree.root_node(),
            language,
        )
    }

    #[test]
    fn php_namespace_trait_import_and_exact_receiver_operators() -> Result<()> {
        let text = include_str!("../tests/fixtures/syntax-repairs/sample.php")
            .replace("\r\n", "\n")
            .replace('\n', "\r\n");
        let result = repaired("php", &text)?;
        assert_ne!(result.status, "partial-syntax");
        for (kind, spelling) in [
            ("namespace", "Fictional\\Archive"),
            ("import", "LocalReader"),
            ("mixin", "Named"),
            ("call", "$reader->open"),
            ("call", "$reader?->close"),
            ("call", "LocalReader::create"),
            ("dynamic_call", "$reader->$method"),
        ] {
            assert!(
                result
                    .facts
                    .iter()
                    .any(|fact| fact.kind == kind && fact.name.contains(spelling)),
                "missing {kind} {spelling}"
            );
        }
        for fact in result.facts {
            let span = fact.code.unwrap().span.unwrap();
            assert!(text[span.start_byte..span.end_byte].contains(&fact.name));
        }
        Ok(())
    }

    #[test]
    fn ruby_literal_imports_mixins_receiver_calls_and_dynamic_dispatch() -> Result<()> {
        let text = include_str!("../tests/fixtures/syntax-repairs/sample.rb");
        let result = repaired("ruby", text)?;
        assert_ne!(result.status, "partial-syntax");
        for (kind, spelling) in [
            ("import", "fictional/archive"),
            ("mixin", "Named"),
            ("mixin", "LocalHelpers"),
            ("call", "reader.open"),
            ("call", "reader&.close"),
            ("dynamic_call", "self.send"),
        ] {
            assert!(
                result
                    .facts
                    .iter()
                    .any(|fact| fact.kind == kind && fact.name.contains(spelling)),
                "missing {kind} {spelling}"
            );
        }
        let dynamic = repaired(
            "ruby",
            "require \"#{directory}/archive\"\ninclude selected_mixin\n",
        )?;
        assert!(
            !dynamic
                .facts
                .iter()
                .any(|fact| matches!(fact.kind.as_str(), "import" | "mixin"))
        );
        Ok(())
    }

    #[test]
    fn postgresql_cte_join_aliases_and_ddl_have_written_names() -> Result<()> {
        let result = repaired(
            "sql-postgresql",
            include_str!("../tests/fixtures/syntax-repairs/postgresql.sql"),
        )?;
        assert_ne!(result.status, "partial-syntax");
        for (kind, name) in [
            ("cte", "recent"),
            ("query_scope", "SELECT"),
            ("join", "JOIN"),
            ("alias", "entry_id"),
            ("table_reference", "archive.entries"),
            ("definition", "archive.answer"),
            ("definition", "entries_after"),
        ] {
            assert!(
                result
                    .facts
                    .iter()
                    .any(|fact| fact.kind == kind && fact.name.contains(name)),
                "missing {kind} {name}"
            );
        }
        Ok(())
    }

    #[test]
    fn mysql_quoted_references_aliases_and_query_scope_are_preserved() -> Result<()> {
        let result = repaired(
            "sql-mysql",
            include_str!("../tests/fixtures/syntax-repairs/mysql.sql"),
        )?;
        assert_ne!(result.status, "partial-syntax");
        assert!(
            result
                .facts
                .iter()
                .any(|fact| fact.kind == "table_reference" && fact.name == "`entries`")
        );
        assert!(
            result
                .facts
                .iter()
                .any(|fact| fact.kind == "alias" && fact.name == "e")
        );
        Ok(())
    }

    #[test]
    fn comments_strings_and_malformed_queries_do_not_fabricate_references() -> Result<()> {
        let result = repaired(
            "sql",
            "-- SELECT * FROM fake;\nSELECT 'FROM imaginary' AS note;\nSELECT (",
        )?;
        assert_eq!(result.status, "partial-syntax");
        assert!(
            !result
                .facts
                .iter()
                .any(|fact| fact.kind == "table_reference")
        );
        let php = repaired("php", "<?php // $x->fake();\n$text = '$x->fake()';")?;
        assert!(!php.facts.iter().any(|fact| fact.kind == "call"));
        Ok(())
    }

    #[test]
    fn explicit_procedure_header_is_structural_and_generic_sql_stays_partial() -> Result<()> {
        let result = repaired(
            "sql-postgresql",
            "CREATE PROCEDURE archive.rotate() LANGUAGE SQL AS $$ SELECT 1 $$;",
        )?;
        assert_eq!(result.status, "partial-syntax");
        assert!(!result.facts.iter().any(|fact| fact.kind == "definition"));
        let procedure = result
            .facts
            .iter()
            .find(|fact| fact.kind == "procedure")
            .expect("qualified procedure framing");
        assert_eq!(procedure.name, "archive.rotate");
        assert_eq!(
            procedure.code.as_ref().unwrap().basis,
            EvidenceBasis::StructuralRule
        );
        assert!(procedure.code.as_ref().unwrap().fingerprints.is_none());
        let generic = repaired(
            "sql",
            "CREATE PROCEDURE archive.rotate() LANGUAGE SQL AS $$ SELECT 1 $$;",
        )?;
        assert!(!generic.facts.iter().any(|fact| fact.kind == "procedure"));
        Ok(())
    }

    #[test]
    fn repair_fact_overflow_is_explicit() {
        assert!(
            repaired("ruby", &"reader.open\n".repeat(10_001))
                .is_err_and(|error| error.to_string().contains("fact limit"))
        );
    }
}
