// SPDX-License-Identifier: Apache-2.0
//! Capacity decoration preserves authority descriptors without inventing one.
use super::*;
use airk_core::{
    identity::Sha256Digest,
    service::{ScopedConnectRequest, WindowScopeSelection},
};
use airk_mcp::remote::{CombinedScope, ScopedScope};
use serde_json::json;

enum Source {
    Catalog(ScopedConnectRequest),
    Selection(WindowScopeSelection),
    Combined(CombinedScope),
    Unscoped,
}
impl RemoteTools for Source {
    fn scoped_selection(&self) -> Option<&ScopedConnectRequest> {
        match self {
            Self::Catalog(selection) => Some(selection),
            _ => None,
        }
    }
    fn scoped_scope(&self) -> Option<ScopedScope> {
        match self {
            Self::Catalog(selection) => Some(ScopedScope::Catalog(selection.clone())),
            Self::Selection(selection) => Some(ScopedScope::Selection {
                window_scope_selection: selection.clone(),
                scope_identity: Sha256Digest::from_bytes([0x5c; 32]),
            }),
            _ => None,
        }
    }
    fn combined_scope(&self) -> Option<CombinedScope> {
        match self {
            Self::Combined(scope) => Some(scope.clone()),
            _ => None,
        }
    }
    fn call(&self, _: String, _: serde_json::Map<String, serde_json::Value>) -> RemoteFuture<'_> {
        Box::pin(async { anyhow::bail!("descriptor inspection must not dispatch a read") })
    }
}

#[test]
fn capacity_wrapper_keeps_catalog_selection_combined_and_unscoped_descriptors() -> Result<()> {
    let catalog = json!({"schema":1,"context_key":"01".repeat(32),"namespace":"02".repeat(16),
        "expected_catalog_revision":7,"grant_id":format!("grt_{}","03".repeat(16)),"expected_grant_revision":1,
        "index_id":format!("idx_{}","04".repeat(16)),"project_ids":[format!("prj_{}","05".repeat(16))]});
    let selection = json!({"namespace":"02".repeat(16),"expected_catalog_revision":7,
        "index_ids":[format!("idx_{}","04".repeat(16))],"project_ids":[format!("prj_{}","05".repeat(16))]});
    let requested = json!({"window_scope_selection":selection,"scope_identity":"5c".repeat(32)});
    let window: WindowScopeSelection = serde_json::from_value(selection)?;
    let combined = CombinedScope {
        project_ids: window.project_ids.clone(),
        index_ids: Some(window.index_ids.clone()),
        requested: requested.clone(),
    };
    for (source, single, union) in [
        (
            Source::Catalog(serde_json::from_value(catalog.clone())?),
            Some(catalog),
            false,
        ),
        (Source::Selection(window), Some(requested.clone()), false),
        (Source::Combined(combined), None, true),
        (Source::Unscoped, None, false),
    ] {
        let source = Arc::new(source);
        let decorated =
            with_query_capacity(Arc::new(move || Ok(source.clone())), Arc::new(|| 8))()?;
        assert_eq!(decorated.query_concurrency(), 8);
        match (decorated.scoped_scope(), single) {
            (Some(scope), Some(expected)) => assert_eq!(
                airk_mcp::remote::scoped_scope_setup_failure(
                    &scope,
                    "scoped_status_prepare_timeout",
                    true,
                    None
                )["structuredContent"]["requested_scope"]["selection"],
                expected
            ),
            (None, None) => {}
            _ => anyhow::bail!("capacity wrapper changed the single-index authority source"),
        }
        assert_eq!(decorated.combined_scope().is_some(), union);
        if let Some(scope) = decorated.combined_scope() {
            assert_eq!(scope.requested, requested);
            assert_eq!(
                scope.project_ids[0].to_string(),
                format!("prj_{}", "05".repeat(16))
            );
            assert_eq!(
                scope.index_ids.unwrap()[0].to_string(),
                format!("idx_{}", "04".repeat(16))
            );
        }
    }
    Ok(())
}
