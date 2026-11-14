// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::install_record::canonical;
use serde_json::Value;

const CASES: &str = include_str!("../../../../../fixtures/split/i4/inno_handoff.json");

#[test]
fn inno_handoff_binds_both_files_and_exact_i4_identity_in_independent_cases() {
    let fixture: Value = serde_json::from_str(CASES).unwrap();
    let image: InstallationImage = serde_json::from_value(fixture["image"].clone()).unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 12);
    for case in cases {
        let text = case["text"].as_str().unwrap();
        let outcome = match InnoHandoff::decode(text.as_bytes(), &image) {
            Ok(value) => {
                assert_eq!(canonical(&value), text.as_bytes());
                "ok"
            }
            Err(error) => error.code(),
        };
        assert_eq!(
            outcome,
            case["expect"].as_str().unwrap(),
            "{}",
            case["name"]
        );
    }
    if let Some(output) = std::env::var_os("AIRK_INNO_HANDOFF_CASE_OUTPUT") {
        let text = cases[0]["text"].as_str().unwrap();
        let value = InnoHandoff::decode(text.as_bytes(), &image).unwrap();
        std::fs::write(
            output,
            serde_json::to_vec_pretty(&serde_json::json!({
                "handoff_text": String::from_utf8(canonical(&value)).unwrap(), "accepted": true,
            }))
            .unwrap(),
        )
        .unwrap();
    }
}
