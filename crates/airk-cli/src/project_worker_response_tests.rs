// SPDX-License-Identifier: Apache-2.0
//! Actual worker response helpers preserve mandatory scope and transport shape.
use super::*;
use airk_core::{
    identity::{Incarnation, Sha256Digest},
    index_catalog::*,
};
use serde_json::{Value, json};

fn session(kernel: &Kernel) -> Result<Kernel> {
    let project = QueryProjectId::from_identity(Incarnation::from_bytes([1; 16]));
    let index = IndexId::from_identity(Incarnation::from_bytes([2; 16]));
    let source = SourceRootId::from_identity(Incarnation::from_bytes([3; 16]));
    // Deserialize the native wire payload so this exercises admission's actual
    // required authority fields rather than a mock observation renderer.
    let authority = serde_json::from_value(json!({
        "schema":1,
        "backing":airk_core::service::ProjectRegistration::new(
            kernel.root().to_str().unwrap().into(), kernel.data_dir().to_str().unwrap().into(),
            kernel.collection().into())?.binding,
        "index_id":index,"project_ids":[project],
        "provenance":airk_core::grant_provenance::GrantProvenance::UserReviewedProjectGrant,
        "grant":ReviewedGrant { id:GrantId::from_identity(Incarnation::from_bytes([4;16])),
            host:Sha256Digest::from_bytes([5;32]),revision:CatalogRevision::INITIAL,revoked:false,
            memberships:vec![GrantedMembership { membership:Membership { project_id:project,index_id:index,
                revision:CatalogRevision::INITIAL,scope:vec![ScopeAtom {source_root_id:source,
                    kind:PathScope::Subtree,path:"public".into()}]},source_revision:CatalogRevision::INITIAL,
                policy_epoch:0,policy_digest:None }] },
        "sources":[{"source_root_id":source,"original_root":kernel.root().to_str().unwrap()}]
    }))?;
    kernel.with_scoped_reads(authority)
}
fn body(reply: &Value) -> Value {
    serde_json::from_str(reply["content"][0]["text"].as_str().unwrap()).unwrap()
}

#[test]
fn scoped_worker_errors_and_admission_preserve_canonical_receipts() -> Result<()> {
    let root = tempfile::tempdir()?;
    let admin = Kernel::open(root.path(), None)?;
    let scoped = session(&admin)?;
    let expected = serde_json::to_value(scoped.scope_observation())?;
    let error = anyhow::Error::new(airk_core::scoped_reads::ScopeReadRefused)
        .context("private/path must not appear in the public refusal");
    let native = scoped_worker_error(&scoped, &error, true);
    let mcp = scoped_worker_error(&scoped, &error, false);
    assert_eq!(native, body(&mcp));
    assert_eq!(native["scope_observation"], expected);
    assert_eq!(mcp["isError"], true);
    assert!(!native.to_string().contains("private/path"));
    assert!(serde_json::to_vec(&mcp)?.len() + 2048 <= airk_core::MAX_RESPONSE_BYTES);
    // Worker helpers do not project away this mandatory identity for either
    // Full or Terse calls; both pass through this same outer error boundary.
    for reason in [
        airk_core::service::AdmissionError::Busy,
        airk_core::service::AdmissionError::Expired,
    ] {
        let legacy = admission_rejection(reason);
        let observed = admission_rejection_observed(reason, &scoped)?;
        let mut detail = body(&observed.result);
        assert_eq!(
            detail.as_object_mut().unwrap().remove("scope_observation"),
            Some(expected.clone())
        );
        assert_eq!(detail, body(&legacy.result));
        assert_eq!(observed.rebind, legacy.rebind);
        assert_eq!(
            serde_json::to_value(observed.status_receipt)?,
            serde_json::to_value(legacy.status_receipt)?
        );
        assert_eq!(
            admission_rejection_observed(reason, &admin)?.result,
            legacy.result
        );
    }
    scoped.revoke_scoped_reads();
    assert_eq!(
        scoped_worker_error(&scoped, &error, true)["scope_observation"],
        expected
    );
    Ok(())
}
