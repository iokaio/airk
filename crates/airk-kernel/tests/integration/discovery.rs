// SPDX-License-Identifier: Apache-2.0
//! Resumable discovery over original flat-directory and declaration fixtures.
use airk_core::{
    FactQuery,
    discovery::DiscoveryOptions,
    evidence::{EvidenceReadOptions, EvidenceReadStatus},
};
use airk_kernel::Kernel;
use anyhow::{Context, Result};
use std::{collections::BTreeSet, fs};

fn query() -> FactQuery {
    FactQuery {
        path_prefix: "many.rs".into(),
        name: None,
        kind: Some("definition".into()),
        language: None,
        exact_name: true,
        limit: 17,
        max_bytes: 8192,
        path_exact: true,
        document_only: false,
    }
}

#[test]
fn keyset_discovery_enumerates_flat_files_facts_and_compact_outlines() -> Result<()> {
    let root = tempfile::tempdir()?;
    let aliases = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    for index in 0..127 {
        fs::write(
            root.path().join(format!("file-{index:03}.txt")),
            "Fictional fixture.\n",
        )?;
    }
    let source = (0..137)
        .map(|index| format!("pub fn item_{index:03}() {{}}\n"))
        .collect::<String>();
    fs::write(root.path().join("many.rs"), source)?;
    fs::write(
        aliases.path().join("alias.txt"),
        "Original aliased fixture.\n",
    )?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    // This continuation fixture includes durable declaration keys.
    kernel.configure_index_detail(airk_core::extraction::IndexDetail::Balanced)?;
    kernel.add_root("extra", aliases.path())?;
    kernel.index()?;
    let mut page = DiscoveryOptions::default();
    let mut paths = BTreeSet::new();
    loop {
        let result = kernel.list_files_page("", 19, 512, &page)?;
        assert_eq!(
            result.page.record_bytes,
            Some(
                result
                    .paths
                    .iter()
                    .map(|p| serde_json::to_vec(p).unwrap().len())
                    .sum()
            )
        );
        for path in result.paths {
            assert!(paths.insert(path), "duplicate path");
        }
        let Some(cursor) = result.page.cursor else {
            assert!(!result.truncated);
            break;
        };
        page.cursor = Some(cursor);
    }
    assert_eq!(paths.len(), 129);
    assert!(paths.contains("@extra/alias.txt"));

    let mut page = DiscoveryOptions::default();
    let mut definitions = BTreeSet::new();
    loop {
        let result = kernel.facts_page(&query(), &page)?;
        assert!(result.page.record_bytes.unwrap() <= query().max_bytes);
        for fact in result.facts {
            assert!(definitions.insert(fact.name), "duplicate definition");
        }
        let Some(cursor) = result.page.cursor else {
            assert!(!result.truncated);
            break;
        };
        page.cursor = Some(cursor);
    }
    assert_eq!(definitions.len(), 137);

    let mut page = DiscoveryOptions {
        minimal: false,
        compact: true,
        cursor: None,
    };
    let mut outlined = BTreeSet::new();
    loop {
        let result = kernel
            .outline_conditional_page("many.rs", 23, 8192, &Default::default(), &page)?
            .content
            .context("outline content")?;
        assert!(result.facts.is_empty());
        for fact in result.outline {
            assert!(fact.continuity_key.is_some() && fact.span.is_some());
            assert!(outlined.insert(fact.name), "duplicate outline item");
        }
        let Some(cursor) = result.page.cursor else {
            assert!(!result.truncated);
            break;
        };
        page.cursor = Some(cursor);
    }
    assert_eq!(outlined, definitions);
    Ok(())
}

#[test]
fn discovery_binds_revision_filters_representation_and_reports_oversized_records() -> Result<()> {
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    fs::write(
        root.path().join("many.rs"),
        "pub fn first() {}\npub fn second() {}\n",
    )?;
    fs::write(root.path().join("other.txt"), "Before.\n")?;
    fs::write(
        root.path().join("dupes.rs"),
        "fn repeated() {} fn repeated() {}\n",
    )?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.index()?;
    let mut duplicates = query();
    duplicates.path_prefix = "dupes.rs".into();
    duplicates.limit = 1;
    let duplicate_first = kernel.facts(&duplicates)?;
    let duplicate_second = kernel.facts_page(
        &duplicates,
        &DiscoveryOptions {
            minimal: false,
            cursor: duplicate_first.page.cursor.clone(),
            compact: false,
        },
    )?;
    assert_eq!(
        duplicate_first.facts[0].name,
        duplicate_second.facts[0].name
    );
    assert_eq!(
        duplicate_first.facts[0].citation.start_line,
        duplicate_second.facts[0].citation.start_line
    );
    assert_ne!(
        duplicate_first.facts[0]
            .code
            .as_ref()
            .unwrap()
            .span
            .as_ref()
            .unwrap()
            .start_byte,
        duplicate_second.facts[0]
            .code
            .as_ref()
            .unwrap()
            .span
            .as_ref()
            .unwrap()
            .start_byte
    );
    assert!(!duplicate_second.truncated);
    let first = kernel.list_files("", 1, 8192)?;
    let token = first.publication.context("publication")?.token;
    let cursor = first.page.cursor.context("list continuation")?;
    let continued = DiscoveryOptions {
        minimal: false,
        cursor: Some(cursor.clone()),
        compact: false,
    };
    assert!(
        kernel
            .list_files_page("other", 1, 8192, &continued)
            .is_err()
    );
    let tampered = cursor.replace("\"ordinal\":0", "\"ordinal\":9");
    assert_ne!(tampered, cursor);
    assert!(
        kernel
            .list_files_page(
                "",
                1,
                8192,
                &DiscoveryOptions {
                    minimal: false,
                    cursor: Some(tampered),
                    compact: false
                }
            )
            .is_err()
    );
    let tiny = kernel.list_files("", 10, 1)?;
    assert!(tiny.paths.is_empty() && tiny.truncated && tiny.page.cursor.is_none());
    assert_eq!(
        kernel
            .list_files(
                "",
                10,
                tiny.page.required_record_bytes.context("path bytes")?
            )?
            .paths
            .len(),
        1
    );

    let mut q = query();
    q.max_bytes = 1;
    let tiny = kernel.facts(&q)?;
    assert!(tiny.facts.is_empty() && tiny.truncated && tiny.page.cursor.is_none());
    q.max_bytes = tiny.page.required_record_bytes.context("fact bytes")?;
    let fact = kernel.facts(&q)?;
    assert_eq!(fact.facts.len(), 1);
    let fact_cursor = DiscoveryOptions {
        minimal: false,
        cursor: fact.page.cursor,
        compact: false,
    };
    q.kind = Some("import".into());
    assert!(kernel.facts_page(&q, &fact_cursor).is_err());

    let compact = DiscoveryOptions {
        minimal: false,
        compact: true,
        cursor: None,
    };
    let outline =
        kernel.outline_conditional_page("many.rs", 1, 8192, &Default::default(), &compact)?;
    let remembered = EvidenceReadOptions {
        if_evidence: outline.read.validator,
        ..Default::default()
    };
    assert_eq!(
        kernel
            .outline_conditional_page("many.rs", 1, 8192, &remembered, &compact)?
            .read
            .status,
        EvidenceReadStatus::NotModified
    );
    assert_eq!(
        kernel
            .outline_conditional("many.rs", 1, 8192, &remembered)?
            .read
            .status,
        EvidenceReadStatus::Content
    );
    let outline_cursor = DiscoveryOptions {
        minimal: false,
        cursor: outline.content.context("outline")?.page.cursor,
        compact: true,
    };
    assert!(
        kernel
            .outline_conditional_page(
                "many.rs",
                1,
                8192,
                &Default::default(),
                &DiscoveryOptions {
                    minimal: false,
                    compact: false,
                    ..outline_cursor.clone()
                }
            )
            .is_err()
    );
    fs::write(
        root.path().join("other.txt"),
        "After unrelated publication.\n",
    )?;
    kernel.index()?;
    assert!(kernel.list_files_page("", 1, 8192, &continued).is_err());
    assert!(
        kernel
            .outline_conditional_page("many.rs", 1, 8192, &Default::default(), &outline_cursor)
            .is_err()
    );
    let historical = EvidenceReadOptions {
        revision: Some(token),
        ..Default::default()
    };
    assert!(
        kernel
            .outline_conditional_page("many.rs", 1, 8192, &historical, &outline_cursor)
            .is_err()
    );
    Ok(())
}
