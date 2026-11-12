// SPDX-License-Identifier: Apache-2.0
//! Pure setup decisions against reviewed expectations:
//! `fixtures/split/i4/interruptions.json` for resume, and literal cases for
//! admission, acknowledgement and the location rules.
use super::plan::{self, Action, Mode};
use airk_core::{
    identity::{Incarnation, Sha256Digest},
    install_record::{
        Acknowledgements, IndexDeletion, Locations, OperationKind, SETUP_OPERATION_SCHEMA,
        SETUP_RECEIPT_SCHEMA, SetupOperation, SetupOptions, SetupPhase, SetupReceipt, TrustChoice,
    },
};
use serde_json::Value;
use std::{collections::BTreeSet, path::PathBuf};

const INTERRUPTIONS: &str = include_str!("../../../../fixtures/split/i4/interruptions.json");

fn parse<T: serde::de::DeserializeOwned>(value: &Value) -> T {
    serde_json::from_value(value.clone()).expect("fixture value")
}

#[test]
fn resumes_run_the_reviewed_remaining_actions() {
    let fixture: Value = serde_json::from_str(INTERRUPTIONS).unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    assert!(!cases.is_empty());
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let kind: OperationKind = parse(&case["kind"]);
        let options: SetupOptions = parse(&fixture["options"][case["options"].as_str().unwrap()]);
        let mode: Mode = parse(&case["mode"]);
        let recorded: Vec<SetupPhase> = parse(&case["recorded"]);
        let observed = plan::remaining(kind, &options, mode, &recorded);
        match case.get("refused") {
            Some(code) => assert_eq!(observed, Err(code.as_str().unwrap()), "{name}"),
            None => {
                let expected: Vec<Action> = parse(&case["expect"]);
                assert_eq!(observed, Ok(expected), "{name}");
            }
        }
    }
}

/// A host-absolute fixture home (Windows paths need a drive).
fn base() -> &'static str {
    if cfg!(windows) {
        "C:/Users/fixture"
    } else {
        "/home/fixture"
    }
}

fn locations(root: &str) -> Locations {
    let join = |leaf: &str| format!("{root}{leaf}");
    Locations {
        program: join("/Programs/AIRK"),
        data: join("/AIRK/data/indexes"),
        state: join("/AIRK/state/service"),
        cache: join("/AIRK/cache/service"),
        logs: join("/AIRK/logs/service"),
    }
}

fn operation(kind: OperationKind) -> SetupOperation {
    SetupOperation {
        schema: SETUP_OPERATION_SCHEMA.into(),
        operation_id: Incarnation::from_bytes([0x0e; 16]),
        kind,
        created_unix_ms: 1_800_000_000_000,
        expires_unix_ms: 1_800_000_600_000,
        requested_by: "inno".into(),
        acknowledgements: Acknowledgements {
            predecessor_removal: false,
            index_reset: false,
        },
        locations: locations(base()),
        payload_inventory_sha256: Sha256Digest::from_bytes([5; 32]),
        options: SetupOptions {
            login_startup: true,
            trust: TrustChoice::Ask,
            start: true,
            delete_indexes: IndexDeletion::Keep,
        },
    }
}

fn receipt(operation_sha256: Sha256Digest, closed: bool) -> SetupReceipt {
    SetupReceipt {
        schema: SETUP_RECEIPT_SCHEMA.into(),
        operation_id: Incarnation::from_bytes([0x0e; 16]),
        operation_sha256,
        kind: OperationKind::Install,
        expires_unix_ms: 1_800_000_600_000,
        steps: Vec::new(),
        retired: Vec::new(),
        leftovers: Vec::new(),
        failure: None,
        closed,
    }
}

#[test]
fn a_receipt_admits_only_its_own_open_operation() {
    let bytes = Sha256Digest::from_bytes([1; 32]);
    let install = operation(OperationKind::Install);
    assert_eq!(
        plan::admit_resume(&receipt(bytes, false), &install, bytes),
        Ok(())
    );
    assert_eq!(
        plan::admit_resume(&receipt(bytes, true), &install, bytes),
        Err("setup_operation_consumed")
    );
    assert_eq!(
        plan::admit_resume(
            &receipt(bytes, false),
            &install,
            Sha256Digest::from_bytes([2; 32])
        ),
        Err("setup_operation_changed")
    );
    assert_eq!(
        plan::admit_resume(
            &receipt(bytes, false),
            &operation(OperationKind::Repair),
            bytes
        ),
        Err("setup_operation_changed")
    );
}

#[test]
fn predecessors_need_both_acknowledgements() {
    let mut install = operation(OperationKind::Install);
    assert_eq!(plan::acknowledged(&install, 0), Ok(()));
    assert_eq!(
        plan::acknowledged(&install, 1),
        Err("predecessor_acknowledgement_required")
    );
    install.acknowledgements.predecessor_removal = true;
    assert_eq!(
        plan::acknowledged(&install, 1),
        Err("predecessor_acknowledgement_required")
    );
    install.acknowledgements.index_reset = true;
    assert_eq!(plan::acknowledged(&install, 2), Ok(()));
}

#[test]
fn chosen_locations_may_not_nest_or_overlap_earlier_roots() {
    let base = base();
    let good = locations(base);
    let earlier = vec![
        PathBuf::from(format!("{base}/AIRK/service-1.4")),
        PathBuf::from(format!("{base}/AIRK/state/service-1.4")),
    ];
    assert_eq!(
        plan::location_conflicts(&good, &earlier, false),
        Vec::<String>::new()
    );

    let mut nested = good.clone();
    nested.cache = format!("{base}/AIRK/data/indexes/cache");
    let problems = plan::location_conflicts(&nested, &earlier, false);
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(problems[0].contains("data location") && problems[0].contains("cache location"));

    let mut inside = good.clone();
    inside.data = format!("{base}/AIRK/service-1.4/indexes");
    assert_eq!(plan::location_conflicts(&inside, &earlier, false).len(), 1);

    let mut containing = good.clone();
    containing.logs = format!("{base}/AIRK/state");
    // It contains the earlier state root and the chosen state location.
    assert_eq!(
        plan::location_conflicts(&containing, &earlier, false).len(),
        2
    );

    let mut top = good.clone();
    top.program = "/home".into();
    assert!(!plan::location_conflicts(&top, &[], false).is_empty());

    let mut relative = good;
    relative.state = "state/service".into();
    assert!(!plan::location_conflicts(&relative, &[], false).is_empty());
}

#[test]
fn windows_comparison_ignores_case() {
    let base = base();
    let mut chosen = locations(base);
    chosen.data = format!("{}/airk/SERVICE-1.4", base.to_uppercase());
    let earlier = vec![PathBuf::from(format!("{base}/AIRK/service-1.4"))];
    assert_eq!(plan::location_conflicts(&chosen, &earlier, true).len(), 1);
    assert!(plan::location_conflicts(&chosen, &earlier, false).is_empty());
}

#[test]
fn a_program_folder_holds_only_listed_and_owned_files() {
    let listed: BTreeSet<String> = ["bin/airkctl.exe", "service/airk.exe", "notices/LICENSE"]
        .into_iter()
        .map(str::to_owned)
        .collect();
    let entries: Vec<String> = [
        "bin/airkctl.exe",
        "install-record.json",
        "notices/LICENSE",
        "payload-inventory.json",
        "service/airk.exe",
        "unins000.dat",
        "unins000.exe",
        "unins01.exe",
        "uninsXYZ.exe",
        "readme.txt",
        "service/airk-old.exe",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    assert_eq!(
        plan::foreign_program_entries(&entries, &listed),
        vec![
            "unins01.exe".to_owned(),
            "uninsXYZ.exe".to_owned(),
            "readme.txt".to_owned(),
            "service/airk-old.exe".to_owned(),
        ]
    );
}

#[test]
fn program_relative_paths_stay_inside() {
    for good in [
        "bin/airkctl.exe",
        "service/airk.exe",
        "notices/inno-setup/LICENSE.txt",
    ] {
        assert!(plan::relative_inside(good), "{good}");
    }
    for bad in [
        "",
        "/bin/airkctl",
        "../airk.exe",
        "bin/../x",
        "bin\\airkctl.exe",
        "C:/x",
        "bin//x",
        "./x",
    ] {
        assert!(!plan::relative_inside(bad), "{bad}");
    }
    assert_eq!(
        plan::program_file(&PathBuf::from("/opt/airk"), "service/airk"),
        PathBuf::from("/opt/airk").join("service").join("airk")
    );
}

#[test]
fn this_airk_pairs_with_airkctls_i2_contract() {
    assert_eq!(super::I2_CONTRACT, airkctl::contract::I2_CONTRACT);
}
