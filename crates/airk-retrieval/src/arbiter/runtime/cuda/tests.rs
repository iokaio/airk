// SPDX-License-Identifier: Apache-2.0
use super::*;
#[test]
fn phase_capacity_uses_incremental_work_after_resident_memory_is_counted() -> Result<()> {
    let mut claims = Claims::default();
    let cold = claims.reserve(120, 100, 20)?;
    assert!(claims.reserve(120, 1, 20).is_err());
    claims.unspent.remove(&cold);
    // Native free80 already excludes resident40; only warm20+headroom20 is due.
    let warm = claims.reserve(80, 20, 20)?;
    assert_eq!(claims.unspent[&warm], (20, 20));
    assert!(
        Claims::default().reserve(80, 100, 20).is_err(),
        "never reserve full100 atop resident40"
    );
    Ok(())
}
#[test]
fn concurrent_claims_preserve_largest_headroom_and_refuse_overflow() -> Result<()> {
    let mut c = Claims::default();
    c.reserve(100, 30, 50)?;
    assert!(c.reserve(100, 30, 1).is_err());
    assert!(Claims::default().reserve(u64::MAX, u64::MAX, 1).is_err());
    assert!(DeviceCapacityPlan::new(100, 101, 1).is_err());
    assert!(DeviceCapacityPlan::new(1, 1, 0).is_err());
    Ok(())
}
fn manifest() -> manifest::RuntimeManifest {
    let f = |name: &str| manifest::RuntimeFile {
        name: name.into(),
        bytes: 12,
        sha256: Sha256Digest::from_bytes([1; 32]),
        imports: BTreeSet::new(),
        delay_imports: BTreeSet::new(),
    };
    manifest::RuntimeManifest {
        schema: "airk.native.cuda-runtime-manifest/1".into(),
        ort_version: "1.30.0".into(),
        static_core_build: Sha256Digest::from_bytes([2; 32]),
        producer_name: "producer.json".into(),
        producer_bytes: 10,
        producer_sha256: Sha256Digest::from_bytes([2; 32]),
        files: vec![
            f("onnxruntime_providers_shared.dll"),
            f("onnxruntime_providers_cuda.dll"),
        ],
        load_order: vec![
            "onnxruntime_providers_shared.dll".into(),
            "onnxruntime_providers_cuda.dll".into(),
        ],
        system_imports: BTreeSet::new(),
        fits: Vec::new(),
    }
}
#[test]
fn manifest_observation_is_not_compiled_approval_and_cannot_name_second_core() -> Result<()> {
    let mut m = manifest();
    m.validate()?;
    assert!(manifest::RuntimeManifest::approved(b"{}").is_err());
    m.files[0].name = "onnxruntime.dll".into();
    assert!(m.validate().is_err());
    Ok(())
}
#[test]
fn manifest_refuses_ambient_imports_paths_duplicates_and_wrong_static_build() -> Result<()> {
    let mut m = manifest();
    m.files[0].imports.insert("evil.dll".into());
    assert!(m.validate().is_err());
    let mut m = manifest();
    m.system_imports.insert("../evil.dll".into());
    assert!(m.validate().is_err());
    let mut m = manifest();
    m.load_order[1] = m.load_order[0].clone();
    assert!(m.validate().is_err());
    let mut m = manifest();
    m.static_core_build = Sha256Digest::from_bytes([3; 32]);
    assert!(m.validate().is_err());
    Ok(())
}
#[test]
fn pe_parser_refuses_invalid_or_truncated_images() {
    for b in [b"".as_slice(), b"MZ", b"MZnot a native DLL"] {
        assert!(pe::imports(b).is_err());
    }
}
#[test]
fn pe_import_census_reads_literal_import_and_rejects_va_delay_descriptor() -> Result<()> {
    let mut b = vec![0u8; 1024];
    b[0..2].copy_from_slice(b"MZ");
    b[0x3c..0x40].copy_from_slice(&64u32.to_le_bytes());
    b[64..68].copy_from_slice(b"PE\0\0");
    b[68..70].copy_from_slice(&0x8664u16.to_le_bytes());
    b[70..72].copy_from_slice(&1u16.to_le_bytes());
    b[84..86].copy_from_slice(&240u16.to_le_bytes());
    b[88..90].copy_from_slice(&0x20bu16.to_le_bytes());
    b[148..152].copy_from_slice(&1024u32.to_le_bytes());
    b[196..200].copy_from_slice(&16u32.to_le_bytes());
    b[208..212].copy_from_slice(&512u32.to_le_bytes());
    b[212..216].copy_from_slice(&40u32.to_le_bytes());
    b[524..528].copy_from_slice(&600u32.to_le_bytes());
    b[600..613].copy_from_slice(b"kernel32.dll\0");
    assert_eq!(
        pe::imports(&b)?,
        (BTreeSet::from(["kernel32.dll".into()]), BTreeSet::new())
    );
    b[304..308].copy_from_slice(&640u32.to_le_bytes());
    b[308..312].copy_from_slice(&64u32.to_le_bytes());
    b[644..648].copy_from_slice(&600u32.to_le_bytes());
    assert!(pe::imports(&b).is_err());
    Ok(())
}

#[test]
fn native_fault_classifier_is_typed_and_excludes_input_source_deadline_errors() {
    use ort::ErrorCode;
    for code in [
        ErrorCode::ExecutionProviderFailure,
        ErrorCode::RuntimeException,
        ErrorCode::EngineError,
    ] {
        let e = run_failure(ort::Error::new_with_code(
            code,
            "literal native execution fault",
        ));
        assert!(super::super::is_native_device_failure(&e));
    }
    for code in [
        ErrorCode::InvalidArgument,
        ErrorCode::InvalidGraph,
        ErrorCode::InvalidProtobuf,
        ErrorCode::NoSuchFile,
    ] {
        let e = run_failure(ort::Error::new_with_code(
            code,
            "CUDA failure text cannot change class",
        ));
        assert!(!super::super::is_native_device_failure(&e));
    }
    for text in [
        "original deadline expired",
        "authority changed",
        "source digest differs",
        "cancelled",
        "CUDA allocation failed",
    ] {
        assert!(!super::super::is_native_device_failure(&anyhow::anyhow!(
            text
        )));
    }
}
#[test]
fn explicit_zero_incremental_requirement_is_not_missing_or_full_peak() {
    assert!(DeviceCapacityPlan::new(100, 0, 20).is_ok());
    let mut c = Claims::default();
    let id = c.reserve(20, 0, 20).unwrap();
    assert_eq!(c.unspent[&id], (0, 20));
    assert!(c.reserve(19, 0, 20).is_err());
}

#[test]
fn missing_or_invalid_runtime_approval_is_not_a_native_execution_failure() {
    let error = manifest::RuntimeManifest::approved(b"{}").err().unwrap();
    assert!(!super::super::is_native_device_failure(&error));
    assert!(!super::super::is_native_device_failure(
        &anyhow::Error::new(RuntimeUnavailable::new("literal unavailable"))
    ));
}

#[test]
fn native_broker_lease_identity_is_actual_arc_not_equal_payload() {
    let a: Arc<dyn Send + Sync> = Arc::new(String::from("same"));
    let b = a.clone();
    let c: Arc<dyn Send + Sync> = Arc::new(String::from("same"));
    assert!(same_native_lease(&a, &b));
    assert!(!same_native_lease(&a, &c));
}

#[test]
fn native_probe_calibration_has_complete_literal_unavailable_dispositions() {
    use airk_core::arbiter::context_action::calibration::{ClassSupport, ContextActionClass};
    let rows = unavailable_classes();
    assert_eq!(
        rows.iter().map(|v| v.action).collect::<Vec<_>>(),
        [
            ContextActionClass::Read,
            ContextActionClass::Sufficient,
            ContextActionClass::Clarify,
            ContextActionClass::NoJustifiedRead
        ]
    );
    assert!(
        rows.iter()
            .all(|v| matches!(&v.support,ClassSupport::Unsupported{reason} if !reason.is_empty()))
    );
}

#[test]
fn native_probe_runtime_metadata_checks_exact_pin_and_original_byte_cap() -> Result<()> {
    let value = manifest();
    let raw = serde_json::to_vec(&value)?;
    let pin = Sha256Digest::from_bytes(Sha256::digest(&raw).into());
    assert_eq!(probe_runtime_identity(&raw, pin)?, value.static_core_build);
    assert!(probe_runtime_identity(&raw, Sha256Digest::from_bytes([9; 32])).is_err());
    let oversized = vec![b' '; (128 << 10) + 1];
    let pin = Sha256Digest::from_bytes(Sha256::digest(&oversized).into());
    assert!(probe_runtime_identity(&oversized, pin).is_err());
    Ok(())
}

#[cfg(feature = "test-support")]
#[test]
fn shared_native_focus_probe_metadata_keeps_literal_raw_binary_readout() -> Result<()> {
    let profile = super::super::catalog_profiles()?
        .into_iter()
        .find(|p| p.model == airk_core::arbiter::ModelChoice::FocusQwenReranker06B)
        .context("Focus descriptor")?;
    let policy = profile.task_policies[&airk_core::arbiter::Task::FocusRank];
    let raw = super::super::qualification::focus_raw_metadata(&profile, policy)?;
    let value: serde_json::Value = serde_json::from_slice(&raw)?;
    assert_eq!(value["schema"], "airk.private.focus-relevance-metadata/1");
    assert_eq!(value["readout_order"], serde_json::json!(["no", "yes"]));
    assert_eq!(value["readout_tokens"], serde_json::json!([2152, 9693]));
    assert_eq!(value["learned_probability_calibration"], false);
    assert_eq!(value["release_qualified"], false);
    Ok(())
}

#[cfg(feature = "test-support")]
#[test]
fn native_probe_builder_returns_new_bound_unavailable_cal_without_old_rules() -> Result<()> {
    use airk_core::arbiter::{
        AssetRole, EvidenceRole, Gap, ModelChoice, Stage, Task, context_action::calibration::*,
        context_action::*,
    };
    let mut cpu = super::super::catalog_profiles()?
        .into_iter()
        .find(|p| p.model == ModelChoice::ContextQwenCompact)
        .context("Compact descriptor")?;
    cpu.profile_id = "private-synthetic-cuda-metadata-control".into();
    cpu.manifest = Sha256Digest::from_bytes([8; 32]);
    cpu.max_tokens = 8192;
    cpu.max_candidates = 8;
    cpu.stage = Some(Stage::Context);
    cpu.tasks = BTreeSet::from([Task::ContextAction]);
    cpu.policy = crate::arbiter::compiler::qwen::policy_digest()?;
    let task = crate::arbiter::context_action::policy_digest()?;
    cpu.task_policies = BTreeMap::from([(Task::ContextAction, task)]);
    cpu.downloads.clear();
    cpu.qualified_targets.clear();
    cpu.qualification = None;
    let original = JointCalibration {
        schema: CALIBRATION_SCHEMA.into(),
        model: cpu.model,
        compiler: cpu.compiler,
        task_policy: task,
        score_abi: cpu.abi.clone(),
        assets: cpu
            .assets
            .iter()
            .filter(|a| {
                matches!(
                    a.role,
                    AssetRole::Model | AssetRole::ExternalWeights | AssetRole::Tokenizer
                )
            })
            .map(|a| {
                (
                    a.name.clone(),
                    JointAssetBinding {
                        role: a.role.clone(),
                        sha256: a.sha256,
                    },
                )
            })
            .collect(),
        rules: vec![JointRule {
            action: ContextActionClass::Read,
            allowed_gaps: vec![Gap::Unsupported],
            allowed_reasons: vec![ContextReason::AdditionalEvidenceMayHelp],
            allowed_roles: vec![EvidenceRole::Unknown],
            allowed_relevance: vec![RelevanceHypothesis::Uncertain],
            minimum_raw_margin: f32::MAX,
        }],
        class_support: None,
    };
    let raw = serde_json::to_vec_pretty(&original)?;
    let digest = Sha256Digest::from_bytes(Sha256::digest(&raw).into());
    let asset = cpu
        .assets
        .iter_mut()
        .find(|a| a.role == AssetRole::Calibration)
        .unwrap();
    asset.bytes = raw.len() as u64;
    asset.sha256 = digest;
    cpu.calibration = digest;
    cpu.validate()?;
    original.validate(&cpu, task)?;
    struct CalibrationOnly(Vec<u8>);
    impl super::super::AssetSource for CalibrationOnly {
        fn read_asset(&self, asset: &airk_core::arbiter::Asset) -> Result<Vec<u8>> {
            ensure!(
                asset.role == AssetRole::Calibration,
                "pure metadata control must not read neural assets"
            );
            Ok(self.0.clone())
        }
    }
    let source = CalibrationOnly(raw.clone());
    let runtime = serde_json::to_vec(&manifest())?;
    let runtime_pin = Sha256Digest::from_bytes(Sha256::digest(&runtime).into());
    let plan = DeviceCapacityPlan::probe(12 << 30, 1 << 30, 2 << 30)?;
    let result = prepare_fp32_probe_profile(&cpu, &source, &runtime, runtime_pin, plan)?;
    let cal =
        JointCalibration::parse_and_validate(&result.calibration_bytes, &result.profile, task)?;
    assert_eq!(source.0, raw);
    assert_eq!(original.rules.len(), 1);
    assert!(cal.rules.is_empty());
    assert_eq!(cal.class_support.as_ref().unwrap().len(), 4);
    assert!(
        cal.class_support
            .unwrap()
            .iter()
            .all(|r| matches!(r.support, ClassSupport::Unsupported { .. }))
    );
    let pin = Sha256Digest::from_bytes(Sha256::digest(&result.calibration_bytes).into());
    assert_eq!(result.profile.calibration, pin);
    assert_ne!(pin, cpu.calibration);
    let asset = result
        .profile
        .assets
        .iter()
        .find(|a| a.role == AssetRole::Calibration)
        .unwrap();
    assert_eq!(asset.sha256, pin);
    assert_eq!(asset.bytes, result.calibration_bytes.len() as u64);
    assert_eq!(asset.name, result.calibration_name);
    assert_ne!(result.profile.manifest, cpu.manifest);
    assert!(
        result.profile.qualification.is_none()
            && result.profile.qualified_targets.is_empty()
            && result.profile.downloads.is_empty()
    );
    assert_eq!(
        result
            .profile
            .assets
            .iter()
            .filter(|a| a.role != AssetRole::Calibration)
            .collect::<Vec<_>>(),
        cpu.assets
            .iter()
            .filter(|a| a.role != AssetRole::Calibration)
            .collect::<Vec<_>>()
    );
    cpu.assets
        .iter_mut()
        .find(|a| a.role == AssetRole::Model)
        .unwrap()
        .sha256 = Sha256Digest::from_bytes([9; 32]);
    assert!(prepare_fp32_probe_profile(&cpu, &source, &runtime, runtime_pin, plan).is_err());
    Ok(())
}

fn product_manifest() -> manifest::ProductRuntimeManifest {
    let old = manifest();
    let identity = "literal-native-product-identity".to_owned();
    manifest::ProductRuntimeManifest {
        schema: "airk.native.cuda-runtime-manifest/2".into(),
        ort_version: "1.30.0".into(),
        static_core_build: Sha256Digest::from_bytes([2; 32]),
        provenance: airk_core::arbiter::Asset {
            name: "airk-cuda-build-provenance.json".into(),
            role: airk_core::arbiter::AssetRole::Notice,
            bytes: 12,
            sha256: Sha256Digest::from_bytes([3; 32]),
        },
        files: old.files,
        load_order: old.load_order,
        system_imports: old.system_imports,
        requirements: vec![manifest::VariantRequirement {
            manifest: Sha256Digest::from_bytes([4; 32]),
            execution_identity: identity.clone(),
            target: "x86_64-pc-windows-msvc".into(),
            compute_capabilities: vec![[8, 6]],
            minimum_driver_api: 12080,
            cold_peak_limit_bytes: 100,
            warm_extra_limit_bytes: 20,
            headroom_bytes: 10,
            host_warm_extra_limit_bytes: 1,
            artifact_evidence: DeviceEvidence {
                target: "x86_64-pc-windows-msvc".into(),
                execution_identity: identity,
                numerical: Sha256Digest::from_bytes([5; 32]),
                resources: Sha256Digest::from_bytes([6; 32]),
                placement: Sha256Digest::from_bytes([7; 32]),
            },
            acceptance_fixture: airk_core::arbiter::Asset {
                name: "synthetic-input.json".into(),
                role: airk_core::arbiter::AssetRole::Fixture,
                bytes: 15,
                sha256: Sha256Digest::from_bytes([8; 32]),
            },
        }],
    }
}
#[test]
fn product_manifest_is_distinct_strict_and_has_no_scientific_fit_transfer() -> Result<()> {
    let product = product_manifest();
    product.validate()?;
    let raw = serde_json::to_vec(&product)?;
    let parsed = manifest::RuntimeDescriptor::parse(&raw)?;
    assert!(parsed.is_product());
    assert!(parsed.fits().is_err());
    let mut mixed = serde_json::to_value(&product)?;
    mixed["fits"] = serde_json::json!([]);
    assert!(manifest::RuntimeDescriptor::parse(&serde_json::to_vec(&mixed)?).is_err());
    let mut legacy = serde_json::to_value(manifest())?;
    legacy["schema"] = serde_json::json!("airk.native.cuda-runtime-manifest/2");
    assert!(manifest::RuntimeDescriptor::parse(&serde_json::to_vec(&legacy)?).is_err());
    let mut altered = product.clone();
    altered.requirements[0].compute_capabilities = vec![[8, 9]];
    assert!(altered.validate().is_err());
    let mut altered = product.clone();
    altered.requirements[0].artifact_evidence.resources =
        altered.requirements[0].artifact_evidence.numerical;
    assert!(altered.validate().is_err());
    let mut altered = product;
    altered.provenance.name = "private-producer.json".into();
    assert!(altered.validate().is_err());
    Ok(())
}
#[test]
fn product_compatibility_is_not_reference_uuid_or_driver_image_equality() -> Result<()> {
    let product = product_manifest();
    let row = &product.requirements[0];
    let mut actual = AdapterIdentity {
        uuid: [11; 16],
        ordinal: 3,
        compute_major: 8,
        compute_minor: 6,
        driver_api_version: 12080,
        driver_image_sha256: Sha256Digest::from_bytes([12; 32]),
        physical_total_bytes: 110,
        dedicated_memory: true,
    };
    row.supports(&actual)?;
    actual.uuid = [13; 16];
    actual.driver_image_sha256 = Sha256Digest::from_bytes([14; 32]);
    row.supports(&actual)?;
    for (major, minor) in [(7, 5), (8, 0), (8, 7), (8, 9), (9, 0)] {
        actual.compute_major = major;
        actual.compute_minor = minor;
        assert!(row.supports(&actual).is_err());
    }
    actual.compute_major = 8;
    actual.compute_minor = 6;
    actual.physical_total_bytes = 109;
    assert!(is_device_capacity_failure(
        &row.supports(&actual).unwrap_err()
    ));
    actual.physical_total_bytes = 110;
    actual.driver_api_version = 12079;
    assert!(!is_device_capacity_failure(
        &row.supports(&actual).unwrap_err()
    ));
    actual.driver_api_version = 12080;
    actual.dedicated_memory = false;
    assert!(row.supports(&actual).is_err());
    Ok(())
}

#[test]
fn qualified_scientific_route_preserves_scoring_while_product_requires_live_eligibility()
-> Result<()> {
    let legacy = manifest::RuntimeDescriptor::Scientific(manifest());
    let scientific = eligibility::initial_admission(&legacy);
    assert!(matches!(
        scientific,
        eligibility::AdmissionState::Scientific
    ));
    scientific.authorize(Sha256Digest::from_bytes([9; 32]))?;
    let product = manifest::RuntimeDescriptor::Product(product_manifest());
    let product = eligibility::initial_admission(&product);
    assert!(matches!(product, eligibility::AdmissionState::Pending));
    assert!(
        product
            .authorize(Sha256Digest::from_bytes([9; 32]))
            .is_err()
    );
    Ok(())
}

#[cfg(feature = "test-support")]
#[test]
fn package_projection_refuses_unpinned_nonabsolute_and_aliased_descriptor_requests() {
    let sha = Sha256Digest::from_bytes([1; 32]);
    assert!(
        package_request(
            Path::new("relative"),
            Path::new("relative/descriptor.json"),
            sha
        )
        .is_err()
    );
    let absolute = std::env::current_dir().unwrap();
    assert!(package_request(&absolute, &absolute.join("../descriptor.json"), sha).is_err());
    assert!(package_request(&absolute, &absolute.join("nested/descriptor.json"), sha).is_err());
    assert!(package_request(&absolute, &absolute.join("DESCRIPTOR.json"), sha).is_err());
    assert!(
        package_request(
            &absolute,
            &absolute.join("descriptor.json"),
            Sha256Digest::from_bytes([0; 32])
        )
        .is_err()
    );
}
