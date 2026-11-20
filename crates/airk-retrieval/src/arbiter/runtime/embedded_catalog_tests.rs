// SPDX-License-Identifier: Apache-2.0
//! The embedded 1.8.x catalog carries exactly the three selected unqualified
//! local-test descriptors. Source-policy admission is independent of the series gate.
use super::*;

const SELECTED: [(ModelChoice, &str); 3] = [
    (
        ModelChoice::FocusQwenReranker06B,
        "unqualified-local-test-focus-qwen3-reranker-06b-w8-r1",
    ),
    (
        ModelChoice::ContextQwenCompact,
        "unqualified-local-test-context-qwen3-17b-fp32-r1",
    ),
    (
        ModelChoice::ContextQwenLarge,
        "unqualified-local-test-context-qwen3-4b-fp32-r1",
    ),
];

#[test]
fn embedded_catalog_has_the_three_selected_local_test_profiles() -> Result<()> {
    let profiles = catalog_profiles()?;
    assert_eq!(profiles.len(), 3);
    for (model, id) in SELECTED {
        let profile = profiles
            .iter()
            .find(|profile| profile.model == model)
            .expect("selected model is in the catalog");
        assert_eq!(profile.profile_id, id);
        assert!(profile.qualification.is_none() && profile.qualified_targets.is_empty());
        assert_eq!(profile.downloads.len(), profile.assets.len());
        for download in &profile.downloads {
            for part in &download.parts {
                assert!(part.backup_urls.is_empty());
                assert!(part.url.starts_with(
                    "https://github.com/iokaio/airk/releases/download/models-1.8.201/"
                ));
                assert!(part.bytes > 0 && part.bytes <= 1 << 30);
            }
        }
    }
    Ok(())
}

#[test]
fn selected_local_test_profiles_are_admitted_at_1_8_201_and_refused_from_1_9_0() -> Result<()> {
    for (model, _) in SELECTED {
        let profile = catalog_profiles()?
            .into_iter()
            .find(|profile| profile.model == model)
            .expect("selected model is in the catalog");
        profile.validate()?;
        profile.validate_release_downloads()?;
        if model == ModelChoice::FocusQwenReranker06B {
            assert!(validate_selected_policies(&profile).is_err());
            let execution = ExecutionProfile::from_release(profile.manifest)?;
            let transition = execution
                .source_transition_record()?
                .expect("legacy Focus source witness");
            assert_eq!(transition["legacy_manifest"], profile.manifest.to_string());
            assert_eq!(transition["task_qualification_transferred"], false);
        } else {
            // Immutable public Context descriptors bind the historical task source.
            // The series exception does not bypass the changed semantic policy.
            assert!(validate_selected_policies(&profile).is_err());
            assert!(ExecutionProfile::from_release(profile.manifest).is_err());
            assert_ne!(
                profile.task_policies.get(&Task::ContextAction),
                Some(&super::super::context_action::policy_digest()?)
            );
        }
        assert!(release_qualification_gate(&profile, "1.8.201").is_ok());
        for version in ["1.9.0", "1.9.1", "2.0.0", "1.8.201-rc1"] {
            assert!(
                release_qualification_gate(&profile, version).is_err(),
                "{version}"
            );
        }
    }
    Ok(())
}

#[test]
fn current_context_source_admission_requires_new_metadata_without_relabeling_public_assets()
-> Result<()> {
    let profiles = catalog_profiles()?;
    for model in [
        ModelChoice::ContextQwenCompact,
        ModelChoice::ContextQwenLarge,
    ] {
        let original = profiles
            .iter()
            .find(|profile| profile.model == model)
            .unwrap();
        let retained = serde_json::to_value(original)?;
        let mut current = original.clone();
        current.task_policies.insert(
            Task::ContextAction,
            super::super::context_action::policy_digest()?,
        );
        current.max_tokens = 8192;
        // This checks the native source-policy predicate only. Authenticated new
        // compiler/calibration bytes and private regeneration remain separate gates.
        validate_selected_policies(&current)?;
        assert_eq!(serde_json::to_value(original)?, retained);
        assert_eq!(original.max_tokens, 2048);
        assert!(ExecutionProfile::from_release(original.manifest).is_err());
        assert!(current.qualification.is_none() && current.qualified_targets.is_empty());
    }
    Ok(())
}
