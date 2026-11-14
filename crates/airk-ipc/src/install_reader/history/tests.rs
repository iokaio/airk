// SPDX-License-Identifier: Apache-2.0
use super::*;
use airk_core::install_record::{SetupFailure, canonical};
use sha2::{Digest, Sha256};

fn digest(bytes: &[u8]) -> Sha256Digest {
    Sha256Digest::from_bytes(Sha256::digest(bytes).into())
}
fn identity(root: &OpenedDirectory) -> RootIdentity {
    match root.identity().unwrap() {
        ObjectIdentity::Unix { device, inode } => RootIdentity::Unix { device, inode },
        ObjectIdentity::Windows {
            volume_serial,
            file_id,
        } => RootIdentity::Windows {
            volume_serial,
            file_id: file_id.iter().map(|byte| format!("{byte:02x}")).collect(),
        },
    }
}
fn write(root: &OpenedDirectory, name: &str, bytes: &[u8]) {
    crate::private_record::write_private_at(root, name, bytes, MAX_RECOVERY_BYTES, true).unwrap();
}

#[test]
fn only_exact_settled_private_history_and_successful_receipt_prove_full_inventory() {
    let temporary = tempfile::tempdir().unwrap();
    let base = temporary.path().canonicalize().unwrap().join("private");
    #[cfg(windows)]
    let base = std::path::PathBuf::from(base.to_str().unwrap().trim_start_matches(r"\\?\"));
    for path in [
        &base,
        &base.join("authority"),
        &base.join("state"),
        &base.join("state/setup"),
    ] {
        crate::endpoint::prepare_root(path).unwrap();
    }
    let authority = OpenedDirectory::open(&base.join("authority")).unwrap();
    let setup = OpenedDirectory::open(&base.join("state/setup")).unwrap();
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../fixtures/split/i4/setup_active.json"
    ))
    .unwrap();
    let row = fixture["decode_cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["name"] == "settled")
        .unwrap();
    let value: serde_json::Value = serde_json::from_str(row["text"].as_str().unwrap()).unwrap();
    #[cfg(unix)]
    let value = {
        let mut value = value;
        fn unix_facts(value: &mut serde_json::Value) {
            if value.get("file_id").is_some() {
                let id = u64::from_str_radix(value["file_id"].as_str().unwrap(), 16).unwrap();
                *value = serde_json::json!({"platform":"unix","device":17,"inode":id});
                return;
            }
            match value {
                serde_json::Value::Array(rows) => {
                    for row in rows {
                        unix_facts(row)
                    }
                }
                serde_json::Value::Object(map) => {
                    for row in map.values_mut() {
                        unix_facts(row)
                    }
                }
                _ => {}
            }
        }
        unix_facts(&mut value);
        for image in ["candidate", "prior"] {
            value["target"]["publication"][image]["record"]["signing"] =
                serde_json::json!({"platform":"linux","signed":false,"integrity":"record-sha256"});
        }
        value
    };
    let mut active: ActiveSetup = serde_json::from_value(value).unwrap();
    for (role, path) in [
        ("program", base.join("program")),
        ("data", base.join("data")),
        ("state", base.join("state")),
        ("cache", base.join("cache")),
        ("logs", base.join("logs")),
    ] {
        let text = path.to_string_lossy().into_owned();
        match role {
            "program" => active.operation.locations.program = text,
            "data" => active.operation.locations.data = text,
            "state" => active.operation.locations.state = text,
            "cache" => active.operation.locations.cache = text,
            _ => active.operation.locations.logs = text,
        }
    }
    active.operation_sha256 = digest(&canonical(&active.operation));
    active.authority_root = identity(&authority);
    let Target::Place { publication } = &mut active.target else {
        panic!("placing fixture")
    };
    publication.operation_sha256 = active.operation_sha256;
    for image in std::iter::once(&mut publication.candidate).chain(publication.prior.iter_mut()) {
        image.record.locations = active.operation.locations.clone();
        image.pointer.program = active.operation.locations.program.clone();
        image.pointer.record_sha256 = digest(&canonical(&image.record));
    }
    let image = publication.candidate.clone();
    active.validate().unwrap();
    let receipt_row = fixture["settlements"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["active"] == "settled" && row["expect"] == "ok")
        .unwrap();
    let mut receipt: SetupReceipt =
        serde_json::from_str(receipt_row["receipt"].as_str().unwrap()).unwrap();
    receipt.operation_sha256 = active.operation_sha256;
    let pointer = base.join("authority/current.json");
    assert!(completed_inventory(&pointer, &image).unwrap().is_none());
    write(&authority, "active-setup.json", &canonical(&active));
    write(
        &setup,
        &format!("{}.json", active.operation.operation_id),
        &canonical(&receipt),
    );
    assert_eq!(
        completed_inventory(&pointer, &image).unwrap(),
        Some(active.operation.payload_inventory_sha256)
    );
    let history = crate::endpoint::create_root_at(&authority, OsStr::new("setup-history")).unwrap();
    write(
        &history,
        &format!("{}.json", active.operation.operation_id),
        &canonical(&active),
    );
    crate::private_record::remove_private_at(&authority, "active-setup.json", &canonical(&active))
        .unwrap();
    assert_eq!(
        completed_inventory(&pointer, &image).unwrap(),
        Some(active.operation.payload_inventory_sha256)
    );
    let mut changed = image.clone();
    changed.record.installed_by.installed_unix_ms += 1;
    changed.pointer.record_sha256 = digest(&canonical(&changed.record));
    assert!(completed_inventory(&pointer, &changed).is_err());
    receipt.failure = Some(SetupFailure {
        code: "fixture_failure".into(),
        message: "failed".into(),
        at_unix_ms: receipt.expires_unix_ms,
    });
    write(
        &setup,
        &format!("{}.json", active.operation.operation_id),
        &canonical(&receipt),
    );
    assert!(completed_inventory(&pointer, &image).is_err());
}
