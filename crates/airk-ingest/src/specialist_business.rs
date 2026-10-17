// SPDX-License-Identifier: Apache-2.0
//! Original-source qualified Fortran, COBOL, Delphi and VB.NET evidence; no evaluation or resolution.
mod cobol;
mod delphi;
mod fortran;
mod vbnet;
use crate::{build_facts::Facts, syntax::Analysis};
use airk_core::{
    CodeSpan, EvidenceBasis,
    location::{ByteSpan, HostBudget},
};
use anyhow::{Result, ensure};
use tree_sitter::{Language, Node, Parser};

struct Cue<'a> {
    node: Node<'a>,
    kind: &'static str,
    name: String,
    body: Option<Node<'a>>,
    signature: Option<ByteSpan>,
    declaration: bool,
    structural: bool,
}
fn cue<'a>(node: Node<'a>, kind: &'static str, name: &str, declaration: bool) -> Cue<'a> {
    let mut cursor = node.walk();
    let mut bodies = node.children_by_field_name("body", &mut cursor);
    let mut body = bodies.next();
    if bodies.next().is_some() {
        body = None;
    }
    Cue {
        node,
        kind,
        name: name.into(),
        body,
        signature: None,
        declaration,
        structural: false,
    }
}
fn text<'a>(node: Node<'_>, source: &'a str) -> &'a str {
    source.get(node.byte_range()).unwrap_or("")
}
fn field<'a>(node: Node<'_>, source: &'a str, name: &str) -> Option<&'a str> {
    node.child_by_field_name(name)
        .map(|node| text(node, source))
}
fn named<'a>(node: Node<'a>, kind: &str) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .find(|node| node.kind() == kind)
}
fn source_form(
    source: &str,
    adapter: &str,
    budget: &mut HostBudget,
) -> Result<(bool, Vec<ByteSpan>)> {
    let mut partial = false;
    let mut excluded = Vec::new();
    let mut offset = 0;
    for line in source.split_inclusive('\n') {
        budget.charge(1, 0, 0, 0)?;
        let bytes = line.as_bytes();
        let comment = match adapter {
            "fortran-fixed" => bytes
                .first()
                .is_some_and(|c| matches!(c, b'C' | b'c' | b'*' | b'!')),
            "cobol-fixed" => bytes.get(6).is_some_and(|c| matches!(c, b'*' | b'/')),
            _ => false,
        };
        if comment {
            excluded.push(ByteSpan {
                start: offset,
                end: offset + line.len(),
            });
        } else if adapter.ends_with("-fixed") && !line.trim().is_empty() {
            let marker = if adapter == "fortran-fixed" { 5 } else { 6 };
            if bytes
                .get(marker)
                .is_some_and(|c| !c.is_ascii_whitespace() && *c != b'0')
            {
                partial = true;
                excluded.push(ByteSpan {
                    start: offset,
                    end: offset + line.len(),
                });
            }
            if bytes
                .iter()
                .take(marker)
                .any(|b| !b.is_ascii_digit() && !b.is_ascii_whitespace())
                || bytes
                    .get(72..)
                    .is_some_and(|tail| tail.iter().any(|b| !b.is_ascii_whitespace()))
            {
                partial = true;
            }
        }
        if adapter == "delphi" && line.contains("{$") {
            partial = true;
        }
        if (adapter.starts_with("fortran-") && line.trim_start().starts_with('#'))
            || (adapter.starts_with("cobol-") && line.trim_start().starts_with(">>"))
        {
            partial = true;
        }
        offset += line.len();
    }
    Ok((partial, excluded))
}

pub(crate) fn analyze(path: &str, source: &str, adapter: &str) -> Result<Analysis> {
    let mut budget = HostBudget::new(500_000, 10_000, 16_777_216)?;
    budget.charge(0, 0, source.len(), 0)?;
    analyze_budgeted(path, source, adapter, &mut budget)
}

/// Caller admits source/map bytes once. Sibling literate chunks share parser/fact/hash work.
pub(crate) fn analyze_budgeted(
    path: &str,
    source: &str,
    adapter: &str,
    budget: &mut HostBudget,
) -> Result<Analysis> {
    ensure!(
        source.len() <= crate::MAX_STRUCTURED_BYTES,
        "language source byte limit"
    );
    if adapter == "vbnet" {
        return vbnet::analyze(path, source, budget);
    }
    if adapter == "cobol-free" {
        return cobol::analyze_free(path, source, budget);
    }
    if adapter == "delphi"
        && let Some(package) = delphi::package(path, source, budget)?
    {
        return Ok(package);
    }
    let (form_partial, excluded) = source_form(source, adapter, budget)?;
    let grammar: Language = match adapter {
        "fortran-free" | "fortran-fixed" => tree_sitter_fortran::LANGUAGE.into(),
        "cobol-free" | "cobol-fixed" => tree_sitter_cobol::language().into(),
        "delphi" => tree_sitter_pascal::LANGUAGE.into(),
        _ => anyhow::bail!("unsupported specialist business adapter"),
    };
    budget.charge(1, 0, 0, 0)?;
    let mut parser = Parser::new();
    parser.set_language(&grammar)?;
    let tree = parser
        .parse(source, None)
        .ok_or_else(|| anyhow::anyhow!("language parser returned no tree"))?;
    let mut output = Facts::new(path, source, adapter)?;
    let mut pending = vec![(tree.root_node(), None)];
    while let Some((node, owner)) = pending.pop() {
        budget.charge(1, 0, 0, 0)?;
        let mut next_owner = owner;
        let position = excluded.partition_point(|span| span.end <= node.start_byte());
        if excluded
            .get(position)
            .is_some_and(|span| span.start <= node.start_byte() && node.end_byte() <= span.end)
        {
            continue;
        }
        let broken_tail = node.next_named_sibling().is_some_and(|next| {
            next.is_error() && source[node.end_byte()..next.start_byte()].trim().is_empty()
        });
        if !node.has_error() && !node.is_missing() && !broken_tail {
            let cues = match adapter {
                "fortran-free" | "fortran-fixed" => fortran::select(node, source),
                "cobol-free" | "cobol-fixed" => cobol::select(node, source),
                _ => delphi::select(node, source),
            };
            for selected in cues {
                budget.charge(0, 1, 0, 0)?;
                let index = output.facts.len();
                output.push(
                    selected.node.start_byte(),
                    selected.node.end_byte(),
                    selected.kind,
                    selected.node.kind(),
                    &selected.name,
                )?;
                if index == output.facts.len() {
                    continue;
                }
                let owner_id =
                    owner.map(|i: usize| output.facts[i].code.as_ref().unwrap().symbol_id.clone());
                let mut scope = owner.map_or_else(Vec::new, |i| {
                    output.facts[i].code.as_ref().unwrap().scope.clone()
                });
                let truncated_owner =
                    owner.is_some_and(|i| output.facts[i].code.as_ref().unwrap().details_truncated);
                if let Some(i) = owner {
                    scope.push(output.facts[i].name.clone());
                }
                ensure!(scope.len() <= 128, "language owner depth limit");
                let signature = selected.signature.or_else(|| {
                    selected.body.map(|body| ByteSpan {
                        start: selected.node.start_byte(),
                        end: body.start_byte(),
                    })
                });
                let fingerprints = if selected.declaration && !selected.structural {
                    let mut hashes = crate::syntax::declaration_fingerprints(
                        source,
                        &selected.name,
                        &scope.iter().map(String::as_str).collect::<Vec<_>>(),
                        signature,
                        selected.body,
                        None,
                        budget,
                    )?;
                    if truncated_owner {
                        hashes.qualified_name = None;
                    }
                    Some(hashes)
                } else {
                    None
                };
                let code = output.facts[index].code.as_mut().unwrap();
                code.owner_id = owner_id;
                code.scope = scope;
                code.details_truncated |= truncated_owner;
                code.fingerprints = fingerprints;
                code.signature_span = signature.map(|s| CodeSpan {
                    start_byte: s.start,
                    end_byte: s.end,
                });
                code.basis = if selected.structural {
                    EvidenceBasis::StructuralRule
                } else {
                    EvidenceBasis::Syntax
                };
                code.producer = if selected.structural {
                    format!("airk-language-structure/{adapter}")
                } else {
                    format!("airk-tree-sitter/{adapter}")
                };
                code.producer_version = "specialist-facts/1".into();
                code.coverage="Selected written syntax only; imports, preprocessor directives and calls are not evaluated or resolved; documentation continuity unknown".into();
                if matches!(
                    selected.kind,
                    "call" | "import" | "package_load" | "require"
                ) {
                    code.target = Some(selected.name.chars().take(1024).collect());
                }
                if selected.declaration {
                    next_owner = Some(index);
                }
            }
        }
        // Literal contents are evidence values, never new executable declarations.
        if matches!(
            node.kind(),
            "comment"
                | "line_comment"
                | "block_comment"
                | "string_literal"
                | "string_lit"
                | "string_expression"
                | "indented_string_expression"
                | "prefixed_string_literal"
                | "quote_expression"
                | "quote_statement"
                | "quoting_lit"
                | "syn_quoting_lit"
                | "dis_expr"
                | "evaling_lit"
                | "tagged_or_ctor_lit"
        ) {
            continue;
        }
        for index in (0..node.named_child_count()).rev() {
            ensure!(pending.len() < 500_000, "language pending node limit");
            if let Some(child) = node.named_child(index) {
                pending.push((child, next_owner));
            }
        }
    }
    ensure!(
        serde_json::to_vec(&output.facts)?.len() <= 16_777_216,
        "language evidence byte limit"
    );
    let mut result = output.finish(tree.root_node().has_error() || form_partial);
    if result.status != "partial-syntax" {
        result.status = "syntax".into();
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn has(result: &Analysis, kind: &str, name: &str) {
        assert!(
            result
                .facts
                .iter()
                .any(|f| f.kind == kind && f.name.contains(name)),
            "missing {kind}/{name}: {:?}",
            result.facts
        );
    }
    #[test]
    fn free_and_fixed_fortran_keep_original_forms() -> Result<()> {
        let free = analyze(
            "original.f90",
            include_str!("../tests/fixtures/specialist-business/original.f90"),
            "fortran-free",
        )?;
        has(&free, "definition", "render");
        has(&free, "import", "archive");
        has(&free, "declaration", "offset");
        let fixed = analyze(
            "original.f",
            include_str!("../tests/fixtures/specialist-business/original.f"),
            "fortran-fixed",
        )?;
        has(&fixed, "definition", "RENDER");
        has(&fixed, "call", "RENDER");
        Ok(())
    }
    #[test]
    fn cobol_divisions_paragraph_data_copy_and_calls() -> Result<()> {
        let result = analyze(
            "original.cob",
            include_str!("../tests/fixtures/specialist-business/original.cob"),
            "cobol-fixed",
        )?;
        for (kind, name) in [
            ("program", "ARCHIVE"),
            ("division", "procedure"),
            ("paragraph", "RENDER"),
            ("declaration", "CAPTION"),
            ("import", "ARCHIVE-DATA"),
            ("call", "RENDER"),
        ] {
            has(&result, kind, name);
        }
        let header = analyze(
            "original.cpy",
            include_str!("../tests/fixtures/specialist-business/original.cpy"),
            "cobol-fixed",
        )?;
        has(&header, "declaration", "ARCHIVE");
        Ok(())
    }
    #[test]
    fn explicit_free_cobol_and_conditional_profiles_are_qualified_separately() -> Result<()> {
        let source = include_str!("../tests/fixtures/specialist-business/free.cob");
        let result = analyze("free.cob", source, "cobol-free")?;
        has(&result, "program", "FREE-ARCHIVE");
        has(&result, "call", "RENDER");
        has(&result, "declaration", "CAPTION");
        has(&result, "import", "ARCHIVE-DATA");
        assert!(result.facts.iter().all(|f| f.code.as_ref().unwrap().basis
            == EvidenceBasis::StructuralRule
            && f.code.as_ref().unwrap().fingerprints.is_none()));
        let negative = analyze(
            "negative.cob",
            "*> CALL \"PHANTOM\"\n01 CAPTION PIC X(40) VALUE \"CALL HIDDEN\".\n",
            "cobol-free",
        )?;
        assert!(!negative.facts.iter().any(|f| f.kind == "call"));
        assert!(analyze("bad.cob", "CALL \"unterminated\n", "cobol-free").is_err());
        let mut budget = HostBudget::new(1000, 100, 0)?;
        assert!(
            source_form(
                "#ifdef FEATURE\nprogram sample\nend program sample\n#endif\n",
                "fortran-free",
                &mut budget
            )?
            .0
        );
        assert!(source_form(">>SOURCE FORMAT FREE\n", "cobol-free", &mut budget)?.0);
        Ok(())
    }
    #[test]
    fn vbnet_selected_written_members_and_receiver_calls() -> Result<()> {
        let source = include_str!("../tests/fixtures/specialist-business/original.vb")
            .replace("\r\n", "\n")
            .replace('\n', "\r\n");
        let result = analyze("original.vb", &source, "vbnet")?;
        for (kind, name) in [
            ("namespace", "Archive"),
            ("import", "System"),
            ("written_definition", "Record"),
            ("written_definition", "Render"),
            ("property", "Title"),
            ("call", "Console.WriteLine"),
        ] {
            has(&result, kind, name);
        }
        assert_eq!(result.status, "partial-syntax");
        for f in result.facts {
            let c = f.code.unwrap();
            assert_eq!(c.basis, EvidenceBasis::StructuralRule);
            assert!(c.fingerprints.is_none());
            let s = c.span.unwrap();
            assert!(source.get(s.start_byte..s.end_byte).is_some());
        }
        Ok(())
    }
    #[test]
    fn delphi_units_routines_and_packages_have_distinct_provenance() -> Result<()> {
        let unit = analyze(
            "original.pas",
            include_str!("../tests/fixtures/specialist-business/original.pas"),
            "delphi",
        )?;
        has(&unit, "definition", "Archive");
        has(&unit, "definition", "Render");
        has(&unit, "import", "SysUtils");
        let package = analyze(
            "original.dpk",
            include_str!("../tests/fixtures/specialist-business/original.dpk"),
            "delphi",
        )?;
        has(&package, "package", "ArchivePackage");
        has(&package, "dependency", "rtl");
        has(&package, "module", "Archive");
        assert!(
            package
                .facts
                .iter()
                .all(|f| f.code.as_ref().unwrap().basis == EvidenceBasis::StructuralRule)
        );
        Ok(())
    }
    #[test]
    fn comments_strings_and_fixed_columns_do_not_add_authority() -> Result<()> {
        let vb = analyze(
            "negative.vb",
            "' Public Class Phantom\nRem Public Sub Hidden()\nDim caption = \"Function Fake()\"\n",
            "vbnet",
        )?;
        assert!(vb.facts.is_empty());
        assert!(
            analyze(
                "bad.vb",
                "Dim caption = \"unterminated\nPublic Class Phantom\n",
                "vbnet"
            )
            .is_err()
        );
        let mut budget = HostBudget::new(1000, 100, 0)?;
        let (partial, excluded) = source_form(
            "C CALL PHANTOM()\n     +CALL HIDDEN()\n",
            "fortran-fixed",
            &mut budget,
        )?;
        assert!(partial);
        assert_eq!(excluded.len(), 2);
        Ok(())
    }
    #[test]
    fn structural_sibling_budget_does_not_reset() -> Result<()> {
        let mut budget = HostBudget::new(1000, 1, 0)?;
        analyze_budgeted("one.vb", "Imports System\n", "vbnet", &mut budget)?;
        assert!(analyze_budgeted("two.vb", "Imports System\n", "vbnet", &mut budget).is_err());
        Ok(())
    }
}
