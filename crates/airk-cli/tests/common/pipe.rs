// SPDX-License-Identifier: Apache-2.0
//! A test-side `RemoteTools` that reads through the service pipe (M3
//! `ToolsCall`) with one fixed remaining read authority: a window selection
//! (W1 `WindowOpen`, then W4 `WindowScopeGrant`) or a U105 open read. It is a
//! transport adapter only: every result is the service's own envelope, and a
//! refusal before a result is an error carrying the complete native response.
#![allow(dead_code)]
use airk_core::{
    identity::{Incarnation, Sha256Digest},
    index_catalog::{CatalogRevision, IndexId, QueryProjectId},
    service::{ControlMethod, ControlResponse, ToolSelection, WindowLease, WindowScopeSelection},
};
use airk_mcp::remote::{RemoteFuture, RemoteTools};
use anyhow::{Context, Result, bail, ensure};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

/// One open window of this test process, granted a scope.
#[derive(Clone)]
pub struct Window {
    pub window_id: Incarnation,
    pub lease: WindowLease,
    pub service_instance: Incarnation,
    pub selection: WindowScopeSelection,
    /// The current W4 grant's identity and catalog revision.
    pub scope_identity: Sha256Digest,
    pub catalog_revision: CatalogRevision,
}

/// The lease is never printed.
impl std::fmt::Debug for Window {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Window")
            .field("window_id", &self.window_id)
            .field("service_instance", &self.service_instance)
            .field("selection", &self.selection)
            .field("scope_identity", &self.scope_identity)
            .field("catalog_revision", &self.catalog_revision)
            .finish_non_exhaustive()
    }
}

impl Window {
    /// The selection observation a read under the current grant reports.
    pub fn source(&self) -> airk_core::scoped_reads::ScopeSelectionObservation {
        airk_core::scoped_reads::ScopeSelectionObservation {
            owner: airk_core::scoped_reads::SelectionOwnerKind::Window,
            scope_identity: self.scope_identity,
            catalog_revision: self.catalog_revision,
        }
    }
    pub fn tool_selection(&self) -> ToolSelection {
        ToolSelection::Window {
            window_id: self.window_id,
            lease: self.lease.clone(),
        }
    }
    /// The in-process `airkctl` session route reading through this window.
    pub fn route(&self) -> airkctl::cli::WindowRoute {
        airkctl::cli::WindowRoute {
            window_id: self.window_id,
            lease: self.lease.clone(),
            instance: self.service_instance,
            refused: None,
        }
    }
}

pub async fn clock(control: &Path) -> Result<airk_core::service::CatalogClock> {
    let ControlResponse::Health { observation } =
        airk_cli::service_runtime::request(control, ControlMethod::Health).await?
    else {
        bail!("service health unavailable");
    };
    observation
        .catalog_clock
        .context("service has no catalog clock")
}

/// W1 then W4 at the service's current catalog revision.
pub async fn open_window(
    control: &Path,
    label: &str,
    index_ids: Vec<IndexId>,
    project_ids: Vec<QueryProjectId>,
) -> Result<Window> {
    let clock = clock(control).await?;
    let opened = airk_cli::service_runtime::request(
        control,
        ControlMethod::WindowOpen {
            label: label.into(),
            owner_pid: std::process::id(),
        },
    )
    .await?;
    let ControlResponse::WindowOpened {
        window_id,
        lease,
        service_instance,
        ..
    } = opened
    else {
        bail!("window did not open: {}", serde_json::to_string(&opened)?);
    };
    let mut window = Window {
        window_id,
        lease,
        service_instance,
        selection: WindowScopeSelection {
            namespace: clock.namespace,
            expected_catalog_revision: clock.revision,
            index_ids,
            project_ids,
        },
        scope_identity: Sha256Digest::from_bytes([0; 32]),
        catalog_revision: clock.revision,
    };
    grant(control, &mut window).await?;
    Ok(window)
}

/// W4 again at the service's current catalog revision (after a catalog
/// mutation the previous grant no longer reads). A grant already at the
/// current revision is kept, so consecutive reads share one grant. A refused
/// grant leaves the window and its previous grant unchanged, on the service
/// and here.
pub async fn regrant(control: &Path, window: &mut Window) -> Result<()> {
    let clock = clock(control).await?;
    if clock.namespace == window.selection.namespace && clock.revision == window.catalog_revision {
        return Ok(());
    }
    let mut next = window.clone();
    next.selection.namespace = clock.namespace;
    next.selection.expected_catalog_revision = clock.revision;
    grant(control, &mut next).await?;
    *window = next;
    Ok(())
}

async fn grant(control: &Path, window: &mut Window) -> Result<()> {
    let granted = airk_cli::service_runtime::request(
        control,
        ControlMethod::WindowScopeGrant {
            window_id: window.window_id,
            lease: window.lease.clone(),
            selection: window.selection.clone(),
        },
    )
    .await?;
    let ControlResponse::WindowScopeGranted {
        scope_identity,
        catalog_revision,
        ..
    } = granted
    else {
        bail!(
            "window scope was not granted: {}",
            serde_json::to_string(&granted)?
        );
    };
    window.scope_identity = scope_identity;
    window.catalog_revision = catalog_revision;
    Ok(())
}

/// W2: keep the window alive between reads, as an editor session does.
pub async fn renew(control: &Path, window: &Window) -> Result<()> {
    let renewed = airk_cli::service_runtime::request(
        control,
        ControlMethod::WindowRenew {
            window_id: window.window_id,
            lease: window.lease.clone(),
        },
    )
    .await?;
    ensure!(
        matches!(renewed, ControlResponse::WindowRenewed { .. }),
        "window was not renewed: {}",
        serde_json::to_string(&renewed)?
    );
    Ok(())
}

pub async fn close_window(control: &Path, window: &Window) -> Result<()> {
    let closed = airk_cli::service_runtime::request(
        control,
        ControlMethod::WindowClose {
            window_id: window.window_id,
            lease: window.lease.clone(),
        },
    )
    .await?;
    ensure!(
        matches!(closed, ControlResponse::WindowClosed {}),
        "window was not closed: {}",
        serde_json::to_string(&closed)?
    );
    Ok(())
}

enum Authority {
    Window(Mutex<Window>),
    Open(ToolSelection),
}

/// Pipe reads under one fixed authority.
pub struct PipeTools {
    control: PathBuf,
    authority: Authority,
}

impl PipeTools {
    pub fn window(control: &Path, window: Window) -> Arc<Self> {
        Arc::new(Self {
            control: control.into(),
            authority: Authority::Window(Mutex::new(window)),
        })
    }
    pub fn open(control: &Path, index_ids: &[IndexId]) -> Arc<Self> {
        Arc::new(Self {
            control: control.into(),
            authority: Authority::Open(ToolSelection::Open {
                index_ids: index_ids.iter().map(ToString::to_string).collect(),
                index_set_ids: Vec::new(),
                workspace: None,
            }),
        })
    }
    pub async fn open_window(
        control: &Path,
        label: &str,
        index_ids: Vec<IndexId>,
        project_ids: Vec<QueryProjectId>,
    ) -> Result<Arc<Self>> {
        Ok(Self::window(
            control,
            open_window(control, label, index_ids, project_ids).await?,
        ))
    }
    /// The window this adapter reads through, if any.
    pub fn current_window(&self) -> Option<Window> {
        match &self.authority {
            Authority::Window(window) => Some(window.lock().unwrap().clone()),
            Authority::Open(_) => None,
        }
    }
    /// Re-grant the window at the current catalog revision.
    pub async fn regrant(&self) -> Result<()> {
        let Authority::Window(window) = &self.authority else {
            bail!("only a window selection is re-granted");
        };
        let mut current = window.lock().unwrap().clone();
        regrant(&self.control, &mut current).await?;
        *window.lock().unwrap() = current;
        Ok(())
    }
    pub fn tool_selection(&self) -> ToolSelection {
        match &self.authority {
            Authority::Window(window) => window.lock().unwrap().tool_selection(),
            Authority::Open(selection) => selection.clone(),
        }
    }
    async fn forward(
        &self,
        tool: String,
        arguments: serde_json::Map<String, serde_json::Value>,
        read_attempt: Option<airk_core::read_attempt::ReadAttemptBudget>,
    ) -> Result<serde_json::Value> {
        if let Some(window) = self.current_window() {
            renew(&self.control, &window).await?;
        }
        let response = airk_cli::service_runtime::request(
            &self.control,
            ControlMethod::ToolsCall {
                tool,
                arguments,
                selection: self.tool_selection(),
                response_mode: airk_core::service::ToolResponseMode::Full,
                usage_client: airk_core::usage::ClientFamily::Unknown,
                read_attempt,
            },
        )
        .await?;
        match response {
            ControlResponse::ToolResult { result } => Ok(result),
            other => bail!(
                "pipe read refused before a result: {}",
                serde_json::to_string(&other)?
            ),
        }
    }
}

impl RemoteTools for PipeTools {
    fn call(
        &self,
        tool: String,
        arguments: serde_json::Map<String, serde_json::Value>,
    ) -> RemoteFuture<'_> {
        // As `airkctl tools call` sends it: no caller read allowance, so the
        // service owns the read's lifetime and retry credit.
        Box::pin(self.forward(tool, arguments, None))
    }
    fn close(&self) {}
}
