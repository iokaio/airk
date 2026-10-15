// SPDX-License-Identifier: Apache-2.0
use super::*;

#[test]
fn vendor_payload_binding_preserves_enclosing_owners_and_apex_declarations() -> Result<()> {
    use airk_core::binding::{ArtifactBinding, bind_payload};
    use sha2::{Digest, Sha256};
    for (path, source) in [
        (
            "original.abap",
            include_str!("../../tests/fixtures/specialist-vendor/original.abap"),
        ),
        (
            "original.cls",
            include_str!("../../tests/fixtures/specialist-vendor/original.cls"),
        ),
    ] {
        let source = source.replace("\r\n", "\n").replace('\n', "\r\n");
        let extracted = crate::documents::extract(path, source.as_bytes())?;
        let source_hash =
            airk_core::identity::Sha256Digest::from_bytes(Sha256::digest(source.as_bytes()).into());
        let artifact = airk_core::Artifact {
            path: path.into(),
            content_hash: source_hash.to_string(),
            bytes: source.len(),
            chunks: extracted.chunks,
            language: extracted.language,
            extraction_method: extracted.method,
            parse_status: extracted.parse_status,
            facts: extracted.facts,
            retained: None,
        };
        let profile = crate::adapters::legacy::legacy_profile(&artifact)?;
        let payload =
            crate::adapters::legacy::convert_native_legacy(&artifact, source.as_bytes(), &profile)?;
        let bound = bind_payload(
            &payload,
            &profile,
            ArtifactBinding {
                path,
                source_hash,
                source_bytes: source.len(),
            },
        )?;
        assert_eq!(
            serde_json::to_value(&bound)?,
            serde_json::to_value(&artifact)?
        );
        assert!(
            payload
                .facts
                .iter()
                .any(|f| f.code.as_ref().is_some_and(|c| c.owner_id.is_some()))
        );
        if path.ends_with(".cls") {
            let draft = crate::continuity::code::describe(
                &payload,
                &profile,
                &mut airk_core::continuity::ContinuityBudget::default(),
            )?;
            assert!(
                draft
                    .descriptors
                    .iter()
                    .any(|d| d.category == "class_declaration")
            );
            assert!(
                draft
                    .descriptors
                    .iter()
                    .any(|d| d.category == "method_declaration" && d.owner.is_some())
            );
        }
    }
    Ok(())
}

#[test]
fn required_vendor_row_branches_include_tables_views_interfaces_and_package_bodies() -> Result<()> {
    let sql = check(
        "tsql",
        "CREATE TABLE archive_items (id int);\nCREATE VIEW archive_visible AS SELECT id FROM archive_items;\n",
        &[("table", "archive_items"), ("view", "archive_visible")],
    )?;
    assert!(sql.facts.iter().any(|fact| fact.kind == "query"));
    let apex = check(
        "apex",
        "public interface ArchiveContract { Integer countItems(); }\n",
        &[
            ("definition", "ArchiveContract"),
            ("definition", "countItems"),
        ],
    )?;
    assert!(
        apex.facts
            .iter()
            .all(|fact| fact.code.as_ref().unwrap().basis == EvidenceBasis::Syntax)
    );
    let package = check(
        "plsql",
        "CREATE OR REPLACE PACKAGE BODY archive_tools AS\nEND archive_tools;\n/\n",
        &[("package", "archive_tools")],
    )?;
    assert!(
        package
            .facts
            .iter()
            .any(|fact| fact.syntax_kind == "vendor_package_body_header")
    );
    Ok(())
}

fn check(adapter: &str, source: &str, expected: &[(&str, &str)]) -> Result<Analysis> {
    let source = source.replace("\r\n", "\n").replace('\n', "\r\n");
    let result = analyze("original", &source, adapter)?;
    for (kind, name) in expected {
        assert!(
            result
                .facts
                .iter()
                .any(|fact| fact.kind == *kind && fact.name == *name),
            "missing {adapter} {kind} {name}: {:?}",
            result.facts
        );
    }
    for fact in &result.facts {
        let code = fact.code.as_ref().unwrap();
        let span = code.span.as_ref().unwrap();
        assert!(span.start_byte < span.end_byte);
        assert!(source.get(span.start_byte..span.end_byte).is_some());
        assert_eq!(
            fact.start_line,
            source[..span.start_byte]
                .bytes()
                .filter(|b| *b == b'\n')
                .count()
                + 1
        );
        if adapter != "apex" {
            assert!(code.fingerprints.is_none());
        }
    }
    Ok(result)
}

#[test]
fn plsql_package_spec_body_cursor_and_trigger_headers() -> Result<()> {
    check(
        "plsql",
        include_str!("../../tests/fixtures/specialist-vendor/original.pks"),
        &[
            ("package", "archive_tools"),
            ("procedure", "record_item"),
            ("function", "item_count"),
        ],
    )?;
    check(
        "plsql",
        include_str!("../../tests/fixtures/specialist-vendor/original.pkb"),
        &[
            ("package", "archive_tools"),
            ("cursor", "pending_items"),
            ("table_reference", "archive_items"),
        ],
    )?;
    check(
        "plsql",
        include_str!("../../tests/fixtures/specialist-vendor/original.pls"),
        &[("trigger", "audit_archive")],
    )?;
    Ok(())
}

#[test]
fn tsql_batches_bracketed_names_and_written_queries() -> Result<()> {
    let result = check(
        "tsql",
        include_str!("../../tests/fixtures/specialist-vendor/tsql.sql"),
        &[
            ("procedure", "[archive].[list_items]"),
            ("function", "archive.bump"),
            ("trigger", "archive.changed"),
            ("batch", "GO"),
        ],
    )?;
    assert!(
        result
            .facts
            .iter()
            .any(|f| f.kind == "table_reference" && f.name == "archive.labels")
    );
    Ok(())
}

#[test]
fn abap_periods_multiline_headers_and_owners() -> Result<()> {
    let result = check(
        "abap",
        include_str!("../../tests/fixtures/specialist-vendor/original.abap"),
        &[
            ("report", "z_archive_report"),
            ("include", "z_archive_helpers"),
            ("interface", "lif_archive"),
            ("class", "lcl_archive"),
            ("method", "lif_archive~render"),
            ("form", "render_header"),
            ("module", "show_archive"),
        ],
    )?;
    let method = result
        .facts
        .iter()
        .find(|f| f.name == "lif_archive~render")
        .unwrap();
    assert_eq!(method.code.as_ref().unwrap().scope, vec!["lcl_archive"]);
    check(
        "abap",
        "REPORT\n z_café.\nWRITE 'fake. CLASS not_code DEFINITION.'.\n",
        &[("report", "z_café")],
    )?;
    Ok(())
}

#[test]
fn abap_comments_literals_templates_and_truncation_never_invent_declarations() -> Result<()> {
    let source = "* CLASS fake DEFINITION.\nREPORT z_real.\nWRITE 'METHOD fake.'. \" CLASS fake2 DEFINITION.\nWRITE |text { expression } CLASS fake3.|.\nCLASS unfinished";
    let result = analyze("x.abap", source, "abap")?;
    assert_eq!(result.status, "partial-syntax");
    assert_eq!(result.facts.len(), 1);
    assert_eq!(result.facts[0].name, "z_real");
    Ok(())
}

#[test]
fn apex_native_class_method_query_trigger_and_spans() -> Result<()> {
    let result = check(
        "apex",
        include_str!("../../tests/fixtures/specialist-vendor/original.cls"),
        &[
            ("definition", "ArchiveService"),
            ("definition", "findAccounts"),
            ("call", "System.debug"),
        ],
    )?;
    assert!(result.facts.iter().any(|f| f.kind == "query"));
    assert!(
        result
            .facts
            .iter()
            .filter(|f| f.kind == "definition")
            .all(|f| f.code.as_ref().unwrap().fingerprints.is_some())
    );
    check(
        "apex",
        include_str!("../../tests/fixtures/specialist-vendor/original.trigger"),
        &[
            ("definition", "ArchiveAccountChanged"),
            ("call", "ArchiveService.findAccounts"),
        ],
    )?;
    Ok(())
}

#[test]
fn apex_class_selection_requires_visible_strong_marker() {
    assert!(qualifies_class(include_bytes!(
        "../../tests/fixtures/specialist-vendor/original.cls"
    )));
    assert!(!qualifies_class(include_bytes!(
        "../../tests/fixtures/specialist-vendor/ambiguous.cls"
    )));
    assert!(!qualifies_class(b"public class GenericClass {}"));
    assert!(!qualifies_class(
        b"// public with sharing class Fake {}\nclass Plain {}"
    ));
    assert!(!qualifies_class(b"'public with sharing class Fake {}'"));
    assert!(!qualifies_class(b"/* public with sharing class Fake {}"));
    assert!(qualifies_class(b"@isTest private class Checks {}"));
    assert!(!qualifies_class(
        format!("{}public with sharing class Late {{}}", " ".repeat(4096)).as_bytes()
    ));
}

#[test]
fn sql_alternative_literals_comments_and_malformed_headers_are_conservative() -> Result<()> {
    let source = "BEGIN value := q'[CREATE PROCEDURE fake AS BEGIN NULL; END;]'; END;\n-- CREATE PROCEDURE comment_fake AS BEGIN NULL; END;\nCREATE OR REPLACE PROCEDURE real_one AS BEGIN NULL; END;";
    let result = analyze("x.pls", source, "plsql")?;
    assert!(!result.facts.iter().any(|f| f.name.contains("fake")));
    assert!(
        result
            .facts
            .iter()
            .any(|f| f.kind == "procedure" && f.name == "real_one")
    );
    let broken = analyze("x.sql", "CREATE PROCEDURE missing(@x int", "tsql")?;
    assert!(!broken.facts.iter().any(|f| f.kind == "procedure"));
    assert_eq!(broken.status, "partial-syntax");
    Ok(())
}

#[test]
fn vendor_shared_work_budget_rejects_before_unbounded_evidence() -> Result<()> {
    for (adapter, source) in [
        ("abap", "REPORT test."),
        ("tsql", "CREATE PROCEDURE test AS SELECT 1;"),
        ("apex", "public class Test {}"),
    ] {
        assert!(analyze_budgeted("x", source, adapter, &mut HostBudget::new(1, 1, 1)?).is_err());
    }
    Ok(())
}
