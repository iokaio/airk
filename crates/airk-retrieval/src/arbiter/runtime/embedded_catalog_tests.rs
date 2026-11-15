// SPDX-License-Identifier: Apache-2.0
//! The embedded 1.8.x catalog carries exactly the three selected unqualified
//! local-test profiles, and they are admitted only in a pre-1.9.0 series.
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
        validate_selected_policies(&profile)?;
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
