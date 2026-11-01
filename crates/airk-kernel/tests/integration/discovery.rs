// SPDX-License-Identifier: Apache-2.0
//! Resumable discovery over original flat-directory and declaration fixtures.
use airk_core::{
    FactQuery,
    discovery::{DiscoveryOptions, FactDiscoveryOptions, NameMatch},
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

#[test]
fn inventory_record_budget_and_count_limit_choose_identical_continuations() -> Result<()> {
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    let expected = ["a.txt", "bΩ.txt", "c中😀.txt"];
    for path in expected {
        fs::write(root.path().join(path), "Original budget fixture.\n")?;
    }
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.configure_vectors(false)?;
    kernel.index()?;
    let first_bytes = serde_json::to_vec(expected[0])?.len();
    let second_bytes = serde_json::to_vec(expected[1])?.len();
    let too_small = kernel.list_files("", 3, first_bytes - 1)?;
    assert!(too_small.paths.is_empty() && too_small.truncated);
    assert_eq!(too_small.page.record_bytes, Some(0));
    assert_eq!(too_small.page.required_record_bytes, Some(first_bytes));
    assert!(too_small.page.cursor.is_none());
    let by_bytes = kernel.list_files("", 3, first_bytes)?;
    let by_count = kernel.list_files("", 1, 4096)?;
    assert_eq!(by_bytes.paths, [expected[0]]);
    assert_eq!(by_bytes.page.cursor, by_count.page.cursor);
    assert_eq!(by_bytes.page.record_bytes, by_count.page.record_bytes);
    assert_eq!(by_bytes.page.record_bytes, Some(first_bytes));
    assert!(by_bytes.truncated && by_count.truncated);
    let pair_budget = first_bytes + second_bytes;
    let pair = kernel.list_files("", 3, pair_budget)?;
    assert_eq!(pair.paths, expected[..2]);
    assert_eq!(pair.page.record_bytes, Some(pair_budget));
    assert_eq!(
        pair.page.cursor,
        kernel.list_files("", 2, 4096)?.page.cursor
    );
    let below = kernel.list_files("", 3, pair_budget - 1)?;
    assert_eq!(below.paths, [expected[0]]);
    let options = DiscoveryOptions {
        cursor: by_bytes.page.cursor,
        ..Default::default()
    };
    let too_small_continuation = kernel.list_files_page("", 3, second_bytes - 1, &options)?;
    assert!(too_small_continuation.paths.is_empty() && too_small_continuation.truncated);
    assert_eq!(
        too_small_continuation.page.required_record_bytes,
        Some(second_bytes)
    );
    assert_eq!(too_small_continuation.page.cursor, options.cursor);
    let second = kernel.list_files_page("", 1, second_bytes, &options)?;
    assert_eq!(second.paths, [expected[1]]);
    assert_eq!(second.page.record_bytes, Some(second_bytes));
    let tail = kernel.list_files_page(
        "",
        3,
        4096,
        &DiscoveryOptions {
            cursor: second.page.cursor,
            ..Default::default()
        },
    )?;
    assert_eq!(tail.paths, [expected[2]]);
    assert!(!tail.truncated && tail.page.cursor.is_none());
    Ok(())
}

#[test]
fn explicit_fact_name_matching_is_scoped_case_sensitive_and_counted_without_extra_scan()
-> Result<()> {
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    fs::write(
        root.path().join("types.rs"),
        "pub struct Session;\npub struct OwnerSession;\npub struct SessionArchive;\n\
         pub struct RemoteSession;\npub struct lower_session;\npub struct Séssion;\n",
    )?;
    fs::write(
        root.path().join("outside.rs"),
        "pub struct OutsideSession;\n",
    )?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.configure_vectors(false)?;
    kernel.index()?;
    let q = FactQuery {
        path_prefix: "types.rs".into(),
        name: Some("Session".into()),
        limit: 100,
        max_bytes: 65536,
        ..query()
    };
    let page = DiscoveryOptions::default();
    let original = kernel.code_facts_page(&q, &page)?;
    let unchanged = kernel.code_facts_page_with_options(&q, &page, &Default::default())?;
    assert_eq!(
        serde_json::to_value(&original)?,
        serde_json::to_value(unchanged)?
    );
    assert_eq!(original.facts.len(), 1);
    assert_eq!(original.facts[0].name, "Session");
    assert!(original.page.match_count.is_none());
    for name in [
        original.facts[0]
            .code
            .as_ref()
            .context("code evidence")?
            .qualified_name
            .clone(),
        original.facts[0]
            .code
            .as_ref()
            .context("code evidence")?
            .symbol_id
            .clone(),
    ] {
        let mut by_identity = q.clone();
        by_identity.name = Some(name);
        let found = kernel.code_facts_page_with_options(
            &by_identity,
            &page,
            &FactDiscoveryOptions {
                name_match: Some(NameMatch::Exact),
                include_count: false,
            },
        )?;
        assert_eq!(found.facts.len(), 1);
        assert_eq!(found.facts[0].name, "Session");
    }
    let exact = kernel.code_facts_page_with_options(
        &q,
        &page,
        &FactDiscoveryOptions {
            name_match: Some(NameMatch::Exact),
            include_count: true,
        },
    )?;
    let count = exact.page.match_count.context("requested exact count")?;
    assert_eq!(count.total_matches, Some(1));
    assert_eq!(count.lower_bound, 1);
    assert!(count.complete);
    assert_eq!(count.scope, "query");
    for (mode, expected) in [
        (
            NameMatch::Suffix,
            vec!["OwnerSession", "RemoteSession", "Session"],
        ),
        (
            NameMatch::Contains,
            vec!["OwnerSession", "RemoteSession", "Session", "SessionArchive"],
        ),
    ] {
        let result = kernel.code_facts_page_with_options(
            &q,
            &page,
            &FactDiscoveryOptions {
                name_match: Some(mode),
                include_count: true,
            },
        )?;
        let names = result
            .facts
            .iter()
            .map(|fact| fact.name.as_str())
            .collect::<BTreeSet<_>>();
        assert_eq!(names, expected.into_iter().collect());
        let count = result.page.match_count.context("requested match count")?;
        assert_eq!(count.total_matches, Some(names.len()));
        assert_eq!(count.lower_bound, names.len());
        assert!(count.complete && !result.truncated);
        assert!(
            result
                .facts
                .iter()
                .all(|fact| fact.citation.path == "types.rs")
        );
    }
    let suffix = FactDiscoveryOptions {
        name_match: Some(NameMatch::Suffix),
        include_count: true,
    };
    let mut unicode = q.clone();
    unicode.name = Some("éssion".into());
    assert_eq!(
        kernel
            .code_facts_page_with_options(&unicode, &page, &suffix)?
            .facts[0]
            .name,
        "Séssion"
    );
    unicode.name = Some("Éssion".into());
    let none = kernel.code_facts_page_with_options(&unicode, &page, &suffix)?;
    assert!(none.facts.is_empty());
    assert_eq!(
        none.page.match_count.context("zero count")?.total_matches,
        Some(0)
    );
    let mut unknown_language = q.clone();
    unknown_language.language = Some("fictional-unindexed-language".into());
    assert!(
        kernel
            .code_facts_page_with_options(&unknown_language, &page, &suffix)?
            .facts
            .is_empty()
    );
    let mut absent_name = q.clone();
    absent_name.name = None;
    assert!(
        kernel
            .code_facts_page_with_options(&absent_name, &page, &suffix)
            .is_err()
    );
    absent_name.name = Some(String::new());
    assert!(
        kernel
            .code_facts_page_with_options(&absent_name, &page, &suffix)
            .is_err()
    );
    Ok(())
}

#[test]
fn matched_fact_continuations_bind_mode_counts_and_publication_and_report_partial_totals()
-> Result<()> {
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    fs::write(
        root.path().join("types.rs"),
        "pub struct Session;\npub struct OwnerSession;\npub struct RemoteSession;\n",
    )?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.configure_vectors(false)?;
    kernel.index()?;
    let mut q = FactQuery {
        path_prefix: "types.rs".into(),
        name: Some("Session".into()),
        limit: 1,
        max_bytes: 65536,
        ..query()
    };
    let options = FactDiscoveryOptions {
        name_match: Some(NameMatch::Suffix),
        include_count: true,
    };
    let mut page = DiscoveryOptions::default();
    let first = kernel.code_facts_page_with_options(&q, &page, &options)?;
    assert!(first.truncated);
    let count = first.page.match_count.as_ref().context("partial count")?;
    assert_eq!(count.total_matches, None);
    assert_eq!(count.lower_bound, 2);
    assert!(!count.complete);
    assert_eq!(count.scope, "page_scan");
    let mut seen = BTreeSet::from([first.facts[0].name.clone()]);
    page.cursor = first.page.cursor;
    assert!(page.cursor.is_some());
    for changed in [
        FactDiscoveryOptions {
            name_match: Some(NameMatch::Contains),
            ..options.clone()
        },
        FactDiscoveryOptions {
            include_count: false,
            ..options.clone()
        },
    ] {
        assert!(
            kernel
                .code_facts_page_with_options(&q, &page, &changed)
                .is_err()
        );
    }
    let original_cursor = page.clone();
    loop {
        let next = kernel.code_facts_page_with_options(&q, &page, &options)?;
        assert!(
            next.page
                .match_count
                .as_ref()
                .context("continued count")?
                .total_matches
                .is_none()
        );
        for fact in next.facts {
            assert!(seen.insert(fact.name), "duplicate matching fact");
        }
        let Some(cursor) = next.page.cursor else {
            assert!(!next.truncated);
            break;
        };
        page.cursor = Some(cursor);
    }
    assert_eq!(
        seen,
        BTreeSet::from([
            "Session".into(),
            "OwnerSession".into(),
            "RemoteSession".into()
        ])
    );
    q.max_bytes = 1;
    let tiny = kernel.code_facts_page_with_options(&q, &Default::default(), &options)?;
    assert!(tiny.facts.is_empty() && tiny.truncated);
    assert!(
        tiny.page
            .required_record_bytes
            .context("required record bytes")?
            > 1
    );
    assert_eq!(
        tiny.page.match_count.context("tiny count")?.total_matches,
        None
    );
    fs::write(root.path().join("new.txt"), "Publication change.\n")?;
    kernel.index()?;
    q.max_bytes = 65536;
    assert!(
        kernel
            .code_facts_page_with_options(&q, &original_cursor, &options)
            .is_err()
    );
    Ok(())
}

#[test]
fn omitted_import_match_mode_keeps_case_insensitive_substring_behavior() -> Result<()> {
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    fs::write(
        root.path().join("types.rs"),
        "use crate::MixedCaseSession;\n",
    )?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.configure_vectors(false)?;
    kernel.index()?;
    let q = FactQuery {
        path_prefix: "types.rs".into(),
        name: Some("mixedcasesession".into()),
        kind: Some("import".into()),
        exact_name: false,
        ..query()
    };
    let before = kernel.facts_page(&q, &Default::default())?;
    let after = kernel.facts_page_with_options(&q, &Default::default(), &Default::default())?;
    assert_eq!(serde_json::to_value(&before)?, serde_json::to_value(after)?);
    assert_eq!(before.facts.len(), 1);
    assert!(
        kernel
            .facts_page_with_options(
                &q,
                &Default::default(),
                &FactDiscoveryOptions {
                    name_match: Some(NameMatch::Contains),
                    include_count: false
                },
            )?
            .facts
            .is_empty()
    );
    Ok(())
}

#[test]
fn sparse_suffix_discovery_reports_unknown_total_and_advances_empty_bounded_pages() -> Result<()> {
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    for ordinal in 0..257 {
        let name = if ordinal == 256 {
            "LateSession"
        } else {
            "Unrelated"
        };
        fs::write(
            root.path().join(format!("f-{ordinal:03}.rs")),
            format!("pub struct {name};\n"),
        )?;
    }
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.configure_vectors(false)?;
    kernel.index()?;
    let q = FactQuery {
        path_prefix: "f-".into(),
        path_exact: false,
        name: Some("Session".into()),
        limit: 100,
        max_bytes: 65536,
        ..query()
    };
    let options = FactDiscoveryOptions {
        name_match: Some(NameMatch::Suffix),
        include_count: true,
    };
    let first = kernel.code_facts_page_with_options(&q, &Default::default(), &options)?;
    assert!(first.truncated && first.facts.is_empty());
    let count = first.page.match_count.context("bounded empty page count")?;
    assert_eq!(count.total_matches, None);
    assert_eq!(count.lower_bound, 0);
    assert!(!count.complete);
    let last = kernel.code_facts_page_with_options(
        &q,
        &DiscoveryOptions {
            cursor: first.page.cursor,
            ..Default::default()
        },
        &options,
    )?;
    assert!(!last.truncated && last.page.cursor.is_none());
    assert_eq!(last.facts.len(), 1);
    assert_eq!(last.facts[0].name, "LateSession");
    let count = last.page.match_count.context("continued page count")?;
    assert_eq!(count.total_matches, None);
    assert_eq!(count.lower_bound, 1);
    assert!(!count.complete);
    Ok(())
}
