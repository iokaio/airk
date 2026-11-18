// SPDX-License-Identifier: Apache-2.0
//! Literal synthetic refusal controls, never model qualification.
use super::*;
fn legacy_focus() -> Result<Profile> {
    let catalog: serde_json::Value =
        serde_json::from_slice(include_bytes!("../../release-catalog.json"))?;
    serde_json::from_value(catalog["profiles"][0].clone()).map_err(Into::into)
}
#[test]
fn exact_legacy_descriptor_and_public_sources_remain_byte_stable() -> Result<()> {
    let profile = legacy_focus()?;
    let before = serde_json::to_vec(&profile)?;
    assert_eq!(
        profile_sha(&profile)?.to_string(),
        "73d884fb1c2b37709d4d1b1daa93fd65687ce310cbd4096af0ef106a615e608c"
    );
    let witness = CpuSourceTransition::admit(&profile)?;
    witness.validate(&profile)?;
    let record = serde_json::to_value(witness.record(&profile)?)?;
    assert_eq!(
        record["legacy_manifest"],
        "04821edaed45ac10aa5b0b7b2c0b7b28bf5d04a06c8b623cfe6d762f77d5e23b"
    );
    assert_eq!(
        record["legacy_task_policy"],
        "cfbcbe4cce111b9f802918488dae82feefb075666a0a7e7a9f93dd082b71c331"
    );
    assert_eq!(
        record["current_task_policy"],
        "25c96233573bd34cd71d03317bc757677d31e48ffbe362c462499d5fb1b7eff7"
    );
    assert_eq!(
        record["current_core_semantics"],
        "914692048e6d833e0d18c10485e7d9f57f2e0c8874f428b31002bcf7c09cdeef"
    );
    assert_eq!(record["task_qualification_transferred"], false);
    assert_eq!(record["public_source_rewritten"], false);
    assert_eq!(serde_json::to_vec(&profile)?, before);
    Ok(())
}
#[test]
fn closed_profile_refuses_neural_metadata_fit_and_source_drift() -> Result<()> {
    let profile = legacy_focus()?;
    for mutation in 0..15 {
        let mut changed = profile.clone();
        match mutation {
            0 => changed.manifest = Sha256Digest::from_bytes([1; 32]),
            1 => changed.compiler = Sha256Digest::from_bytes([2; 32]),
            2 => changed.calibration = Sha256Digest::from_bytes([3; 32]),
            3 => {
                changed
                    .assets
                    .iter_mut()
                    .find(|a| a.role == AssetRole::Model)
                    .unwrap()
                    .sha256 = Sha256Digest::from_bytes([4; 32])
            }
            4 => {
                changed
                    .assets
                    .iter_mut()
                    .find(|a| a.role == AssetRole::ExternalWeights)
                    .unwrap()
                    .sha256 = Sha256Digest::from_bytes([5; 32])
            }
            5 => {
                changed
                    .assets
                    .iter_mut()
                    .find(|a| a.role == AssetRole::Tokenizer)
                    .unwrap()
                    .sha256 = Sha256Digest::from_bytes([6; 32])
            }
            6 => changed.abi.readout = Readout::PointerHead,
            7 => changed.cpu_fit.as_mut().unwrap().intra_threads = 8,
            8 => changed.max_tokens = 8192,
            9 => changed.runtime = "onnxruntime-cuda/1.30.0".into(),
            10 => changed.downloads[0].parts[0].url.push_str("?changed"),
            11 => changed.qualification = Some(Sha256Digest::from_bytes([7; 32])),
            12 => {
                changed
                    .task_policies
                    .insert(Task::FocusRank, Sha256Digest::from_bytes([8; 32]));
            }
            13 => changed.memory_reservation_bytes += 1,
            14 => changed.model = ModelChoice::ContextQwenCompact,
            _ => unreachable!(),
        }
        assert!(
            CpuSourceTransition::admit(&changed).is_err(),
            "mutation {mutation}"
        );
    }
    Ok(())
}
#[test]
fn source_row_refuses_wrong_current_core_or_task_policy() -> Result<()> {
    validate_current_sources(digest(CURRENT_CORE)?, digest(CURRENT_TASK)?)?;
    assert!(validate_current_sources(digest(LEGACY_CORE)?, digest(CURRENT_TASK)?).is_err());
    assert!(validate_current_sources(digest(CURRENT_CORE)?, digest(LEGACY_TASK)?).is_err());
    assert!(
        validate_current_sources(
            digest("77fa25e4623f32e568a7163553cf7531cd4425dbb157ea2b67fef7940998c57f")?,
            digest(CURRENT_TASK)?,
        )
        .is_err()
    );
    assert!(
        validate_current_sources(
            digest(CURRENT_CORE)?,
            digest("cc25cb7e3a3b6ea4cb78f232c0c1bf6764005e79775af20a1d0b3aa5d8d1bdab")?,
        )
        .is_err()
    );
    assert!(
        validate_current_sources(Sha256Digest::from_bytes([0; 32]), digest(CURRENT_TASK)?).is_err()
    );
    Ok(())
}
#[test]
fn transition_never_admits_context_or_relabels_a_current_profile() -> Result<()> {
    let catalog: serde_json::Value =
        serde_json::from_slice(include_bytes!("../../release-catalog.json"))?;
    for index in [1, 2] {
        let profile: Profile = serde_json::from_value(catalog["profiles"][index].clone())?;
        assert!(CpuSourceTransition::admit(&profile).is_err());
    }
    let mut relabeled = legacy_focus()?;
    relabeled.policy = digest(CURRENT_TASK)?;
    relabeled
        .task_policies
        .insert(Task::FocusRank, digest(CURRENT_TASK)?);
    assert!(CpuSourceTransition::admit(&relabeled).is_err());
    Ok(())
}
#[test]
fn witness_rechecks_the_exact_descriptor_and_keeps_new_input_provenance() -> Result<()> {
    let profile = legacy_focus()?;
    let witness = CpuSourceTransition::admit(&profile)?;
    let mut wrong_row = witness.clone();
    wrong_row.row = Sha256Digest::from_bytes([0x99; 32]);
    assert!(wrong_row.validate(&profile).is_err());
    let first =
        serde_json::to_value(witness.bind_input(&profile, Sha256Digest::from_bytes([0x31; 32]))?)?;
    let second =
        serde_json::to_value(witness.bind_input(&profile, Sha256Digest::from_bytes([0x32; 32]))?)?;
    assert_ne!(first["binding"], second["binding"]);
    assert_ne!(first["binding"], first["current_compiled_input"]);
    assert_eq!(first["transition"]["row"], second["transition"]["row"]);
    assert!(
        witness
            .bind_input(&profile, Sha256Digest::from_bytes([0; 32]))
            .is_err()
    );
    let mut changed = profile.clone();
    changed.peak_disk_bytes += 1;
    assert!(witness.validate(&changed).is_err());
    assert!(
        witness
            .bind_input(&changed, Sha256Digest::from_bytes([0x31; 32]))
            .is_err()
    );
    Ok(())
}
#[test]
fn native_version_scope_is_pre_190_exact_release_only() {
    for version in ["1.8.0", "1.8.207"] {
        assert!(super::super::local_test_series(version));
    }
    for version in [
        "1.9.0",
        "1.9.1",
        "2.0.0",
        "1.8.207-gpu",
        "01.8.207",
        "1.8",
        "garbage",
    ] {
        assert!(
            !super::super::local_test_series(version),
            "version {version}"
        );
    }
}

#[test]
fn current_runtime_predicate_preserves_the_exact_legacy_cpu_literal() -> Result<()> {
    let profile = legacy_focus()?;
    assert_eq!(profile.advice_runtime()?, "onnxruntime-cpu/1.30.0");
    assert!(airk_core::arbiter::device::valid_advice_runtime(
        "onnxruntime-cpu/1.30.0"
    ));
    for runtime in [
        "onnxruntime-cpu/1.30.0/changed",
        "onnxruntime-cpu/1.31.0",
        "onnxruntime-cuda/1.30.0",
    ] {
        assert!(!airk_core::arbiter::device::valid_advice_runtime(runtime));
    }
    Ok(())
}

#[test]
fn publisher_census_row_refuses_old_task_and_preserves_legacy_payload() -> Result<()> {
    assert_eq!(
        source_sha(include_bytes!("../../compiler/focus.rs"))?.to_string(),
        "a58243020b534c08c7cc049cb9155d4bdfe4e220f96112967a048692a0813015"
    );
    assert!(
        validate_current_sources(
            digest("77fa25e4623f32e568a7163553cf7531cd4425dbb157ea2b67fef7940998c57f")?,
            digest("5add2b52410e5c891aac185c130535ea59451034e51835a4ceee6f9bba1944ee")?,
        )
        .is_err()
    );
    let profile = legacy_focus()?;
    let original = serde_json::to_vec(&profile)?;
    let metadata = serde_json::to_value(&profile)?;
    let witness = CpuSourceTransition::admit(&profile)?;
    let record = serde_json::to_value(witness.record(&profile)?)?;
    assert_eq!(
        record["current_task_policy"],
        "25c96233573bd34cd71d03317bc757677d31e48ffbe362c462499d5fb1b7eff7"
    );
    assert_eq!(
        record["legacy_task_policy"],
        "cfbcbe4cce111b9f802918488dae82feefb075666a0a7e7a9f93dd082b71c331"
    );
    assert_eq!(record["task_qualification_transferred"], false);
    assert_eq!(record["public_source_rewritten"], false);
    let after = serde_json::to_value(&profile)?;
    for field in ["calibration", "assets", "downloads"] {
        assert_eq!(after[field], metadata[field], "legacy {field} changed");
    }
    assert_eq!(serde_json::to_vec(&profile)?, original);
    assert_eq!(
        profile_sha(&profile)?.to_string(),
        "73d884fb1c2b37709d4d1b1daa93fd65687ce310cbd4096af0ef106a615e608c"
    );
    Ok(())
}

#[test]
fn cuda_bridge_and_driver_closure_preserve_closed_cpu_runtime_source() -> Result<()> {
    assert_eq!(
        source_sha(include_bytes!("../../runtime.rs"))?.to_string(),
        "6bb97b2f5ccaba894b304ae6ec3f1b0f8260bbc2b6e3a2c19c427227af0fcd8f"
    );
    assert_eq!(
        source_sha(include_bytes!("../cuda/windows.rs"))?.to_string(),
        "1f577d70657997313743f1c973a3433a575a485d218497abf9080a9079e4ff51"
    );
    assert_eq!(
        source_sha(include_bytes!("../cuda/modules.rs"))?.to_string(),
        "171be06a31093b1d93a6b8db0dcbf1408eab375937fd751d2ccd6226dbbbc3a9"
    );
    let profile = legacy_focus()?;
    let before = serde_json::to_vec(&profile)?;
    let witness = CpuSourceTransition::admit(&profile)?;
    let record = serde_json::to_value(witness.record(&profile)?)?;
    assert_eq!(
        record["current_runtime_source"],
        "6bb97b2f5ccaba894b304ae6ec3f1b0f8260bbc2b6e3a2c19c427227af0fcd8f"
    );
    assert_eq!(
        record["current_task_policy"],
        "25c96233573bd34cd71d03317bc757677d31e48ffbe362c462499d5fb1b7eff7"
    );
    assert_eq!(record["task_qualification_transferred"], false);
    assert_eq!(record["public_source_rewritten"], false);
    assert_eq!(serde_json::to_vec(&profile)?, before);
    Ok(())
}

#[test]
fn placement_diagnostic_supplement_binds_current_runtime_without_qualification_transfer()
-> Result<()> {
    assert_eq!(
        source_sha(include_bytes!("../../runtime.rs"))?.to_string(),
        "6bb97b2f5ccaba894b304ae6ec3f1b0f8260bbc2b6e3a2c19c427227af0fcd8f"
    );
    assert_eq!(
        source_sha(include_bytes!("../cuda/diagnostic.rs"))?.to_string(),
        "8b1e5334f3205367df116764dba85652f27171966567d214e37e81115c969495"
    );
    assert_eq!(
        source_sha(include_bytes!("../cuda.rs"))?.to_string(),
        "31fdb63b4b388f71900703fc627be9e75598e716d6db50020a7ee1286c9d0dc4"
    );
    assert_ne!(
        CURRENT_RUNTIME_SOURCE,
        Some("9e539508f52e403368124ee38f91085f4b318f87fcffe45f898728cf9c0db49d")
    );
    let profile = legacy_focus()?;
    let original = serde_json::to_vec(&profile)?;
    let record = serde_json::to_value(CpuSourceTransition::admit(&profile)?.record(&profile)?)?;
    assert_eq!(record["task_qualification_transferred"], false);
    assert_eq!(record["public_source_rewritten"], false);
    assert_eq!(serde_json::to_vec(&profile)?, original);
    Ok(())
}

#[test]
fn final_graph_metadata_supplement_binds_current_source_without_policy_admission() -> Result<()> {
    assert_eq!(
        source_sha(include_bytes!("../../runtime.rs"))?.to_string(),
        "6bb97b2f5ccaba894b304ae6ec3f1b0f8260bbc2b6e3a2c19c427227af0fcd8f"
    );
    assert_eq!(
        source_sha(include_bytes!("../cuda/graph_metadata.rs"))?.to_string(),
        "022f8432d0c54cefb130fa8b84ced38579e768f0b8545e3c3f86bee4d003f708"
    );
    assert_ne!(
        CURRENT_RUNTIME_SOURCE,
        Some("a59c063c5d08e1bf5adc7c531336c2715078adeb50d3f9b90faa8f93951b3ff5")
    );
    let profile = legacy_focus()?;
    let before = serde_json::to_vec(&profile)?;
    let record = serde_json::to_value(CpuSourceTransition::admit(&profile)?.record(&profile)?)?;
    assert_eq!(record["task_qualification_transferred"], false);
    assert_eq!(record["public_source_rewritten"], false);
    assert_eq!(serde_json::to_vec(&profile)?, before);
    Ok(())
}
