// SPDX-License-Identifier: Apache-2.0
use super::current_inspection::InspectCurrent;
use airk_kernel::{Kernel, limits::ScanLimits};
use anyhow::Result;

#[test]
fn limits_are_persisted_isolated_and_failure_preserves_revision() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let root = directory.path().join("source");
    std::fs::create_dir(&root)?;
    let data = directory.path().join("state");
    std::fs::write(root.join("one.md"), "original requirement\n")?;
    let kernel = Kernel::open_collection(&root, Some(data.clone()), "first")?;
    kernel.index()?;
    let generation = kernel.status()?.generation;
    let initial = kernel.scan_limits()?;
    assert_eq!(kernel.describe_scan_limits()?.mode, "automatic");
    let limited = ScanLimits {
        files: 1,
        ..initial
    };
    kernel.configure_scan_limits(limited)?;
    std::fs::write(root.join("two.md"), "second requirement\n")?;
    assert!(kernel.index().is_err());
    assert_eq!(kernel.status()?.generation, generation);
    assert_eq!(kernel.coverage(true)?.capacity_blocked, 1);
    let reopened = Kernel::open_collection(&root, Some(data.clone()), "first")?;
    assert_eq!(reopened.describe_scan_limits()?.mode, "configured");
    assert_eq!(reopened.scan_limits()?, limited);
    let before_approval = std::fs::read(data.join("collections-1.2/first/scan-limits.json"))?;
    assert!(
        reopened
            .configure_scan_limits_if_current(initial, Some(initial))
            .unwrap_err()
            .to_string()
            .contains("approval was pending")
    );
    assert_eq!(
        std::fs::read(data.join("collections-1.2/first/scan-limits.json"))?,
        before_approval
    );
    assert_eq!(
        Kernel::open_collection(&root, Some(data.clone()), "second")?.scan_limits()?,
        initial
    );
    reopened.configure_scan_limits_if_current(
        ScanLimits {
            files: 2,
            ..initial
        },
        Some(limited),
    )?;
    assert_ne!(reopened.coverage(false)?.percent, Some(100));
    reopened.index()?;
    assert_eq!(reopened.status()?.files, 2);
    let before = std::fs::read(data.join("collections-1.2/first/scan-limits.json"))?;
    assert!(
        reopened
            .configure_scan_limits(ScanLimits {
                files: 0,
                ..initial
            })
            .is_err()
    );
    assert_eq!(
        std::fs::read(data.join("collections-1.2/first/scan-limits.json"))?,
        before
    );
    // Explicit OCR cannot bypass ignored/source-internal files.
    std::fs::write(root.join(".airkignore"), "secret.png\n")?;
    std::fs::write(root.join("secret.png"), b"not an image")?;
    assert!(airk_ingest::read_selected_source(&root, &data, "secret.png").is_err());
    assert!(airk_ingest::read_selected_source(&root, &data, "../secret.png").is_err());
    assert_eq!(reopened.reset_scan_limits()?, initial);
    assert_eq!(reopened.describe_scan_limits()?.mode, "automatic");
    assert_eq!(reopened.status()?.files, 2);
    Ok(())
}

#[test]
fn large_text_retains_exact_source_reports_partial_and_preserves_revision_on_capacity_failure()
-> Result<()> {
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    // Cross both the former 4 MiB text ceiling and 16 MiB retained-source ceiling.
    let text = format!("{}\nlarge_source_marker\n", "x".repeat(17 * 1024 * 1024));
    std::fs::write(root.path().join("large.ts"), &text)?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    let report = kernel.index()?;
    assert_eq!(report.indexed, 1);
    assert!(report.skipped.is_empty());
    let store = airk_store::FileStore::new(data.path().join("collections-1.2/default"));
    let catalog = store.current_test_catalog()?.unwrap();
    assert_eq!(
        store.read_retained_source(&catalog.snapshot.artifacts["large.ts"])?,
        text.as_bytes()
    );
    assert!(
        kernel
            .file_info("large.ts")?
            .parse_status
            .starts_with("partial-text-only:")
    );
    assert!(
        kernel.read_file("large.ts", 2, 2, 1024)?.hits[0]
            .text
            .contains("large_source_marker")
    );
    let coverage = kernel.coverage(true)?;
    assert!(coverage.scan_complete);
    assert_eq!(coverage.eligible_source_bytes, text.len());
    assert_eq!(coverage.largest_sources[0].path, "large.ts");
    assert_eq!(coverage.largest_sources[0].source_bytes, text.len());
    assert_eq!(kernel.index()?.reused, 1);
    let generation = kernel.status()?.generation;
    kernel.configure_scan_limits(ScanLimits {
        corpus_bytes: text.len() - 1,
        ..kernel.scan_limits()?
    })?;
    let error = kernel.index().unwrap_err();
    assert_eq!(
        airk_kernel::diagnostics::indexing_failure_code(&error).as_deref(),
        Some("index_scan_sources_source_byte_limit_exceeded")
    );
    // Stage context must preserve the original actionable capacity explanation.
    let error = format!("{error:#}");
    assert!(
        error.contains("eligible source bytes at large.ts"),
        "{error}"
    );
    assert!(error.contains(&text.len().to_string()), "{error}");
    assert_eq!(kernel.status()?.generation, generation);
    assert_eq!(std::fs::read_to_string(root.path().join("large.ts"))?, text);
    Ok(())
}

#[test]
fn optional_extractors_reject_pending_roots_and_file_budget_before_launch() -> Result<()> {
    use airk_ingest::{
        ocr::OcrRequest,
        providers::{LiveRequest, ProviderCommand},
    };
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    let alias = tempfile::tempdir()?;
    std::fs::write(root.path().join("one.md"), "original\n")?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.index()?;
    let generation = kernel.status()?.generation;
    let command = ProviderCommand {
        program: data.path().join("must-not-be-launched"),
        args: vec![],
        timeout_ms: 100,
    };
    let provider = LiveRequest {
        command: command.clone(),
        producer: "fixture".into(),
        producer_version: "1".into(),
        build_inputs: vec![],
        paths: vec!["one.md".into()],
    };
    let ocr = OcrRequest {
        tesseract: command,
        pdf_renderer: None,
        tessdata_dir: data.path().into(),
        language: "eng".into(),
        first_page: 1,
        last_page: 1,
        dpi: 150,
    };
    kernel.add_root("extra", alias.path())?;
    assert!(
        kernel
            .prepare_analysis("fixture", "1", &[])
            .unwrap_err()
            .to_string()
            .contains("pending source root")
    );
    assert!(
        kernel
            .run_provider("ctags", &provider)
            .unwrap_err()
            .to_string()
            .contains("pending source root")
    );
    assert!(
        kernel
            .extract_ocr("one.md", &ocr)
            .unwrap_err()
            .to_string()
            .contains("pending source root")
    );
    kernel.remove_root("extra")?;
    kernel.configure_scan_limits(ScanLimits {
        file_bytes: 8,
        ..kernel.scan_limits()?
    })?;
    std::fs::write(root.path().join("image.png"), [0; 32])?;
    assert!(
        kernel
            .extract_ocr("image.png", &ocr)
            .unwrap_err()
            .to_string()
            .contains("file limit")
    );
    assert_eq!(kernel.status()?.generation, generation);
    assert_eq!(std::fs::read(root.path().join("image.png"))?, [0; 32]);
    Ok(())
}
