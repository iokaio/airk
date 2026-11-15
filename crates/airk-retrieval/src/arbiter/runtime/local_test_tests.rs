// SPDX-License-Identifier: Apache-2.0
//! Unqualified local-test admission: pre-1.9.0 series only, fail closed.
use super::*;

fn base() -> Profile {
    let mut profile: Profile = serde_json::from_slice(include_bytes!(
        "../../../../../fixtures/arbiter/profile-downloads.json"
    ))
    .expect("retained profile fixture");
    profile.qualification = None;
    profile.qualified_targets = Vec::new();
    profile.cpu_fit = Some(CpuFit::default());
    profile.profile_id = format!("{LOCAL_TEST_PROFILE_PREFIX}-focus-r1");
    profile
}

#[test]
fn local_test_profile_is_admitted_below_1_9_without_qualification() {
    let profile = base();
    for version in ["1.8.201", "1.8.0", "1.0.0", "0.9.9"] {
        assert!(
            release_qualification_gate(&profile, version).is_ok(),
            "{version}"
        );
    }
}

#[test]
fn local_test_profile_is_refused_from_1_9_and_for_unparseable_versions() {
    let profile = base();
    for version in [
        "1.9.0",
        "1.9.1",
        "1.10.0",
        "2.0.0",
        "",
        "1.8",
        "1.8.201.1",
        "1.8.x",
        "01.8.1",
        "1.08.1",
        "1.8.201-rc1",
        "+1.8.1",
        " 1.8.1",
    ] {
        assert!(
            release_qualification_gate(&profile, version).is_err(),
            "{version:?}"
        );
    }
}

#[test]
fn profile_without_the_prefix_and_without_qualification_is_refused() {
    let mut profile = base();
    profile.profile_id = "focus-r1".into();
    assert!(release_qualification_gate(&profile, "1.8.201").is_err());
    profile.profile_id = format!("x-{LOCAL_TEST_PROFILE_PREFIX}");
    assert!(release_qualification_gate(&profile, "1.8.201").is_err());
}

#[test]
fn local_test_profile_still_requires_a_cpu_fit() {
    let mut profile = base();
    profile.cpu_fit = None;
    assert!(release_qualification_gate(&profile, "1.8.201").is_err());
}

#[test]
fn series_check_uses_exact_literals() {
    assert!(local_test_series("1.8.201"));
    assert!(!local_test_series("1.9.0"));
    assert!(!local_test_series("2.0.0"));
    assert!(!local_test_series("not.a.version"));
    assert!(local_test_profile_for(&base(), "1.8.201"));
    assert!(!local_test_profile_for(&base(), "1.9.0"));
}
