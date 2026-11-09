// SPDX-License-Identifier: Apache-2.0
use super::*;
use airk_core::identity::{Incarnation, PublicationReceipt};
use std::{collections::BTreeSet, time::Duration};
fn fixture(engine: &str, schema: u32) -> (Manifest, PublicationReceipt, LegacyInventory) {
    let digest = Sha256Digest::from_bytes([1; 32]);
    let receipt = PublicationReceipt {
        token: RevisionToken {
            incarnation: Incarnation::from_bytes([2; 16]),
            generation: 1,
            revision: digest,
        },
        inventory_digest: digest,
        manifest_digest: Sha256Digest::from_bytes([3; 32]),
    };
    let components = BTreeMap::from(["inventory.json", "lineage.json", "catalog.sqlite"].map(
        |name| {
            (
                name.into(),
                SavedFile {
                    bytes: 3,
                    sha256: digest,
                },
            )
        },
    ));
    let sealed = LegacyInventory {
        files: components
            .iter()
            .map(|(name, value)| (format!("generations/{digest}/{name}"), value.clone()))
            .collect(),
        directories: BTreeSet::from(["generations".into(), format!("generations/{digest}")]),
        bytes: 9,
    };
    (
        Manifest {
            schema,
            engine: engine.into(),
            token: receipt.token,
            collection: "saved".into(),
            chunks: 0,
            vectors: 0,
            model_sha256: None,
            embedding: "none".into(),
            inventory_digest: digest,
            components,
        },
        receipt,
        sealed,
    )
}
fn until() -> Instant {
    Instant::now() + Duration::from_secs(10)
}
#[test]
fn legacy_adoption_historical_manifest_accepts_only_released_bound_formats() -> Result<()> {
    for (engine, schema) in [(ENGINE_2, 2), (ENGINE_3, 3), (ENGINE_3_SOURCE, 3)] {
        let (mut manifest, receipt, sealed) = fixture(engine, schema);
        verify_manifest(&manifest, &receipt, &sealed, until())?;
        manifest.engine.push_str("/unknown");
        assert!(verify_manifest(&manifest, &receipt, &sealed, until()).is_err());
    }
    let (manifest, mut receipt, sealed) = fixture(ENGINE_3, 3);
    receipt.token.generation += 1;
    assert!(verify_manifest(&manifest, &receipt, &sealed, until()).is_err());
    let (manifest, mut receipt, sealed) = fixture(ENGINE_3, 3);
    receipt.inventory_digest = Sha256Digest::from_bytes([9; 32]);
    assert!(verify_manifest(&manifest, &receipt, &sealed, until()).is_err());
    Ok(())
}
#[test]
fn legacy_adoption_historical_components_refuse_overlays_changes_and_unsafe_paths() -> Result<()> {
    let (manifest, receipt, mut sealed) = fixture(ENGINE_3, 3);
    let key = format!("generations/{}/catalog.sqlite", receipt.token.revision);
    sealed.files.get_mut(&key).unwrap().sha256 = Sha256Digest::from_bytes([9; 32]);
    assert!(verify_manifest(&manifest, &receipt, &sealed, until()).is_err());
    let (manifest, receipt, mut sealed) = fixture(ENGINE_3, 3);
    sealed.files.insert(
        format!("generations/{}/catalog.sqlite-wal", receipt.token.revision),
        SavedFile {
            bytes: 0,
            sha256: Sha256Digest::from_bytes([8; 32]),
        },
    );
    assert!(verify_manifest(&manifest, &receipt, &sealed, until()).is_err());
    let (mut manifest, receipt, mut sealed) = fixture(ENGINE_3, 3);
    let unknown = SavedFile {
        bytes: 0,
        sha256: Sha256Digest::from_bytes([8; 32]),
    };
    manifest
        .components
        .insert("../outside".into(), unknown.clone());
    sealed.files.insert(
        format!("generations/{}/../outside", receipt.token.revision),
        unknown,
    );
    assert!(verify_manifest(&manifest, &receipt, &sealed, until()).is_err());
    let (manifest, receipt, mut sealed) = fixture(ENGINE_3, 3);
    sealed
        .directories
        .insert(format!("generations/{}/unlisted", receipt.token.revision));
    assert!(verify_manifest(&manifest, &receipt, &sealed, until()).is_err());
    Ok(())
}
