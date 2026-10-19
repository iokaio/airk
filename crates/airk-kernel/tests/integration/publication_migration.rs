// SPDX-License-Identifier: Apache-2.0
//! Obsolete snapshots are rejected; full rebuild never migrates OCR or evidence.
use airk_core::{Artifact, Chunk, DocumentLocation, Snapshot};
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
    // Original obsolete fixture: OCR must be explicitly rerun after a rebuild.
    snapshot.extractor_version = "native-facts/7".into();
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
fn obsolete_json_ocr_is_rejected_without_migrating_derived_evidence() -> Result<()> {
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    let source = b"\x89PNG\r\n\x1a\noriginal binary fixture\0";
    fs::write(root.path().join("scan.png"), source)?;
    let store = FileStore::new(data.path().join("collections/default"));
    let artifact = ocr_artifact("scan.png", source)?;
    let original_legacy_bytes = save_legacy(root.path(), &store, &artifact, source)?;
    let original_head = store.expected_head()?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    assert!(kernel.index().is_err());
    assert!(
        kernel.reindex().is_err(),
        "unbound JSON-only state requires explicit derived-state removal"
    );
    assert!(kernel.read_file("scan.png", 1, 1, 4096).is_err());
    assert_eq!(store.expected_head()?, original_head);
    assert_eq!(
        fs::read(store.directory().join("snapshot.json"))?,
        original_legacy_bytes
    );
    assert_eq!(fs::read(root.path().join("scan.png"))?, source);
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
fn obsolete_ocr_rejection_preserves_alias_configuration_and_original_source() -> Result<()> {
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
    let config = fs::read(store.directory().join("roots.json"))?;
    assert!(kernel.index().is_err());
    assert!(kernel.read_file("@docs/scan.png", 1, 1, 4096).is_err());
    assert_eq!(fs::read(store.directory().join("roots.json"))?, config);
    assert_eq!(
        kernel.configured_roots()?["docs"],
        alias.path().canonicalize()?.to_string_lossy()
    );
    assert_eq!(fs::read(alias.path().join("scan.png"))?, source);
    Ok(())
}
