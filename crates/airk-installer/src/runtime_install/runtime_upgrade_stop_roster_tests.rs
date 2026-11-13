// SPDX-License-Identifier: Apache-2.0
use super::*;

fn attach_historical(root: &Path, version: &str) -> Result<(tempfile::TempDir, String)> {
    attach_historical_materialized(root, version, true)
}

fn attach_historical_materialized(
    root: &Path,
    version: &str,
    materialized: bool,
) -> Result<(tempfile::TempDir, String)> {
    let (owner, original, identity) = super::super::tests::unselected_historical_fixture()?;
    let directory = original.join("runtimes").join(&identity);
    let pair_path = directory.join("airk-runtime.json");
    let mut pair: runtime_pair::RuntimePair = serde_json::from_slice(&fs::read(&pair_path)?)?;
    pair.version = version.into();
    fs::write(&pair_path, serde_json::to_vec(&pair)?)?;
    let manifest_path = directory.join("airk-install.json");
    let mut inventory: InstallManifest = serde_json::from_slice(&fs::read(&manifest_path)?)?;
    inventory.version = version.into();
    inventory.files.insert(
        "airk-runtime.json".into(),
        runtime_pair::digest_file(&pair_path)?,
    );
    let bytes = serde_json::to_vec(&inventory)?;
    let identity = format!("{:x}", Sha256::digest(&bytes));
    fs::write(&manifest_path, bytes)?;
    replace_json(
        &directory,
        "approval.json",
        &Approval {
            schema: 1,
            manifest_sha256: identity.clone(),
            version: version.into(),
            verification_basis: VerificationBasis::OrganizationDigest,
        },
    )?;
    fs::rename(&directory, root.join("runtimes").join(&identity))?;
    let (main, _) = verify_runtime(root, &identity)?;
    if materialized {
        airk_host::process_roles::install(&main)?;
    }
    Ok((owner, identity))
}

#[test]
fn clean_break_absent_optional_alias_requires_complete_zero_match_process_observation() -> Result<()>
{
    let path = Path::new("/fixture/airk-service");
    let complete = crate::legacy_processes::Audit {
        matching_executable_pids: Vec::new(),
        scanned: 5,
        complete: true,
        unobserved: 0,
        failure: None,
    };
    require_absent_optional_alias_observation(path, &complete)?;
    let mut incomplete = complete.clone();
    incomplete.complete = false;
    incomplete.unobserved = 1;
    assert!(require_absent_optional_alias_observation(path, &incomplete).is_err());
    let mut matched = complete.clone();
    matched.matching_executable_pids.push(42);
    assert!(require_absent_optional_alias_observation(path, &matched).is_err());
    let mut failed = complete;
    failed.failure = Some("process_scan_budget".into());
    assert!(require_absent_optional_alias_observation(path, &failed).is_err());
    assert!(
        require_absent_optional_alias_observation(
            path,
            &crate::legacy_processes::Audit::unavailable()
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn clean_break_absent_optional_alias_catalog_is_read_only_and_never_omits_sealed_images()
-> Result<()> {
    let (_candidate_owner, root, candidate) = super::super::tests::unselected_fixture()?;
    let (_old_owner, old) = attach_historical_materialized(&root, "1.8.11", false)?;
    let original_manifest = fs::read(root.join("runtimes").join(&old).join("airk-install.json"))?;
    let missing = retirement::absent_optional_process_aliases(&root, &candidate)?;
    assert_eq!(missing.len(), if cfg!(target_os = "macos") { 0 } else { 3 });
    for image in &missing {
        assert!(!image.path.exists());
        assert!(
            [
                airk_host::process_roles::Role::Service,
                airk_host::process_roles::Role::Index,
                airk_host::process_roles::Role::Source
            ]
            .iter()
            .any(|role| image.path.file_name() == Some(role.name().as_ref()))
        );
        let error = require_absent_optional_alias_observation(
            &image.path,
            &crate::legacy_processes::Audit::unavailable(),
        )
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("clean_break_predecessor_process_observation_unsupported")
        );
    }
    assert_eq!(
        fs::read(root.join("runtimes").join(&old).join("airk-install.json"))?,
        original_manifest
    );
    assert!(!root.join("rebuild.json").exists());
    assert!(!root.join("superseded-retirement.json").exists());
    let (main, _) = verify_runtime(&root, &old)?;
    fs::remove_file(&main)?;
    assert!(
        retirement::absent_optional_process_aliases(&root, &candidate).is_err(),
        "a missing sealed main is never downgraded to an optional alias"
    );
    Ok(())
}

#[test]
fn clean_break_stop_roster_contains_every_approved_predecessor_and_both_launchers() -> Result<()> {
    let (_candidate_owner, root, candidate) = super::super::tests::unselected_fixture()?;
    let (_first_owner, first) = attach_historical(&root, "1.8.10")?;
    let (_second_owner, second) = attach_historical(&root, "1.8.11")?;
    let (first_main, _, inventory) = verify_runtime_inventory(&root, &first)?;
    prepare_launcher(&root, &first_main, &inventory)?;
    let universal = root.parent().unwrap().join(if cfg!(windows) {
        "universal-airk.exe"
    } else {
        "universal-airk"
    });
    fs::write(&universal, b"owned inert universal launcher fixture")?;
    let universal_hash = runtime_pair::digest_file(&universal)?;
    let paths = all_predecessor_images_with_universal(&root, &candidate, || {
        Ok(Some((universal.clone(), universal_hash)))
    })?;
    for identity in [&first, &second] {
        let (main, _) = verify_runtime(&root, identity)?;
        assert!(paths.contains(&main));
        assert!(paths.contains(&runtime_pair::payload_root(&main)?.join(helper_name())));
        if cfg!(target_os = "macos") {
            assert!(
                paths.contains(
                    &root
                        .join("runtimes")
                        .join(identity)
                        .join(MAC_LAUNCHER_IMAGE)
                )
            );
        } else {
            for role in [
                airk_host::process_roles::Role::Service,
                airk_host::process_roles::Role::Index,
                airk_host::process_roles::Role::Source,
            ] {
                assert!(paths.contains(&airk_host::process_roles::executable(&main, role)?));
            }
        }
    }
    assert!(paths.contains(&launcher_path(&root)));
    assert!(paths.contains(&universal));
    assert!(
        paths
            .iter()
            .all(|path| !path.starts_with(root.join("runtimes").join(&candidate)))
    );
    let unique: std::collections::BTreeSet<_> = paths.iter().collect();
    assert_eq!(unique.len(), paths.len());
    assert!(paths.iter().all(|path| path.is_file()));
    Ok(())
}

#[test]
fn clean_break_stop_roster_refuses_modified_owned_images_without_including_unrelated_files()
-> Result<()> {
    let (_candidate_owner, root, candidate) = super::super::tests::unselected_fixture()?;
    let (_old_owner, old) = attach_historical(&root, "1.8.11")?;
    let (main, _, inventory) = verify_runtime_inventory(&root, &old)?;
    prepare_launcher(&root, &main, &inventory)?;
    let unrelated = root.parent().unwrap().join("unrelated-airk.exe");
    fs::write(&unrelated, b"unrelated process image")?;
    let paths = all_predecessor_images_with_universal(&root, &candidate, || Ok(None))?;
    assert!(!paths.contains(&unrelated));
    fs::write(launcher_path(&root), b"changed stable launcher")?;
    assert!(all_predecessor_images_with_universal(&root, &candidate, || Ok(None)).is_err());
    assert_eq!(fs::read(&unrelated)?, b"unrelated process image");
    Ok(())
}
