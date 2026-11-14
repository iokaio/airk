// SPDX-License-Identifier: Apache-2.0
use super::*;
use serde_json::Value;

const CASES: &str = include_str!("../../../../fixtures/split/i4/portable_cases.json");

#[test]
fn portable_contract_has_explicit_ownership_and_no_implicit_os_integration() {
    let cases: Value = serde_json::from_str(CASES).unwrap();
    let rows = cases["operations"].as_array().unwrap();
    assert_eq!(rows.len(), 6);
    for row in rows {
        let text = row["text"].as_str().unwrap();
        let parsed: SetupOperation = serde_json::from_str(text).unwrap();
        let result = SetupOperation::decode(text.as_bytes(), parsed.created_unix_ms);
        let outcome = match result {
            Ok(value) => {
                assert_eq!(canonical(&value), text.as_bytes());
                "ok"
            }
            Err(error) => error.code(),
        };
        assert_eq!(outcome, row["expect"].as_str().unwrap(), "{}", row["name"]);
    }
    let pointer = InstallPointer::decode(cases["pointer"].as_str().unwrap().as_bytes()).unwrap();
    let bytes = cases["record"].as_str().unwrap().as_bytes();
    let record = InstallRecord::decode(bytes, &pointer).unwrap();
    assert_eq!(record.installed_by.installer, Installer::Portable);
    assert_eq!(canonical(&record), bytes);
    if let Some(output) = std::env::var_os("AIRK_PORTABLE_CASE_OUTPUT") {
        let emitted = serde_json::json!({
            "record_text": String::from_utf8(canonical(&record)).unwrap(),
            "pointer_text": String::from_utf8(canonical(&pointer)).unwrap(),
            "record_accepted": true,
            "installer": record.installed_by.installer,
        });
        std::fs::write(output, serde_json::to_vec_pretty(&emitted).unwrap()).unwrap();
    }
    use sha2::Digest;
    assert_eq!(
        format!(
            "{:x}",
            sha2::Sha256::digest(include_bytes!(
                "../../../../fixtures/split/i4/i4_cases.json"
            ))
        ),
        cases["legacy_i4_sha256"].as_str().unwrap()
    );
}
