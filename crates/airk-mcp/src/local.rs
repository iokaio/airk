// SPDX-License-Identifier: Apache-2.0
//! Local inspection uses the same MCP server, schemas and request handlers as stdio.
use crate::AirkServer;
use airk_core::usage::ClientFamily;
use airk_kernel::Kernel;
use rmcp::{
    RoleClient, RoleServer, ServiceExt,
    model::{CallToolRequestParams, CallToolResult, Tool},
    service::RunningService,
};

pub use airk_core::native_reads::{NativeRead, ReportFormat, SettingsView};

fn scoped_metadata_view(
    view: SettingsView,
) -> Option<airk_kernel::scoped_reads::ScopedIndexMetadataView> {
    use airk_kernel::scoped_reads::ScopedIndexMetadataView as Scoped;
    Some(match view {
        SettingsView::ExtractionPolicy => Scoped::ExtractionPolicy,
        SettingsView::VectorSelection => Scoped::VectorSelection,
        SettingsView::VectorStatus => Scoped::VectorStatus,
        SettingsView::Roots => Scoped::Roots,
        SettingsView::ScanLimits => Scoped::ScanLimits,
        SettingsView::QuotaPolicy | SettingsView::QuotaStatus => return None,
    })
}

/// Listing/help require neither a collection nor a source scan.
pub fn tools() -> Vec<Tool> {
    static TOOLS: std::sync::OnceLock<Vec<Tool>> = std::sync::OnceLock::new();
    TOOLS.get_or_init(build_tools).clone()
}

// Exact bundled definitions only; per-session projects/admission/runtime state stays outside.
fn build_tools() -> Vec<Tool> {
    let mut tools = AirkServer::shared_tool_router().list_all();
    tools.sort_by(|a, b| a.name.cmp(&b.name));
    tools
}

pub struct LocalTools {
    work: std::sync::Arc<std::sync::RwLock<Option<serde_json::Value>>>,
    runtime: std::sync::Arc<std::sync::RwLock<Option<serde_json::Value>>>,
    kernel: Kernel,
    family: ClientFamily,
    owns_admission: bool,
    responses: std::sync::Arc<crate::response::Session>,
    client: RunningService<RoleClient, ()>,
    server: RunningService<RoleServer, AirkServer>,
    admission: crate::execution::Admission,
    direct_server: AirkServer,
}

impl LocalTools {
    pub async fn new(kernel: Kernel) -> anyhow::Result<Self> {
        Self::with_client(kernel, ClientFamily::Cli).await
    }

    pub async fn with_client(kernel: Kernel, client: ClientFamily) -> anyhow::Result<Self> {
        let admission = crate::execution::Admission::for_kernel(&kernel);
        Self::with_admission(kernel, client, admission, true).await
    }

    async fn with_admission(
        kernel: Kernel,
        family: ClientFamily,
        admission: crate::execution::Admission,
        owns_admission: bool,
    ) -> anyhow::Result<Self> {
        let (client_transport, server_transport) = tokio::io::duplex(64 * 1024);
        let mut server = AirkServer::new(kernel.clone());
        server.client = family;
        server.admission = admission.clone();
        let responses = server.responses.clone();
        let runtime = server.runtime.clone();
        let work = server.work.clone();
        let direct_server = server.clone();
        let (client, server) = tokio::try_join!(
            async { Ok::<_, anyhow::Error>(().serve(client_transport).await?) },
            async { Ok::<_, anyhow::Error>(server.serve(server_transport).await?) }
        )?;
        Ok(Self {
            work,
            runtime,
            kernel,
            family,
            owns_admission,
            responses,
            client,
            server,
            admission,
            direct_server,
        })
    }

    /// A direct client owns its response observations, while native work still
    /// shares the owning worker's finite admission budget.
    pub async fn session(&self) -> anyhow::Result<Self> {
        self.session_with_client(self.family).await
    }
    pub async fn session_with_client(&self, client: ClientFamily) -> anyhow::Result<Self> {
        Self::with_admission(self.kernel.clone(), client, self.admission.clone(), false).await
    }

    /// Separate native session with frozen read authority; shares only admission.
    pub async fn scoped_session(
        &self,
        authority: airk_kernel::scoped_reads::ScopedReadAuthority,
    ) -> anyhow::Result<Self> {
        self.scoped_session_with_client(authority, self.family)
            .await
    }
    pub async fn scoped_session_with_client(
        &self,
        authority: airk_kernel::scoped_reads::ScopedReadAuthority,
        client: ClientFamily,
    ) -> anyhow::Result<Self> {
        let kernel = self.kernel.with_scoped_reads(authority)?;
        Self::with_admission(kernel, client, self.admission.clone(), false).await
    }
    pub fn revoke_scoped_reads(&self) {
        self.kernel.revoke_scoped_reads();
    }

    /// In-process fence handle for the owning native worker; no transport authority.
    pub fn scoped_read_revocation_handle(&self) -> Kernel {
        self.kernel.clone()
    }

    pub fn set_runtime(&self, value: serde_json::Value) {
        if let Ok(mut runtime) = self.runtime.write() {
            *runtime = Some(value);
        }
    }
    pub fn set_work(&self, value: serde_json::Value) {
        if let Ok(mut work) = self.work.write() {
            *work = Some(value);
        }
    }

    pub async fn call(
        &self,
        name: String,
        arguments: serde_json::Map<String, serde_json::Value>,
    ) -> anyhow::Result<CallToolResult> {
        Ok(self
            .client
            .call_tool(CallToolRequestParams::new(name).with_arguments(arguments))
            .await?)
    }

    /// Context-bound native dispatch uses the exact SDK router and handlers;
    /// request-local context must not be lost crossing the local duplex SDK.
    pub async fn call_with_read_context(
        &self,
        name: String,
        arguments: serde_json::Map<String, serde_json::Value>,
        context: crate::read_attempt::ReadAttemptContext,
    ) -> anyhow::Result<CallToolResult> {
        let mut server = self.direct_server.clone();
        server.read_attempt = Some(if context.uses_remote_admission() {
            context.clone()
        } else {
            context.execution_child()
        });
        let mut request_context = rmcp::service::RequestContext::new(
            rmcp::model::NumberOrString::Number(0),
            self.server.peer().clone(),
        );
        request_context.ct = context.cancellation().clone();
        let result = server
            .tool_router
            .call(rmcp::handler::server::tool::ToolCallContext::new(
                &server,
                CallToolRequestParams::new(name).with_arguments(arguments),
                request_context,
            ))
            .await?;
        let rmcp::model::CallToolResponse::Complete(mut result) = result else {
            anyhow::bail!("native read returned an unsupported response kind");
        };
        // Match the SDK-completed wire result consumed by the existing local client.
        result.result_type = None;
        Ok(result)
    }

    pub async fn read_native(
        &self,
        request: NativeRead,
        project: airk_core::service::ProjectBinding,
    ) -> anyhow::Result<serde_json::Value> {
        self.read_native_inner(request, project, None).await
    }

    pub async fn read_native_with_context(
        &self,
        request: NativeRead,
        project: airk_core::service::ProjectBinding,
        context: crate::read_attempt::ReadAttemptContext,
    ) -> anyhow::Result<serde_json::Value> {
        self.read_native_inner(request, project, Some(context))
            .await
    }

    async fn read_native_inner(
        &self,
        request: NativeRead,
        project: airk_core::service::ProjectBinding,
        context: Option<crate::read_attempt::ReadAttemptContext>,
    ) -> anyhow::Result<serde_json::Value> {
        let native_deadline = context
            .as_ref()
            .and_then(|context| context.native_retry_deadline())
            .unwrap_or_else(|| std::time::Instant::now() + std::time::Duration::from_secs(5));
        let lifetime_guard = context
            .as_ref()
            .and_then(|context| context.lifetime_guard());
        let scoped = self
            .kernel
            .scoped_read_fingerprint()
            .map_err(|error| self.kernel.observe_scoped_error(error))?
            .is_some();
        if let NativeRead::SourceProjection { publication } = &request {
            if !scoped {
                return Err(airk_core::scoped_reads::ScopeReadRefused.into());
            }
            let permit = self
                .admission
                .admit_read(true, context.as_ref())
                .await
                .map_err(|_| {
                    self.kernel
                        .observe_scoped_error(anyhow::anyhow!("native read admission unavailable"))
                })?;
            let kernel = self.kernel.clone();
            let publication = *publication;
            // One exact retained publication observation. This does not probe
            // current source roots, reread configuration or repeat evidence.
            let result = tokio::task::spawn_blocking(move || {
                let _permit = permit;
                let _lifetime_guard = lifetime_guard;
                serde_json::to_value(kernel.scoped_source_projection(project, publication)?)
                    .map_err(anyhow::Error::from)
            })
            .await
            .map_err(|error| self.kernel.observe_scoped_error(error.into()))?
            .map_err(|error| self.kernel.observe_scoped_error(error))?;
            self.kernel
                .scoped_read_fingerprint()
                .map_err(|error| self.kernel.observe_scoped_error(error))?;
            return Ok(result);
        }
        if let NativeRead::VerifyProjection {
            value,
            controls,
            allocation,
            receipt_bytes_budget,
        } = &request
        {
            if !scoped {
                return Err(airk_core::scoped_reads::ScopeReadRefused.into());
            }
            // Source checking is one dispatched observation. In particular,
            // never replay it through the settings reader's contention loop.
            let permit = self
                .admission
                .admit_read(true, context.as_ref())
                .await
                .map_err(|_| {
                    self.kernel
                        .observe_scoped_error(anyhow::anyhow!("native read admission unavailable"))
                })?;
            let kernel = self.kernel.clone();
            let value = value.clone();
            let controls = *controls;
            let mut allocation = *allocation;
            if let (Some(context), Some(allocation)) = (&context, &mut allocation) {
                allocation.deadline_ms = allocation.deadline_ms.min(
                    u64::try_from(
                        context
                            .deadline()
                            .saturating_duration_since(std::time::Instant::now())
                            .as_millis(),
                    )
                    .unwrap_or(u64::MAX),
                );
            }
            let receipt_bytes_budget = *receipt_bytes_budget;
            let result = tokio::task::spawn_blocking(move || {
                let _permit = permit;
                let _lifetime_guard = lifetime_guard;
                kernel.verify_scoped_projection_budgeted(
                    project,
                    value,
                    controls,
                    allocation,
                    receipt_bytes_budget,
                )
            })
            .await
            .map_err(|error| self.kernel.observe_scoped_error(error.into()))?
            .map_err(|error| self.kernel.observe_scoped_error(error))?;
            self.kernel
                .scoped_read_fingerprint()
                .map_err(|error| self.kernel.observe_scoped_error(error))?;
            return Ok(result);
        }
        if scoped {
            // InputFile validation opens caller-selected paths; quota is shared
            // across collections and usage includes retained historical scopes.
            // Full current-index permission does not authorize those families.
            if !matches!(&request,NativeRead::Settings {view,..} if scoped_metadata_view(*view).is_some())
            {
                return Err(self
                    .kernel
                    .observe_scoped_error(airk_core::scoped_reads::ScopeReadRefused.into()));
            }
        }
        let permit = self
            .admission
            .admit_read(true, context.as_ref())
            .await
            .map_err(|_| {
                self.kernel
                    .observe_scoped_error(anyhow::anyhow!("native read admission unavailable"))
            })?;
        let permit = std::sync::Arc::new(permit);
        let selected_once = context.is_some();
        let deadline = context.as_ref().map_or(native_deadline, |context| {
            native_deadline.min(context.deadline())
        });
        let kernel = match &context {
            Some(context) => {
                crate::execution::select_read_snapshot(&self.kernel, permit.clone(), Some(context))
                    .await
                    .map_err(|error| self.kernel.observe_scoped_error(error))?
            }
            None => self.kernel.clone(),
        };
        if let Some(context) = &context {
            context.mark_handler_started();
        }
        let result = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            let _lifetime_guard = lifetime_guard;
            retry_native_read(deadline, !selected_once, || {
                let kernel = if selected_once {
                    kernel.clone()
                } else {
                    kernel.refresh_service_snapshot()?
                };
                if kernel.scoped_read_fingerprint()?.is_some() {
                    let NativeRead::Settings {
                        view,
                        cursor,
                        max_bytes,
                    } = request.clone()
                    else {
                        return Err(airk_core::scoped_reads::ScopeReadRefused.into());
                    };
                    let metadata = kernel.scoped_index_metadata(
                        project,
                        scoped_metadata_view(view)
                            .ok_or(airk_core::scoped_reads::ScopeReadRefused)?,
                    )?;
                    return Ok(serde_json::to_value(airk_kernel::usage_page::scoped_page(
                        kernel.attach_rebuild_observation(
                            serde_json::json!({"project":project,"value":metadata.value}),
                        )?,
                        cursor.as_deref(),
                        max_bytes,
                        metadata.observation,
                        metadata.publication,
                    )?)?);
                }
                match kernel.prepare_native_read(request.clone())? {
                    NativeRead::PrepareInputFile {
                        path,
                        max_source_bytes,
                    } => Ok(serde_json::to_value(kernel.prepare_source_input(
                        std::path::Path::new(&path),
                        max_source_bytes,
                    )?)?),
                    NativeRead::VerifyProjection { .. } | NativeRead::SourceProjection { .. } => {
                        anyhow::bail!("verification requires its scoped single-dispatch owner")
                    }
                    NativeRead::ValidateSettingsFile { .. }
                    | NativeRead::UsageReportFile { .. } => {
                        anyhow::bail!("native file intent was not prepared")
                    }
                    NativeRead::ValidateSettings {
                        view,
                        input,
                        cursor,
                        max_bytes,
                    } => {
                        let value = match view {
                            SettingsView::ExtractionPolicy => {
                                let policy = serde_json::from_slice(
                                    &airk_kernel::service_configuration::verified_input(
                                        &input, 131_072,
                                    )?,
                                )?;
                                serde_json::to_value(kernel.validate_extraction_policy(&policy)?)?
                            }
                            SettingsView::VectorSelection => {
                                let policy = serde_json::from_slice(
                                    &airk_kernel::service_configuration::verified_input(
                                        &input, 16_384,
                                    )?,
                                )?;
                                serde_json::to_value(kernel.validate_vector_selection(&policy)?)?
                            }
                            SettingsView::ScanLimits => {
                                let limits: airk_kernel::limits::ScanLimits =
                                    serde_json::from_slice(
                                        &airk_kernel::service_configuration::verified_input(
                                            &input, 65_536,
                                        )?,
                                    )?;
                                limits.validate()?;
                                serde_json::to_value(limits)?
                            }
                            SettingsView::QuotaPolicy => {
                                let policy: airk_kernel::storage::QuotaPolicy =
                                    serde_json::from_slice(
                                        &airk_kernel::service_configuration::verified_input(
                                            &input, 16_384,
                                        )?,
                                    )?;
                                policy.validate()?;
                                serde_json::to_value(policy)?
                            }
                            _ => {
                                anyhow::bail!("settings view does not accept validation input")
                            }
                        };
                        Ok(serde_json::to_value(airk_kernel::usage_page::page(
                            kernel.attach_rebuild_observation(
                                serde_json::json!({"project":project,"value":value}),
                            )?,
                            cursor.as_deref(),
                            max_bytes,
                        )?)?)
                    }
                    NativeRead::RetentionMigrationPreview { target } => Ok(serde_json::to_value(
                        kernel.preview_retained_migration(target)?,
                    )?),
                    NativeRead::Settings {
                        view,
                        cursor,
                        max_bytes,
                    } => {
                        let value = match view {
                            SettingsView::ExtractionPolicy => {
                                serde_json::to_value(kernel.extraction_policy()?)?
                            }
                            SettingsView::VectorSelection => {
                                serde_json::to_value(kernel.vector_selection_status()?)?
                            }
                            SettingsView::VectorStatus => {
                                serde_json::to_value(kernel.vector_status()?)?
                            }
                            SettingsView::Roots => serde_json::to_value(kernel.roots()?)?,
                            SettingsView::ScanLimits => {
                                serde_json::to_value(kernel.describe_scan_limits()?)?
                            }
                            SettingsView::QuotaPolicy => {
                                serde_json::to_value(kernel.quota_policy()?)?
                            }
                            SettingsView::QuotaStatus => {
                                serde_json::to_value(kernel.quota_status()?)?
                            }
                        };
                        Ok(serde_json::to_value(airk_kernel::usage_page::page(
                            kernel.attach_rebuild_observation(
                                serde_json::json!({"project":project,"value":value}),
                            )?,
                            cursor.as_deref(),
                            max_bytes,
                        )?)?)
                    }
                    NativeRead::UsagePolicy => {
                        serde_json::to_value(kernel.usage_policy()?).map_err(Into::into)
                    }
                    NativeRead::UsageReport {
                        days,
                        cursor,
                        max_bytes,
                        format,
                        price,
                    } => {
                        let view = match price {
                            Some(price) => kernel.usage_report_with_price(days, &price)?,
                            None => kernel.usage_report(days)?,
                        };
                        let view = match format {
                            ReportFormat::Json => serde_json::to_value(view)?,
                            _ => serde_json::Value::String(airk_kernel::usage::render_report(
                                &view,
                                format.name(),
                            )?),
                        };
                        Ok(serde_json::to_value(airk_kernel::usage_page::page(
                            serde_json::json!({"project":project,"value":view}),
                            cursor.as_deref(),
                            max_bytes,
                        )?)?)
                    }
                }
            })
        })
        .await
        .map_err(|error| self.kernel.observe_scoped_error(error.into()))?
        .map_err(|error| self.kernel.observe_scoped_error(error))?;
        self.kernel
            .scoped_read_fingerprint()
            .map_err(|error| self.kernel.observe_scoped_error(error))?;
        Ok(result)
    }

    pub fn status_receipt(&self) -> crate::response::StatusReceipt {
        self.responses.receipt()
    }

    pub fn restore_status_receipt(&self, receipt: crate::response::StatusReceipt) {
        self.responses.restore(receipt);
    }

    pub async fn close(&mut self) -> anyhow::Result<()> {
        let client_result = self.client.close().await;
        let server_result = self.server.close().await;
        // The project owner drains global admission after all client sessions;
        // closing one client must not close admission for independent clients.
        if self.owns_admission {
            self.admission.shutdown().await;
        }
        client_result?;
        server_result?;
        Ok(())
    }
}

#[cfg(test)]
mod native_scoped_settings_tests;

/// Native settings/report reads use the same typed contention contract as MCP
/// evidence reads. Keep the original request/cursor and one bounded deadline.
fn retry_native_read<T>(
    deadline: std::time::Instant,
    retry_selection: bool,
    mut read: impl FnMut() -> anyhow::Result<T>,
) -> anyhow::Result<T> {
    loop {
        match read() {
            Err(error) => {
                if !retry_selection
                    && error.is::<airk_kernel::retrieval_retry::SelectionReadAdmissionContention>()
                {
                    return Err(error);
                }
                let Some(hint) = airk_kernel::retrieval_retry::read_retry_hint(&error) else {
                    return Err(error);
                };
                let delay = std::time::Duration::from_millis(u64::from(hint.retry_after_ms));
                if std::time::Instant::now() + delay >= deadline {
                    return Err(error);
                }
                std::thread::sleep(delay);
            }
            result => return result,
        }
    }
}

#[cfg(test)]
mod rebuild_parity_tests;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shared_native_routes_keep_exact_definitions_and_isolate_router_mutations() {
        let mut direct = AirkServer::tool_router().list_all();
        direct.sort_by(|a, b| a.name.cmp(&b.name));
        assert_eq!(
            serde_json::to_value(direct).unwrap(),
            serde_json::to_value(tools()).unwrap()
        );
        let mut first = AirkServer::shared_tool_router();
        let second = AirkServer::shared_tool_router();
        assert!(std::sync::Arc::ptr_eq(
            &first.map["airk_file_info"].call,
            &second.map["airk_file_info"].call
        ));
        let route = first.map.get_mut("airk_file_info").unwrap();
        std::sync::Arc::make_mut(&mut route.attr.input_schema)
            .insert("caller-local".into(), serde_json::json!(true));
        first.map.remove("airk_status");
        let third = AirkServer::shared_tool_router();
        assert!(third.map.contains_key("airk_status"));
        assert!(
            third.map["airk_file_info"]
                .attr
                .input_schema
                .get("caller-local")
                .is_none()
        );
    }

    #[tokio::test]
    async fn shared_routes_dispatch_only_to_the_current_servers_kernel() -> anyhow::Result<()> {
        let owner = tempfile::tempdir()?;
        let mut clients = Vec::new();
        for name in ["first", "second"] {
            let root = owner.path().join(name);
            std::fs::create_dir(&root)?;
            std::fs::write(
                root.join(format!("{name}.rs")),
                "pub struct SharedMarker;\n",
            )?;
            let kernel = Kernel::open(&root, Some(owner.path().join(format!("{name}-data"))))?;
            kernel.configure_vectors(false)?;
            kernel.index()?;
            clients.push(LocalTools::new(kernel).await?);
        }
        let mut bindings = Vec::new();
        for (index, name) in ["first", "second", "first"].iter().enumerate() {
            let value = body(clients[index % 2].call("airk_symbols".into(),
                serde_json::json!({"name":"SharedMarker","compact":false,"response_mode":"full"}).as_object().unwrap().clone()).await?)?;
            assert_eq!(value["items"][0]["citation"]["path"], format!("{name}.rs"));
            assert_eq!(value["source_verification"]["status"], "verified");
            bindings.push(value["source_verification"]["origins"][0]["legacy_project"].clone());
        }
        assert_ne!(bindings[0], bindings[1]);
        assert_eq!(bindings[0], bindings[2]);
        for client in &mut clients {
            client.close().await?;
        }
        Ok(())
    }

    #[test]
    fn immutable_catalog_preserves_exact_definitions_and_isolates_caller_mutation() {
        let expected = serde_json::to_vec(&build_tools()).unwrap();
        assert_eq!(serde_json::to_vec(&tools()).unwrap(), expected);
        let mut changed = tools();
        changed[0].description = Some("caller-local description".into());
        std::sync::Arc::make_mut(&mut changed[0].input_schema)
            .insert("caller-local".into(), serde_json::json!(true));
        changed.reverse();
        changed.pop();
        assert_ne!(serde_json::to_vec(&changed).unwrap(), expected);
        assert_eq!(serde_json::to_vec(&tools()).unwrap(), expected);
    }

    #[test]
    fn native_read_retries_only_typed_contention_with_one_deadline() {
        use std::time::{Duration, Instant};
        let mut attempts = 0;
        let value = retry_native_read(Instant::now() + Duration::from_secs(1), true, || {
            attempts += 1;
            if attempts == 1 {
                Err(anyhow::anyhow!("publication applying")
                    .context(airk_kernel::CurrentGateContention::Applying))
            } else {
                Ok(42)
            }
        })
        .unwrap();
        assert_eq!((value, attempts), (42, 2));
        attempts = 0;
        let result: anyhow::Result<()> =
            retry_native_read(Instant::now() + Duration::from_secs(1), true, || {
                attempts += 1;
                anyhow::bail!("invalid retained evidence")
            });
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("invalid retained evidence")
        );
        assert_eq!(attempts, 1);
        attempts = 0;
        let result: anyhow::Result<()> = retry_native_read(Instant::now(), true, || {
            attempts += 1;
            Err(anyhow::anyhow!("publication applying")
                .context(airk_kernel::CurrentGateContention::Applying))
        });
        assert!(result.is_err());
        assert_eq!(attempts, 1);
    }
    fn body(value: CallToolResult) -> anyhow::Result<serde_json::Value> {
        anyhow::ensure!(!value.is_error.unwrap_or(false), "{value:?}");
        Ok(serde_json::from_str(
            &value.content[0].as_text().unwrap().text,
        )?)
    }
    #[tokio::test]
    async fn client_sessions_keep_status_separate_and_closing_one_preserves_other_admission()
    -> anyhow::Result<()> {
        let directory = tempfile::tempdir()?;
        let root = directory.path().join("source");
        std::fs::create_dir(&root)?;
        std::fs::write(root.join("one.rs"), "fn alpha() -> bool { true }\n")?;
        let kernel = Kernel::open(&root, Some(directory.path().join("data")))?;
        kernel.configure_vectors(false)?;
        kernel.index()?;
        let mut owner = LocalTools::new(kernel).await?;
        let mut first = owner.session().await?;
        let mut second = owner.session().await?;
        let args = serde_json::json!({"name":"alpha","response_mode":"terse"})
            .as_object()
            .unwrap()
            .clone();
        let initial = body(first.call("airk_code_facts".into(), args.clone()).await?)?;
        assert!(initial.get("freshness").is_some());
        let repeated = body(first.call("airk_code_facts".into(), args.clone()).await?)?;
        assert!(repeated.get("freshness").is_none());
        let independent = body(second.call("airk_code_facts".into(), args.clone()).await?)?;
        assert!(independent.get("freshness").is_some());
        assert_eq!(
            initial["publication"]["token"],
            independent["publication"]["token"]
        );
        first.close().await?;
        let mut requested = args.clone();
        requested.insert("include_status".into(), true.into());
        let requested = body(second.call("airk_code_facts".into(), requested).await?)?;
        assert!(requested.get("freshness").is_some());
        let mut full = args;
        full.insert("response_mode".into(), "full".into());
        full.insert("compact".into(), false.into());
        let full = body(second.call("airk_code_facts".into(), full).await?)?;
        assert!(full["items"][0]["code"].get("producer").is_some());
        assert!(full.get("freshness").is_some());
        assert_eq!(
            initial["publication"]["token"],
            full["publication"]["token"]
        );
        let mut validators = Vec::new();
        for mode in ["terse", "full"] {
            let args = serde_json::json!({"path":"one.rs","start_line":1,"end_line":1,"response_mode":mode}).as_object().unwrap().clone();
            let read = body(second.call("airk_read_file".into(), args).await?)?;
            assert!(read["read"]["validator"].is_string());
            validators.push(read["read"]["validator"].clone());
            assert_eq!(read["hits"][0]["text"], "fn alpha() -> bool { true }\n");
        }
        assert_eq!(validators[0], validators[1]);
        second.close().await?;
        owner.close().await?;
        Ok(())
    }

    #[tokio::test]
    async fn status_and_diagnostics_preserve_typed_publication_contention() -> anyhow::Result<()> {
        let directory = tempfile::tempdir()?;
        let root = directory.path().join("source");
        let data = directory.path().join("data");
        std::fs::create_dir(&root)?;
        std::fs::write(root.join("one.rs"), "fn alpha() {}\n")?;
        let kernel = Kernel::open(&root, Some(data.clone()))?;
        kernel.configure_vectors(false)?;
        kernel.index()?;
        let publication = kernel.status_summary()?["publication"]["token"].clone();
        let mut tools = LocalTools::new(kernel).await?;
        tools.set_runtime(serde_json::json!({"schema":1,"service":{"responsive":true}}));
        tools.set_work(serde_json::json!({"schema":1,"pending_job":null}));
        // Hold the actual publication gate without altering any source or index
        // bytes. Both handlers must distinguish this from an unavailable index.
        let gate = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(data.join("collections-1.2/.current-gates/default"))?;
        assert!(
            gate.try_lock().is_ok(),
            "fixture publication gate must be exclusive"
        );
        for tool in ["airk_status", "airk_diagnostics"] {
            for mode in ["terse", "full"] {
                let reply = tools
                    .call(
                        tool.into(),
                        serde_json::json!({"response_mode":mode})
                            .as_object()
                            .unwrap()
                            .clone(),
                    )
                    .await?;
                assert_eq!(reply.is_error, Some(true), "{tool} {mode}: {reply:?}");
                let hint: serde_json::Value =
                    serde_json::from_str(&reply.content[0].as_text().unwrap().text)?;
                assert_eq!(hint["code"], "publication_busy", "{hint}");
                assert_eq!(hint["action"], "repeat_same_request", "{hint}");
                assert_eq!(hint["retryable"], true, "{hint}");
                assert!(
                    hint["retry_after_ms"]
                        .as_u64()
                        .is_some_and(|delay| delay > 0 && delay <= 1000),
                    "{hint}"
                );
                assert!(hint.get("index_state").is_none(), "{hint}");
            }
        }
        gate.unlock()?;
        drop(gate);
        for tool in ["airk_status", "airk_diagnostics"] {
            let status = body(tools.call(tool.into(), Default::default()).await?)?;
            assert_eq!(status["index_state"], "available");
            assert_eq!(status["publication"]["token"], publication);
        }
        tools.close().await?;
        Ok(())
    }

    #[tokio::test]
    async fn status_summary_is_light_and_diagnostics_are_explicit_and_client_local()
    -> anyhow::Result<()> {
        let directory = tempfile::tempdir()?;
        let root = directory.path().join("source");
        std::fs::create_dir(&root)?;
        std::fs::write(root.join("one.rs"), "fn alpha() {}\n")?;
        let kernel = Kernel::open(&root, Some(directory.path().join("data")))?;
        kernel.configure_vectors(false)?;
        kernel.index()?;
        let mut owner = LocalTools::new(kernel).await?;
        let mut first = owner.session().await?;
        let mut second = owner.session().await?;
        let runtime = serde_json::json!({"schema":1,"mcp_frontend":{"process":{"version":"1.4.0"}},"service":null,"service_error":"service_health_unavailable"});
        first.set_runtime(runtime.clone());
        let work = serde_json::json!({"schema":1,"progress_current_worker":false,"pending_job":null,"failed_observations":["watch_unavailable"]});
        first.set_work(work.clone());
        for mode in ["terse", "full", "terse"] {
            let args = serde_json::json!({"response_mode":mode,"include_status":true})
                .as_object()
                .unwrap()
                .clone();
            assert_eq!(
                body(first.call("airk_diagnostics".into(), args).await?)?["runtime"],
                runtime
            );
            assert_eq!(
                body(
                    first
                        .call(
                            "airk_diagnostics".into(),
                            serde_json::json!({"include_status":true})
                                .as_object()
                                .unwrap()
                                .clone()
                        )
                        .await?
                )?["work"],
                work
            );
        }
        assert_eq!(
            body(
                second
                    .call(
                        "airk_diagnostics".into(),
                        serde_json::json!({"response_mode":"full"})
                            .as_object()
                            .unwrap()
                            .clone()
                    )
                    .await?
            )?["runtime"]["error"],
            "native_runtime_context_unavailable"
        );
        for _ in 0..2 {
            let summary = body(first.call("airk_status".into(), Default::default()).await?)?;
            assert_eq!(summary["files"], 1);
            assert_eq!(summary["version"], env!("CARGO_PKG_VERSION"));
            assert!(summary["health"].is_string());
            assert!(summary.get("runtime").is_none() && summary.get("work").is_none());
            assert!(
                summary.get("freshness").is_some() && summary.get("semantic_readiness").is_some()
            );
            assert!(serde_json::to_vec(&summary)?.len() < 1024);
        }
        first.close().await?;
        second.close().await?;
        owner.close().await?;
        Ok(())
    }

    #[tokio::test]
    async fn native_status_keeps_service_and_work_diagnostics_before_first_publication()
    -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        let data = tempfile::tempdir()?;
        let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
        let mut owner = LocalTools::new(kernel).await?;
        let summary = body(owner.call("airk_status".into(), Default::default()).await?)?;
        assert_eq!(summary["index_state"], "unavailable");
        assert!(summary.get("runtime").is_none());
        owner.set_runtime(serde_json::json!({"schema":1,"service":{"responsive":true}}));
        owner.set_work(serde_json::json!({"schema":1,"progress":{"phase":"discovering"}}));
        let result = owner
            .call(
                "airk_diagnostics".into(),
                serde_json::json!({"response_mode":"full"})
                    .as_object()
                    .unwrap()
                    .clone(),
            )
            .await?;
        assert!(!result.is_error.unwrap_or(false));
        let status = body(result)?;
        assert_eq!(status["index_state"], "unavailable");
        assert!(status["files"].is_null());
        assert!(status["publication"].is_null());
        assert_eq!(status["runtime"]["service"]["responsive"], true);
        assert_eq!(status["work"]["progress"]["phase"], "discovering");
        assert!(status["index_error"]["code"].is_string());
        owner.close().await?;
        Ok(())
    }

    #[test]
    fn evidence_tools_advertise_response_controls_and_discovery_keeps_its_closed_metadata_contract()
    {
        for tool in tools() {
            let schema = serde_json::to_value(&tool.input_schema).unwrap();
            if tool.name == "airk_query_scope" {
                assert_eq!(
                    schema["properties"],
                    airk_core::combined_reads::query_scope::schema()["properties"]
                );
                for key in [
                    "response_mode",
                    "include_status",
                    "reply_detail",
                    "verification",
                ] {
                    assert!(
                        schema["properties"].get(key).is_none(),
                        "query_scope must not advertise {key}"
                    );
                }
                continue;
            }
            assert!(
                schema["properties"].get("response_mode").is_some(),
                "{}",
                tool.name
            );
            assert!(
                schema["properties"].get("include_status").is_some(),
                "{}",
                tool.name
            );
            let mut choices: Vec<serde_json::Value> = airk_core::reply::ReplyDetail::ALL
                .into_iter()
                .map(|v| serde_json::json!(v))
                .collect();
            choices.push(serde_json::Value::Null);
            assert_eq!(
                schema["properties"]["reply_detail"]["enum"],
                serde_json::json!(choices),
                "{}",
                tool.name
            );
            assert!(
                tool.description
                    .as_deref()
                    .is_some_and(|description| description.contains("response_mode=full")),
                "{}: description must explain response_mode=full",
                tool.name
            );
        }
    }

    #[tokio::test]
    async fn retained_changes_page_across_response_modes_with_index_current_validation()
    -> anyhow::Result<()> {
        let directory = tempfile::tempdir()?;
        let root = directory.path().join("source");
        std::fs::create_dir(&root)?;
        std::fs::write(root.join("one.rs"), "fn alpha() -> u32 { 1 }\n")?;
        let kernel = Kernel::open(&root, Some(directory.path().join("data")))?;
        kernel.configure_vectors(false)?;
        kernel.index()?;
        let baseline = kernel.status()?.publication.unwrap().token;
        for value in [2, 3] {
            std::fs::write(
                root.join("one.rs"),
                format!("fn alpha() -> u32 {{ {value} }}\n"),
            )?;
            kernel.index()?;
        }
        let mut tools = LocalTools::new(kernel).await?;
        let mut args = serde_json::json!({"since":baseline,"selectors":[{"kind":"path","path":"one.rs"}],"limit":1,"max_bytes":65536,"response_mode":"full"}).as_object().unwrap().clone();
        let first = body(tools.call("airk_changes".into(), args.clone()).await?)?;
        assert_eq!(first["baseline_status"], "verified");
        assert_eq!(first["changes"][0]["observations"], 2);
        assert!(first["cursor"].is_string());
        args.insert("cursor".into(), first["cursor"].clone());
        args.insert("response_mode".into(), "terse".into());
        let second = body(tools.call("airk_changes".into(), args).await?)?;
        assert_eq!(second["validations"][0]["outcome"], "index-current");
        assert_eq!(
            first["publication"]["token"],
            second["publication"]["token"]
        );
        assert_eq!(second["truncated"], false);
        assert_eq!(second["freshness"]["live_source_verified"], false);
        tools.close().await?;
        Ok(())
    }

    #[tokio::test]
    async fn terminal_call_names_keep_written_targets_and_candidate_labels() -> anyhow::Result<()> {
        let directory = tempfile::tempdir()?;
        let root = directory.path().join("source");
        std::fs::create_dir(&root)?;
        std::fs::write(
            root.join("one.py"),
            "def run(x):\n    x.next().reclaim_objects()\n",
        )?;
        let kernel = Kernel::open(&root, Some(directory.path().join("data")))?;
        kernel.configure_vectors(false)?;
        kernel.index()?;
        let mut tools = LocalTools::new(kernel).await?;
        for mode in ["terse", "full"] {
            for compact in [true, false] {
                let args = serde_json::json!({"name":"reclaim_objects","kind":"call","compact":compact,"response_mode":mode});
                let result = body(
                    tools
                        .call("airk_code_facts".into(), args.as_object().unwrap().clone())
                        .await?,
                )?;
                assert_eq!(result["items"].as_array().unwrap().len(), 1);
                assert_eq!(result["items"][0]["name"], "x.next().reclaim_objects");
                assert_eq!(result["items"][0]["candidate"], true);
                if !compact {
                    assert_eq!(
                        result["items"][0]["code"]["target"],
                        "x.next().reclaim_objects"
                    );
                    assert_eq!(result["items"][0]["code"]["basis"], "syntax");
                }
            }
        }
        tools.close().await?;
        Ok(())
    }

    #[tokio::test]
    async fn mixed_tools_measure_terse_bytes_without_changing_evidence() -> anyhow::Result<()> {
        let directory = tempfile::tempdir()?;
        let root = directory.path().join("source");
        std::fs::create_dir(&root)?;
        let source = format!(
            "import os\n{}",
            (0..12)
                .map(|n| format!("def item_{n}(value):\n    return os.path.exists(value)\n\n"))
                .collect::<String>()
        );
        std::fs::write(root.join("one.py"), source)?;
        std::fs::write(
            root.join("notes.md"),
            "# Local fixture\n\nThe client must preserve original source locations.\n",
        )?;
        let kernel = Kernel::open(&root, Some(directory.path().join("data")))?;
        kernel.configure_vectors(false)?;
        kernel.index()?;
        let mut tools = LocalTools::new(kernel).await?;
        let mut full_bytes = 0;
        let mut terse_bytes = 0;
        for (tool, args) in [
            (
                "airk_outline",
                serde_json::json!({"path":"one.py","compact":false}),
            ),
            ("airk_symbols", serde_json::json!({"compact":false})),
            ("airk_imports", serde_json::json!({"compact":false})),
            (
                "airk_code_facts",
                serde_json::json!({"kind":"call","compact":false}),
            ),
            ("airk_document_facts", serde_json::json!({"compact":false})),
            (
                "airk_relationships",
                serde_json::json!({"symbol":"file:one.py"}),
            ),
            (
                "airk_search",
                serde_json::json!({"query":"preserve","mode":"lexical"}),
            ),
            (
                "airk_read_file",
                serde_json::json!({"path":"one.py","start_line":1,"end_line":3}),
            ),
            (
                "airk_context",
                serde_json::json!({"path":"one.py","line":2}),
            ),
        ] {
            let mut results = Vec::new();
            let mut bytes = Vec::new();
            for mode in ["full", "terse"] {
                let mut args = args.as_object().unwrap().clone();
                args.insert("response_mode".into(), mode.into());
                let result = tools.call(tool.into(), args).await?;
                bytes.push(serde_json::to_vec(&result)?.len());
                results.push(body(result)?);
            }
            assert_eq!(
                results[0]["publication"]["token"],
                results[1]["publication"]["token"]
            );
            if results[0].get("read").is_some() {
                assert_eq!(
                    results[0]["read"]["validator"],
                    results[1]["read"]["validator"]
                );
            }
            for key in ["items", "hits"] {
                if let Some(rows) = results[0][key].as_array() {
                    assert_eq!(rows.len(), results[1][key].as_array().unwrap().len());
                    for (full, terse) in rows.iter().zip(results[1][key].as_array().unwrap()) {
                        for field in ["name", "citation", "text", "resolution"] {
                            assert_eq!(full.get(field), terse.get(field), "{tool}: {field}");
                        }
                    }
                }
            }
            assert!(bytes[1] < bytes[0], "{tool}: {bytes:?}");
            eprintln!(
                "mixed response fixture {tool}: full={} terse={}",
                bytes[0], bytes[1]
            );
            full_bytes += bytes[0];
            terse_bytes += bytes[1];
        }
        let rich = body(
            tools
                .call(
                    "airk_symbols".into(),
                    serde_json::json!({"compact":false,"response_mode":"terse","limit":100,"max_bytes":65536})
                        .as_object()
                        .unwrap()
                        .clone(),
                )
                .await?,
        )?;
        let minimal = body(
            tools
                .call(
                    "airk_symbols".into(),
                    serde_json::json!({"response_mode":"terse","limit":100,"max_bytes":65536})
                        .as_object()
                        .unwrap()
                        .clone(),
                )
                .await?,
        )?;
        assert_eq!(
            rich["items"].as_array().unwrap().len(),
            minimal["items"].as_array().unwrap().len()
        );
        assert!(serde_json::to_vec(&minimal)?.len() < serde_json::to_vec(&rich)?.len());
        eprintln!(
            "minimal symbol fixture: rich={} minimal={}",
            serde_json::to_vec(&rich)?.len(),
            serde_json::to_vec(&minimal)?.len()
        );
        eprintln!("mixed response fixture total: full={full_bytes} terse={terse_bytes}");
        tools.close().await?;
        Ok(())
    }

    #[tokio::test]
    async fn minimal_navigation_keeps_validators_and_has_one_fact_result_key() -> anyhow::Result<()>
    {
        let directory = tempfile::tempdir()?;
        let root = directory.path().join("source");
        std::fs::create_dir(&root)?;
        std::fs::write(
            root.join("one.rs"),
            "fn alpha() -> bool { true }\nfn beta() {}\n",
        )?;
        let kernel = Kernel::open(&root, Some(directory.path().join("data")))?;
        kernel.configure_vectors(false)?;
        kernel.index()?;
        let mut tools = LocalTools::new(kernel).await?;
        let mut validators = Vec::new();
        for mode in ["terse", "full"] {
            let args = serde_json::json!({"path":"one.rs","response_mode":mode})
                .as_object()
                .unwrap()
                .clone();
            let outline = body(tools.call("airk_outline".into(), args).await?)?;
            assert_eq!(outline["representation"], "minimal");
            assert_eq!(outline["items"][0]["name"], "alpha");
            assert!(outline["items"][0].get("qualified_name").is_none());
            assert!(outline["items"][0].get("file").is_none());
            assert_eq!(outline["files"][0]["path"], "one.rs");
            validators.push(outline["read"]["validator"].clone());
            assert!(validators.last().unwrap().is_string());
        }
        assert_eq!(validators[0], validators[1]);
        let conditional = body(
            tools
                .call(
                    "airk_outline".into(),
                    serde_json::json!({"path":"one.rs","if_evidence":validators[0]})
                        .as_object()
                        .unwrap()
                        .clone(),
                )
                .await?,
        )?;
        assert!(conditional["items"].as_array().unwrap().is_empty());
        for (tool, args) in [
            ("airk_symbols", serde_json::json!({})),
            ("airk_imports", serde_json::json!({})),
            ("airk_code_facts", serde_json::json!({})),
            ("airk_document_facts", serde_json::json!({})),
            ("airk_relationships", serde_json::json!({"symbol":"absent"})),
            (
                "airk_outline",
                serde_json::json!({"path":"one.rs","compact":false}),
            ),
        ] {
            let result = body(
                tools
                    .call(tool.into(), args.as_object().unwrap().clone())
                    .await?,
            )?;
            assert!(result["items"].is_array(), "{tool}: {result}");
            for old in ["facts", "outline", "relationships", "minimal"] {
                assert!(result.get(old).is_none(), "{tool}: {old}");
            }
        }
        tools.close().await?;
        Ok(())
    }
    #[tokio::test]
    async fn service_clients_share_bounded_materialization_and_close_independently()
    -> anyhow::Result<()> {
        let owner = tempfile::tempdir()?;
        let root = owner.path().join("source");
        let data = owner.path().join("data");
        std::fs::create_dir(&root)?;
        std::fs::create_dir(&data)?;
        std::fs::write(root.join("fixture.rs"), "pub fn copper() {}\n")?;
        let kernel = Kernel::open_service_collection(&root, Some(data), "fixture")?;
        kernel.configure_vectors(false)?;
        kernel.index()?;
        let mut tools = LocalTools::new(kernel).await?;
        let mut first = tools.session().await?;
        let mut second = tools.session().await?;
        for client in [&tools, &first, &second] {
            assert_eq!(client.admission.status()["shared_execution_budget"], true);
            assert_eq!(client.admission.status()["evidence_workers"], 65);
        }
        let args = serde_json::json!({"name":"copper"})
            .as_object()
            .unwrap()
            .clone();
        assert!(
            !first
                .call("airk_symbols".into(), args.clone())
                .await?
                .is_error
                .unwrap_or(false)
        );
        first.close().await?;
        assert!(
            !second
                .call("airk_symbols".into(), args)
                .await?
                .is_error
                .unwrap_or(false)
        );
        second.close().await?;
        tools.close().await?;
        Ok(())
    }
}
