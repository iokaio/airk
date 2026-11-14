// SPDX-License-Identifier: Apache-2.0
use super::*;

// This is deliberately not produced through the type being decoded. Whitespace
// and key order are allowed in this existing raw-digested packaging format.
const PAYLOAD: &str = r#"{
 "schema":"airk-service-payload/1", "version":"1.8.37", "channel":"stable",
 "target":"x86_64-unknown-linux-gnu",
 "executables":{"airkctl":"bin/airkctl","airk":"service/airk"},
 "files":{
  "bin/airkctl":{"sha256":"1111111111111111111111111111111111111111111111111111111111111111","bytes":12},
  "service/airk":{"sha256":"2222222222222222222222222222222222222222222222222222222222222222","bytes":24}
 },
 "admin_assets":{"inventory_sha256":"3333333333333333333333333333333333333333333333333333333333333333","bundle_sha256":"4444444444444444444444444444444444444444444444444444444444444444"},
 "signing":{"platform":"linux","signed":false,"integrity":"record-sha256"}
}
"#;

fn hash(bytes: &[u8]) -> Sha256Digest {
    Sha256Digest::from_bytes(Sha256::digest(bytes).into())
}

#[test]
fn raw_payload_inventory_bytes_and_existing_deserialize_shape_are_preserved() {
    let parsed = PayloadInventory::decode(PAYLOAD.as_bytes(), hash(PAYLOAD.as_bytes())).unwrap();
    assert_eq!(parsed.version, "1.8.37");
    assert_eq!(parsed.executables.airkctl, "bin/airkctl");
    assert_eq!(parsed.files["service/airk"].bytes, 24);
    let compact =
        serde_json::to_vec(&serde_json::from_str::<serde_json::Value>(PAYLOAD).unwrap()).unwrap();
    assert!(
        PayloadInventory::decode(&compact, hash(PAYLOAD.as_bytes())).is_err(),
        "no reserialization before digest verification"
    );
    assert!(PayloadInventory::decode(&compact, hash(&compact)).is_ok());
}

#[test]
fn raw_payload_inventory_refuses_unknown_fields_unsafe_names_and_missing_images() {
    for (old, new) in [
        ("\"version\":", "\"extra\":true,\"version\":"),
        ("bin/airkctl", "../outside"),
        ("service/airk", "install-record.json"),
    ] {
        let bad = PAYLOAD.replace(old, new);
        assert!(PayloadInventory::decode(bad.as_bytes(), hash(bad.as_bytes())).is_err());
    }
    let bad = PAYLOAD.replace("\"airk\":\"service/airk\"", "\"airk\":\"missing\"");
    assert!(PayloadInventory::decode(bad.as_bytes(), hash(bad.as_bytes())).is_err());
    assert!(
        PayloadInventory::decode(PAYLOAD.as_bytes(), Sha256Digest::from_bytes([0; 32])).is_err()
    );
}
