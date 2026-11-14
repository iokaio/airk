// SPDX-License-Identifier: Apache-2.0
use super::*;
use serde_json::Value;

const FIXTURE: &str = include_str!("../../../../../../fixtures/split/i4/setup_active.json");

fn fixture() -> Value {
    serde_json::from_str(FIXTURE).expect("independent active setup fixture")
}
fn rows<'a>(fixture: &'a Value, key: &str) -> &'a [Value] {
    let rows = fixture[key].as_array().expect("case list");
    assert!(!rows.is_empty(), "{key}: zero cases");
    rows
}
fn decode_case(case: &Value) -> Result<ActiveSetup, Refusal> {
    let operation: SetupOperation =
        serde_json::from_str(case["operation"].as_str().unwrap()).unwrap();
    ActiveSetup::decode(case["text"].as_str().unwrap().as_bytes(), &operation)
}
fn active(name: &str) -> ActiveSetup {
    let fixture = fixture();
    decode_case(
        rows(&fixture, "decode_cases")
            .iter()
            .find(|case| case["name"] == name)
            .expect("named independent case"),
    )
    .expect("valid owner")
}
fn label(result: Result<(), Refusal>) -> &'static str {
    if result.is_ok() { "ok" } else { "refused" }
}

#[test]
fn active_setup_exact_canonical_bytes_and_foreign_bindings_match_independent_cases() {
    let fixture = fixture();
    for case in rows(&fixture, "decode_cases") {
        let observed = match decode_case(case) {
            Ok(value) => {
                assert_eq!(canonical(&value), case["text"].as_str().unwrap().as_bytes());
                "ok"
            }
            Err(error) => error.code(),
        };
        assert_eq!(
            observed,
            case["expect"].as_str().unwrap(),
            "{}",
            case["name"]
        );
    }
}

#[test]
fn active_setup_interruptions_preserve_owner_and_only_allow_ordered_successors() {
    let fixture = fixture();
    for case in rows(&fixture, "transitions") {
        let before = active(case["before"].as_str().unwrap());
        let after = active(case["after"].as_str().unwrap());
        assert_eq!(
            label(after.validate_successor(&before)),
            case["expect"].as_str().unwrap(),
            "{}",
            case["name"]
        );
    }
    let before = active("prepared");
    let mut rebound = before.clone();
    rebound.sequence += 1;
    rebound.selected_roots.program_replacement = active("program_prepared")
        .selected_roots
        .program_replacement;
    assert!(
        rebound.validate().is_ok(),
        "complete claims alone do not establish transition authority"
    );
    assert!(
        rebound.validate_successor(&before).is_err(),
        "a wrapper cannot introduce staging identities after prepare"
    );
    let mut exhausted = before.clone();
    exhausted.sequence = u64::MAX;
    let mut wrapped = exhausted.clone();
    wrapped.sequence = 1;
    assert!(
        wrapped.validate_successor(&exhausted).is_err(),
        "sequence exhaustion never wraps ownership"
    );
}

#[test]
fn active_setup_competing_prepares_never_take_over_on_expiry_or_process_loss() {
    let owner = active("prepared");
    owner.admit_same_operation(&owner.operation).unwrap();
    let after_expiry = owner.operation.expires_unix_ms + 1;
    let mut competing = owner.operation.clone();
    competing.operation_id = Incarnation::from_bytes([0xee; 16]);
    competing.created_unix_ms = after_expiry;
    competing.expires_unix_ms = after_expiry + 60_000;
    assert!(SetupOperation::decode(&canonical(&competing), after_expiry).is_ok());
    assert!(
        owner.admit_same_operation(&competing).is_err(),
        "a valid newer request cannot own an unsettled placement"
    );
    let mut changed = owner.operation.clone();
    changed.payload_inventory_sha256 = Sha256Digest::from_bytes([0xee; 32]);
    assert!(owner.admit_same_operation(&changed).is_err());
    assert!(
        ActiveSetup::decode(&canonical(&owner), &owner.operation).is_ok(),
        "same exact operation resumes against its original admission"
    );
}

#[test]
fn active_setup_live_roots_refuse_replacements_and_require_prebound_displaced_program_proof() {
    let fixture = fixture();
    for case in rows(&fixture, "live_cases") {
        let owner = active(case["active"].as_str().unwrap());
        let authority: RootIdentity = serde_json::from_value(case["authority"].clone()).unwrap();
        let selected: RootSnapshot = serde_json::from_value(case["selected"].clone()).unwrap();
        let prior: Option<RootSnapshot> = serde_json::from_value(case["prior"].clone()).unwrap();
        let displaced: Option<RootObservation> =
            serde_json::from_value(case["displaced_program"].clone()).unwrap();
        assert_eq!(
            label(owner.validate_live_roots(
                &authority,
                &selected,
                prior.as_ref(),
                displaced.as_ref()
            )),
            case["expect"].as_str().unwrap(),
            "{}",
            case["name"]
        );
    }
}

#[test]
fn active_setup_startup_and_os_manager_eligibility_require_the_published_exact_candidate() {
    let owner = active("pair_published");
    let Target::Place { publication } = &owner.target else {
        panic!("fixture places a pair")
    };
    let install = publication.candidate.record.install_id;
    let main = publication.candidate.record.executables.airk.sha256;
    for name in [
        "admitted",
        "prepared",
        "remove_prepared",
        "remove_removal_applied",
        "settled",
    ] {
        assert!(
            !active(name).candidate_start_allowed(install, main),
            "{name} must not authorize startup"
        );
    }
    assert!(owner.candidate_start_allowed(install, main));
    assert!(!owner.candidate_start_allowed(Incarnation::from_bytes([0xee; 16]), main));
    assert!(!owner.candidate_start_allowed(install, Sha256Digest::from_bytes([0xee; 32])));
}

#[test]
fn active_setup_settlement_requires_ordered_successful_matching_terminal_receipts() {
    let fixture = fixture();
    for case in rows(&fixture, "settlements") {
        let owner = active(case["active"].as_str().unwrap());
        let receipt: SetupReceipt =
            serde_json::from_str(case["receipt"].as_str().unwrap()).unwrap();
        assert_eq!(
            label(owner.validate_settlement(&receipt)),
            case["expect"].as_str().unwrap(),
            "{}",
            case["name"]
        );
    }
}

#[test]
fn active_setup_decoders_refuse_oversize_noncanonical_and_unknown_schema() {
    let owner = active("admitted");
    assert_eq!(
        ActiveSetup::decode(&vec![b' '; MAX_RECOVERY_BYTES + 1], &owner.operation).unwrap_err(),
        Refusal::Oversize
    );
    let mut bytes = canonical(&owner);
    bytes.pop();
    assert_eq!(
        ActiveSetup::decode(&bytes, &owner.operation).unwrap_err(),
        Refusal::NonCanonical
    );
    let mut unknown = owner.clone();
    unknown.schema = "airk-active-setup/2".into();
    assert_eq!(
        ActiveSetup::decode(&canonical(&unknown), &owner.operation).unwrap_err(),
        Refusal::UnknownSchema
    );
}
