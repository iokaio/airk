// SPDX-License-Identifier: Apache-2.0
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
    assert_eq!(reopened.scan_limits()?, limited);
    assert_eq!(
        Kernel::open_collection(&root, Some(data.clone()), "second")?.scan_limits()?,
        initial
    );
    reopened.configure_scan_limits(ScanLimits {
        files: 2,
        ..initial
    })?;
    assert_ne!(reopened.coverage(false)?.percent, Some(100));
    reopened.index()?;
    assert_eq!(reopened.status()?.files, 2);
    let before = std::fs::read(data.join("collections/first/scan-limits.json"))?;
    assert!(
        reopened
            .configure_scan_limits(ScanLimits {
                files: 0,
                ..initial
            })
            .is_err()
    );
    assert_eq!(
        std::fs::read(data.join("collections/first/scan-limits.json"))?,
        before
    );
    // Explicit OCR cannot bypass ignored/source-internal files.
    std::fs::write(root.join(".airkignore"), "secret.png\n")?;
    std::fs::write(root.join("secret.png"), b"not an image")?;
    assert!(airk_ingest::read_selected_source(&root, &data, "secret.png").is_err());
    assert!(airk_ingest::read_selected_source(&root, &data, "../secret.png").is_err());
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
