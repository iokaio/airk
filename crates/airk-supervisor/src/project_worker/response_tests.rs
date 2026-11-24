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
        "selection":{"owner":"window","grant_nonce":Incarnation::from_bytes([4;16]),
            "service_instance":Incarnation::from_bytes([6;16]),"scope_identity":Sha256Digest::from_bytes([5;32]),
            "catalog_revision":CatalogRevision::INITIAL},
        "atoms":[[project,ScopeAtom {source_root_id:source,kind:PathScope::Subtree,path:"public".into()}]],
        "membership_revisions":[[project,CatalogRevision::INITIAL]],
        "source_revision":CatalogRevision::INITIAL,"policy_epoch":0,"policy_digest":null,
        "sources":[{"source_root_id":source,"original_root":kernel.root().to_str().unwrap()}]
    }))?;
    kernel.with_selection_reads(authority)
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

#[test]
fn actual_unscoped_worker_boundary_redacts_unknown_internal_failure_and_preserves_typed_retry()
-> Result<()> {
    let sentinel = "private/source/secret-content-sentinel";
    let reply = unscoped_worker_error(&anyhow::anyhow!(sentinel));
    assert_eq!(reply["isError"], true);
    assert_eq!(body(&reply)["error"]["code"], "read_failed");
    assert_eq!(body(&reply)["error"]["retryable"], false);
    assert!(!reply.to_string().contains(sentinel));
    let typed = anyhow::Error::new(airk_store::quota::QuotaContention).context(sentinel);
    let reply = unscoped_worker_error(&typed);
    assert_eq!(body(&reply)["code"], "selection_busy");
    assert_eq!(body(&reply)["action"], "repeat_same_request");
    assert!(!reply.to_string().contains(sentinel));
    Ok(())
}

#[test]
fn early_worker_errors_use_closed_native_envelopes() -> Result<()> {
    let root = tempfile::Builder::new().tempdir_in(std::env::temp_dir().canonicalize()?)?;
    let admin = Kernel::open(root.path(), None)?;
    let scoped = session(&admin)?;
    let error = anyhow::Error::new(airk_core::scoped_reads::ScopeReadRefused);
    let sdk = airk_mcp::response::scoped_error_result(&scoped, &error);
    assert!(
        sdk.result_type.is_some(),
        "control must exercise the real SDK constructor"
    );
    let sdk_value = serde_json::to_value(&sdk)?;
    assert!(
        airk_kernel::combined_reads::context_member_payload_bytes(&sdk_value).is_err(),
        "the public SDK envelope is not an unreserved native frame"
    );
    let native = scoped_worker_error(&scoped, &error, false);
    assert!(native.get("resultType").is_none());
    assert_eq!(body(&native), scoped_worker_error(&scoped, &error, true));
    let mut expected = sdk_value;
    expected.as_object_mut().unwrap().remove("resultType");
    assert_eq!(
        native, expected,
        "only the native/public discriminator differs"
    );
    assert!(airk_kernel::combined_reads::context_member_payload_bytes(&native)? > 0);
    assert_eq!(body(&native)["error"]["code"], "scoped_read_unavailable");
    let typed = anyhow::Error::new(airk_store::quota::QuotaContention);
    let sdk =
        airk_mcp::response::typed_error_result(&typed).context("real typed refusal absent")?;
    assert!(sdk.result_type.is_some());
    let native = unscoped_worker_error(&typed);
    let mut expected = serde_json::to_value(sdk)?;
    expected.as_object_mut().unwrap().remove("resultType");
    assert_eq!(native, expected);
    assert!(airk_kernel::combined_reads::context_member_payload_bytes(&native)? > 0);
    let unknown = unscoped_worker_error(&anyhow::anyhow!("private/sentinel"));
    assert!(unknown.get("resultType").is_none());
    assert!(airk_kernel::combined_reads::context_member_payload_bytes(&unknown)? > 0);
    assert!(!unknown.to_string().contains("private/sentinel"));
    Ok(())
}

/// Change-log entry 53: an Answer reply above 64 KiB used to fail its frame
/// at the worker, which dropped the stream, so the caller saw only
/// `scoped_observation_unavailable`. An Answer query's reply now has the
/// Answer frame; other tools keep 65,536; a reply above its frame is
/// answered with a typed refusal that fits the default frame.
#[test]
fn an_answer_reply_has_the_answer_frame_and_an_oversized_one_is_refused_typed() -> Result<()> {
    let mut reply = admission_rejection(airk_core::service::AdmissionError::Busy);
    let text = "x".repeat(100_000);
    reply.result = json!({"content":[{"type":"text","text":text}],"isError":false});
    let mut frame = Vec::new();
    assert!(service_ipc::write_frame_limited(&mut frame, &reply, MAX_FRAME_BYTES).is_err());
    assert_eq!(airk_core::service::query_reply_frame("airk_search"), 65_536);
    assert_eq!(
        airk_core::service::query_reply_frame("airk_answer"),
        147_456
    );
    frame.clear();
    service_ipc::write_frame_limited(
        &mut frame,
        &reply,
        airk_core::service::query_reply_frame("airk_answer"),
    )?;
    let refused = frame_exceeded(&reply, MAX_FRAME_BYTES);
    frame.clear();
    service_ipc::write_frame_limited(&mut frame, &refused, MAX_FRAME_BYTES)?;
    assert_eq!(refused.result["isError"], true);
    assert_eq!(
        body(&refused.result)["error"]["code"],
        "reply_frame_exceeded"
    );
    assert_eq!(
        refused.result["structuredContent"]["error"]["maximum_bytes"],
        65_536
    );
    Ok(())
}
