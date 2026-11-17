// SPDX-License-Identifier: Apache-2.0
//! Synthetic metadata/refusal cases only; no DLL, device or model qualification.
use super::*;
fn hash(n: u8) -> Sha256Digest {
    Sha256Digest::from_bytes([n; 32])
}
fn fixture() -> (PublicRuntimeProvenance, Vec<manifest::RuntimeFile>) {
    let files = vec![
        PublicFile {
            name: "onnxruntime_providers_shared.dll".into(),
            bytes: 7,
            sha256: hash(1),
        },
        PublicFile {
            name: "onnxruntime_providers_cuda.dll".into(),
            bytes: 9,
            sha256: hash(2),
        },
    ];
    let native = files
        .iter()
        .map(|v| manifest::RuntimeFile {
            name: v.name.clone(),
            bytes: v.bytes,
            sha256: v.sha256,
            imports: BTreeSet::new(),
            delay_imports: BTreeSet::new(),
        })
        .collect();
    let components = vec![
        Component {
            name: "onnxruntime".into(),
            version: Some("1.30.0".into()),
            upstream_sha1: None,
            source_revision: None,
            original_url: None,
            source_kind: SourceKind::Archive,
            source_sha256: hash(3),
            inventory_sha256: Some(hash(4)),
            source_changes: vec![hash(5)],
        },
        Component {
            name: "cuda".into(),
            version: Some("12.8.1".into()),
            upstream_sha1: None,
            source_revision: None,
            original_url: None,
            source_kind: SourceKind::InstalledToolkitInventory,
            source_sha256: hash(6),
            inventory_sha256: Some(hash(6)),
            source_changes: vec![],
        },
        Component {
            name: "cudnn".into(),
            version: Some("9.10.2.21".into()),
            upstream_sha1: None,
            source_revision: None,
            original_url: None,
            source_kind: SourceKind::Archive,
            source_sha256: hash(7),
            inventory_sha256: Some(hash(8)),
            source_changes: vec![],
        },
    ];
    let legal = vec![
        LegalNotice {
            component: "onnxruntime".into(),
            file: PublicFile {
                name: "airk-cuda-notice-onnxruntime-license.txt".into(),
                bytes: 2,
                sha256: hash(9),
            },
            original_source_sha256: hash(3),
        },
        LegalNotice {
            component: "cuda".into(),
            file: PublicFile {
                name: "airk-cuda-notice-cuda-license.txt".into(),
                bytes: 3,
                sha256: hash(10),
            },
            original_source_sha256: hash(6),
        },
        LegalNotice {
            component: "cudnn".into(),
            file: PublicFile {
                name: "airk-cuda-notice-cudnn-license.txt".into(),
                bytes: 4,
                sha256: hash(11),
            },
            original_source_sha256: hash(7),
        },
    ];
    (
        PublicRuntimeProvenance {
            schema: "airk.native.cuda-runtime-provenance/1".into(),
            target: "x86_64-pc-windows-msvc".into(),
            ort_version: "1.30.0".into(),
            static_core_build: hash(12),
            producer_script_sha256: hash(13),
            producer_lock_sha256: hash(14),
            verifier_sha256: hash(15),
            build_proof_sha256: hash(16),
            configure_argv_sha256: hash(17),
            legal_inventory_sha256: hash(18),
            legal_owner_sha256: hash(19),
            checkpoint_history: None,
            incremental_history: None,
            historical_failed_parent: Some(hash(20)),
            aggregate: Aggregate {
                file: PublicFile {
                    name: "onnxruntime.lib".into(),
                    bytes: 70_000,
                    sha256: hash(21),
                },
                proof_sha256: hash(22),
                receipt_sha256: hash(23),
                plan_sha256: hash(24),
            },
            components,
            tools: vec![
                Tool {
                    name: "compiler".into(),
                    version: Some("14.44.35207".into()),
                    version_kind: Some("msvc-toolset".into()),
                    bytes: 3,
                    sha256: hash(25),
                },
                Tool {
                    name: "cmake".into(),
                    version: Some("3.31.6".into()),
                    version_kind: Some("reported-tool-version".into()),
                    bytes: 4,
                    sha256: hash(26),
                },
                Tool {
                    name: "ninja".into(),
                    version: Some("1.12.1".into()),
                    version_kind: Some("reported-tool-version".into()),
                    bytes: 5,
                    sha256: hash(27),
                },
            ],
            options: vec![
                BuildOption {
                    name: "onnxruntime_USE_CUDA".into(),
                    value: "ON".into(),
                },
                BuildOption {
                    name: "onnxruntime_BUILD_SHARED_LIB".into(),
                    value: "OFF".into(),
                },
                BuildOption {
                    name: "onnxruntime_BUILD_CUDA_EP_AS_PLUGIN".into(),
                    value: "OFF".into(),
                },
                BuildOption {
                    name: "CMAKE_CUDA_ARCHITECTURES".into(),
                    value: "86".into(),
                },
            ],
            files,
            legal,
        },
        native,
    )
}
fn raw_asset(value: &PublicRuntimeProvenance) -> Result<(Vec<u8>, Asset)> {
    let raw = serde_json::to_vec(value)?;
    let asset = Asset {
        name: NAME.into(),
        role: AssetRole::Notice,
        bytes: raw.len() as u64,
        sha256: Sha256Digest::from_bytes(Sha256::digest(&raw).into()),
    };
    Ok((raw, asset))
}
fn accepts(value: &PublicRuntimeProvenance, native: &[manifest::RuntimeFile]) -> bool {
    let (raw, asset) = raw_asset(value).unwrap();
    validate(&raw, &asset, hash(12), native).is_ok()
}
#[test]
fn public_provenance_roundtrip_and_exact_legal_assets() -> Result<()> {
    let (value, native) = fixture();
    let (raw, asset) = raw_asset(&value)?;
    let parsed = validate(&raw, &asset, hash(12), &native)?;
    assert_eq!(serde_json::to_vec(&parsed)?, raw);
    let legal = parsed.legal_assets();
    assert_eq!(
        legal.iter().map(|v| v.name.as_str()).collect::<Vec<_>>(),
        [
            "airk-cuda-notice-onnxruntime-license.txt",
            "airk-cuda-notice-cuda-license.txt",
            "airk-cuda-notice-cudnn-license.txt"
        ]
    );
    assert!(legal.iter().all(|v| v.role == AssetRole::Notice));
    let text = String::from_utf8(raw)?;
    assert!(!text.contains("C:\\"));
    assert!(!text.contains("/Users/"));
    Ok(())
}
#[test]
fn public_provenance_refuses_tampered_bytes_and_wrong_static_build() -> Result<()> {
    let (value, native) = fixture();
    let (mut raw, asset) = raw_asset(&value)?;
    assert!(validate(&raw, &asset, hash(31), &native).is_err());
    raw.push(b' ');
    assert!(validate(&raw, &asset, hash(12), &native).is_err());
    let mut wrong = asset.clone();
    wrong.role = AssetRole::Fixture;
    assert!(validate(&serde_json::to_vec(&value)?, &wrong, hash(12), &native).is_err());
    Ok(())
}
#[test]
fn public_provenance_refuses_driver_second_core_case_alias_and_roster_drift() {
    let (value, native) = fixture();
    for name in [
        "nvcuda.dll",
        "onnxruntime.dll",
        "cuinj64_128.dll",
        "helper.dll",
        "ONNXRUNTIME_PROVIDERS_CUDA.DLL",
        "../cudart64_12.dll",
    ] {
        let mut changed = value.clone();
        let mut forbidden = changed.files[1].clone();
        forbidden.name = name.into();
        changed.files.push(forbidden);
        let mut matching_native = native.clone();
        let mut forbidden_native = matching_native[1].clone();
        forbidden_native.name = name.into();
        matching_native.push(forbidden_native);
        let (raw, asset) = raw_asset(&changed).unwrap();
        let error = validate(&raw, &asset, hash(12), &matching_native)
            .expect_err("intrinsically forbidden DLL accepted");
        assert_eq!(
            error.to_string(),
            "unapproved/duplicate runtime payload",
            "{name}"
        );
    }
    let mut changed = value.clone();
    changed.files[1].sha256 = hash(30);
    assert!(!accepts(&changed, &native));
    let mut changed = value.clone();
    changed.files.push(changed.files[0].clone());
    assert!(!accepts(&changed, &native));
}
#[test]
fn public_provenance_refuses_missing_legal_and_private_paths() {
    let (value, native) = fixture();
    let mut changed = value.clone();
    changed.legal.pop();
    assert!(!accepts(&changed, &native));
    for name in [
        "C:/Users/tdotn/license.txt",
        "notices/cuda/license.txt",
        "airk-cuda-notice-cuda-../license.txt",
        "airk-cuda-notice-cuda-library.dll",
    ] {
        let mut changed = value.clone();
        changed.legal[1].file.name = name.into();
        assert!(!accepts(&changed, &native));
    }
    let mut changed = value.clone();
    changed.tools[0].version = Some("C:/private/compiler.log".into());
    assert!(!accepts(&changed, &native));
}
#[test]
fn public_provenance_refuses_duplicate_or_unknown_fields() -> Result<()> {
    let (value, native) = fixture();
    let (raw, _) = raw_asset(&value)?;
    let text = String::from_utf8(raw)?;
    for changed in [
        text.replacen(
            "{",
            "{\"schema\":\"airk.native.cuda-runtime-provenance/1\",",
            1,
        ),
        text.replacen("{", "{\"qualified\":true,", 1),
    ] {
        let raw = changed.into_bytes();
        let asset = Asset {
            name: NAME.into(),
            role: AssetRole::Notice,
            bytes: raw.len() as u64,
            sha256: Sha256Digest::from_bytes(Sha256::digest(&raw).into()),
        };
        assert!(validate(&raw, &asset, hash(12), &native).is_err());
    }
    Ok(())
}
#[test]
fn public_provenance_refuses_shared_core_options_fake_source_kind_and_bounds() {
    let (value, native) = fixture();
    let mut changed = value.clone();
    changed.options[1].value = "ON".into();
    assert!(!accepts(&changed, &native));
    let mut changed = value.clone();
    changed.components[1].source_kind = SourceKind::Archive;
    assert!(!accepts(&changed, &native));
    let mut changed = value.clone();
    changed.aggregate.file.bytes = 0;
    assert!(!accepts(&changed, &native));
    let mut changed = value.clone();
    changed.options.push(changed.options[0].clone());
    assert!(!accepts(&changed, &native));
}

#[test]
fn legacy_public_bytes_omit_checkpoint_history() -> Result<()> {
    let (value, native) = fixture();
    let raw = serde_json::to_vec(&value)?;
    assert!(!String::from_utf8_lossy(&raw).contains("checkpoint_history"));
    assert!(!String::from_utf8_lossy(&raw).contains("incremental_history"));
    validate_description(&value, value.static_core_build, &native)?;
    Ok(())
}
#[test]
fn public_checkpoint_history_is_path_free_descriptive_and_strict() -> Result<()> {
    let (mut value, native) = fixture();
    let failed: Sha256Digest =
        "3a4c18932dbafbcc719cb77b30f616372b220cc315a1acdba82df24a9734c026".parse()?;
    value.historical_failed_parent = Some(failed);
    value.checkpoint_history = Some(CheckpointHistory {
        mode: "checkpointed-warm".into(),
        cold_build_qualified: false,
        qualification: false,
        approved_plan_sha256: "6bee6b8b95a01fea28adf04117260d12503012ea8120ae225afa232b6589df97"
            .parse()?,
        failed_ancestry: vec![failed],
        sections: vec![
            CheckpointSectionHistory {
                section_id: "flash-remainder".into(),
                section_index: 0,
                receipt_sha256: hash(31),
            },
            CheckpointSectionHistory {
                section_id: "llm-remainder".into(),
                section_index: 1,
                receipt_sha256: hash(32),
            },
            CheckpointSectionHistory {
                section_id: "final-provider-all-closure".into(),
                section_index: 2,
                receipt_sha256: hash(33),
            },
        ],
    });
    validate_description(&value, value.static_core_build, &native)?;
    let raw = serde_json::to_vec(&value)?;
    let decoded: PublicRuntimeProvenance = serde_json::from_slice(&raw)?;
    assert_eq!(serde_json::to_vec(&decoded)?, raw);
    value
        .checkpoint_history
        .as_mut()
        .unwrap()
        .cold_build_qualified = true;
    assert!(validate_description(&value, value.static_core_build, &native).is_err());
    value
        .checkpoint_history
        .as_mut()
        .unwrap()
        .cold_build_qualified = false;
    value
        .checkpoint_history
        .as_mut()
        .unwrap()
        .sections
        .swap(0, 1);
    assert!(validate_description(&value, value.static_core_build, &native).is_err());
    Ok(())
}

fn incremental_fixture() -> Result<(PublicRuntimeProvenance, Vec<manifest::RuntimeFile>)> {
    let (mut value, native) = fixture();
    value
        .components
        .iter_mut()
        .find(|c| c.name == "onnxruntime")
        .unwrap()
        .source_changes
        .extend([hash(44), hash(43)]);
    let failed = "3a4c18932dbafbcc719cb77b30f616372b220cc315a1acdba82df24a9734c026".parse()?;
    value.historical_failed_parent = Some(failed);
    value.checkpoint_history = Some(CheckpointHistory {
        mode: "checkpointed-warm".into(),
        qualification: false,
        cold_build_qualified: false,
        approved_plan_sha256: "6bee6b8b95a01fea28adf04117260d12503012ea8120ae225afa232b6589df97"
            .parse()?,
        failed_ancestry: vec![failed],
        sections: vec![
            CheckpointSectionHistory {
                section_id: "flash-remainder".into(),
                section_index: 0,
                receipt_sha256: hash(31),
            },
            CheckpointSectionHistory {
                section_id: "llm-remainder".into(),
                section_index: 1,
                receipt_sha256: hash(32),
            },
            CheckpointSectionHistory {
                section_id: "final-provider-all-closure".into(),
                section_index: 2,
                receipt_sha256: hash(33),
            },
        ],
    });
    value.incremental_history = Some(IncrementalHistory {
        mode: "incremental-finalgraph".into(),
        parent_producer_sha256: "f9fac8eb2fe1a1cb5191118124503782792ca6d3aff14d02326167cd7276b178"
            .parse()?,
        parent_build_proof_sha256: hash(40),
        parent_aggregate_sha256: "042c970193bf3d2fc7c80f53b055f74130c653d561f7735142fc01b01cebac00"
            .parse()?,
        recipe_sha256: hash(41),
        source_before_sha256: hash(42),
        source_after_sha256: hash(43),
        patch_sha256: hash(44),
        new_object_sha256: hash(45),
        new_session_sha256: hash(46),
        aggregate_sha256: value.aggregate.file.sha256,
        qualification: false,
        cold_build_qualified: false,
    });
    Ok((value, native))
}
#[test]
fn public_incremental_history_is_path_free_and_preserves_parent_history() -> Result<()> {
    let (v, native) = incremental_fixture()?;
    validate_description(&v, v.static_core_build, &native)?;
    let bytes = serde_json::to_vec(&v)?;
    let value: serde_json::Value = serde_json::from_slice(&bytes)?;
    assert_eq!(
        value["incremental_history"]["mode"],
        "incremental-finalgraph"
    );
    assert!(
        value["incremental_history"]
            .as_object()
            .unwrap()
            .keys()
            .all(|k| !k.contains("path"))
    );
    assert_eq!(value["checkpoint_history"]["mode"], "checkpointed-warm");
    let roundtrip: PublicRuntimeProvenance = serde_json::from_slice(&bytes)?;
    assert_eq!(serde_json::to_vec(&roundtrip)?, bytes);
    Ok(())
}
#[test]
fn public_incremental_history_never_relabels_parent_build_or_aggregate() -> Result<()> {
    let (v, native) = incremental_fixture()?;
    let mut changed = v.clone();
    changed.static_core_build = changed
        .incremental_history
        .as_ref()
        .unwrap()
        .parent_producer_sha256;
    assert!(validate_description(&changed, changed.static_core_build, &native).is_err());
    let mut changed = v.clone();
    changed
        .incremental_history
        .as_mut()
        .unwrap()
        .aggregate_sha256 = hash(89);
    assert!(validate_description(&changed, changed.static_core_build, &native).is_err());
    let mut changed = v;
    changed.checkpoint_history = None;
    assert!(validate_description(&changed, changed.static_core_build, &native).is_err());
    Ok(())
}
#[test]
fn public_incremental_history_refuses_fabricated_qualification_and_unknown_fields() -> Result<()> {
    let (v, native) = incremental_fixture()?;
    let mut changed = v.clone();
    changed.incremental_history.as_mut().unwrap().qualification = true;
    assert!(validate_description(&changed, changed.static_core_build, &native).is_err());
    let mut changed = v.clone();
    changed
        .incremental_history
        .as_mut()
        .unwrap()
        .cold_build_qualified = true;
    assert!(validate_description(&changed, changed.static_core_build, &native).is_err());
    let mut raw = serde_json::to_value(&v)?;
    raw["incremental_history"]["approved"] = serde_json::json!(true);
    assert!(serde_json::from_value::<PublicRuntimeProvenance>(raw).is_err());
    Ok(())
}
