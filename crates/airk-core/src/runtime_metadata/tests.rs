// SPDX-License-Identifier: Apache-2.0
//! Independent C7 cases predate this native owner; emission is a consumer oracle.
use super::*;
use serde_json::{Value, json};

fn fixture() -> Value {
    serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/split/receipts/pair_metadata_3.json"
    )))
    .unwrap()
}

fn case_input(base: &Value, case: &Value) -> Value {
    if let Some(replacement) = case.get("replace") {
        return replacement.clone();
    }
    let mut input = base.clone();
    if let Some(changes) = case.get("set").and_then(Value::as_array) {
        for change in changes {
            let path = change[0].as_array().unwrap();
            assert_eq!(path.len(), 1, "pair metadata cases are flat");
            input
                .as_object_mut()
                .unwrap()
                .insert(path[0].as_str().unwrap().into(), change[1].clone());
        }
    }
    if let Some(deletions) = case.get("delete").and_then(Value::as_array) {
        for path in deletions {
            assert_eq!(path.as_array().unwrap().len(), 1);
            input
                .as_object_mut()
                .unwrap()
                .remove(path[0].as_str().unwrap());
        }
    }
    input
}

#[test]
fn runtime_metadata_independent_c7_cases_and_emitted_semantics() -> Result<()> {
    let fixture = fixture();
    assert_eq!(fixture["cases"].as_array().unwrap().len(), 17);
    let mut observed = Vec::new();
    for case in fixture["cases"].as_array().unwrap() {
        let input = case_input(&fixture["base"], case);
        let actual = serde_json::from_value::<SplitRuntimeMetadata>(input.clone());
        assert_eq!(
            actual.is_ok(),
            case["expect"] == "ok",
            "{}: {actual:?}",
            case["name"]
        );
        let canonical = actual
            .as_ref()
            .ok()
            .map(SplitRuntimeMetadata::encode)
            .transpose()?;
        observed.push(
            json!({"name":case["name"],"input":input,"accepted":actual.is_ok(),
            "canonical":canonical.map(|bytes| String::from_utf8(bytes).unwrap())}),
        );
    }
    let emitted = json!({"schema":"airk.runtime-metadata-cases/1","cases":observed});
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/split/receipts/runtime_metadata_cases.json");
    if std::env::var_os("AIRK_WRITE_RUNTIME_METADATA_CASES").as_deref()
        == Some(std::ffi::OsStr::new("1"))
    {
        std::fs::write(&path, serde_json::to_vec_pretty(&emitted)?)?;
    } else {
        let retained: Value = serde_json::from_slice(&std::fs::read(&path).map_err(|error| {
            anyhow::anyhow!("generate native runtime metadata cases first: {error}")
        })?)?;
        assert_eq!(retained, emitted, "Rust-emitted C7 semantic cases drifted");
    }
    Ok(())
}

#[test]
fn runtime_metadata_exact_encoding_targets_and_raw_bounds() -> Result<()> {
    let metadata: SplitRuntimeMetadata = serde_json::from_value(fixture()["base"].clone())?;
    let expected = format!(
        r#"{{"schema":3,"version":"1.9.0","os":"windows","arch":"x86_64","airk_sha256":"{}","airkctl_sha256":"{}","i1_protocol":1,"i2_contract":1,"admin_inventory_sha256":"{}"}}"#,
        "1".repeat(64),
        "2".repeat(64),
        "3".repeat(64)
    );
    assert_eq!(metadata.encode()?, expected.as_bytes());
    assert_eq!(SplitRuntimeMetadata::decode(expected.as_bytes())?, metadata);
    metadata.validate_target("1.9.0", "windows", "x86_64")?;
    for (version, os, arch) in [
        ("1.8.35", "windows", "x86_64"),
        ("1.9.0", "macos", "x86_64"),
        ("1.9.0", "windows", "aarch64"),
    ] {
        assert!(metadata.validate_target(version, os, arch).is_err());
    }
    let duplicate = expected.replacen("\"schema\":3", "\"schema\":3,\"schema\":3", 1);
    assert!(SplitRuntimeMetadata::decode(duplicate.as_bytes()).is_err());
    assert!(SplitRuntimeMetadata::decode(&vec![b' '; MAX_RUNTIME_METADATA_BYTES + 1]).is_err());
    Ok(())
}

#[test]
fn runtime_metadata_known_image_paths_never_depend_on_untrusted_paths() -> Result<()> {
    for (os, arch, service, cli) in [
        ("windows", "x86_64", "airk.exe", "airkctl.exe"),
        ("linux", "x86_64", "airk", "airkctl"),
        (
            "macos",
            "x86_64",
            "AIRK.app/Contents/MacOS/airk",
            "AIRK.app/Contents/MacOS/airkctl",
        ),
        (
            "macos",
            "aarch64",
            "AIRK.app/Contents/MacOS/airk",
            "AIRK.app/Contents/MacOS/airkctl",
        ),
    ] {
        let mut input = fixture()["base"].clone();
        input["os"] = os.into();
        input["arch"] = arch.into();
        let metadata: SplitRuntimeMetadata = serde_json::from_value(input)?;
        assert_eq!(metadata.service_relative_path()?, service);
        assert_eq!(metadata.cli_relative_path()?, cli);
    }
    Ok(())
}

#[test]
fn runtime_metadata_interface_ranges_and_version_components_are_literal_contracts() -> Result<()> {
    let base = fixture()["base"].clone();
    for field in ["i1_protocol", "i2_contract"] {
        for (value, accepted) in [
            (json!(0), false),
            (json!(1), true),
            (json!(65_535), true),
            (json!(65_536), false),
            (json!(true), false),
            (json!(1.0), false),
            (Value::Null, false),
        ] {
            let mut input = base.clone();
            input[field] = value;
            assert_eq!(
                serde_json::from_value::<SplitRuntimeMetadata>(input).is_ok(),
                accepted,
                "{field}"
            );
        }
    }
    for version in ["1.9", "01.9.0", "1.09.0", "1.9.00", "1.9.0-beta", "1.9.0\n"] {
        let mut input = base.clone();
        input["version"] = version.into();
        assert!(
            serde_json::from_value::<SplitRuntimeMetadata>(input).is_err(),
            "{version}"
        );
    }
    let mut incomplete = base;
    incomplete["airkctl_sha256"] = "short".into();
    assert!(serde_json::from_value::<SplitRuntimeMetadata>(incomplete).is_err());
    Ok(())
}
