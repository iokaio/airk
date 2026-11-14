// SPDX-License-Identifier: Apache-2.0
use super::*;
use serde_json::Value;

const FIXTURE: &str = include_str!("../../../../../fixtures/split/i4/setup_recovery.json");
const LEGACY: &[u8] = include_bytes!("../../../../../fixtures/split/i4/i4_cases.json");

fn fixture() -> Value {
    serde_json::from_str(FIXTURE).expect("independent recovery cases")
}

fn text(case: &Value, key: &str) -> Vec<u8> {
    case[key]
        .as_str()
        .expect("canonical fixture text")
        .as_bytes()
        .to_vec()
}

fn rows<'a>(fixture: &'a Value, key: &str) -> &'a [Value] {
    let rows = fixture[key].as_array().expect("fixture cases");
    assert!(!rows.is_empty(), "{key}: zero cases");
    rows
}

fn operation(case: &Value) -> SetupOperation {
    serde_json::from_slice(&text(case, "operation")).expect("literal operation")
}

fn publication(name: &str) -> PublicationReceipt {
    let fixture = fixture();
    let case = rows(&fixture, "publication_cases")
        .iter()
        .find(|row| row["name"] == name)
        .unwrap();
    PublicationReceipt::decode(&text(case, "text"), &operation(case)).expect("valid publication")
}

fn retained() -> RetainedInstallation {
    let fixture = fixture();
    RetainedInstallation::decode(&text(&fixture["retained_cases"][0], "text"))
        .expect("valid retained roots")
}

fn outcome<T: Serialize>(observed: Result<T, Refusal>, expected_bytes: &[u8]) -> String {
    match observed {
        Ok(value) => {
            assert_eq!(canonical(&value), expected_bytes, "exact canonical bytes");
            "ok".into()
        }
        Err(error) => error.code().into(),
    }
}

#[test]
fn publication_contract_decodes_independent_bytes_without_changing_retained_i4_fixtures() {
    let fixture = fixture();
    assert_eq!(
        sha256(LEGACY).to_string(),
        fixture["legacy_i4_sha256"].as_str().unwrap()
    );
    for case in rows(&fixture, "publication_cases") {
        let bytes = text(case, "text");
        assert_eq!(
            outcome(PublicationReceipt::decode(&bytes, &operation(case)), &bytes),
            case["expect"].as_str().unwrap(),
            "{}",
            case["name"]
        );
    }
}

#[test]
fn publication_observations_accept_only_the_ordered_exact_pair_states() {
    let receipt = publication("upgrade");
    let prior = receipt.prior.as_ref().unwrap();
    let before_pointer = canonical(&prior.pointer);
    let before_record = canonical(&prior.record);
    let after_pointer = canonical(&receipt.candidate.pointer);
    let after_record = canonical(&receipt.candidate.record);
    let fixture = fixture();
    for case in rows(&fixture, "positions") {
        let select = |role: &str, old: &[u8], new: &[u8]| match case[role].as_str() {
            Some("prior") => Some(old.to_vec()),
            Some("candidate") => Some(new.to_vec()),
            Some("foreign") => Some(b"foreign bytes\n".to_vec()),
            None => None,
            other => panic!("unrecognized literal fixture selector {other:?}"),
        };
        let pointer = select("pointer", &before_pointer, &after_pointer);
        let record = select("candidate_record", &before_record, &after_record);
        let previous = select("prior_record", &before_record, &after_record);
        let observed = receipt.observe(pointer.as_deref(), record.as_deref(), previous.as_deref());
        let label = match observed {
            Ok(PublicationPosition::BeforeRecord) => "before_record",
            Ok(PublicationPosition::RecordPublished) => "record_published",
            Ok(PublicationPosition::PairPublished) => "pair_published",
            Err(_) => "refused",
        };
        assert_eq!(label, case["expect"].as_str().unwrap(), "{}", case["name"]);
    }
}

#[test]
fn fresh_and_moved_publication_states_require_exact_original_path_observations() {
    let fresh = publication("fresh");
    let candidate_record = canonical(&fresh.candidate.record);
    let candidate_pointer = canonical(&fresh.candidate.pointer);
    assert_eq!(
        fresh.observe(None, None, None),
        Ok(PublicationPosition::BeforeRecord)
    );
    assert_eq!(
        fresh.observe(None, Some(&candidate_record), None),
        Ok(PublicationPosition::RecordPublished)
    );
    assert_eq!(
        fresh.observe(Some(&candidate_pointer), Some(&candidate_record), None),
        Ok(PublicationPosition::PairPublished)
    );
    assert!(fresh.observe(None, Some(b"foreign"), None).is_err());

    let moved = publication("moved");
    let prior = moved.prior.as_ref().unwrap();
    let prior_record = canonical(&prior.record);
    let prior_pointer = canonical(&prior.pointer);
    let candidate_record = canonical(&moved.candidate.record);
    let candidate_pointer = canonical(&moved.candidate.pointer);
    assert_eq!(
        moved.observe(Some(&prior_pointer), None, Some(&prior_record)),
        Ok(PublicationPosition::BeforeRecord)
    );
    assert_eq!(
        moved.observe(
            Some(&prior_pointer),
            Some(&candidate_record),
            Some(&prior_record)
        ),
        Ok(PublicationPosition::RecordPublished)
    );
    assert_eq!(
        moved.observe(
            Some(&candidate_pointer),
            Some(&candidate_record),
            Some(&prior_record)
        ),
        Ok(PublicationPosition::PairPublished)
    );
    assert!(
        moved
            .observe(Some(&candidate_pointer), Some(&candidate_record), None)
            .is_err()
    );
    assert!(
        moved
            .observe(Some(&prior_pointer), None, Some(b"foreign"))
            .is_err()
    );
}

#[test]
fn retained_installation_bytes_refuse_wrong_operations_roles_and_native_identity_shapes() {
    let fixture = fixture();
    for case in rows(&fixture, "retained_cases") {
        let bytes = text(case, "text");
        assert_eq!(
            outcome(RetainedInstallation::decode(&bytes), &bytes),
            case["expect"].as_str().unwrap(),
            "{}",
            case["name"]
        );
    }
    assert!(
        RootIdentity::Unix {
            device: 1,
            inode: 0
        }
        .validate()
        .is_err()
    );
}

#[test]
fn retained_reinstall_requires_the_matching_closed_successful_keep_data_receipt() {
    let retained = retained();
    let fixture = fixture();
    for case in rows(&fixture, "completion_cases") {
        let receipt =
            SetupReceipt::decode(&text(case, "text")).expect("independent valid I4 receipt shape");
        let observed = retained
            .validate_completion(&receipt)
            .map_or_else(|error| error.code(), |()| "ok");
        assert_eq!(
            observed,
            case["expect"].as_str().unwrap(),
            "{}",
            case["name"]
        );
    }
}

#[test]
fn recovery_decoders_bound_bytes_before_parsing_and_reject_noncanonical_documents() {
    let receipt = publication("upgrade");
    let fixture = fixture();
    let operation = operation(&fixture["publication_cases"][0]);
    let oversize = vec![b' '; MAX_RECOVERY_BYTES + 1];
    assert_eq!(
        PublicationReceipt::decode(&oversize, &operation).unwrap_err(),
        Refusal::Oversize
    );
    assert_eq!(
        RetainedInstallation::decode(&oversize).unwrap_err(),
        Refusal::Oversize
    );
    let mut changed = canonical(&receipt);
    changed.pop();
    assert_eq!(
        PublicationReceipt::decode(&changed, &operation).unwrap_err(),
        Refusal::NonCanonical
    );
    let mut changed = canonical(&retained());
    changed.pop();
    assert_eq!(
        RetainedInstallation::decode(&changed).unwrap_err(),
        Refusal::NonCanonical
    );
}
