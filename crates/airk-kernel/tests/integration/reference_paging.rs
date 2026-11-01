// SPDX-License-Identifier: Apache-2.0
use airk_core::{
    references::{ReferenceOptions, ReferenceOrder},
    search_filters::SearchFilters,
};
use airk_kernel::Kernel;
use anyhow::{Context, Result, ensure};
use std::{collections::BTreeSet, fs};

fn options() -> ReferenceOptions {
    ReferenceOptions {
        order: ReferenceOrder::Path,
        include_files: true,
        include_count: true,
        ..Default::default()
    }
}

#[test]
fn reference_pages_resume_mid_chunk_without_duplicates_and_keep_legacy_shape() -> Result<()> {
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    fs::write(
        root.path().join("a.rs"),
        "struct Session;\n// Session occurs here.\n// OwnerSession is a different identifier.\n// Session Session counts as one matching line.\n// session is another case.\n",
    )?;
    fs::write(
        root.path().join("b.md"),
        "# Session\nSession documentation.\n",
    )?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.configure_vectors(false)?;
    kernel.index()?;
    let old = serde_json::to_value(kernel.references("Session", "", 100, 65536)?)?;
    ensure!(
        old.get("page").is_none(),
        "legacy reference envelope changed"
    );
    let all = kernel.references_page("Session", "", 100, 65536, &options())?;
    let count = all
        .page
        .as_ref()
        .context("page")?
        .match_count
        .as_ref()
        .context("count")?;
    assert_eq!(count.total_matches, Some(5));
    assert_eq!(count.scope, "verified_lexical_candidates");
    assert!(count.complete);
    let expected: BTreeSet<_> = all
        .evidence
        .hits
        .iter()
        .map(|hit| {
            (
                hit.citation.path.clone(),
                hit.citation.start_line,
                hit.text.clone(),
            )
        })
        .collect();
    let mut selected = options();
    let mut found = BTreeSet::new();
    for turn in 0..20 {
        let page = kernel.references_page("Session", "", 2, 65536, &selected)?;
        let metadata = page.page.context("page")?;
        assert_eq!(metadata.count_unit, "matching_lines");
        if turn > 0 {
            let count = metadata.match_count.as_ref().context("count")?;
            assert!(count.total_matches.is_none() && !count.complete);
            assert_eq!(count.scope, "page_scan");
        }
        let file_counts: usize = metadata
            .files
            .as_ref()
            .context("files")?
            .iter()
            .map(|row| row.count)
            .sum();
        assert_eq!(file_counts, page.evidence.hits.len());
        for hit in page.evidence.hits {
            ensure!(
                found.insert((hit.citation.path, hit.citation.start_line, hit.text)),
                "duplicate matching line across pages"
            );
        }
        if !metadata.more {
            assert!(metadata.cursor.is_none());
            assert_eq!(found, expected);
            return Ok(());
        }
        selected.cursor = metadata.cursor;
    }
    anyhow::bail!("reference continuation did not finish")
}

#[test]
fn reference_too_large_line_is_explicit_and_cursor_allows_a_larger_budget() -> Result<()> {
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    fs::write(
        root.path().join("a.rs"),
        "// Session πππ is retained intact.\n",
    )?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.configure_vectors(false)?;
    kernel.index()?;
    let mut selected = options();
    let page = kernel.references_page("Session", "", 10, 1, &selected)?;
    assert!(page.evidence.hits.is_empty() && page.evidence.truncated);
    let metadata = page.page.context("page")?;
    assert_eq!(
        metadata.incomplete_reason.as_deref(),
        Some("text_byte_budget")
    );
    let required = metadata
        .required_record_bytes
        .context("required line bytes")?;
    assert!(required > 1 && metadata.more);
    selected.cursor = metadata.cursor;
    let resumed = kernel.references_page("Session", "", 10, required, &selected)?;
    assert_eq!(resumed.evidence.hits.len(), 1);
    assert!(!resumed.evidence.hits[0].truncated);
    assert_eq!(resumed.evidence.hits[0].text.len(), required);
    assert!(!resumed.page.context("page")?.more);
    Ok(())
}

#[test]
fn reference_cursor_rejects_changed_filters_order_and_publication() -> Result<()> {
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    fs::write(root.path().join("a.rs"), "// Session one\n// Session two\n")?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.configure_vectors(false)?;
    kernel.index()?;
    let mut selected = options();
    selected.cursor = kernel
        .references_page("Session", "", 1, 65536, &selected)?
        .page
        .context("page")?
        .cursor;
    assert!(selected.cursor.is_some());
    let mut changed = selected.clone();
    changed.order = ReferenceOrder::CodeFirst;
    assert!(
        kernel
            .references_page("Session", "", 1, 65536, &changed)
            .is_err()
    );
    changed = selected.clone();
    changed.filters.language = Some("rust".into());
    assert!(
        kernel
            .references_page("Session", "", 1, 65536, &changed)
            .is_err()
    );
    assert!(
        kernel
            .references_page("OtherSession", "", 1, 65536, &selected)
            .is_err()
    );
    fs::write(root.path().join("a.rs"), "// Session replacement\n")?;
    kernel.index()?;
    assert!(
        kernel
            .references_page("Session", "", 1, 65536, &selected)
            .is_err()
    );
    Ok(())
}

#[test]
fn reference_line_larger_than_any_legal_text_budget_reports_terminal_gap() -> Result<()> {
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    let source = format!("// Session {}\n", "x".repeat(airk_core::MAX_RESPONSE_BYTES));
    fs::write(root.path().join("a.rs"), &source)?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.configure_vectors(false)?;
    kernel.index()?;
    let result = kernel.references_page("Session", "", 10, 65536, &options())?;
    assert!(result.evidence.hits.is_empty() && result.evidence.truncated);
    let page = result.page.context("page")?;
    assert_eq!(
        page.incomplete_reason.as_deref(),
        Some("matching_line_exceeds_maximum_text_budget")
    );
    assert_eq!(page.required_record_bytes, Some(source.len()));
    assert!(!page.more && page.cursor.is_none());
    assert!(!page.match_count.context("count")?.complete);
    Ok(())
}

#[test]
fn references_reconstruct_split_source_lines_once_including_last_fragment_and_crlf() -> Result<()> {
    for prefix_match in [false, true] {
        let root = tempfile::tempdir()?;
        let data = tempfile::tempdir()?;
        let beginning = if prefix_match { "Session " } else { "" };
        let source = format!("{beginning}{}Session last fragment\r\n", " ".repeat(9000));
        fs::write(root.path().join("a.txt"), &source)?;
        let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
        kernel.configure_vectors(false)?;
        kernel.index()?;
        let result = kernel.references_page("Session", "", 100, 65536, &options())?;
        assert_eq!(
            result.evidence.hits.len(),
            1,
            "multiple matching partitions must not duplicate the physical line"
        );
        let hit = &result.evidence.hits[0];
        assert_eq!(hit.text, source);
        assert_eq!(hit.citation.start_line, 1);
        assert_eq!(hit.citation.end_line, 1);
        assert_eq!(
            hit.citation.location,
            airk_core::DocumentLocation::Lines { start: 1, end: 1 }
        );
        assert!(!hit.truncated);
        let page = result.page.context("page")?;
        assert_eq!(page.coverage.line_completeness, "complete_source_lines");
        assert!(!page.coverage.query_complete);
        assert!(page.coverage.query_total_matches.is_none());
        assert_eq!(page.match_count.context("count")?.total_matches, Some(1));
    }
    Ok(())
}

#[test]
fn lexical_token_seam_is_not_claimed_as_exhaustive_textual_reference_coverage() -> Result<()> {
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    // Bounded native partitions split Session between Se and ssion. Neither
    // lexical partition contains the whole token, despite the retained match.
    let source = format!("{}Session across seam\n", " ".repeat(8190));
    assert!(
        source
            .split(|c: char| !c.is_alphanumeric() && c != '_')
            .any(|word| word == "Session")
    );
    fs::write(root.path().join("a.txt"), &source)?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.configure_vectors(false)?;
    kernel.index()?;
    let result = kernel.references_page("Session", "", 100, 65536, &options())?;
    assert!(result.evidence.hits.is_empty());
    let page = result.page.context("page")?;
    assert!(page.coverage.candidate_scan_complete);
    assert!(!page.coverage.query_complete && !page.coverage.token_boundary_complete);
    assert!(page.coverage.query_total_matches.is_none());
    let count = page.match_count.context("candidate count")?;
    assert_eq!(count.total_matches, Some(0));
    assert_eq!(count.scope, "verified_lexical_candidates");
    Ok(())
}

#[test]
fn mapped_document_fragments_preserve_original_locations_and_explicit_partial_counts() -> Result<()>
{
    // The ordinary kernel test harness cannot dispatch the private PDF worker.
    // Use the original mapped notebook fixture to exercise the same partial
    // evidence branch without weakening subprocess ownership or adding a host.
    let fixture = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../airk-ingest/tests/fixtures/notebooks/original.ipynb");
    let notebook = fs::read_to_string(fixture)?.replace("beacon_value", "Session");
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    fs::write(root.path().join("a.ipynb"), notebook)?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.configure_vectors(false)?;
    kernel.index()?;
    let result = kernel.references_page("Session", "", 100, 65536, &options())?;
    assert!(!result.evidence.hits.is_empty() && result.evidence.truncated);
    assert!(result.evidence.hits.iter().all(|hit| hit.truncated
        && matches!(
            hit.citation.location,
            airk_core::DocumentLocation::Mapped {
                region: airk_core::location::RegionLocation::NotebookCell { position: 2, .. }
            }
        )));
    let page = result.page.context("page")?;
    assert_eq!(page.count_unit, "matching_line_fragments");
    assert_eq!(
        page.coverage.line_completeness,
        "includes_unverified_fragments"
    );
    assert_eq!(
        page.incomplete_reason.as_deref(),
        Some("original_line_completeness_unverified")
    );
    assert!(page.match_count.context("count")?.total_matches.is_none());
    Ok(())
}

#[test]
fn reference_language_filter_finds_code_after_more_than_one_candidate_page() -> Result<()> {
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    for n in 0..105 {
        fs::write(
            root.path().join(format!("a{n:03}.md")),
            "Session documentation\n",
        )?;
    }
    fs::write(root.path().join("z.rs"), "struct Session;\n")?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.configure_vectors(false)?;
    kernel.index()?;
    let mut selected = ReferenceOptions {
        filters: SearchFilters {
            language: Some("rust".into()),
            ..Default::default()
        },
        ..options()
    };
    let mut found = Vec::new();
    for _ in 0..10 {
        let result = kernel.references_page("Session", "", 10, 65536, &selected)?;
        found.extend(result.evidence.hits);
        let page = result.page.context("page")?;
        if !page.more {
            break;
        }
        selected.cursor = page.cursor;
    }
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].citation.path, "z.rs");
    Ok(())
}

#[test]
fn reference_false_positive_candidate_page_can_be_empty_and_still_continue() -> Result<()> {
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    fs::create_dir(root.path().join("Session"))?;
    for n in 0..105 {
        // FTS contains the path's Session token; exact retained lines do not.
        fs::write(
            root.path().join(format!("Session/a{n:03}.md")),
            "Nothing here\n",
        )?;
    }
    fs::write(root.path().join("z.rs"), "struct Session;\n")?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.configure_vectors(false)?;
    kernel.index()?;
    let mut selected = options();
    let first = kernel.references_page("Session", "", 10, 65536, &selected)?;
    assert!(first.evidence.hits.is_empty() && first.evidence.truncated);
    let first = first.page.context("page")?;
    assert!(first.more && first.cursor.is_some());
    assert!(first.match_count.context("count")?.total_matches.is_none());
    selected.cursor = first.cursor;
    for _ in 0..20 {
        let result = kernel.references_page("Session", "", 10, 65536, &selected)?;
        let metadata = result.page.context("page")?;
        if !metadata.more {
            assert_eq!(result.evidence.hits.len(), 1);
            assert_eq!(result.evidence.hits[0].citation.path, "z.rs");
            return Ok(());
        }
        ensure!(
            result.evidence.hits.is_empty(),
            "unexpected matching candidate"
        );
        selected.cursor = metadata.cursor;
    }
    anyhow::bail!("empty candidate scan failed to progress")
}

#[test]
fn reference_code_first_and_bounded_summaries_do_not_drop_evidence() -> Result<()> {
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    fs::write(root.path().join("a.md"), "Session documentation\n")?;
    for n in 0..17 {
        fs::write(root.path().join(format!("z{n:02}.rs")), "struct Session;\n")?;
    }
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.configure_vectors(false)?;
    kernel.index()?;
    let selected = ReferenceOptions {
        order: ReferenceOrder::CodeFirst,
        ..options()
    };
    let result = kernel.references_page("Session", "", 100, 65536, &selected)?;
    assert_eq!(result.evidence.hits.len(), 18);
    assert_eq!(
        result
            .evidence
            .hits
            .last()
            .context("document")?
            .citation
            .path,
        "a.md"
    );
    let page = result.page.context("page")?;
    assert!(page.files_truncated);
    let files = page.files.context("files")?;
    assert!(files.len() <= 16);
    assert!(
        files
            .iter()
            .map(|row| serde_json::to_vec(row).unwrap().len())
            .sum::<usize>()
            <= 4096
    );
    assert_eq!(page.match_count.context("count")?.total_matches, Some(18));
    assert!(!page.more);
    Ok(())
}
