// SPDX-License-Identifier: Apache-2.0
//! Original migration fixtures exercise the public kernel publication boundary.
use airk_core::{
    Artifact, ArtifactSource, Chunk, DocumentLocation, Snapshot, changes::PublicationReason,
};
use airk_ingest::ocr::OcrProvenance;
use airk_kernel::Kernel;
use airk_store::FileStore;
use anyhow::Result;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs, path::Path};

fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn ocr_artifact(path: &str, source: &[u8]) -> Result<Artifact> {
    let provenance = OcrProvenance {
        schema: 1,
        source_sha256: sha(source),
        tesseract_sha256: sha(
            b"original fictional executable identity; no executable is installed",
        ),
        renderer_sha256: None,
        traineddata_sha256: BTreeMap::from([(
            "eng".into(),
            sha(b"original fictional trained data identity"),
        )]),
        language: "eng".into(),
        first_page: 2,
        last_page: 2,
        pages_with_text: 1,
        dpi: 150,
        coverage: "Original migration fixture, not OCR quality evidence".into(),
    };
    let status = serde_json::to_string(&provenance)?;
    Ok(Artifact {
        path: path.into(),
        content_hash: sha(source),
        bytes: source.len(),
        chunks: vec![Chunk {
            start_line: 1,
            end_line: 1,
            text: "Remember the original OCR page.\n".into(),
            location: DocumentLocation::Page { number: 2 },
        }],
        language: "ocr".into(),
        extraction_method: format!("local-tesseract-ocr/1:{}", sha(status.as_bytes())),
        parse_status: status,
        facts: vec![],
        retained: None,
    })
}

fn save_legacy(
    root: &Path,
    store: &FileStore,
    artifact: &Artifact,
    bytes: &[u8],
) -> Result<Vec<u8>> {
    let mut snapshot = Snapshot::empty(root.canonicalize()?.to_string_lossy().into_owned());
    snapshot.generation = 11;
    snapshot.storage_format = 1;
    snapshot.artifacts.insert(
        artifact.path.clone(),
        store.retain_artifact(artifact, Some(bytes))?,
    );
    // Deliberately preserve whitespace distinct from canonical reserialization.
    let mut encoded = serde_json::to_vec_pretty(&snapshot)?;
    encoded.extend_from_slice(b"\n\n");
    fs::create_dir_all(store.directory())?;
    fs::write(store.directory().join("snapshot.json"), &encoded)?;
    Ok(encoded)
}

#[test]
fn legacy_retained_ocr_migrates_without_executables_and_reopens_exact_evidence() -> Result<()> {
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    let source = b"\x89PNG\r\n\x1a\noriginal binary fixture\0";
    fs::write(root.path().join("scan.png"), source)?;
    let store = FileStore::new(data.path().join("collections/default"));
    let artifact = ocr_artifact("scan.png", source)?;
    let original_legacy_bytes = save_legacy(root.path(), &store, &artifact, source)?;
    let original_head = store.expected_head()?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    let report = kernel.index()?;
    assert_eq!(
        report.reused, 1,
        "migration reuses explicit OCR, not native image fallback"
    );
    let catalog = store.catalog()?.unwrap();
    let receipt = catalog.receipt.as_ref().unwrap();
    assert_eq!(receipt.token.generation, 12);
    assert_eq!(
        catalog.lineage.as_ref().unwrap().observation.reason,
        PublicationReason::Migration
    );
    assert!(
        catalog
            .lineage
            .as_ref()
            .unwrap()
            .observation
            .parent
            .is_none()
    );
    assert_ne!(store.expected_head()?, original_head);
    assert_eq!(
        serde_json::to_value(catalog.load_artifact("scan.png")?)?,
        serde_json::to_value(&artifact)?
    );
    assert!(
        catalog.snapshot.artifacts["scan.png"]
            .retained
            .as_ref()
            .unwrap()
            .profile
            .as_ref()
            .is_some_and(|profile| profile.language == "ocr")
    );
    assert_eq!(
        fs::read(store.directory().join("snapshot.json"))?,
        original_legacy_bytes
    );
    assert_eq!(fs::read(root.path().join("scan.png"))?, source);
    let token = receipt.token;
    drop(catalog);
    drop(kernel);
    let reopened = Kernel::open(root.path(), Some(data.path().into()))?;
    let response = reopened.read_file("scan.png", 1, 1, 4096)?;
    assert_eq!(
        response.hits[0].citation.location,
        DocumentLocation::Page { number: 2 }
    );
    assert_eq!(response.hits[0].text, artifact.chunks[0].text);
    assert_eq!(reopened.index()?.generation, token.generation);
    assert_eq!(store.catalog()?.unwrap().receipt.unwrap().token, token);
    Ok(())
}

#[test]
fn malformed_legacy_ocr_and_corrupt_retained_output_never_advance_pointer() -> Result<()> {
    for corrupt_object in [false, true] {
        let root = tempfile::tempdir()?;
        let data = tempfile::tempdir()?;
        let source = b"\x89PNG\r\n\0original fixture";
        fs::write(root.path().join("scan.png"), source)?;
        let store = FileStore::new(data.path().join("collections/default"));
        let mut artifact = ocr_artifact("scan.png", source)?;
        if !corrupt_object {
            artifact.extraction_method.push_str("invalid");
        }
        let legacy_bytes = save_legacy(root.path(), &store, &artifact, source)?;
        let head = store.expected_head()?;
        if corrupt_object {
            let snapshot: Snapshot = serde_json::from_slice(&legacy_bytes)?;
            let object = &snapshot.artifacts["scan.png"]
                .retained
                .as_ref()
                .unwrap()
                .object;
            fs::write(
                store.directory().join("objects/artifacts").join(object),
                b"corrupt derived fixture",
            )?;
        }
        let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
        assert!(kernel.index().is_err());
        assert_eq!(store.expected_head()?, head);
        assert_eq!(
            fs::read(store.directory().join("snapshot.json"))?,
            legacy_bytes
        );
        assert_eq!(fs::read(root.path().join("scan.png"))?, source);
        assert!(!store.directory().join("index.sqlite3").exists());
    }
    Ok(())
}

#[test]
fn aliased_legacy_ocr_keeps_original_root_and_distinct_binding() -> Result<()> {
    let root = tempfile::tempdir()?;
    let alias = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    let source = b"\x89PNG\r\n\0aliased original fixture";
    fs::write(alias.path().join("scan.png"), source)?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.add_root("docs", alias.path())?;
    let store = FileStore::new(data.path().join("collections/default"));
    let artifact = ocr_artifact("@docs/scan.png", source)?;
    let encoded = save_legacy(root.path(), &store, &artifact, source)?;
    let mut snapshot: Snapshot = serde_json::from_slice(&encoded)?;
    snapshot.roots = kernel.configured_roots()?;
    fs::write(
        store.directory().join("snapshot.json"),
        serde_json::to_vec_pretty(&snapshot)?,
    )?;
    kernel.index()?;
    let catalog = store.catalog()?.unwrap();
    let file = &catalog.inventory.as_ref().unwrap().files["@docs/scan.png"];
    assert_eq!(
        file.state.root,
        alias.path().canonicalize()?.to_string_lossy()
    );
    assert_eq!(file.state.path, "scan.png");
    assert_eq!(
        serde_json::to_value(catalog.load_artifact("@docs/scan.png")?)?,
        serde_json::to_value(&artifact)?
    );
    assert_eq!(
        kernel.source_location("@docs/scan.png")?.root,
        file.state.root
    );
    assert!(kernel.read_file("scan.png", 1, 1, 4096).is_err());
    assert_eq!(fs::read(alias.path().join("scan.png"))?, source);
    Ok(())
}
