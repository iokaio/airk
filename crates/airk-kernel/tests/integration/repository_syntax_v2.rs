// SPDX-License-Identifier: Apache-2.0
//! Original P6 acceptance fixtures for admitted repository grammars.
use airk_core::{EvidenceBasis, FactQuery};
use airk_ingest::adapters::profile::{ExtractionPolicy, RuleMatch, SelectionRule};
use airk_kernel::Kernel;
use airk_store::FileStore;
use anyhow::{Context, Result};
use std::fs;

const HCL: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../airk-ingest/tests/fixtures/repository-syntax/service.tf"
));
const DOCKER: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../airk-ingest/tests/fixtures/repository-syntax/Dockerfile.original"
));
const PROTO: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../airk-ingest/tests/fixtures/repository-syntax/archive.proto"
));
const GRAPHQL: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../airk-ingest/tests/fixtures/repository-syntax/archive.graphql"
));

fn qualify(path: &str, moved: &str, fixture: &str, expected: &[(&str, &str)]) -> Result<()> {
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    // Every row traverses CRLF and multibyte bytes, including grammars with ASCII identifiers.
    let comment = if path.ends_with(".proto") {
        "// café original evidence\n"
    } else {
        "# café original evidence\n"
    };
    let source = format!("{comment}{}", fixture.replace("\r\n", "\n")).replace('\n', "\r\n");
    fs::write(root.path().join(path), &source)?;
    fs::write(
        root.path().join("unrelated.rs"),
        "pub fn unaffected() -> u32 { 17 }\n",
    )?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    let cold = kernel.index()?;
    assert_eq!(cold.indexed, 2);
    let store = FileStore::new(data.path().join("collections/default"));
    let original = store.catalog()?.context("catalog")?;
    let entry = original
        .snapshot
        .artifacts
        .get(path)
        .context("compact source reference")?;
    assert_eq!(store.read_retained_source(entry)?, source.as_bytes());
    let inventory = original.inventory.as_ref().context("inventory")?;
    let before = inventory.files[path].state.clone();
    let unrelated = inventory.files["unrelated.rs"].state.clone();
    let facts = kernel.outline(path, 100, 65_536)?;
    assert!(!facts.truncated);
    for (kind, name) in expected {
        assert!(
            facts
                .facts
                .iter()
                .any(|fact| fact.kind == *kind && fact.name.contains(name)),
            "{path}: missing {kind} {name}"
        );
    }
    for fact in &facts.facts {
        let code = fact
            .code
            .as_ref()
            .context("grammar cue needs exact syntax evidence")?;
        assert_eq!(code.basis, EvidenceBasis::Syntax);
        let span = code.span.as_ref().context("exact byte span")?;
        let bytes = source
            .get(span.start_byte..span.end_byte)
            .context("UTF-8 source boundary")?;
        assert!(!bytes.is_empty());
        assert!(
            bytes.contains(&fact.name),
            "display name must come from original syntax: {}",
            fact.name
        );
        let start_line = source[..span.start_byte]
            .bytes()
            .filter(|b| *b == b'\n')
            .count()
            + 1;
        let end_line = source[..span.end_byte]
            .bytes()
            .filter(|b| *b == b'\n')
            .count()
            + usize::from(!source[..span.end_byte].ends_with('\n'));
        assert_eq!(fact.citation.start_line, start_line);
        assert_eq!(fact.citation.end_line, end_line.max(start_line));
        if let Some(key) = fact.continuity_key {
            let bounded = kernel.continuity_facts(key, 1, 4096)?;
            assert!(
                !bounded.facts.is_empty(),
                "an emitted key must resolve its fact"
            );
            assert!(bounded.facts.len() <= 1);
            assert!(
                bounded
                    .facts
                    .iter()
                    .all(|hit| hit.continuity_key == Some(key))
            );
        }
        // Unsupported dimensions are not manufactured from bounded display values.
        if let Some(fingerprints) = &code.fingerprints
            && fingerprints.body.is_none()
        {
            assert!(!fingerprints.body_informative);
        }
    }
    let limited = kernel.facts(&FactQuery {
        path_prefix: path.into(),
        name: None,
        kind: None,
        language: None,
        exact_name: false,
        limit: 2,
        max_bytes: 4096,
        path_exact: true,
        document_only: false,
    })?;
    assert!(limited.facts.len() <= 2);
    assert!(limited.truncated);
    let warm = kernel.index()?;
    assert_eq!((warm.indexed, warm.reused), (0, 2));

    kernel.configure_extraction_policy(ExtractionPolicy {
        version: 1,
        rules: vec![SelectionRule {
            selector: RuleMatch::Exact { path: path.into() },
            case_sensitive: true,
            adapter_id: "text".into(),
        }],
    })?;
    let overridden = kernel.index()?;
    assert_eq!((overridden.indexed, overridden.reused), (1, 1));
    let changed = store.catalog()?.context("changed catalog")?;
    let changed_files = &changed
        .inventory
        .as_ref()
        .context("changed inventory")?
        .files;
    assert_ne!(changed_files[path].state.profile_id, before.profile_id);
    assert_eq!(changed_files["unrelated.rs"].state, unrelated);
    drop(changed);
    kernel.configure_extraction_policy(ExtractionPolicy {
        version: 1,
        rules: vec![],
    })?;
    kernel.index()?;
    fs::rename(root.path().join(path), root.path().join(moved))?;
    let renamed = kernel.index()?;
    assert_eq!(
        (renamed.indexed, renamed.reused),
        (0, 2),
        "compatible rename must avoid parsing"
    );
    let after = store.catalog()?.context("renamed catalog")?;
    let moved_state = &after.inventory.as_ref().context("renamed inventory")?.files[moved].state;
    assert_eq!(moved_state.payload_digest, before.payload_digest);
    assert_eq!(moved_state.source_hash, before.source_hash);
    assert!(
        kernel
            .outline(moved, 100, 65_536)?
            .facts
            .iter()
            .all(|fact| fact.citation.path == moved)
    );
    let moved_facts = kernel.outline(moved, 100, 65_536)?;
    let keys: std::collections::BTreeSet<_> = moved_facts
        .facts
        .iter()
        .filter_map(|fact| fact.continuity_key)
        .collect();
    assert!(
        !keys.is_empty(),
        "qualified declarations need continuity keys"
    );
    let inserted = if moved.ends_with(".proto") {
        "// inserted observation\r\n"
    } else {
        "# inserted observation\r\n"
    };
    fs::write(root.path().join(moved), format!("{inserted}{source}"))?;
    assert_eq!(kernel.index()?.indexed, 1);
    let shifted = kernel.outline(moved, 100, 65_536)?;
    assert_eq!(
        keys,
        shifted
            .facts
            .iter()
            .filter_map(|fact| fact.continuity_key)
            .collect()
    );
    for (before, after) in moved_facts.facts.iter().zip(&shifted.facts) {
        assert_eq!((&before.kind, &before.name), (&after.kind, &after.name));
        assert_eq!(after.citation.start_line, before.citation.start_line + 1);
        assert_eq!(after.citation.end_line, before.citation.end_line + 1);
    }
    assert_eq!(moved_facts.facts.len(), shifted.facts.len());
    Ok(())
}

#[test]
fn hcl_blocks_labels_attributes_and_written_references() -> Result<()> {
    qualify(
        "service.tf",
        "renamed.tf",
        HCL,
        &[
            ("definition", "fictional_service"),
            ("definition", "worker"),
            ("key", "depends_on"),
            ("reference", "var"),
        ],
    )
}

#[test]
fn docker_stages_flags_and_multiline_instructions() -> Result<()> {
    // LF continuation is qualified in the grammar unit test. The pinned grammar
    // reports CRLF shell continuation as partial; this fixture exercises CRLF
    // instructions without rewriting source offsets during extraction.
    let crlf_compatible = DOCKER.replace("\r\n", "\n").replace("\\\n    two", "two");
    qualify(
        "Dockerfile.original",
        "Dockerfile.renamed",
        &crlf_compatible,
        &[
            ("directive", "RUN"),
            ("stage", "compile"),
            ("image", "scratch"),
            ("copy_from", "--from=compile"),
            ("argument", "RELEASE"),
            ("environment", "SERVICE"),
            ("configuration", "8087"),
        ],
    )
}

#[test]
fn proto_nested_messages_fields_numbers_services_and_rpc_types() -> Result<()> {
    qualify(
        "archive.proto",
        "renamed.proto",
        PROTO,
        &[
            ("package", "fictional.archive"),
            ("import", "shared.proto"),
            ("definition", "Envelope"),
            ("definition", "Archive"),
            ("definition", "Watch"),
            ("field_number", "2"),
            ("type_reference", "Envelope"),
        ],
    )
}

#[test]
fn graphql_schema_types_operations_and_fragment_references() -> Result<()> {
    qualify(
        "archive.graphql",
        "renamed.graphql",
        GRAPHQL,
        &[
            ("schema", "query"),
            ("definition", "Entry"),
            ("definition", "Filter"),
            ("operation", "Browse"),
            ("field", "name"),
            ("fragment_reference", "EntryFields"),
            ("type_reference", "Token"),
        ],
    )
}
