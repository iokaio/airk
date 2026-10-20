// SPDX-License-Identifier: Apache-2.0
//! Original local protocol fixtures qualify publication, not real provider quality.
use super::current_inspection::InspectCurrent;
use airk_core::{ArtifactSource, DocumentLocation};
use airk_ingest::{
    ocr::OcrRequest,
    providers::{LiveRequest, ProviderCommand},
};
use airk_kernel::Kernel;
use airk_store::FileStore;
use anyhow::{Result, ensure};
use std::{fs, path::Path, process::Command};

fn producer(program: &Path, mode: Option<&str>) -> ProviderCommand {
    ProviderCommand {
        program: program.into(),
        args: mode.into_iter().map(str::to_owned).collect(),
        timeout_ms: 30_000,
    }
}

#[test]
fn actual_ctags_lsp_and_ocr_writers_publish_reopenable_receipts() -> Result<()> {
    let tools = tempfile::tempdir()?;
    let executable = tools.path().join(if cfg!(windows) {
        "provider.exe"
    } else {
        "provider"
    });
    let compiled = Command::new("rustc")
        .arg("--edition=2024")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/publication_provider.rs"))
        .arg("-o")
        .arg(&executable)
        .output()?;
    ensure!(
        compiled.status.success(),
        "fixture compile failed: {}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    fs::create_dir(root.path().join("src"))?;
    let source = "pub fn one() -> u32 { 1 }\n";
    fs::write(root.path().join("src/lib.rs"), source)?;
    fs::write(
        root.path().join("Cargo.toml"),
        "[package]\nname = \"original\"\nversion = \"0.1.0\"\n",
    )?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.index()?;
    let store = FileStore::new(data.path().join("collections-1.2/default"));
    let baseline = store.current_test_catalog()?.unwrap();
    let native = baseline.load_artifact("src/lib.rs")?.facts;
    let mut parent = baseline.token;
    for adapter in ["ctags", "lsp"] {
        let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
        kernel.run_provider(
            adapter,
            &LiveRequest {
                command: producer(&executable, (adapter == "lsp").then_some("lsp")),
                producer: "original-publication-fixture".into(),
                producer_version: "1".into(),
                build_inputs: vec!["Cargo.toml".into()],
                paths: vec!["src/lib.rs".into()],
            },
        )?;
        let catalog = store.current_test_catalog()?.unwrap();
        assert!(catalog.token.generation > parent.generation);
        assert_eq!(catalog.snapshot.imported_facts.len(), 1);
        assert_eq!(catalog.snapshot.imported_facts[0].name, "one");
        assert_eq!(
            serde_json::to_value(catalog.load_artifact("src/lib.rs")?.facts)?,
            serde_json::to_value(&native)?
        );
        parent = catalog.token;
        let reopened = Kernel::open(root.path(), Some(data.path().into()))?;
        assert_eq!(
            reopened.file_info("src/lib.rs")?.generation,
            parent.generation
        );
        assert_eq!(fs::read_to_string(root.path().join("src/lib.rs"))?, source);
    }
    // Only dimensions are consumed by AIRK; the original fake producer does not
    // decode image pixels or load real trained data.
    let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
    png.extend_from_slice(&1u32.to_be_bytes());
    png.extend_from_slice(&1u32.to_be_bytes());
    fs::write(root.path().join("scan.png"), &png)?;
    fs::write(
        tools.path().join("eng.traineddata"),
        b"original fixture asset",
    )?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.extract_ocr(
        "scan.png",
        &OcrRequest {
            tesseract: producer(&executable, None),
            pdf_renderer: None,
            tessdata_dir: tools.path().into(),
            language: "eng".into(),
            first_page: 1,
            last_page: 1,
            dpi: 150,
        },
    )?;
    let catalog = store.current_test_catalog()?.unwrap();
    assert!(catalog.token.generation > parent.generation);
    let artifact = catalog.load_artifact("scan.png")?;
    assert_eq!(
        artifact.chunks[0].location,
        DocumentLocation::Page { number: 1 }
    );
    assert!(artifact.chunks[0].text.contains("REQ-42"));
    assert!(artifact.parse_status.contains("traineddata_sha256"));
    let reopened = Kernel::open(root.path(), Some(data.path().into()))?;
    assert_eq!(
        reopened.file_info("scan.png")?.generation,
        catalog.snapshot.generation
    );
    assert_eq!(reopened.index()?.generation, catalog.snapshot.generation);
    assert_eq!(fs::read(root.path().join("scan.png"))?, png);
    Ok(())
}
