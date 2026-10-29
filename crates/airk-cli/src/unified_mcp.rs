// SPDX-License-Identifier: Apache-2.0
//! Unified transport composition; the broker and kernel never own UI behavior.
use airk_core::service::{ProjectBinding, ProjectRegistration};
use airk_mcp::{
    remote::{RemoteFuture, RemoteTools},
    unified::{ExplorerTools, Project, Unified},
};
use anyhow::{Context, Result, ensure};
use serde_json::{Map, Value, json};
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    path::PathBuf,
    process::Stdio,
    sync::Arc,
    time::{Duration, Instant},
};

struct Explorer {
    descriptor: PathBuf,
    roots: BTreeMap<String, PathBuf>,
    scopes: BTreeMap<String, crate::service_frontend::RootScope>,
    control: PathBuf,
}

struct RootNative {
    scope: crate::service_frontend::RootScope,
    control: PathBuf,
    install: PathBuf,
    ready: Arc<tokio::sync::OnceCell<()>>,
    client: std::sync::Mutex<airk_core::usage::ClientFamily>,
    backend: tokio::sync::Mutex<Option<(ProjectBinding, Arc<dyn RemoteTools>)>>,
}
impl RemoteTools for RootNative {
    fn client(&self, client: airk_core::usage::ClientFamily) {
        *self.client.lock().unwrap() = client;
    }
    fn call(&self, tool: String, arguments: Map<String, Value>) -> RemoteFuture<'_> {
        Box::pin(async move {
            self.ready
                .get_or_try_init(|| async {
                    let control = crate::runtime_install::installed_control(&self.install).await?;
                    ensure!(control == self.control, "selected service control changed");
                    Ok::<(), anyhow::Error>(())
                })
                .await?;
            let control = self.control.clone();
            let scope = self.scope.clone();
            let observed = control.clone();
            let resolved = tokio::task::spawn_blocking(move || scope.resolve(&observed)).await??;
            let registration = match resolved {
                crate::service_frontend::RootResolution::Registered(registration) => registration,
                other => {
                    *self.backend.lock().await = None;
                    let (code, state, message) = root_failure(other);
                    return Ok(if tool == "airk_status" {
                        airk_mcp::unified::success(
                            json!({"version":env!("CARGO_PKG_VERSION"),"health":"needs_attention","index_state":"unavailable","files":null,"chunks":null,"publication":null,"registration_state":state,"source_root":self.scope.root,"index_error":{"code":code,"message":message}}),
                        )
                    } else {
                        airk_mcp::unified::failure(code, message)
                    });
                }
            };
            let backend = {
                let mut cached = self.backend.lock().await;
                if cached
                    .as_ref()
                    .is_none_or(|(binding, _)| *binding != registration.binding)
                {
                    *cached = Some((
                        registration.binding,
                        crate::project_client::tools(control, registration.binding),
                    ));
                }
                cached.as_ref().unwrap().1.clone()
            };
            backend.client(*self.client.lock().unwrap());
            backend.call(tool, arguments).await
        })
    }
}
fn root_failure(
    resolution: crate::service_frontend::RootResolution,
) -> (&'static str, &'static str, &'static str) {
    match resolution {
        crate::service_frontend::RootResolution::Missing => (
            "PROJECT_NOT_REGISTERED",
            "missing",
            "No index is registered for this source folder. Open the trusted folder in AIRK Explorer to prepare it. This MCP connection remains usable after registration.",
        ),
        crate::service_frontend::RootResolution::Removed => (
            "PROJECT_REMOVED",
            "removed",
            "This source folder was explicitly removed from AIRK. Restore it in Management before using its index.",
        ),
        crate::service_frontend::RootResolution::Ambiguous => (
            "PROJECT_AMBIGUOUS",
            "ambiguous",
            "Multiple active indexes use this source folder. Configure --index-collection to choose one explicitly.",
        ),
        crate::service_frontend::RootResolution::Registered(_) => unreachable!(),
    }
}

struct Native {
    install: Option<PathBuf>,
    control: PathBuf,
    ready: Arc<tokio::sync::OnceCell<()>>,
    backend: Arc<dyn RemoteTools>,
}
impl RemoteTools for Native {
    fn client(&self, client: airk_core::usage::ClientFamily) {
        self.backend.client(client);
    }
    fn call(&self, tool: String, arguments: Map<String, Value>) -> RemoteFuture<'_> {
        Box::pin(async move {
            if let Some(install) = &self.install {
                self.ready
                    .get_or_try_init(|| async {
                        let control = crate::runtime_install::installed_control(install).await?;
                        ensure!(control == self.control, "selected service control changed");
                        Ok::<(), anyhow::Error>(())
                    })
                    .await?;
            }
            self.backend.call(tool, arguments).await
        })
    }
}
impl ExplorerTools for Explorer {
    fn call(
        &self,
        project: Option<String>,
        tool: String,
        arguments: Map<String, Value>,
        client: airk_core::usage::ClientFamily,
    ) -> RemoteFuture<'_> {
        let descriptor = self.descriptor.clone();
        let root = project.as_ref().and_then(|id| self.roots.get(id)).cloned();
        let scope = project.as_ref().and_then(|id| self.scopes.get(id)).cloned();
        let control = self.control.clone();
        Box::pin(async move {
            tokio::task::spawn_blocking(move || {
                let (project, root) = if let Some(scope) = scope {
                    match scope.resolve(&control)? {
                        crate::service_frontend::RootResolution::Registered(registration) => {
                            (Some(registration.binding.to_string()), Some(scope.root))
                        }
                        other => {
                            let (code, _, message) = root_failure(other);
                            return Ok(airk_mcp::unified::failure(code, message));
                        }
                    }
                } else {
                    (project, root)
                };
                call_explorer(descriptor, project, root, tool, arguments, client)
            })
            .await?
        })
    }
}
fn call_explorer(
    descriptor: PathBuf,
    project: Option<String>,
    root: Option<PathBuf>,
    tool: String,
    arguments: Map<String, Value>,
    client: airk_core::usage::ClientFamily,
) -> Result<Value> {
    let launch = crate::explorer_assistance::plan(&descriptor)?;
    let input = serde_json::to_vec(
        &json!({"tool":tool,"arguments":arguments,"client":format!("{client:?}")}),
    )?;
    ensure!(input.len() <= 65536, "Explorer request exceeds bound");
    let mut command = crate::explorer_assistance::command(&launch);
    command
        .arg("--one-shot")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    if let (Some(project), Some(root)) = (project, root) {
        command
            .arg("--project-binding")
            .arg(project)
            .arg("--project-root")
            .arg(root);
    }
    let mut child = crate::native_process::ContainedChild::spawn(&mut command)?;
    let mut stdin = child.stdin.take().context("Explorer stdin unavailable")?;
    let stdout = child.stdout.take().context("Explorer stdout unavailable")?;
    let writer = std::thread::spawn(move || stdin.write_all(&input));
    let reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout
            .take(1_048_577)
            .read_to_end(&mut bytes)
            .map(|_| bytes)
    });
    let deadline = Instant::now() + Duration::from_secs(15);
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break Some(status);
        }
        if Instant::now() >= deadline {
            break None;
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    // Drain our exact child before joining its pipe threads, including timeout/error paths.
    drop(child);
    writer
        .join()
        .map_err(|_| anyhow::anyhow!("Explorer input thread failed"))??;
    let bytes = reader
        .join()
        .map_err(|_| anyhow::anyhow!("Explorer output thread failed"))??;
    ensure!(
        status.is_some_and(|s| s.success()) && bytes.len() <= 1_048_576,
        "Explorer exchange failed or timed out"
    );
    let reply: Value = serde_json::from_slice(&bytes)?;
    if reply["ok"] == true {
        Ok(airk_mcp::unified::success(reply["value"].clone()))
    } else {
        Ok(airk_mcp::unified::failure(reply["code"].as_str().unwrap_or("UI_UNAVAILABLE"),reply["detail"].as_str().unwrap_or("Explorer request unavailable; inspect status for attachment and project selection.")))
    }
}

pub async fn serve(
    control: PathBuf,
    install: Option<PathBuf>,
    registration: ProjectRegistration,
    additional: Vec<ProjectBinding>,
    no_default: bool,
    descriptor: Option<PathBuf>,
) -> Result<()> {
    ensure!(additional.len() < 32, "too many configured projects");
    let primary = registration.binding.to_string();
    let mut registrations = vec![registration];
    if !additional.is_empty() {
        let registry = airk_store::service::registry::ServiceRegistry::open_saved(&control)?;
        for binding in additional {
            let saved = registry.get(binding)?;
            registrations.push(saved.registration);
        }
    }
    let roots = registrations
        .iter()
        .map(|p| (p.binding.to_string(), PathBuf::from(&p.root)))
        .collect();
    let ready = Arc::new(tokio::sync::OnceCell::new());
    let projects = registrations
        .into_iter()
        .map(|p| Project {
            binding: p.binding.to_string(),
            name: PathBuf::from(&p.root)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| p.collection.clone()),
            collection: p.collection,
            backend: Arc::new(Native {
                install: install.clone(),
                control: control.clone(),
                ready: ready.clone(),
                backend: crate::project_client::tools(control.clone(), p.binding),
            }),
        })
        .collect();
    let explorer = descriptor.map(|descriptor| {
        Arc::new(Explorer {
            descriptor,
            roots,
            scopes: BTreeMap::new(),
            control: control.clone(),
        }) as Arc<dyn ExplorerTools>
    });
    let backend = Unified::new(projects, (!no_default).then_some(primary), explorer)?;
    crate::client_owner::watch()?;
    airk_mcp::remote::serve(Arc::new(backend)).await
}

pub async fn serve_roots(
    control: PathBuf,
    install: PathBuf,
    scopes: Vec<crate::service_frontend::RootScope>,
    no_default: bool,
    descriptor: Option<PathBuf>,
) -> Result<()> {
    ensure!(
        !scopes.is_empty() && scopes.len() <= 32,
        "invalid configured source count"
    );
    let primary = scopes[0].key.clone();
    let ready = Arc::new(tokio::sync::OnceCell::new());
    let configured = scopes
        .iter()
        .map(|scope| (scope.key.clone(), scope.clone()))
        .collect();
    let projects = scopes
        .into_iter()
        .map(|scope| Project {
            binding: scope.key.clone(),
            name: scope.root.to_string_lossy().into_owned(),
            collection: scope
                .collection
                .clone()
                .unwrap_or_else(|| "automatic".into()),
            backend: Arc::new(RootNative {
                scope,
                control: control.clone(),
                install: install.clone(),
                ready: ready.clone(),
                client: std::sync::Mutex::new(airk_core::usage::ClientFamily::Unknown),
                backend: tokio::sync::Mutex::new(None),
            }),
        })
        .collect();
    let explorer = descriptor.map(|descriptor| {
        Arc::new(Explorer {
            descriptor,
            roots: BTreeMap::new(),
            scopes: configured,
            control,
        }) as Arc<dyn ExplorerTools>
    });
    let backend = Unified::new(projects, (!no_default).then_some(primary), explorer)?;
    crate::client_owner::watch()?;
    airk_mcp::remote::serve(Arc::new(backend)).await
}
