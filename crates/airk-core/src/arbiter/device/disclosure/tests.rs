// SPDX-License-Identifier: Apache-2.0
//! Literal sentences. Each expected string is written by hand from the typed
//! facts of its fixture.
use super::*;
use crate::arbiter::device::tests::literal_profile;

fn id(value: u8) -> Sha256Digest {
    Sha256Digest::from_bytes([value; 32])
}
/// A CPU tier with a literal token budget and thread count.
fn tier(model: SelectedModel, manifest: u8, max_tokens: usize, threads: usize) -> Result<Profile> {
    let mut profile = literal_profile(model, manifest, false)?;
    profile.max_tokens = max_tokens;
    profile.cpu_fit = Some(CpuFit {
        minimum_physical_cores: threads,
        intra_threads: threads,
        inter_threads: 1,
        sequential: true,
        spinning: false,
    });
    Ok(profile)
}
fn cuda(model: SelectedModel, manifest: u8, max_tokens: usize) -> Result<Profile> {
    let mut profile = literal_profile(model, manifest, true)?;
    profile.max_tokens = max_tokens;
    Ok(profile)
}
fn selection(
    model: SelectedModel,
    policy: DevicePolicy,
    selected: Option<&Profile>,
    reasons: Vec<CandidateRejection>,
    operation_failure: Option<SelectionReason>,
) -> Result<DeviceSelection> {
    Ok(DeviceSelection {
        schema: DEVICE_STATUS_SCHEMA.into(),
        model,
        policy,
        gpu_precision: GpuPrecisionPreference::Auto,
        selected: match selected {
            Some(profile) => Some(SelectedVariant {
                manifest: profile.manifest,
                execution_identity: profile.device_execution_identity()?,
                backend: profile.device_execution.as_ref().map(|row| row.backend),
                precision: profile.device_execution.as_ref().map(|row| row.precision),
                adapter_key: "literal".into(),
                capability_epoch: 1,
            }),
            None => None,
        },
        reasons,
        operation_failure,
    })
}
fn global(reason: SelectionReason) -> CandidateRejection {
    CandidateRejection {
        manifest: None,
        adapter_key: None,
        reason,
    }
}
fn row(profile: &Profile, reason: SelectionReason) -> CandidateRejection {
    CandidateRejection {
        manifest: Some(profile.manifest),
        adapter_key: Some("literal".into()),
        reason,
    }
}

#[test]
fn fast_tier_after_full_did_not_fit_with_gpu_use_off_is_one_literal_sentence_set() -> Result<()> {
    let full = tier(SelectedModel::Arbiter, 1, 8192, 8)?;
    let fast = tier(SelectedModel::Arbiter, 2, 2048, 8)?;
    let profiles = [full.clone(), fast.clone()];
    let resolve = |pin| profiles.iter().find(|profile| profile.manifest == pin);
    let value = selection(
        SelectedModel::Arbiter,
        DevicePolicy::Auto,
        Some(&fast),
        vec![
            global(SelectionReason::GpuNotOptedIn),
            row(&full, SelectionReason::HostMemoryInsufficient),
        ],
        None,
    )?;
    assert_eq!(
        stage_disclosure(&value, resolve),
        "Arbiter is active on CPU with 8 threads at the 2,048-token budget. GPU use is not turned on in service resources. The 8,192-token CPU tier with 8 threads was not used: host_memory_insufficient."
    );
    Ok(())
}

#[test]
fn explicit_cpu_policy_gpu_route_and_focus_are_literal() -> Result<()> {
    let full = tier(SelectedModel::Arbiter, 1, 8192, 8)?;
    let large = cuda(SelectedModel::Arbiter, 3, 8192)?;
    let focus = tier(SelectedModel::Focus, 4, 1024, 4)?;
    let single = tier(SelectedModel::Focus, 5, 512, 1)?;
    let profiles = [full.clone(), large.clone(), focus.clone(), single.clone()];
    let resolve = |pin| profiles.iter().find(|profile| profile.manifest == pin);
    assert_eq!(
        stage_disclosure(
            &selection(
                SelectedModel::Arbiter,
                DevicePolicy::Cpu,
                Some(&full),
                vec![global(SelectionReason::ForcedCpu)],
                None,
            )?,
            resolve
        ),
        "Arbiter is active on CPU with 8 threads at the 8,192-token budget. The device policy is CPU."
    );
    assert_eq!(
        stage_disclosure(
            &selection(
                SelectedModel::Arbiter,
                DevicePolicy::Auto,
                Some(&large),
                vec![],
                None,
            )?,
            resolve
        ),
        "Arbiter is active on GPU through CUDA at the 8,192-token budget."
    );
    assert_eq!(
        stage_disclosure(
            &selection(
                SelectedModel::Focus,
                DevicePolicy::Auto,
                Some(&focus),
                vec![global(SelectionReason::NoQualifiedGpuRoute)],
                None,
            )?,
            resolve
        ),
        "Focus is active on CPU with 4 threads at the 1,024-token budget. No qualified GPU route exists for this model."
    );
    assert_eq!(
        stage_disclosure(
            &selection(
                SelectedModel::Focus,
                DevicePolicy::Auto,
                Some(&single),
                vec![global(SelectionReason::NoGpu)],
                None,
            )?,
            resolve
        ),
        "Focus is active on CPU with 1 thread at the 512-token budget. No GPU adapter was observed."
    );
    Ok(())
}

#[test]
fn no_fit_and_operation_failure_name_every_cause_and_never_claim_an_active_route() -> Result<()> {
    let full = tier(SelectedModel::Arbiter, 1, 8192, 8)?;
    let fast = tier(SelectedModel::Arbiter, 2, 2048, 8)?;
    let gpu = cuda(SelectedModel::Arbiter, 3, 8192)?;
    let profiles = [full.clone(), fast.clone(), gpu.clone()];
    let resolve = |pin| profiles.iter().find(|profile| profile.manifest == pin);
    assert_eq!(
        stage_disclosure(
            &selection(
                SelectedModel::Arbiter,
                DevicePolicy::Auto,
                None,
                vec![
                    global(SelectionReason::NoQualifiedGpuRoute),
                    row(&full, SelectionReason::CpuFitInsufficient),
                    row(&fast, SelectionReason::HostMemoryInsufficient),
                ],
                None,
            )?,
            resolve
        ),
        "Arbiter is not active. No admitted route fits this system. No qualified GPU route exists for this model. The 8,192-token CPU tier with 8 threads was not used: cpu_fit_insufficient. The 2,048-token CPU tier with 8 threads was not used: host_memory_insufficient."
    );
    assert_eq!(
        stage_disclosure(
            &selection(
                SelectedModel::Arbiter,
                DevicePolicy::Auto,
                None,
                vec![global(SelectionReason::NoQualifiedCpu)],
                None,
            )?,
            resolve
        ),
        "Arbiter is not active. No admitted route fits this system. No CPU tier is admitted for this model."
    );
    // The stopping cause is stated once, in the first sentence.
    assert_eq!(
        stage_disclosure(
            &selection(
                SelectedModel::Arbiter,
                DevicePolicy::Auto,
                None,
                vec![
                    row(&gpu, SelectionReason::ProviderInitializationFailed),
                    global(SelectionReason::DeadlineExpired),
                ],
                Some(SelectionReason::DeadlineExpired),
            )?,
            resolve
        ),
        "Arbiter is not active. The operation stopped and nothing further was tried: deadline_expired. The CUDA route at the 8,192-token budget was not used: provider_initialization_failed."
    );
    // A manifest the catalog no longer describes is named without tier facts.
    assert_eq!(
        stage_disclosure(
            &selection(
                SelectedModel::Arbiter,
                DevicePolicy::Auto,
                Some(&full),
                vec![CandidateRejection {
                    manifest: Some(id(99)),
                    adapter_key: None,
                    reason: SelectionReason::Unqualified,
                }],
                None,
            )?,
            |_| None
        ),
        "Arbiter is active on CPU. A candidate was not used: unqualified."
    );
    Ok(())
}

#[test]
fn a_long_row_list_is_counted_and_never_cut_inside_a_sentence() -> Result<()> {
    let full = tier(SelectedModel::Arbiter, 1, 8192, 8)?;
    let rows = (0..20)
        .map(|manifest| CandidateRejection {
            manifest: Some(id(100 + manifest)),
            adapter_key: Some("cpu".into()),
            reason: SelectionReason::HostMemoryInsufficient,
        })
        .collect::<Vec<_>>();
    let value = selection(SelectedModel::Arbiter, DevicePolicy::Auto, None, rows, None)?;
    let text = stage_disclosure(&value, |_| Some(&full));
    // 58 bytes of head, 80 per cause with its space: eleven causes fit beside
    // the 42-byte note, and nine are counted.
    let cause = " The 8,192-token CPU tier with 8 threads was not used: host_memory_insufficient.";
    let expected = format!(
        "Arbiter is not active. No admitted route fits this system.{} 9 more causes are in the selection rows.",
        cause.repeat(11)
    );
    assert_eq!(text, expected);
    assert_eq!(text.len(), 979);
    assert!(text.len() <= MAX_DISCLOSURE_BYTES);
    assert_eq!(MAX_DISCLOSURE_BYTES, 1024);
    Ok(())
}

#[test]
fn variant_reasons_are_literal_and_carry_the_tier_facts() -> Result<()> {
    let full = tier(SelectedModel::Arbiter, 1, 8192, 8)?;
    let fast = tier(SelectedModel::Arbiter, 2, 2048, 8)?;
    let large = cuda(SelectedModel::Arbiter, 3, 8192)?;
    assert_eq!(
        variant_disclosure(&fast, VariantState::Active, None),
        "Arbiter on CPU with 8 threads at the 2,048-token budget is active."
    );
    assert_eq!(
        variant_disclosure(
            &full,
            VariantState::Admitted,
            Some(SelectionReason::HostMemoryInsufficient)
        ),
        "Arbiter on CPU with 8 threads at the 8,192-token budget is admitted and not active. It was not used: host_memory_insufficient."
    );
    assert_eq!(
        variant_disclosure(&full, VariantState::Admitted, None),
        "Arbiter on CPU with 8 threads at the 8,192-token budget is admitted and not active."
    );
    assert_eq!(
        variant_disclosure(&large, VariantState::NotAdmitted, None),
        "Arbiter on GPU through CUDA at the 8,192-token budget has no current native admission."
    );
    assert_eq!(
        variant_disclosure(&large, VariantState::ResidentNotEligible, None),
        "Arbiter on GPU through CUDA at the 8,192-token budget remains resident, but its local eligibility is not current. Recover can reaccept it under a bounded management operation."
    );
    Ok(())
}

#[test]
fn no_sentence_states_a_weight_precision() -> Result<()> {
    let full = tier(SelectedModel::Arbiter, 1, 8192, 8)?;
    let mut half = cuda(SelectedModel::Arbiter, 3, 8192)?;
    half.device_execution.as_mut().unwrap().precision = DevicePrecision::Float16;
    let wide = cuda(SelectedModel::Arbiter, 4, 8192)?;
    let profiles = [full.clone(), half.clone(), wide.clone()];
    let resolve = |pin| profiles.iter().find(|profile| profile.manifest == pin);
    let mut texts = Vec::new();
    for selected in [&full, &half, &wide] {
        texts.push(stage_disclosure(
            &selection(
                SelectedModel::Arbiter,
                DevicePolicy::Auto,
                Some(selected),
                vec![
                    row(&half, SelectionReason::GpuPrecisionExcluded),
                    row(&wide, SelectionReason::DeviceMemoryInsufficient),
                    row(&full, SelectionReason::CpuFitInsufficient),
                ],
                None,
            )?,
            resolve,
        ));
        for state in [
            VariantState::Active,
            VariantState::ResidentNotEligible,
            VariantState::Admitted,
            VariantState::NotAdmitted,
        ] {
            texts.push(variant_disclosure(
                selected,
                state,
                Some(SelectionReason::GpuPrecisionExcluded),
            ));
        }
    }
    assert_eq!(texts.len(), 15);
    for text in texts {
        let lower = text.to_ascii_lowercase();
        for word in ["fp32", "fp16", "float32", "float16", "half", "weights"] {
            assert!(!lower.contains(word), "{text}");
        }
        assert!(text.len() <= MAX_DISCLOSURE_BYTES);
    }
    Ok(())
}
