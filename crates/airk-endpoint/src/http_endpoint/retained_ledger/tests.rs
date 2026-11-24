// SPDX-License-Identifier: Apache-2.0
use super::*;
use serde_json::json;

fn binding(root: &Path) -> Result<InstallationBinding> {
    InstallationBinding::derive(
        root,
        &crate::local_ipc::current_user()?,
        Incarnation::from_bytes([2; 16]),
    )
}

fn id(byte: u8) -> Incarnation {
    Incarnation::from_bytes([byte; 16])
}

fn write(root: &Path, name: &str, bytes: &[u8]) -> Result<()> {
    std::fs::write(root.join(name), bytes)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(root.join(name), std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

/// A ledger with the exact record shapes a pre-B installation wrote: a scoped
/// grant connection, an open connection with its stored defaults and agent
/// choice, an expired one and a revoked one. Written by hand, independently
/// of the reader.
fn old_install(root: &Path, binding: &InstallationBinding) -> Result<Vec<u8>> {
    let namespace = binding.namespace;
    let scoped = json!({"kind":"scoped","selection":{"schema":1,
        "context_key":"06".repeat(32),"namespace":namespace,"expected_catalog_revision":1,
        "grant_id":format!("grt_{}", "05".repeat(16)),"expected_grant_revision":1,
        "index_id":format!("idx_{}", "05".repeat(16)),"project_ids":[format!("prj_{}", "05".repeat(16))]}});
    let open = json!({"kind":"open","selection":{"namespace":namespace}});
    let record = |seed: u8, selection: &Value, expires: u64, revoked: bool| {
        let mut value = json!({"client":"codex","id":id(seed),"generation":1,
            "selection":selection,"explorer_descriptor":null,"expires_unix_ms":expires,
            "verifier":format!("{seed:02x}").repeat(32),"revoked":revoked});
        if revoked {
            value["revocation_reason"] = json!("explicit");
        }
        value
    };
    let mut open_record = record(11, &open, u64::MAX, false);
    open_record["open"] = json!({"defaults_revision":3,"default_index_ids":[],
        "default_index_set_ids":[],"agent_choice":"on"});
    let ledger = json!({"schema":1,"binding":binding,"records":[
        record(10, &scoped, u64::MAX, false), open_record,
        record(12, &scoped, 5, false), record(13, &scoped, u64::MAX, true)]});
    let bytes = serde_json::to_vec(&ledger)?;
    write(root, STORE, &bytes)?;
    // The active tokens are exactly 103 bytes; the revoked one left a marker.
    for seed in [10, 11, 12] {
        let token = format!("airk1.{}.{}", id(seed), "f".repeat(64));
        assert_eq!(token.len(), MAX_TOKEN);
        write(root, &filename(id(seed)), token.as_bytes())?;
    }
    let marker = json!({"schema":2,"binding":binding,"id":id(13),"revoked":true,
        "retirement":{"reason":"explicit","client":"codex","expires_unix_ms":u64::MAX,
        "explorer_descriptor_sha256":null,"authority_sha256":"07".repeat(32)}});
    write(root, &filename(id(13)), &serde_json::to_vec(&marker)?)?;
    // A revoked connection compacted out of the ledger keeps only its marker.
    let compacted = json!({"schema":1,"binding":binding,"id":id(14),"revoked":true});
    write(root, &filename(id(14)), &serde_json::to_vec(&compacted)?)?;
    Ok(bytes)
}

#[test]
fn an_old_install_ledger_is_recognised_for_cleanup_and_left_byte_identical() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    let root = temporary.path().join("http-1");
    crate::local_ipc::prepare_root(&root)?;
    let binding = binding(&root)?;
    let bytes = old_install(&root, &binding)?;
    let ledger = RetainedLedger::read_existing(&root, binding.clone())?;
    let owners = ledger.cleanup_owners(10)?;
    let summary: Vec<_> = owners
        .iter()
        .map(|owner| {
            (
                owner.connection_id,
                owner.ownership,
                owner.credential_sha256,
            )
        })
        .collect();
    let verifier = |seed: u8| Some(Sha256Digest::from_bytes([seed; 32]));
    assert_eq!(
        summary,
        vec![
            (id(10), HttpCleanupOwnership::Active, verifier(10)),
            (id(11), HttpCleanupOwnership::Active, verifier(11)),
            (id(12), HttpCleanupOwnership::Expired, verifier(12)),
            (id(13), HttpCleanupOwnership::Revoked, verifier(13)),
            (id(14), HttpCleanupOwnership::Revoked, None),
        ]
    );
    // Read-only: the retained ledger is unchanged and nothing was created.
    assert_eq!(std::fs::read(root.join(STORE))?, bytes);
    assert_eq!(std::fs::read_dir(&root)?.count(), 6);
    assert!(!holds_no_credentials(&root)?);
    Ok(())
}

#[test]
fn another_binding_missing_or_malformed_ledgers_are_never_proof_of_foreign_ids() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    let root = temporary.path().join("http-1");
    crate::local_ipc::prepare_root(&root)?;
    let binding = binding(&root)?;
    // Missing: unknown, not an empty owner list.
    assert!(RetainedLedger::read_existing(&root, binding.clone()).is_err());
    assert!(holds_no_credentials(&root)?);
    old_install(&root, &binding)?;
    let mut other = binding.clone();
    other.namespace = id(9);
    assert!(RetainedLedger::read_existing(&root, other).is_err());
    for broken in [
        json!({"schema":2,"binding":binding,"records":[]}),
        json!({"schema":1,"binding":binding,"records":[{"id":id(1)}]}),
        json!({"schema":1,"binding":binding,"records":[
            {"id":id(1),"revoked":false,"expires_unix_ms":1,"verifier":"01".repeat(32)},
            {"id":id(1),"revoked":false,"expires_unix_ms":1,"verifier":"01".repeat(32)}]}),
    ] {
        write(&root, STORE, &serde_json::to_vec(&broken)?)?;
        assert!(RetainedLedger::read_existing(&root, binding.clone()).is_err());
    }
    // A token without its record is an orphan, never an owner.
    write(
        &root,
        STORE,
        &serde_json::to_vec(&json!({"schema":1,"binding":binding,"records":[]}))?,
    )?;
    let ledger = RetainedLedger::read_existing(&root, binding)?;
    assert!(ledger.cleanup_owners(10).is_err());
    Ok(())
}

/// The ledger bytes captured from the pre-U105 code
/// (`fixtures/split/agents/credentials_ledger_v1.json`) are recognised.
#[test]
fn the_captured_v1_ledger_is_recognised() -> Result<()> {
    use sha2::{Digest, Sha256};
    const LEDGER_V1: &str =
        include_str!("../../../../../fixtures/split/agents/credentials_ledger_v1.json");
    let fixture: Value = serde_json::from_str(LEDGER_V1)?;
    let bytes = fixture["bytes"].as_str().context("captured bytes")?;
    assert_eq!(
        format!("{:x}", Sha256::digest(bytes.as_bytes())),
        fixture["bytes_sha256"]
            .as_str()
            .context("captured digest")?
    );
    let captured: Value = serde_json::from_str(bytes)?;
    let binding: InstallationBinding = serde_json::from_value(captured["binding"].clone())?;
    let records = parse(bytes.as_bytes(), &binding)?;
    assert_eq!(
        records.len(),
        captured["records"].as_array().context("records")?.len()
    );
    assert!(!records.is_empty());
    Ok(())
}
