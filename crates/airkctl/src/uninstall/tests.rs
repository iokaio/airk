// SPDX-License-Identifier: Apache-2.0
use super::*;
use airk_core::install_record::{InstallPointer, InstallRecord};

const CASES: &str = include_str!("../../../../fixtures/split/i4/i4_cases.json");

fn verified() -> Verified {
    let cases: Value = serde_json::from_str(CASES).unwrap();
    let valid = &cases["records"][0];
    assert_eq!(valid["expect"], "ok");
    let pointer = InstallPointer::decode(valid["pointer"].as_str().unwrap().as_bytes()).unwrap();
    let record =
        InstallRecord::decode(valid["text"].as_str().unwrap().as_bytes(), &pointer).unwrap();
    Verified {
        pointer,
        record,
        airk: PathBuf::from("airk"),
        airkctl: PathBuf::from("airkctl"),
    }
}

#[test]
fn the_handed_over_operation_is_a_valid_uninstall_of_the_installation() {
    let verified = verified();
    let now = 1_800_000_000_000;
    let id = Incarnation::from_bytes([0x0e; 16]);
    let inventory = Sha256Digest::from_bytes([5; 32]);
    for purge in [false, true] {
        let built = operation(&verified, id, inventory, purge, now);
        let decoded = SetupOperation::decode(&canonical(&built), now + 1).unwrap();
        assert_eq!(decoded, built);
        assert_eq!(decoded.kind, OperationKind::Uninstall);
        assert_eq!(decoded.locations, verified.record.locations);
        assert_eq!(decoded.payload_inventory_sha256, inventory);
        assert_eq!(
            decoded.options.delete_indexes,
            if purge {
                IndexDeletion::Purge
            } else {
                IndexDeletion::Keep
            }
        );
        assert!(!decoded.options.start && !decoded.options.login_startup);
        assert_eq!(decoded.expires_unix_ms - decoded.created_unix_ms, 600_000);
    }
}
