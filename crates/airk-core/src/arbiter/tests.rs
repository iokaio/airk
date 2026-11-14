// SPDX-License-Identifier: Apache-2.0
use super::*;

#[test]
fn retained_disabled_configuration_bytes_are_independent_and_identity_bound() -> Result<()> {
    let bytes = include_bytes!("../../../../fixtures/arbiter/configuration-disabled-v2.json");
    let installation = Sha256Digest::from_bytes([1; 32]);
    let catalog = Incarnation::from_bytes([2; 16]);
    let configuration = Configuration::decode(bytes, installation, catalog)?;
    assert!(!configuration.enabled && configuration.indexes.is_empty());
    assert_eq!(configuration.canonical_bytes()?, bytes);
    assert_eq!(
        Configuration::disabled(installation, catalog).canonical_bytes()?,
        bytes
    );
    assert!(Configuration::decode(bytes, Sha256Digest::from_bytes([3; 32]), catalog).is_err());
    assert!(Configuration::decode(bytes, installation, Incarnation::from_bytes([3; 16])).is_err());
    // Historical unpublished prototype bytes remain retained and cannot be
    // interpreted as a new SHA-256 installation identity.
    assert!(
        Configuration::decode(
            include_bytes!("../../../../fixtures/arbiter/configuration-disabled-v1.json"),
            installation,
            catalog
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn enablement_never_enrolls_new_or_recreated_indexes() -> Result<()> {
    let mut configuration = Configuration::disabled(
        Sha256Digest::from_bytes([1; 32]),
        Incarnation::from_bytes([2; 16]),
    );
    let selected = IndexId::from_identity(Incarnation::from_bytes([3; 16]));
    let recreated = IndexId::from_identity(Incarnation::from_bytes([4; 16]));
    configuration.enabled = true;
    assert!(configuration.validate().is_err());
    configuration.selected_profile = Some(Sha256Digest::from_bytes([5; 32]));
    assert!(configuration.validate().is_err());
    configuration.indexes.push(Enrollment {
        index_id: selected,
        tasks: BTreeSet::from([Task::EvidenceRelevance]),
    });
    configuration.validate()?;
    assert!(configuration.eligible(selected, Task::EvidenceRelevance));
    assert!(!configuration.eligible(selected, Task::ContextGap));
    assert!(!configuration.eligible(recreated, Task::EvidenceRelevance));
    configuration.enabled = false;
    assert!(!configuration.eligible(selected, Task::EvidenceRelevance));
    Ok(())
}

#[test]
fn noncanonical_unknown_and_duplicate_enrollment_state_refuses() -> Result<()> {
    let bytes = include_bytes!("../../../../fixtures/arbiter/configuration-disabled-v2.json");
    let installation = Sha256Digest::from_bytes([1; 32]);
    let catalog = Incarnation::from_bytes([2; 16]);
    let mut value: serde_json::Value = serde_json::from_slice(bytes)?;
    value["download_on_read"] = true.into();
    assert!(Configuration::decode(&serde_json::to_vec(&value)?, installation, catalog).is_err());
    assert!(
        Configuration::decode(
            &serde_json::to_vec_pretty(&Configuration::disabled(installation, catalog))?,
            installation,
            catalog
        )
        .is_err()
    );
    let enrollment = Enrollment {
        index_id: IndexId::from_identity(Incarnation::from_bytes([3; 16])),
        tasks: BTreeSet::from([Task::ContextNextRead]),
    };
    let mut configuration = Configuration::disabled(installation, catalog);
    configuration.indexes = vec![enrollment.clone(), enrollment];
    assert!(configuration.validate().is_err());
    Ok(())
}

#[test]
fn task_and_four_choice_catalogs_are_independently_fixed() -> Result<()> {
    for (json, task) in [
        ("airk.request_supported.v1", Task::RequestSupported),
        ("airk.retrieval_route.v1", Task::RetrievalRoute),
        ("airk.recipe_choice.v1", Task::RecipeChoice),
        ("airk.evidence_relevance.v1", Task::EvidenceRelevance),
        ("airk.evidence_role.v1", Task::EvidenceRole),
        ("airk.context_next_read.v1", Task::ContextNextRead),
        ("airk.context_gap.v1", Task::ContextGap),
    ] {
        assert_eq!(serde_json::to_value(task)?, json);
    }
    for (json, model) in [
        ("arbiter-small-intern-08b", ModelChoice::SmallIntern),
        ("arbiter-small-strands-2b", ModelChoice::SmallStrands),
        ("arbiter-large-intern-4b", ModelChoice::LargeIntern),
        ("arbiter-large-kev-4b", ModelChoice::LargeKev),
    ] {
        assert_eq!(serde_json::to_value(model)?, json);
    }
    assert!(serde_json::from_str::<ModelChoice>("\"minilm\"").is_err());
    Ok(())
}

#[test]
fn request_cannot_inflate_original_work_or_add_execution_authority() -> Result<()> {
    let mut request = ContextRequest {
        schema: REQUEST_SCHEMA.into(),
        question: "Where is this defined and tested?".into(),
        budget: Budget::default(),
    };
    request.validate()?;
    request.budget.rounds = 5;
    assert!(request.validate().is_err());
    request.budget = Budget::default();
    let mut value = serde_json::to_value(request)?;
    value["enroll_all_indexes"] = true.into();
    assert!(serde_json::from_value::<ContextRequest>(value).is_err());
    Ok(())
}

#[test]
fn compiled_asset_download_closure_is_bounded_official_and_preserves_legacy_omission() -> Result<()>
{
    let original: Profile = serde_json::from_slice(include_bytes!(
        "../../../../fixtures/arbiter/profile-downloads.json"
    ))?;
    original.validate_release_downloads()?;
    let mut legacy = original.clone();
    legacy.downloads.clear();
    legacy.validate()?;
    let bytes = serde_json::to_vec(&legacy)?;
    assert!(!String::from_utf8(bytes.clone())?.contains("downloads"));
    assert_eq!(
        serde_json::to_vec(&serde_json::from_slice::<Profile>(&bytes)?)?,
        bytes
    );
    assert!(legacy.validate_release_downloads().is_err());
    for url in [
        "http://github.com/iokaio/airk/releases/download/v1.9.0/model.bin",
        "https://github.com/other/airk/releases/download/v1.9.0/model.bin",
        "https://github.com/iokaio/airk/releases/download/v1.9.0/model.bin?token=secret",
        "https://github.com/iokaio/airk/releases/download/../model.bin",
        "https://github.com/iokaio/airk/releases/download/v1.9.0/%2e%2e",
        "https://github.com/iokaio/airk/releases/download/v1.9.0/model.bin#fragment",
    ] {
        let mut profile = original.clone();
        profile.downloads[0].parts[0].url = url.into();
        assert!(profile.validate_release_downloads().is_err(), "{url}");
    }
    let mut too_large = original.clone();
    too_large.downloads.last_mut().unwrap().parts[0].bytes += 1;
    assert!(too_large.validate_release_downloads().is_err());
    let mut duplicate = original.clone();
    duplicate.downloads[0] = duplicate.downloads[1].clone();
    assert!(duplicate.validate_release_downloads().is_err());
    let mut short = original.clone();
    short.downloads.last_mut().unwrap().parts.pop();
    assert!(short.validate_release_downloads().is_err());
    if let Some(path) = std::env::var_os("AIRK_ARBITER_DOWNLOAD_CASE_OUTPUT") {
        std::fs::write(
            path,
            serde_json::to_vec_pretty(
                &serde_json::json!({"profile":original,"release_downloads_accepted":true,"legacy_bytes_accepted":true}),
            )?,
        )?;
    }
    Ok(())
}

#[test]
fn platform_qualification_requires_explicit_matching_execution_and_task_policy_is_closed()
-> Result<()> {
    let mut profile: Profile = serde_json::from_slice(include_bytes!(
        "../../../../fixtures/arbiter/profile-downloads.json"
    ))?;
    profile.cpu_fit = Some(CpuFit::default());
    profile.qualified_targets = vec![QualifiedTarget {
        target: "x86_64-pc-windows-msvc".into(),
        execution_identity: "onnxruntime-cpu/1.30.0/physical1/intra1/inter1/sequential/spinning0"
            .into(),
        receipt: Sha256Digest::from_bytes([42; 32]),
    }];
    profile
        .task_policies
        .insert(Task::ContextNextRead, Sha256Digest::from_bytes([43; 32]));
    profile.validate()?;
    let mut foreign = profile.clone();
    foreign.qualified_targets[0].execution_identity =
        "onnxruntime-cpu/1.30.0/physical8/intra8/inter1/sequential/spinning0".into();
    assert!(foreign.validate().is_err());
    let mut duplicate = profile.clone();
    duplicate
        .qualified_targets
        .push(profile.qualified_targets[0].clone());
    assert!(duplicate.validate().is_err());
    let mut unknown = profile.clone();
    unknown.qualified_targets[0].target = "aarch64-unknown-unqualified".into();
    assert!(unknown.validate().is_err());
    let mut missing = profile.clone();
    missing.cpu_fit = None;
    assert!(missing.validate().is_err());
    let mut task = profile.clone();
    task.task_policies
        .insert(Task::ContextGap, Sha256Digest::from_bytes([44; 32]));
    assert!(task.validate().is_err());
    Ok(())
}

#[test]
fn unobserved_optional_configuration_never_grants_ready_profile_or_advice() -> Result<()> {
    let publication = crate::evidence::RevisionEnvelope::new(
        "fixture".into(),
        crate::identity::RevisionToken {
            incarnation: Incarnation::from_bytes([1; 16]),
            generation: 1,
            revision: Sha256Digest::from_bytes([2; 32]),
        },
    );
    let original = IndexApplication {
        scope: ScopeBinding {
            installation: Sha256Digest::from_bytes([3; 32]),
            catalog: Incarnation::from_bytes([4; 16]),
            index_id: IndexId::from_identity(Incarnation::from_bytes([5; 16])),
            authorization: Sha256Digest::from_bytes([6; 32]),
            configuration_revision: 0,
            publication,
        },
        profile: None,
        availability: Availability::Failed,
        reason:
            "Optional configuration is unobserved; canonical evidence remains independently scoped."
                .into(),
    };
    original.validate()?;
    let mut profile = original.clone();
    profile.profile = Some(Sha256Digest::from_bytes([7; 32]));
    assert!(profile.validate().is_err());
    for availability in [
        Availability::Ready,
        Availability::Disabled,
        Availability::IndexDisabled,
        Availability::Warming,
    ] {
        let mut changed = original.clone();
        changed.availability = availability;
        assert!(changed.validate().is_err());
    }
    let advice = Advice {
        schema: ADVICE_SCHEMA.into(),
        task: Task::ContextNextRead,
        profile: Sha256Digest::from_bytes([7; 32]),
        runtime: RUNTIME.into(),
        compiler: Sha256Digest::from_bytes([8; 32]),
        calibration: Sha256Digest::from_bytes([9; 32]),
        policy: Sha256Digest::from_bytes([10; 32]),
        scope: original.scope.clone(),
        input_digest: Sha256Digest::from_bytes([11; 32]),
        evidence: vec![],
        work: Work::default(),
        outcome: Outcome::Unavailable {
            reason: "not applied".into(),
        },
    };
    assert!(advice.validate().is_err());
    Ok(())
}

#[test]
fn profile_descriptors_bind_exact_singleton_asset_bytes_without_digest_domain_substitution()
-> Result<()> {
    let profile: Profile = serde_json::from_slice(include_bytes!(
        "../../../../fixtures/arbiter/profile-downloads.json"
    ))?;
    profile.validate()?;
    for compiler in [true, false] {
        let mut mismatch = profile.clone();
        if compiler {
            mismatch.compiler = Sha256Digest::from_bytes([234; 32]);
        } else {
            mismatch.calibration = Sha256Digest::from_bytes([235; 32]);
        }
        assert!(mismatch.validate().is_err());
    }
    for role in [
        AssetRole::Model,
        AssetRole::Tokenizer,
        AssetRole::Compiler,
        AssetRole::Calibration,
    ] {
        let mut duplicate = profile.clone();
        let mut asset = duplicate
            .assets
            .iter()
            .find(|asset| asset.role == role)
            .unwrap()
            .clone();
        asset.name = "different-unique-asset-name.bin".into();
        duplicate.assets.push(asset);
        assert!(duplicate.validate().is_err());
        let mut missing = profile.clone();
        missing.assets.retain(|asset| asset.role != role);
        assert!(missing.validate().is_err());
    }
    Ok(())
}

#[test]
fn explicit_cpu16_tier_is_distinct_and_never_accepts_mixed_or_fallback_threads() -> Result<()> {
    let fit: CpuFit = serde_json::from_slice(include_bytes!(
        "../../../../fixtures/arbiter/cpu-fit-16.json"
    ))?;
    assert_eq!(fit.minimum_physical_cores, 16);
    assert_eq!(fit.intra_threads, 16);
    fit.validate()?;
    assert_eq!(
        fit.runtime_identity()?,
        "onnxruntime-cpu/1.30.0/physical16/intra16/inter1/sequential/spinning0"
    );
    let mut minimum = fit.clone();
    minimum.minimum_physical_cores = 8;
    assert!(minimum.validate().is_err());
    let mut intra = fit.clone();
    intra.intra_threads = 8;
    assert!(intra.validate().is_err());
    let mut inter = fit.clone();
    inter.inter_threads = 2;
    assert!(inter.validate().is_err());
    let mut parallel = fit.clone();
    parallel.sequential = false;
    assert!(parallel.validate().is_err());
    let mut spinning = fit;
    spinning.spinning = true;
    assert!(spinning.validate().is_err());
    Ok(())
}

#[test]
fn explicit_cpu24_tier_is_distinct_and_refuses_all_mixed_prior_tiers() -> Result<()> {
    let fit: CpuFit = serde_json::from_slice(include_bytes!(
        "../../../../fixtures/arbiter/cpu-fit-24.json"
    ))?;
    assert_eq!(fit.minimum_physical_cores, 24);
    assert_eq!(fit.intra_threads, 24);
    fit.validate()?;
    assert_eq!(
        fit.runtime_identity()?,
        "onnxruntime-cpu/1.30.0/physical24/intra24/inter1/sequential/spinning0"
    );
    for prior in [1, 8, 16] {
        let mut minimum = fit.clone();
        minimum.minimum_physical_cores = prior;
        assert!(minimum.validate().is_err());
        let mut intra = fit.clone();
        intra.intra_threads = prior;
        assert!(intra.validate().is_err());
    }
    let mut arbitrary = fit;
    arbitrary.minimum_physical_cores = 20;
    arbitrary.intra_threads = 20;
    assert!(arbitrary.validate().is_err());
    Ok(())
}
