// SPDX-License-Identifier: Apache-2.0
//! Original fixtures for privately finalized retrieval generations and receipts.
mod support;
use airk_core::{
    Artifact, Chunk, DocumentLocation, SearchQuery, Snapshot,
    identity::{Incarnation, RevisionToken, Sha256Digest, revision_digest},
    publication::InventoryV2,
};
use airk_retrieval::persistent::{
    self, ManifestV2, PersistentIndex, V2Preparation, retrieval_specification,
};
use anyhow::Result;
use sha2::{Digest, Sha256};
use std::{fs, path::Path, sync::Arc};

const LINEAGE: &[u8] = b"{\"generation\":1}";

fn inventory(source: &Snapshot) -> Result<InventoryV2> {
    let mut inventory = support::inventory(source, &support::FixtureSource::new(source)?, None)?;
    inventory.lineage_digest = support::digest(LINEAGE);
    Ok(inventory)
}

#[allow(clippy::too_many_arguments)]
fn prepare_fixture(
    base: &Path,
    preparation: &V2Preparation<'_>,
    model: Option<&Path>,
    source: &Snapshot,
    charge: Option<airk_retrieval::bounded::Charge>,
    staging: Option<&Path>,
    catalog: impl FnOnce(&Path) -> Result<()>,
) -> Result<airk_core::identity::PublicationReceipt> {
    let source = support::FixtureSource::new(source)?;
    let objects = model
        .map(|model| support::install_model(base, model))
        .transpose()?;
    persistent::prepare_streaming(
        base,
        preparation,
        objects.as_ref(),
        &source,
        charge,
        staging,
        None,
        catalog,
        |_| Ok(()),
    )
}

fn fixture() -> Snapshot {
    let mut snapshot = Snapshot::empty("original-v2-fixture".into());
    snapshot.generation = 1;
    let text = "Original aurora publication evidence.\n";
    snapshot.artifacts.insert(
        "one.txt".into(),
        Artifact {
            path: "one.txt".into(),
            content_hash: format!("{:x}", Sha256::digest(text)),
            bytes: text.len(),
            chunks: vec![Chunk {
                start_line: 1,
                end_line: 1,
                text: text.into(),
                location: DocumentLocation::Lines { start: 1, end: 1 },
            }],
            language: "text".into(),
            extraction_method: "utf8".into(),
            parse_status: "complete".into(),
            facts: vec![],
            retained: None,
        },
    );
    snapshot
}

fn token(inventory: &[u8]) -> RevisionToken {
    RevisionToken {
        incarnation: Incarnation::from_bytes([1; 16]),
        generation: 1,
        revision: revision_digest(retrieval_specification(), inventory),
    }
}

fn catalog(directory: &Path) -> Result<()> {
    // SQL semantics belong to the store callback. This fixture tests the
    // retrieval boundary: its bytes must be present before manifest hashing.
    assert!(!directory.join("manifest.json").exists());
    fs::write(
        directory.join("catalog.sqlite"),
        b"original closed catalog fixture",
    )?;
    fs::write(directory.join("lineage.json"), LINEAGE)?;
    Ok(())
}

#[test]
fn receipt_binds_exact_inventory_and_post_callback_catalog() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let base = temp.path().join("collection");
    let source = fixture();
    let authoritative = inventory(&source)?;
    let snapshot = authoritative.projection()?;
    let bytes = authoritative.canonical_bytes()?;
    let inventory = bytes.as_slice();
    let preparation = V2Preparation {
        snapshot: &snapshot,
        canonical_inventory: inventory,
        token: token(inventory),
    };
    let receipt = prepare_fixture(&base, &preparation, None, &source, None, None, catalog)?;
    let generation = base
        .join("generations")
        .join(receipt.token.revision.to_string());
    assert_eq!(fs::read(generation.join("inventory.json"))?, inventory);
    assert_eq!(
        receipt.inventory_digest,
        Sha256Digest::from_bytes(Sha256::digest(inventory).into())
    );
    let manifest: ManifestV2 =
        serde_json::from_slice(&fs::read(generation.join("manifest.json"))?)?;
    assert_eq!(
        manifest.components["catalog.sqlite"].sha256,
        Sha256Digest::from_bytes(Sha256::digest(b"original closed catalog fixture").into())
    );
    assert!(!base.join("index.sqlite3").exists());
    let reader = PersistentIndex::open_v2(&base, "default", &receipt)?;
    let query = SearchQuery {
        text: "aurora".into(),
        path_prefix: None,
        limit: 1,
        max_bytes: 1024,
    };
    assert_eq!(reader.search(&query)?.hits[0].citation.path, "one.txt");
    assert!(PersistentIndex::open_v2(&base, "other", &receipt).is_err());
    assert!(prepare_fixture(&base, &preparation, None, &source, None, None, catalog).is_err());
    fs::write(generation.join("manifest.json"), b"{}")?;
    assert!(reader.search(&query).is_err());
    Ok(())
}

#[test]
fn empty_locked_lexical_component_verifies_without_reading_locked_bytes() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let base = temp.path().join("collection");
    let source = fixture();
    let authoritative = inventory(&source)?;
    let snapshot = authoritative.projection()?;
    let bytes = authoritative.canonical_bytes()?;
    let preparation = V2Preparation {
        snapshot: &snapshot,
        canonical_inventory: &bytes,
        token: token(&bytes),
    };
    let receipt = prepare_fixture(&base, &preparation, None, &source, None, None, catalog)?;
    let directory = base
        .join("generations")
        .join(receipt.token.revision.to_string());
    let manifest: ManifestV2 = serde_json::from_slice(&fs::read(directory.join("manifest.json"))?)?;
    // The writer lock is independent of the metadata lock acquired by readers.
    let name = "lexical/.tantivy-writer.lock";
    assert_eq!(manifest.components[name].bytes, 0);
    assert_eq!(
        manifest.components[name].sha256,
        Sha256Digest::from_bytes(Sha256::digest([]).into())
    );
    let path = directory.join(name);
    let lock = fs::OpenOptions::new().read(true).write(true).open(&path)?;
    lock.lock()?;
    let opened = PersistentIndex::open_v2(&base, "default", &receipt);
    lock.unlock()?;
    drop(lock);
    drop(opened?);
    fs::write(path, b"tampered nonempty lock")?;
    assert!(PersistentIndex::open_v2(&base, "default", &receipt).is_err());
    Ok(())
}

#[test]
fn component_corruption_and_unowned_sidecars_fail_reopen() -> Result<()> {
    let source = fixture();
    let authoritative = inventory(&source)?;
    let snapshot = authoritative.projection()?;
    let bytes = authoritative.canonical_bytes()?;
    let inventory = bytes.as_slice();
    for name in [
        "catalog.sqlite",
        "lineage.json",
        "inventory.json",
        "unexpected.sqlite",
        "vector-map/meta.json",
        "vector-map/unlisted.bin",
    ] {
        let temp = tempfile::tempdir()?;
        let base = temp.path().join("collection");
        let preparation = V2Preparation {
            snapshot: &snapshot,
            canonical_inventory: inventory,
            token: token(inventory),
        };
        let receipt = prepare_fixture(&base, &preparation, None, &source, None, None, catalog)?;
        fs::write(
            base.join("generations")
                .join(receipt.token.revision.to_string())
                .join(name),
            b"changed bytes",
        )?;
        assert!(
            PersistentIndex::open_v2(&base, "default", &receipt).is_err(),
            "{name}"
        );
    }
    Ok(())
}

#[test]
fn callback_failure_missing_catalog_and_component_mutation_never_publish() -> Result<()> {
    let source = fixture();
    let authoritative = inventory(&source)?;
    let snapshot = authoritative.projection()?;
    let bytes = authoritative.canonical_bytes()?;
    let inventory = bytes.as_slice();
    for mode in 0..4 {
        let temp = tempfile::tempdir()?;
        let base = temp.path().join("collection");
        let preparation = V2Preparation {
            snapshot: &snapshot,
            canonical_inventory: inventory,
            token: token(inventory),
        };
        let result = prepare_fixture(
            &base,
            &preparation,
            None,
            &source,
            None,
            None,
            |directory| {
                match mode {
                    0 => anyhow::bail!("original injected catalog failure"),
                    1 => {}
                    2 => {
                        catalog(directory)?;
                        fs::write(directory.join("inventory.json"), b"replaced")?;
                    }
                    _ => {
                        catalog(directory)?;
                        fs::write(directory.join("catalog.sqlite-wal"), b"open sidecar")?;
                    }
                }
                Ok(())
            },
        );
        assert!(result.is_err());
        assert_eq!(fs::read_dir(base.join("generations"))?.count(), 0);
        assert!(!base.join("index.sqlite3").exists());
    }
    Ok(())
}

#[test]
fn bad_token_and_quota_failure_do_not_reach_callback() -> Result<()> {
    let source = fixture();
    let authoritative = inventory(&source)?;
    let snapshot = authoritative.projection()?;
    let bytes = authoritative.canonical_bytes()?;
    let inventory = bytes.as_slice();
    for bad_token in [true, false] {
        let temp = tempfile::tempdir()?;
        let base = temp.path().join("collection");
        let mut revision = token(inventory);
        if bad_token {
            revision.revision = Sha256Digest::from_bytes([9; 32]);
        }
        let preparation = V2Preparation {
            snapshot: &snapshot,
            canonical_inventory: inventory,
            token: revision,
        };
        let result = prepare_fixture(
            &base,
            &preparation,
            None,
            &source,
            Some(Arc::new(|_| anyhow::bail!("original quota denial"))),
            None,
            |_| panic!("callback ran after rejection"),
        );
        assert!(result.is_err());
        assert!(
            !base
                .join("generations")
                .join(revision.revision.to_string())
                .exists()
        );
    }
    Ok(())
}

#[test]
fn mismatched_projection_incarnation_specification_and_noncanonical_inventory_are_rejected()
-> Result<()> {
    let source = fixture();
    let authoritative = inventory(&source)?;
    let bytes = authoritative.canonical_bytes()?;
    for mode in 0..4 {
        let temp = tempfile::tempdir()?;
        let base = temp.path().join("collection");
        let mut snapshot = authoritative.projection()?;
        let mut bytes = bytes.clone();
        let mut revision = token(&bytes);
        match mode {
            0 => snapshot.root = "foreign original fixture".into(),
            1 => revision.incarnation = Incarnation::from_bytes([3; 16]),
            2 => {
                let mut other = authoritative.clone();
                other.retrieval_specification = "foreign engine".into();
                bytes = other.canonical_bytes()?;
                revision = token(&bytes);
            }
            _ => {
                bytes.push(b'\n');
                revision = token(&bytes);
            }
        }
        let preparation = V2Preparation {
            snapshot: &snapshot,
            canonical_inventory: &bytes,
            token: revision,
        };
        assert!(
            prepare_fixture(&base, &preparation, None, &source, None, None, |_| panic!(
                "invalid identity reached catalog callback"
            ))
            .is_err()
        );
        assert!(!base.join("generations").exists());
    }
    Ok(())
}

#[test]
fn local_vector_assets_are_receipt_bound_and_reopened() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let base = temp.path().join("collection");
    let model = temp.path().join("original-model");
    fs::create_dir(&model)?;
    let vocabulary = br#"{"aurora":[1.0,0.0],"publication":[0.0,1.0]}"#;
    fs::write(model.join("vocabulary.json"), vocabulary)?;
    fs::write(
        model.join("model.json"),
        serde_json::to_vec(&serde_json::json!({
            "schema": 1, "name": "original test vectors", "license": "Apache-2.0",
            "sha256": format!("{:x}", Sha256::digest(vocabulary)), "dimensions": 2,
        }))?,
    )?;
    let source = fixture();
    let mut authoritative = inventory(&source)?;
    authoritative.embedding_model = Some(airk_retrieval::model::inspect(&model)?.identity);
    authoritative.embedding_pipeline_digest =
        Some(airk_retrieval::model::inspect_cache_descriptor(&model)?.identity()?);
    let snapshot = authoritative.projection()?;
    let bytes = authoritative.canonical_bytes()?;
    let preparation = V2Preparation {
        snapshot: &snapshot,
        canonical_inventory: &bytes,
        token: token(&bytes),
    };
    let receipt = prepare_fixture(
        &base,
        &preparation,
        Some(&model),
        &source,
        None,
        None,
        catalog,
    )?;
    let generation = base
        .join("generations")
        .join(receipt.token.revision.to_string());
    let manifest: ManifestV2 =
        serde_json::from_slice(&fs::read(generation.join("manifest.json"))?)?;
    for name in ["vectors.bin", "model-objects.json", "vector-map/meta.json"] {
        assert!(manifest.components.contains_key(name));
    }
    let reader = PersistentIndex::open_v2(&base, "default", &receipt)?;
    assert_eq!(
        reader
            .semantic_search(&SearchQuery {
                text: "aurora".into(),
                path_prefix: None,
                limit: 1,
                max_bytes: 1024
            })?
            .hits[0]
            .citation
            .path,
        "one.txt"
    );
    drop(reader);
    let objects = airk_retrieval::model::inspect_object_plan(&model)?;
    let object = &objects.assets[&airk_core::model_objects::ModelAssetName::VocabularyJson];
    fs::write(
        base.join("objects/models").join(object.sha256.to_string()),
        b"{}",
    )?;
    assert!(PersistentIndex::open_v2(&base, "default", &receipt).is_err());
    Ok(())
}

#[test]
fn old_manifest_rejection_explains_full_rebuild_before_new_field_deserialization() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let base = temp.path().join("collection");
    let source = fixture();
    let authoritative = inventory(&source)?;
    let bytes = authoritative.canonical_bytes()?;
    let preparation = V2Preparation {
        snapshot: &authoritative.projection()?,
        canonical_inventory: &bytes,
        token: token(&bytes),
    };
    let mut receipt = prepare_fixture(&base, &preparation, None, &source, None, None, catalog)?;
    let path = base
        .join("generations")
        .join(receipt.token.revision.to_string())
        .join("manifest.json");
    let bytes = br#"{"schema":3,"engine":"old-generation"}"#;
    fs::write(path, bytes)?;
    receipt.manifest_digest = support::digest(bytes);
    let error = PersistentIndex::open_v2(&base, &source.collection, &receipt)
        .err()
        .unwrap();
    assert!(error.to_string().contains("airk index --full"), "{error}");
    Ok(())
}
