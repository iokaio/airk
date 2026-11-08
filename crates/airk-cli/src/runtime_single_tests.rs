// SPDX-License-Identifier: Apache-2.0
use super::*;

#[test]
fn retired_stdio_rejects_modern_missing_and_malformed_receipts_but_preserves_legacy() -> Result<()>
{
    let owner = tempfile::tempdir()?;
    let main = owner.path().join(crate::runtime_install::main_name());
    fs::create_dir_all(main.parent().unwrap())?;
    fs::write(&main, b"entry guard fixture")?;
    let root = payload_root(&main)?;
    let receipt = root.join("airk-runtime.json");
    let error = require_legacy_stdio(&main).unwrap_err().to_string();
    assert!(error.contains("stdio_mcp_retired") && error.contains("HTTPS"));
    assert!(require_legacy_inference_process(&main).is_err());
    let mut pair = RuntimePair {
        schema: 2,
        version: env!("CARGO_PKG_VERSION").into(),
        os: std::env::consts::OS.into(),
        arch: std::env::consts::ARCH.into(),
        airk_sha256: digest_file(&main)?,
        embed_sha256: String::new(),
    };
    fs::write(&receipt, serde_json::to_vec(&pair)?)?;
    assert!(require_legacy_stdio(&main).is_err());
    assert!(require_legacy_inference_process(&main).is_err());
    pair.schema = 1;
    pair.embed_sha256 = "a".repeat(64);
    fs::write(&receipt, serde_json::to_vec(&pair)?)?;
    require_legacy_stdio(&main)?;
    require_legacy_inference_process(&main)?;
    fs::write(&receipt, br#"{"schema":1}"#)?;
    assert!(require_legacy_stdio(&main).is_err());
    assert!(require_legacy_inference_process(&main).is_err());
    Ok(())
}

#[test]
fn single_executable_receipt_verifies_main_without_helper_or_role_copies() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let root = owner.path().join("payload");
    let main = root.join(crate::runtime_install::main_name());
    fs::create_dir_all(main.parent().unwrap())?;
    fs::write(&main, b"single executable fixture")?;
    let pair = RuntimePair {
        schema: 2,
        version: env!("CARGO_PKG_VERSION").into(),
        os: std::env::consts::OS.into(),
        arch: std::env::consts::ARCH.into(),
        airk_sha256: digest_file(&main)?,
        embed_sha256: String::new(),
    };
    let bytes = serde_json::to_vec(&pair)?;
    assert!(!String::from_utf8_lossy(&bytes).contains("embed_sha256"));
    fs::write(root.join("airk-runtime.json"), &bytes)?;
    assert_eq!(
        verify_installed_pair(&main, env!("CARGO_PKG_VERSION"))?,
        main
    );
    assert!(verify_helper(&main).is_err());
    crate::process_roles::install(&main)?;
    for role in [
        crate::process_roles::Role::Service,
        crate::process_roles::Role::Index,
        crate::process_roles::Role::Source,
    ] {
        assert_eq!(crate::process_roles::executable(&main, role)?, main);
        assert!(!root.join(role.name()).exists());
    }
    let mut invalid: serde_json::Value = serde_json::from_slice(&bytes)?;
    for helper in [
        serde_json::Value::Null,
        serde_json::Value::String("b".repeat(64)),
    ] {
        invalid["embed_sha256"] = helper;
        assert!(serde_json::from_value::<RuntimePair>(invalid.clone()).is_err());
    }
    invalid.as_object_mut().unwrap().remove("embed_sha256");
    invalid["schema"] = 1.into();
    assert!(serde_json::from_value::<RuntimePair>(invalid).is_err());
    fs::write(&main, b"changed")?;
    assert!(verify_installed_pair(&main, env!("CARGO_PKG_VERSION")).is_err());
    Ok(())
}
