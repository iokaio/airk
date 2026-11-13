// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::index_catalog::{IndexId, QueryProjectId};

const LEDGER_V1: &str =
    include_str!("../../../../fixtures/split/agents/credentials_ledger_v1.json");

fn ledger_v1() -> (serde_json::Value, String) {
    let fixture: serde_json::Value = serde_json::from_str(LEDGER_V1).unwrap();
    assert_eq!(
        fixture["schema"],
        "airk.split-fixture.agents-credentials-ledger/1"
    );
    let bytes = fixture["bytes"].as_str().unwrap().to_owned();
    (fixture, bytes)
}

/// U106(b) format proof: the ledger bytes captured from the pre-U105 code
/// parse and re-serialize byte-identically; no record gains an `open` key.
#[test]
fn credentials_ledger_v1_bytes_are_unchanged() -> Result<()> {
    use sha2::{Digest, Sha256};
    let (fixture, bytes) = ledger_v1();
    assert_eq!(
        format!("{:x}", Sha256::digest(bytes.as_bytes())),
        fixture["bytes_sha256"].as_str().unwrap()
    );
    let ledger = Ledger::parse(bytes.as_bytes())?;
    assert_eq!(serde_json::to_vec(&ledger)?, bytes.as_bytes());
    assert!(!bytes.contains("\"open\""));
    assert!(ledger.records.iter().all(|record| record.open.is_none()));
    // The captured attachment path is a Windows path; validate there only.
    if cfg!(windows) {
        ledger.validate(&ledger.binding)?;
    }
    Ok(())
}

fn open_record(ledger: &Ledger) -> Record {
    let mut record = ledger.records[0].clone();
    record.id = Incarnation::from_bytes([0x23; 16]);
    record.selection = HttpSelection::Open {
        selection: crate::service_http::OpenHttpSelection {
            namespace: ledger.binding.namespace,
        },
    };
    record.open = Some(OpenConnectionState::initial());
    record
}

/// The `open` state is present exactly when the selection is `Open`, is
/// bounded, and serializes last, after the pre-U105 fields.
#[test]
fn open_record_state_is_present_exactly_for_open_selections() -> Result<()> {
    let (_, bytes) = ledger_v1();
    let mut ledger = Ledger::parse(bytes.as_bytes())?;
    ledger.records.truncate(1);
    ledger.validate(&ledger.binding)?;
    let open = open_record(&ledger);
    let text = String::from_utf8(serde_json::to_vec(&open)?)?;
    assert!(text.ends_with(
        ",\"revoked\":false,\"open\":{\"defaults_revision\":1,\"default_index_ids\":[],\"default_index_set_ids\":[],\"agent_choice\":\"off\"}}"
    ), "{text}");
    assert!(text.contains("\"selection\":{\"kind\":\"open\",\"selection\":{\"namespace\":\"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\"}}"));
    let mut valid = ledger.clone();
    valid.records.push(open.clone());
    valid.validate(&valid.binding)?;
    let reparsed = Ledger::parse(&serde_json::to_vec(&valid)?)?;
    assert_eq!(serde_json::to_vec(&reparsed)?, serde_json::to_vec(&valid)?);

    let mut missing = ledger.clone();
    let mut record = open.clone();
    record.open = None;
    missing.records.push(record);
    assert!(missing.validate(&missing.binding).is_err());

    let mut stray = ledger.clone();
    stray.records[0].open = Some(OpenConnectionState::initial());
    assert!(stray.validate(&stray.binding).is_err());

    for change in [
        (|state: &mut OpenConnectionState| state.defaults_revision = 0)
            as fn(&mut OpenConnectionState),
        |state| {
            state.default_index_ids = (0..33)
                .map(|n| IndexId::from_identity(Incarnation::from_bytes([n; 16])))
                .collect()
        },
        |state| {
            let id = QueryProjectId::from_identity(Incarnation::from_bytes([1; 16]));
            state.default_index_set_ids = vec![id, id];
        },
    ] {
        let mut invalid = ledger.clone();
        let mut record = open.clone();
        change(record.open.as_mut().unwrap());
        invalid.records.push(record);
        assert!(invalid.validate(&invalid.binding).is_err());
    }
    // An unknown choice or key is refused at decode.
    let on = text.replace("\"agent_choice\":\"off\"", "\"agent_choice\":\"maybe\"");
    assert!(serde_json::from_str::<Record>(&on).is_err());
    let extra = text.replace(
        "\"agent_choice\":\"off\"",
        "\"agent_choice\":\"on\",\"x\":1",
    );
    assert!(serde_json::from_str::<Record>(&extra).is_err());
    Ok(())
}

/// Open equals open only within the same namespace; never another kind.
#[test]
fn open_selections_share_authority_only_with_open_selections() -> Result<()> {
    let (_, bytes) = ledger_v1();
    let ledger = Ledger::parse(bytes.as_bytes())?;
    let open = open_record(&ledger).selection;
    assert!(same_authority(&open, &open));
    assert!(!same_authority(&open, &ledger.records[0].selection));
    assert!(!same_authority(&ledger.records[0].selection, &open));
    let other = HttpSelection::Open {
        selection: crate::service_http::OpenHttpSelection {
            namespace: Incarnation::from_bytes([9; 16]),
        },
    };
    assert!(!same_authority(&open, &other));
    Ok(())
}
