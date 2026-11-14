// SPDX-License-Identifier: Apache-2.0
//! C7 split receipts over inert native images, never process execution.
use super::*;
use serde_json::json;

fn split_fixture() -> Result<(tempfile::TempDir, PathBuf, PathBuf, String)> {
    let owner = tempfile::tempdir()?;
    let root = owner.path().canonicalize()?;
    let main = root.join(crate::install_files::main_name());
    let cli = main.with_file_name(if cfg!(windows) {
        "airkctl.exe"
    } else {
        "airkctl"
    });
    fs::create_dir_all(main.parent().unwrap())?;
    fs::write(&main, b"inert split service image")?;
    fs::write(&cli, b"inert split CLI image")?;
    let receipt = format!(
        r#"{{"schema":3,"version":"{}","os":"{}","arch":"{}","airk_sha256":"{}","airkctl_sha256":"{}","i1_protocol":1,"i2_contract":1,"admin_inventory_sha256":"{}"}}"#,
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH,
        digest_file(&main)?,
        digest_file(&cli)?,
        "3".repeat(64)
    );
    fs::write(root.join("airk-runtime.json"), &receipt)?;
    Ok((owner, main, cli, receipt))
}

#[test]
fn split_runtime_receipt_keeps_the_service_engine_in_process_without_helper_roles() -> Result<()> {
    let (_owner, main, _cli, _receipt) = split_fixture()?;
    assert!(single_executable(&main)?);
    assert_eq!(
        verify_installed_pair(&main, env!("CARGO_PKG_VERSION"))?,
        main
    );
    assert!(verify_helper(&main).is_err());
    assert!(require_legacy_stdio(&main).is_err());
    assert!(require_legacy_inference_process(&main).is_err());
    Ok(())
}

#[test]
fn split_runtime_engine_identity_binds_complete_cli_admin_and_interface_metadata() -> Result<()> {
    let (_owner, main, cli, receipt) = split_fixture()?;
    let expected =
        airk_core::identity::framed_digest(b"airk-maintenance-engine/1.4", &[receipt.as_bytes()]);
    assert_eq!(engine_identity_at(&main)?, expected);
    let manifest = payload_root(&main)?.join("airk-runtime.json");
    let original: serde_json::Value = serde_json::from_str(&receipt)?;
    for (field, replacement) in [
        ("admin_inventory_sha256", json!("4".repeat(64))),
        ("i1_protocol", json!(2)),
        ("i2_contract", json!(2)),
    ] {
        let mut changed = original.clone();
        changed[field] = replacement;
        fs::write(&manifest, serde_json::to_vec(&changed)?)?;
        assert_ne!(
            engine_identity_at(&main)?,
            expected,
            "{field} remains identity-bound"
        );
    }
    fs::write(&cli, b"a different inert CLI")?;
    let mut changed = original.clone();
    changed["airkctl_sha256"] = digest_file(&cli)?.into();
    fs::write(&manifest, serde_json::to_vec(&changed)?)?;
    assert_ne!(engine_identity_at(&main)?, expected);
    assert!(engine_identity_at_version(&main, "1.8.24").is_err());
    Ok(())
}

#[test]
fn split_runtime_rejects_crossed_targets_missing_claims_and_changed_actual_cli() -> Result<()> {
    let (_owner, main, cli, receipt) = split_fixture()?;
    let manifest = payload_root(&main)?.join("airk-runtime.json");
    engine_identity_at(&main)?;
    let original: serde_json::Value = serde_json::from_str(&receipt)?;
    for field in [
        "airkctl_sha256",
        "i1_protocol",
        "i2_contract",
        "admin_inventory_sha256",
    ] {
        let mut changed = original.clone();
        changed.as_object_mut().unwrap().remove(field);
        fs::write(&manifest, serde_json::to_vec(&changed)?)?;
        assert!(engine_identity_at(&main).is_err(), "missing {field}");
    }
    for (field, replacement) in [
        ("os", json!("foreign")),
        ("arch", json!("foreign")),
        ("i1_protocol", json!(0)),
        ("i2_contract", json!(0)),
        ("airkctl_sha256", json!("A".repeat(64))),
        ("admin_inventory_sha256", json!("short")),
        ("embed_sha256", json!("a".repeat(64))),
    ] {
        let mut changed = original.clone();
        changed[field] = replacement;
        fs::write(&manifest, serde_json::to_vec(&changed)?)?;
        assert!(engine_identity_at(&main).is_err(), "crossed {field}");
    }
    fs::write(&manifest, &receipt)?;
    fs::write(&cli, b"changed CLI without metadata change")?;
    assert!(verify_installed_pair(&main, env!("CARGO_PKG_VERSION")).is_err());
    assert!(engine_identity_at(&main).is_err());
    fs::remove_file(&cli)?;
    assert!(engine_identity_at(&main).is_err());
    Ok(())
}

#[test]
fn split_runtime_round_trip_retains_full_metadata_and_refuses_a_lossy_public_view() -> Result<()> {
    let (_owner, _main, cli, receipt) = split_fixture()?;
    let mut pair: RuntimePair = serde_json::from_str(&receipt)?;
    assert_eq!(
        pair.split.as_ref().unwrap().airkctl_sha256.to_string(),
        digest_file(&cli)?
    );
    assert_eq!(serde_json::to_string(&pair)?, receipt);
    let duplicate = receipt.replacen("\"schema\":3", "\"schema\":3,\"schema\":3", 1);
    assert!(serde_json::from_str::<RuntimePair>(&duplicate).is_err());
    pair.version = "0.0.0".into();
    assert!(
        serde_json::to_vec(&pair).is_err(),
        "crossed outer fields must not hide a split identity"
    );
    assert!(pair.validate_target(env!("CARGO_PKG_VERSION")).is_err());
    pair.version = env!("CARGO_PKG_VERSION").into();
    pair.split = None;
    assert!(
        serde_json::to_vec(&pair).is_err(),
        "schema three cannot serialize a lossy legacy projection"
    );
    assert!(pair.validate_target(env!("CARGO_PKG_VERSION")).is_err());
    Ok(())
}

#[test]
fn split_runtime_predecessor_observation_uses_the_explicit_approved_version() -> Result<()> {
    let (_owner, main, _cli, receipt) = split_fixture()?;
    let predecessor = receipt.replace(env!("CARGO_PKG_VERSION"), "1.8.24");
    fs::write(payload_root(&main)?.join("airk-runtime.json"), &predecessor)?;
    let expected = airk_core::identity::framed_digest(
        b"airk-maintenance-engine/1.4",
        &[predecessor.as_bytes()],
    );
    assert_eq!(engine_identity_at_version(&main, "1.8.24")?, expected);
    assert!(
        engine_identity_at(&main).is_err(),
        "running version observation stays strict"
    );
    Ok(())
}
