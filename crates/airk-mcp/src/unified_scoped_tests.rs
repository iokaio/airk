// SPDX-License-Identifier: Apache-2.0
//! Real native handlers through the selected Unified adapter; failure seams
//! model transport uncertainty without fabricating authorization observations.
use super::*;
use airk_core::{
    identity::{Incarnation, Sha256Digest},
    index_catalog::*,
    scoped_reads::SourceDescriptor,
    service::ScopedConnectRequest,
};
use airk_kernel::{Kernel, scoped_reads::ScopedReadAuthority};
use std::sync::atomic::{AtomicU8, AtomicUsize, Ordering};

struct Native {
    kernel: Kernel,
    selection: ScopedConnectRequest,
    mode: AtomicU8,
    calls: AtomicUsize,
    retained: std::sync::Mutex<Option<Value>>,
}
impl RemoteTools for Native {
    fn scoped_selection(&self) -> Option<&ScopedConnectRequest> {
        Some(&self.selection)
    }
    // Deliberately keep default local_help=true: a selected transport must still
    // reach the native handler rather than the bridge's static help shortcut.
    fn prepare(&self) -> crate::remote::ReadyFuture<'_> {
        Box::pin(async move {
            match self.mode.load(Ordering::SeqCst) {
                1 => anyhow::bail!("private/path/must-not-leak"),
                5 => Err(crate::remote::PreparationExpired.into()),
                _ => Ok(()),
            }
        })
    }
    fn call(&self, tool: String, args: Map<String, Value>) -> RemoteFuture<'_> {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::SeqCst);
            match self.mode.load(Ordering::SeqCst) {
                2 => anyhow::bail!("private/path/must-not-leak"),
                3 => std::future::pending::<()>().await,
                4 => {
                    return Ok(crate::remote::scoped_setup_failure(
                        &self.selection,
                        "grant_revoked",
                        false,
                        Some(json!({"failure":{"code":"grant_revoked","extra":{"complete":true}}})),
                    ));
                }
                _ => {}
            }
            if let Some(reply) = self.retained.lock().unwrap().clone() {
                return Ok(reply);
            }
            let mut tools = crate::local::LocalTools::new(self.kernel.clone()).await?;
            let result = tools.call(tool, args).await;
            tools.close().await?;
            Ok(serde_json::to_value(result?)?)
        })
    }
}
#[derive(Default)]
struct Ui(AtomicUsize);
impl ExplorerTools for Ui {
    fn call(
        &self,
        _: Option<String>,
        _: String,
        _: Map<String, Value>,
        _: airk_core::usage::ClientFamily,
    ) -> RemoteFuture<'_> {
        Box::pin(async move {
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok(success(json!({"static_ui_help":true})))
        })
    }
}
fn fixture() -> Result<(tempfile::TempDir, Unified, Arc<Native>, Arc<Ui>)> {
    let dir = tempfile::tempdir()?;
    let root = dir.path().join("source");
    std::fs::create_dir(&root)?;
    std::fs::write(root.join("a.rs"), "pub struct Fixture;\n")?;
    let kernel = Kernel::open(&root, Some(dir.path().join("data")))?;
    kernel.configure_vectors(false)?;
    kernel.index()?;
    let project = QueryProjectId::from_identity(Incarnation::from_bytes([1; 16]));
    let index = IndexId::from_identity(Incarnation::from_bytes([2; 16]));
    let source = SourceRootId::from_identity(Incarnation::from_bytes([3; 16]));
    let grant = GrantId::from_identity(Incarnation::from_bytes([4; 16]));
    let backing = airk_core::service::ProjectRegistration::new(
        kernel.root().to_str().unwrap().into(),
        kernel.data_dir().to_str().unwrap().into(),
        kernel.collection().into(),
    )?
    .binding;
    let authority = ScopedReadAuthority {
        schema: 1,
        backing,
        index_id: index,
        project_ids: vec![project],
        provenance: airk_core::grant_provenance::GrantProvenance::UserReviewedProjectGrant,
        grant: ReviewedGrant {
            id: grant,
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
                        path: String::new(),
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
    };
    let selection = ScopedConnectRequest {
        schema: 1,
        context_key: Sha256Digest::from_bytes([6; 32]),
        namespace: Incarnation::from_bytes([7; 16]),
        expected_catalog_revision: CatalogRevision::INITIAL,
        grant_id: grant,
        expected_grant_revision: CatalogRevision::INITIAL,
        index_id: index,
        project_ids: vec![project],
    };
    let native = Arc::new(Native {
        kernel: kernel.with_scoped_reads(authority)?,
        selection,
        mode: AtomicU8::new(0),
        calls: AtomicUsize::new(0),
        retained: std::sync::Mutex::new(None),
    });
    let ui = Arc::new(Ui::default());
    let unified = Unified::new(
        vec![Project {
            binding: backing.to_string(),
            name: "fixture".into(),
            collection: kernel.collection().into(),
            backend: native.clone(),
        }],
        Some(backing.to_string()),
        Some(ui.clone()),
    )?;
    Ok((dir, unified, native, ui))
}
async fn call(unified: &Unified, name: &str, args: Value) -> Result<Value> {
    unified
        .call(name.into(), args.as_object().unwrap().clone())
        .await
}
#[tokio::test]
async fn selected_native_help_and_status_keep_actual_handler_scope_without_ui_enrichment()
-> Result<()> {
    let (_dir, unified, native, ui) = fixture()?;
    for (name, args) in [
        ("help", json!({"tool":"search","response_mode":"full"})),
        ("airk_help", json!({"tool":"search","response_mode":"full"})),
        ("status", json!({"response_mode":"full"})),
        ("diagnostics", json!({"response_mode":"full"})),
    ] {
        let native_name = match name {
            "help" => "airk_help",
            "status" => "airk_status",
            "diagnostics" => "airk_diagnostics",
            name => name,
        };
        let expected = native
            .call(native_name.into(), args.as_object().unwrap().clone())
            .await?;
        // Carry this actual handler observation through the adapter unchanged,
        // including timing/resource fields, instead of comparing two clocks.
        *native.retained.lock().unwrap() = Some(expected.clone());
        let actual = call(&unified, name, args).await?;
        *native.retained.lock().unwrap() = None;
        assert_eq!(actual, expected, "{name}");
        let value = body(&actual).unwrap();
        assert!(value.get("scope_observation").is_some(), "{name}");
        assert!(value.get("explorer").is_none());
    }
    assert_eq!(ui.0.load(Ordering::SeqCst), 0);
    let before = native.calls.load(Ordering::SeqCst);
    call(&unified, "help", json!({"query":"static guidance"})).await?;
    call(&unified, "help", json!({"topic_id":"static.topic"})).await?;
    assert_eq!(native.calls.load(Ordering::SeqCst), before);
    assert_eq!(ui.0.load(Ordering::SeqCst), 2);
    Ok(())
}
#[tokio::test]
async fn selected_public_help_tool_aliases_return_complete_successful_native_schema() -> Result<()>
{
    let (_dir, unified, native, ui) = fixture()?;
    let canonical = native
        .call(
            "airk_help".into(),
            json!({"tool":"airk_search","max_bytes":32768,"response_mode":"full"})
                .as_object()
                .unwrap()
                .clone(),
        )
        .await?;
    assert_ne!(canonical["isError"], true, "{canonical}");
    let value = body(&canonical).unwrap();
    assert!(value.get("scope_observation").is_some());
    assert_eq!(value["tool"], "airk_search");
    assert_eq!(value["truncated"], false);
    let schema: Value = serde_json::from_str(value["content"].as_str().unwrap())?;
    assert_eq!(
        schema,
        serde_json::to_value(
            crate::local::tools()
                .into_iter()
                .find(|t| t.name == "airk_search")
                .unwrap()
        )?
    );
    // These calls reach real scoped LocalTools separately: retaining a previous
    // error would miss unsupported aliases and cannot satisfy these assertions.
    for name in ["help", "airk_help"] {
        for tool in ["search", "airk_search"] {
            let actual = call(
                &unified,
                name,
                json!({"tool":tool,"max_bytes":32768,"response_mode":"full"}),
            )
            .await?;
            assert_ne!(actual["isError"], true, "{actual}");
            assert_eq!(actual, canonical);
        }
    }
    assert_eq!(ui.0.load(Ordering::SeqCst), 0);
    Ok(())
}
#[tokio::test]
async fn selected_status_setup_observation_failures_and_native_refusals_stay_distinct() -> Result<()>
{
    let (_dir, unified, native, ui) = fixture()?;
    for (mode, code, submitted) in [
        (1, "scoped_status_prepare_unavailable", json!(false)),
        (5, "scoped_status_prepare_timeout", json!(false)),
        (2, "scoped_status_unavailable", Value::Null),
        (3, "scoped_status_timeout", Value::Null),
    ] {
        native.mode.store(mode, Ordering::SeqCst);
        let reply = call(&unified, "status", json!({})).await?;
        assert_eq!(reply["isError"], true);
        let value = body(&reply).unwrap();
        assert_eq!(value["error"]["code"], code);
        assert_eq!(value["error"]["query_submitted"], submitted);
        assert_eq!(value["requested_scope"]["confirmed"], false);
        assert_eq!(
            value["requested_scope"]["selection"],
            serde_json::to_value(&native.selection)?
        );
        for field in ["scope_observation", "publication", "files", "index_state"] {
            assert!(value.get(field).is_none());
        }
        assert!(!value.to_string().contains("private/path"));
    }
    native.mode.store(4, Ordering::SeqCst);
    let expected = native.call("airk_status".into(), Map::new()).await?;
    assert_eq!(call(&unified, "status", json!({})).await?, expected);
    assert_eq!(ui.0.load(Ordering::SeqCst), 0);
    Ok(())
}
