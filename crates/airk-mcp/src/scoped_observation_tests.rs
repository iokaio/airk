// SPDX-License-Identifier: Apache-2.0
//! Canonical authorization observations survive native MCP sessions and plans.
use super::*;
use airk_core::{
    identity::{Incarnation, Sha256Digest},
    index_catalog::*,
    scoped_reads::{ScopeObservation, SourceDescriptor},
    usage::ClientFamily,
};
use anyhow::{Context, Result};
use serde_json::{Value, json};

fn authority(kernel: &Kernel) -> Result<airk_kernel::scoped_reads::ScopedReadAuthority> {
    let project = QueryProjectId::from_identity(Incarnation::from_bytes([1; 16]));
    let index = IndexId::from_identity(Incarnation::from_bytes([2; 16]));
    let source = SourceRootId::from_identity(Incarnation::from_bytes([3; 16]));
    Ok(airk_kernel::scoped_reads::ScopedReadAuthority {
        schema: 1,
        backing: airk_core::service::ProjectRegistration::new(
            kernel.root().to_str().unwrap().into(),
            kernel.data_dir().to_str().unwrap().into(),
            kernel.collection().into(),
        )?
        .binding,
        index_id: index,
        project_ids: vec![project],
        provenance: airk_core::grant_provenance::GrantProvenance::UserReviewedProjectGrant,
        grant: ReviewedGrant {
            id: GrantId::from_identity(Incarnation::from_bytes([4; 16])),
            host: Sha256Digest::from_bytes([5; 32]),
            revision: CatalogRevision::INITIAL,
            revoked: false,
            memberships: vec![GrantedMembership {
                membership: Membership {
                    project_id: project,
                    index_id: index,
                    revision: CatalogRevision::INITIAL,
                    scope: vec![ScopeAtom {
                        source_root_id: source,
                        kind: PathScope::Subtree,
                        path: "public".into(),
                    }],
                },
                source_revision: CatalogRevision::INITIAL,
                policy_epoch: 0,
                policy_digest: None,
            }],
        },
        sources: vec![SourceDescriptor {
            source_root_id: source,
            original_root: kernel.root().to_str().unwrap().into(),
        }],
    })
}
fn payload(reply: &CallToolResult) -> Result<Value> {
    serde_json::from_str(
        &reply
            .content
            .first()
            .and_then(|content| content.as_text())
            .context("JSON result missing")?
            .text,
    )
    .map_err(Into::into)
}

#[tokio::test]
async fn scope_observation_survives_actual_sessions_terse_errors_and_workbench_copies() -> Result<()>
{
    let temporary = tempfile::tempdir()?;
    let root = temporary.path().join("source");
    std::fs::create_dir_all(root.join("public"))?;
    std::fs::create_dir(root.join("private"))?;
    std::fs::write(root.join("public/a.rs"), "pub struct Public;\n")?;
    std::fs::write(root.join("private/secret.rs"), "pub struct Secret;\n")?;
    let kernel = Kernel::open(&root, Some(temporary.path().join("data")))?;
    kernel.configure_vectors(false)?;
    kernel.index()?;
    let authority = authority(&kernel)?;
    let native = kernel.with_scoped_reads(authority.clone())?;
    let expected = serde_json::to_value(native.scope_observation())?;
    let mut owner = local::LocalTools::new(kernel.clone()).await?;
    let mut scoped = owner
        .scoped_session_with_client(authority, ClientFamily::Cli)
        .await?;
    let fence = scoped.scoped_read_revocation_handle();
    for mode in ["full", "terse", "terse"] {
        let reply=scoped.call("airk_list_files".into(),json!({"prefix":"","limit":10,"max_bytes":4096,"verification":"off","response_mode":mode}).as_object().unwrap().clone()).await?;
        assert_ne!(reply.is_error, Some(true), "{reply:?}");
        let value = payload(&reply)?;
        assert_eq!(value["paths"], json!(["public/a.rs"]));
        assert_eq!(value["scope_observation"], expected);
        let typed: ScopeObservation = serde_json::from_value(value["scope_observation"].clone())?;
        typed.validate()?;
        assert!(!serde_json::to_string(&typed)?.contains(root.to_str().unwrap()));
    }
    let denied = scoped
        .call(
            "airk_status".into(),
            json!({"response_mode":"full"}).as_object().unwrap().clone(),
        )
        .await?;
    assert_eq!(denied.is_error, Some(true));
    assert_eq!(
        payload(&denied)?["error"]["code"],
        "scoped_read_unavailable"
    );
    assert_eq!(payload(&denied)?["scope_observation"], expected);
    let binding = airk_core::service::ProjectRegistration::new(
        kernel.root().to_str().unwrap().into(),
        kernel.data_dir().to_str().unwrap().into(),
        kernel.collection().into(),
    )?
    .binding;
    let native_error = scoped
        .read_native(local::NativeRead::UsagePolicy, binding)
        .await
        .unwrap_err();
    assert_eq!(
        payload(&response::error_result(&native_error))?["scope_observation"],
        expected
    );
    let request = json!({"schema":"airk.parse.request/1","mode":"execute","verification":"off","plan":{"steps":[{"id":"files","tool":"list_files","args":{"prefix":"","limit":10,"max_bytes":4096}}]},"output":{"view":"raw","max_bytes":32768}});
    let direct = native.evidence_plan(serde_json::from_value(request.clone())?)?;
    assert_eq!(serde_json::to_value(&direct.scope_observation)?, expected);
    assert_eq!(
        direct.steps[0].result.as_ref().unwrap()["scope_observation"],
        expected
    );
    let copy: airk_core::workbench::ResultEnvelope =
        serde_json::from_value(serde_json::to_value(&direct)?)?;
    assert_eq!(copy.scope_observation, direct.scope_observation);
    let parsed = scoped
        .call("airk_parse".into(), request.as_object().unwrap().clone())
        .await?;
    assert_ne!(parsed.is_error, Some(true), "{parsed:?}");
    assert_eq!(payload(&parsed)?["scope_observation"], expected);
    assert_eq!(
        payload(&parsed)?["steps"][0]["result"]["scope_observation"],
        expected
    );
    let help = scoped
        .call(
            "airk_help".into(),
            json!({"max_bytes":1024}).as_object().unwrap().clone(),
        )
        .await?;
    assert_eq!(payload(&help)?["scope_observation"], expected);
    let legacy = owner
        .call(
            "airk_list_files".into(),
            json!({"verification":"off","response_mode":"full"})
                .as_object()
                .unwrap()
                .clone(),
        )
        .await?;
    assert!(payload(&legacy)?.get("scope_observation").is_none());
    fence.revoke_scoped_reads();
    let revoked = scoped
        .call(
            "airk_list_files".into(),
            json!({"verification":"off"}).as_object().unwrap().clone(),
        )
        .await?;
    assert_eq!(revoked.is_error, Some(true));
    assert_eq!(payload(&revoked)?["scope_observation"], expected);
    scoped.close().await?;
    owner.close().await?;
    Ok(())
}
