// SPDX-License-Identifier: Apache-2.0
//! Metadata identity compatibility; these inert files are never executed.
use super::*;

#[test]
fn approved_predecessor_engine_preserves_original_schema1_framing() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let main = owner
        .path()
        .join(if cfg!(windows) { "airk.exe" } else { "airk" });
    fs::write(&main, b"inert predecessor identity fixture")?;
    let digest = digest_file(&main)?;
    let manifest = owner.path().join("airk-runtime.json");
    // This is the exact schema/order serialized by the original schema-one
    // owner. Do not derive the expected engine hash through the new decoder.
    let original = format!(
        r#"{{"schema":1,"version":"1.8.12","os":"{}","arch":"{}","airk_sha256":"{}","embed_sha256":"{}"}}"#,
        std::env::consts::OS,
        std::env::consts::ARCH,
        digest,
        "b".repeat(64)
    );
    fs::write(&manifest, &original)?;
    let expected =
        airk_core::identity::framed_digest(b"airk-maintenance-engine/1.4", &[original.as_bytes()]);
    assert_eq!(engine_identity_at_version(&main, "1.8.12")?, expected);
    assert!(
        engine_identity_at(&main).is_err(),
        "running-version observation must remain strict"
    );
    assert!(engine_identity_at_version(&main, "1.8.11").is_err());
    let mut crossed: serde_json::Value = serde_json::from_str(&original)?;
    for field in ["os", "arch", "airk_sha256"] {
        let prior = crossed[field].clone();
        crossed[field] = serde_json::Value::String(if field == "airk_sha256" {
            "c".repeat(64)
        } else {
            "foreign".into()
        });
        fs::write(&manifest, serde_json::to_vec(&crossed)?)?;
        assert!(
            engine_identity_at_version(&main, "1.8.12").is_err(),
            "crossed {field}"
        );
        crossed[field] = prior;
    }
    crossed["embed_sha256"] = serde_json::Value::String("d".repeat(64));
    fs::write(&manifest, serde_json::to_vec(&crossed)?)?;
    assert_ne!(
        engine_identity_at_version(&main, "1.8.12")?,
        expected,
        "legacy helper identity remains bound"
    );
    crossed.as_object_mut().unwrap().remove("embed_sha256");
    fs::write(&manifest, serde_json::to_vec(&crossed)?)?;
    assert!(engine_identity_at_version(&main, "1.8.12").is_err());
    Ok(())
}

#[test]
fn approved_single_image_version_is_explicit_and_digest_bound() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let main = owner
        .path()
        .join(if cfg!(windows) { "airk.exe" } else { "airk" });
    fs::write(&main, b"inert single image fixture")?;
    let pair = RuntimePair {
        schema: 2,
        version: "1.8.14".into(),
        os: std::env::consts::OS.into(),
        arch: std::env::consts::ARCH.into(),
        airk_sha256: digest_file(&main)?,
        embed_sha256: String::new(),
    };
    let bytes = serde_json::to_vec(&pair)?;
    fs::write(owner.path().join("airk-runtime.json"), &bytes)?;
    assert_eq!(
        engine_identity_at_version(&main, "1.8.14")?,
        airk_core::identity::framed_digest(b"airk-maintenance-engine/1.4", &[&bytes])
    );
    assert!(engine_identity_at_version(&main, "1.8.12").is_err());
    fs::write(&main, b"changed image")?;
    assert!(engine_identity_at_version(&main, "1.8.14").is_err());
    Ok(())
}
