// SPDX-License-Identifier: Apache-2.0
use super::*;
use airk_core::{
    identity::{Incarnation, Sha256Digest},
    index_catalog::*,
    service::ScopedConnectRequest,
};
use rmcp::ServiceExt;
use serde_json::{Value, json};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

struct Selected {
    selection: ScopedConnectRequest,
    calls: AtomicUsize,
    failed: AtomicBool,
}
impl RemoteTools for Selected {
    fn scoped_selection(&self) -> Option<&ScopedConnectRequest> {
        Some(&self.selection)
    }
    fn call(&self, _: String, _: serde_json::Map<String, Value>) -> RemoteFuture<'_> {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if self.failed.load(Ordering::SeqCst) {
                anyhow::bail!("private/transport/path");
            }
            Ok(scoped_setup_failure(
                &self.selection,
                "grant_revoked",
                false,
                Some(json!({"failure":{"code":"grant_revoked","retry":{"complete":true}}})),
            ))
        })
    }
}
#[tokio::test]
async fn actual_remote_bridge_selected_help_preserves_full_native_refusal_and_transport_uncertainty()
-> anyhow::Result<()> {
    let selection = ScopedConnectRequest {
        schema: 1,
        context_key: Sha256Digest::from_bytes([1; 32]),
        namespace: Incarnation::from_bytes([2; 16]),
        expected_catalog_revision: CatalogRevision::INITIAL,
        grant_id: GrantId::from_identity(Incarnation::from_bytes([3; 16])),
        expected_grant_revision: CatalogRevision::INITIAL,
        index_id: IndexId::from_identity(Incarnation::from_bytes([4; 16])),
        project_ids: vec![QueryProjectId::from_identity(Incarnation::from_bytes(
            [5; 16],
        ))],
    };
    let selected = Arc::new(Selected {
        selection,
        calls: AtomicUsize::new(0),
        failed: AtomicBool::new(false),
    });
    let bridge = Bridge {
        backend: selected.clone(),
        admission: Arc::new(Admission::default()),
        response_mode: crate::response::ResponseMode::Full,
    };
    let (client_transport, server_transport) = tokio::io::duplex(64 * 1024);
    let (mut client, mut server) = tokio::try_join!(
        async { Ok::<_, anyhow::Error>(().serve(client_transport).await?) },
        async { Ok::<_, anyhow::Error>(bridge.serve(server_transport).await?) }
    )?;
    let reply = client
        .call_tool(CallToolRequestParams::new("airk_help"))
        .await?;
    assert_eq!(
        selected.calls.load(Ordering::SeqCst),
        1,
        "selected help must bypass static bridge help"
    );
    assert_eq!(reply.is_error, Some(true));
    let body = reply.structured_content.unwrap();
    assert_eq!(
        body["error"]["native_control_response"],
        json!({"failure":{"code":"grant_revoked","retry":{"complete":true}}})
    );
    assert_eq!(body["error"]["query_submitted"], false);
    assert_eq!(body["requested_scope"]["confirmed"], false);
    assert!(body.get("scope_observation").is_none() && body.get("publication").is_none());
    selected.failed.store(true, Ordering::SeqCst);
    let reply = call_observed(selected.as_ref(), "airk_status".into(), Default::default()).await?;
    assert_eq!(reply["isError"], true);
    let body = &reply["structuredContent"];
    assert_eq!(body["error"]["query_submitted"], Value::Null);
    assert_eq!(body["error"]["query_replay_safe"], false);
    assert!(!body.to_string().contains("private/transport"));
    client.close().await?;
    server.close().await?;
    Ok(())
}
