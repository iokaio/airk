// SPDX-License-Identifier: Apache-2.0
use super::*;
fn sha(n: u8) -> Sha256Digest {
    Sha256Digest::from_bytes([n; 32])
}
#[test]
fn native_key_requires_all_actual_facts_and_nonce() -> Result<()> {
    let key = EligibilityKey::new(sha(1), sha(2), sha(3), Incarnation::from_bytes([4; 16]))?;
    assert_eq!(key, key.clone());
    for (a, b, c, n) in [
        (sha(0), sha(2), sha(3), [4; 16]),
        (sha(1), sha(0), sha(3), [4; 16]),
        (sha(1), sha(2), sha(0), [4; 16]),
        (sha(1), sha(2), sha(3), [0; 16]),
    ] {
        assert!(EligibilityKey::new(a, b, c, Incarnation::from_bytes(n)).is_err());
    }
    assert_ne!(
        key,
        EligibilityKey::new(sha(1), sha(2), sha(5), Incarnation::from_bytes([4; 16]))?
    );
    Ok(())
}
#[test]
fn pending_and_foreign_thread_cannot_dispatch_management_inputs() -> Result<()> {
    assert!(AdmissionState::Pending.authorize(sha(1)).is_err());
    let mut science = AdmissionState::Scientific;
    science.revoke();
    science.authorize(sha(1))?;
    let mut state = AdmissionState::Validating {
        thread: std::thread::current().id(),
        input: None,
    };
    assert!(state.authorize(sha(1)).is_err());
    state = AdmissionState::Validating {
        thread: std::thread::current().id(),
        input: Some(sha(1)),
    };
    state.authorize(sha(1))?;
    assert!(state.authorize(sha(2)).is_err());
    assert!(
        std::thread::spawn(move || state.authorize(sha(1)))
            .join()
            .unwrap()
            .is_err()
    );
    Ok(())
}
#[test]
fn saved_witness_with_no_actual_device_or_session_never_becomes_current() -> Result<()> {
    let key = EligibilityKey::new(sha(1), sha(2), sha(3), Incarnation::from_bytes([4; 16]))?;
    let witness = Arc::new(DeviceEligibility {
        key: key.clone(),
        device: Weak::new(),
        epoch: 1,
        uuid: [1; 16],
        driver_api: 12080,
        driver_image: sha(5),
        runtime_manifest: sha(6),
        profile: sha(7),
        fixture: sha(8),
        valid: AtomicBool::new(true),
        session_alive: Weak::new(),
        state: Weak::new(),
    });
    assert!(!witness.current(&key));
    let mut state = AdmissionState::Eligible(witness.clone());
    state.revoke();
    assert!(!witness.valid.load(Ordering::Acquire));
    assert!(matches!(state, AdmissionState::Pending));
    Ok(())
}
fn input() -> CompiledInput {
    CompiledInput {
        prompt: "literal synthetic input".into(),
        input_ids: vec![1; 17],
        attention_mask: vec![1; 17],
        position_ids: (0..17).collect(),
        decision_position: 16,
        candidate_positions: vec![32, 33, 34],
        option_mask: vec![true; 3],
        input_digest: sha(1),
    }
}
fn row() -> Row {
    Row {
        name: "literal".into(),
        input: input(),
        reference_same_artifact: vec![0., 1., 2.],
        reference_original_fp32: vec![0., 1., 2.],
    }
}
#[test]
fn numeric_refusal_preserves_original_limits_and_fault_classes() -> Result<()> {
    let row = row();
    check_scores(&[0., 1., 2.], &row, false)?;
    let error = check_scores(&[0., 1., 2.002], &row, false).unwrap_err();
    assert!(is_device_eligibility_failure(&error));
    assert!(!is_device_capacity_failure(&error));
    assert!(!is_device_eligibility_failure(&anyhow::anyhow!(
        "fixture missing/deadline expired/CUDA allocation failed"
    )));
    let mut malformed = row;
    malformed.reference_original_fp32[1] = f32::NAN;
    assert!(!is_device_eligibility_failure(
        &check_scores(&[0., 1., 2.], &malformed, false).unwrap_err()
    ));
    let error = capacity_fault("actual driver error2".into());
    assert!(is_device_capacity_failure(&error));
    assert!(is_device_eligibility_failure(&error));
    assert!(!is_device_capacity_failure(&native_fault(
        "provider fault".into()
    )));
    Ok(())
}
#[test]
fn shared_numeric_owner_retains_literal_softmax_and_first_tie_behavior() -> Result<()> {
    let (raw, mass, winner) = compare_reference_scores(&[0., 0.], &[0., 1.])?;
    assert_eq!(raw, 1.);
    assert!((mass - 0.2310585786300049).abs() < 1e-14);
    assert!(!winner);
    assert_eq!(
        compare_reference_scores(&[2., 2.], &[2., 2.])?,
        (0., 0., true)
    );
    for (a, b) in [
        (vec![], vec![]),
        (vec![1.], vec![1., 2.]),
        (vec![f32::NAN], vec![0.]),
    ] {
        assert!(compare_reference_scores(&a, &b).is_err());
    }
    Ok(())
}
#[cfg(feature = "test-support")]
#[test]
fn complete_fixture_denominator_masks_and_source_are_required_before_native_work() -> Result<()> {
    let mut profile = crate::arbiter::runtime::catalog_profiles()?
        .into_iter()
        .find(|p| p.model == ModelChoice::ContextQwenCompact)
        .context("Compact")?;
    profile.max_tokens = 8192;
    let rows = [(17, 3), (128, 8), (8192, 8)]
        .into_iter()
        .enumerate()
        .map(|(index, (tokens, options))| Row {
            name: format!("literal-{index}"),
            input: CompiledInput {
                prompt: String::new(),
                input_ids: vec![1; tokens],
                attention_mask: vec![1; tokens],
                position_ids: (0..tokens as i64).collect(),
                decision_position: tokens as i64 - 1,
                candidate_positions: (32..32 + options as i64).collect(),
                option_mask: vec![true; options],
                input_digest: sha(1),
            },
            reference_same_artifact: vec![0.; options],
            reference_original_fp32: vec![0.; options],
        })
        .collect();
    let mut fixture = Fixture {
        schema: "airk.arbiter.device-acceptance-input/1".into(),
        model: profile.model,
        compiler: profile.compiler,
        task_policy: profile.task_policies[&Task::ContextAction],
        trained_assets: profile
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
        resource_policy: ResourcePolicy::software_v2().digest()?,
        independent_reference_source: sha(7),
        rows,
    };
    fixture.validate(&profile)?;
    fixture.rows[2].input.attention_mask[8191] = 0;
    assert!(fixture.validate(&profile).is_err());
    fixture.rows[2].input.attention_mask[8191] = 1;
    fixture.rows[2].reference_original_fp32.pop();
    assert!(fixture.validate(&profile).is_err());
    fixture.rows[2].reference_original_fp32.push(0.);
    fixture.rows[2].input.position_ids[8191] = 0;
    assert!(fixture.validate(&profile).is_err());
    fixture.rows[2].input.position_ids[8191] = 8191;
    fixture.independent_reference_source = sha(0);
    assert!(fixture.validate(&profile).is_err());
    fixture.independent_reference_source = sha(7);
    fixture.trained_assets.clear();
    assert!(fixture.validate(&profile).is_err());
    Ok(())
}
