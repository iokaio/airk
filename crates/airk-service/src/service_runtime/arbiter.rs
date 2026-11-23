// SPDX-License-Identifier: Apache-2.0
//! Owner-authenticated management and exact installed-index application.
use super::*;
use airk_core::service::NativeNextReadRequest;
use airk_core::{
    arbiter::*,
    service::{ScopedReadRef, WorkerIdentity},
};
use anyhow::Context;
#[cfg(feature = "local-inference")]
mod assets;
mod card_scoring;
#[cfg(all(feature = "local-inference", feature = "test-support"))]
pub(super) mod catalog_only;
pub(super) mod device_management;
#[cfg(feature = "local-inference")]
mod download;
#[cfg(all(test, feature = "local-inference"))]
mod download_reason_tests;
#[cfg(all(feature = "local-inference", feature = "test-support"))]
pub(super) mod evaluation;
#[cfg(test)]
mod failure_tests;
#[cfg(all(test, feature = "local-inference", feature = "test-support"))]
mod final_distribution;
mod focus_batch;
mod focus_scoring;
#[cfg(any(feature = "local-inference", test))]
mod joint_ledger;
mod joint_scoring;
#[cfg(feature = "local-inference")]
mod idle;
#[cfg(feature = "local-inference")]
mod release;
#[cfg(feature = "local-inference")]
mod lifecycle;
#[cfg(any(feature = "local-inference", test))]
mod loop_ledger;
pub(super) mod policy_delivery;
mod read_device_status;
mod scoring;
#[cfg(all(test, feature = "local-inference", feature = "test-support"))]
pub(super) mod warm_observer;

/// One service composition owner; stages share custody, revisions and the actual
/// inference lane, while their lifecycle cancellation/selection remains separate.
pub(super) struct Owner {
    focus: StageOwner,
    context: StageOwner,
    configuration: Arc<airk_store::service::arbiter::ConfigurationStore>,
    #[cfg(all(feature = "local-inference", feature = "test-support"))]
    ownership: Arc<airk_store::service::ServiceLease>,
    #[cfg(all(feature = "local-inference", feature = "test-support"))]
    gate: airk_catalog_control::catalog_admission::NativeCatalogAdmission,
    #[cfg(feature = "local-inference")]
    assets: Arc<airk_store::service::arbiter_assets::AssetStore>,
}
#[cfg(feature = "local-inference")]
pub(super) struct Shutdown {
    focus: Result<lifecycle::StageShutdown>,
    context: Result<lifecycle::StageShutdown>,
    deadline: Instant,
}
impl Owner {
    pub(super) fn new(
        ownership: Arc<airk_store::service::ServiceLease>,
        gate: airk_catalog_control::catalog_admission::NativeCatalogAdmission,
    ) -> Result<Self> {
        let context = StageOwner::new(ownership.clone(), gate.clone(), Stage::Context)?;
        let focus = StageOwner {
            stage: Stage::Focus,
            #[cfg(feature = "local-inference")]
            devices: context.devices.clone(),
            #[cfg(feature = "local-inference")]
            joint_ledger: context.joint_ledger.clone(),
            #[cfg(feature = "local-inference")]
            loop_ledger: context.loop_ledger.clone(),
            #[cfg(all(feature = "local-inference", feature = "test-support"))]
            decision_mode: None,
            #[cfg(all(feature = "local-inference", feature = "test-support"))]
            evaluation: None,
            #[cfg(all(feature = "local-inference", feature = "test-support"))]
            context_observer: None,
            #[cfg(all(feature = "local-inference", feature = "test-support"))]
            execution_observer: None,
            configuration: context.configuration.clone(),
            #[cfg(feature = "local-inference")]
            assets: context.assets.clone(),
            #[cfg(feature = "local-inference")]
            lifecycle: Arc::new(Mutex::new(lifecycle::State::default())),
            gate: gate.clone(),
            installation: context.installation,
            namespace: context.namespace,
            ownership: ownership.clone(),
        };
        Ok(Self {
            configuration: context.configuration.clone(),
            #[cfg(feature = "local-inference")]
            assets: context.assets.clone(),
            focus,
            context,
            #[cfg(all(feature = "local-inference", feature = "test-support"))]
            ownership,
            #[cfg(all(feature = "local-inference", feature = "test-support"))]
            gate,
        })
    }
    pub(super) fn with_source_diagnostics(self, log: airk_host::service_logs::Log) -> Result<Self> {
        #[cfg(feature = "local-inference")]
        {
            self.focus.bind_source_diagnostics(log.clone())?;
            self.context.bind_source_diagnostics(log)?;
        }
        #[cfg(not(feature = "local-inference"))]
        let _ = log;
        Ok(self)
    }
    #[cfg(feature = "local-inference")]
    pub(in crate::service_runtime) fn begin_shutdown(&self, deadline: Instant) -> Shutdown {
        // Fence/capture/cancel BOTH stages before ingress and connection drain.
        Shutdown {
            focus: self.focus.begin_shutdown(),
            context: self.context.begin_shutdown(),
            deadline,
        }
    }
    #[cfg(feature = "local-inference")]
    pub(in crate::service_runtime) async fn shutdown(&self, shutdown: Shutdown) -> Result<()> {
        let Shutdown {
            focus,
            context,
            deadline,
        } = shutdown;
        let focus = async { self.focus.finish_shutdown(focus?, deadline).await };
        let context = async { self.context.finish_shutdown(context?, deadline).await };
        let (focus, context) = tokio::join!(focus, context);
        focus?;
        context?;
        Ok(())
    }
    fn stage(&self, stage: Stage) -> &StageOwner {
        match stage {
            Stage::Focus => &self.focus,
            Stage::Context => &self.context,
        }
    }
    fn configuration(&self) -> Result<Configuration> {
        self.context.configuration()
    }
    fn status(&self) -> Result<Status> {
        self.context.require_legacy_operation()?;
        let configuration = match self.configuration() {
            Ok(configuration) => configuration,
            Err(error) => {
                let (digest, digest_reason) = match self.configuration.legacy_configuration_digest() {
                    Ok(Some(digest)) => (Some(digest), "A retained legacy configuration digest was positively observed.".to_owned()),
                    Ok(None) => (None, "No legacy configuration file was observed; existing current-state corruption still refuses initialization.".to_owned()),
                    Err(error) => (None, format!("Legacy digest observation is unavailable; initialization remains subject to native refusal: {error:#}.")),
                };
                let reason = bounded_reason(format!(
                    "Optional stage configuration is unavailable. {digest_reason} Explicit disabled initialization requires native verification. Native configuration failure: {error:#}"
                ));
                let unavailable = |stage| -> Result<StageStatus> {
                    #[cfg(feature = "local-inference")]
                    {
                        self.stage(stage).unavailable_status(&reason)
                    }
                    #[cfg(not(feature = "local-inference"))]
                    {
                        Ok(StageStatus {
                            selected_profile: None,
                            active_profile: None,
                            availability: Availability::Failed,
                            reason: reason.clone(),
                            queued: 0,
                            running: 0,
                            profiles: stage_models(stage)
                                .into_iter()
                                .map(|model| ProfileStatus {
                                    cpu_fit: None,
                                    qualified_targets: vec![],
                                    model,
                                    profile: None,
                                    availability: Availability::Unqualified,
                                    reason:
                                        "No stage was enabled or initialized by this status read."
                                            .into(),
                                    tasks: Default::default(),
                                    download_bytes: None,
                                    memory_reservation_bytes: None,
                                })
                                .collect(),
                        })
                    }
                };
                let status = Status {
                    schema: STATUS_SCHEMA.into(),
                    initialization: Some(InitializationStatus {
                        actual_legacy_digest: digest,
                        reason: reason.clone(),
                    }),
                    configuration: Configuration::disabled(
                        self.context.installation,
                        self.context.namespace,
                    ),
                    focus: unavailable(Stage::Focus)?,
                    context: unavailable(Stage::Context)?,
                };
                status.validate()?;
                return Ok(status);
            }
        };
        let focus = self.focus.status()?;
        let context = self.context.status()?;
        ensure!(
            self.configuration()? == configuration,
            "Arbiter configuration changed while assembling stage status"
        );
        let status = Status {
            schema: STATUS_SCHEMA.into(),
            initialization: None,
            configuration,
            focus,
            context,
        };
        status.validate()?;
        Ok(status)
    }
    pub(super) fn application(
        &self,
        worker: WorkerIdentity,
        authority: ScopedReadRef,
        publication: airk_core::evidence::RevisionEnvelope,
    ) -> Result<IndexApplication> {
        self.context.application(worker, authority, publication)
    }
    pub(super) fn focus_application(
        &self,
        worker: WorkerIdentity,
        authority: ScopedReadRef,
        publication: airk_core::evidence::RevisionEnvelope,
    ) -> Result<Option<FocusApplication>> {
        let observed = self.focus.application(worker, authority, publication)?;
        if matches!(
            observed.availability,
            Availability::Disabled | Availability::IndexDisabled
        ) {
            return Ok(None);
        }
        let limits = if observed.availability == Availability::Ready {
            #[cfg(feature = "local-inference")]
            {
                let execution = self.focus.profile_description(
                    observed.profile.context("Ready Focus profile missing")?,
                )?;
                let profile = &execution;
                profile.validate_stage(Stage::Focus)?;
                Some(FocusProfileLimits {
                    max_candidates: profile.max_candidates,
                    pair_input_tokens: profile.max_tokens,
                    max_input_tokens: 8192,
                })
            }
            #[cfg(not(feature = "local-inference"))]
            {
                None
            }
        } else {
            None
        };
        let identity = if observed.availability == Availability::Ready {
            #[cfg(feature = "local-inference")]
            {
                let execution = self.focus.profile_description(
                    observed.profile.context("Ready Focus profile missing")?,
                )?;
                let profile = &execution;
                Some(FocusIdentity {
                    profile: profile.manifest,
                    runtime: profile.advice_runtime()?,
                    compiler: profile.compiler,
                    calibration: profile.calibration,
                    task_policy: *profile
                        .task_policies
                        .get(&Task::FocusRank)
                        .context("Qualified Focus task policy missing")?,
                })
            }
            #[cfg(not(feature = "local-inference"))]
            {
                None
            }
        } else {
            None
        };
        let application = FocusApplication {
            application: observed,
            limits,
            identity,
        };
        application.validate()?;
        Ok(Some(application))
    }
    pub(in crate::service_runtime) fn singleton_focus_limits(
        &self,
        index: airk_core::index_catalog::IndexId,
    ) -> Result<Option<FocusProfileLimits>> {
        let current = self.context.read_configuration()?;
        if !current.eligible_for(Stage::Focus, index, Task::FocusRank)
            || self.focus.read_stage_status()?.availability != Availability::Ready
        {
            return Ok(None);
        }
        #[cfg(feature = "local-inference")]
        {
            let execution = self.focus.profile_description(
                current
                    .selected_profile(Stage::Focus)
                    .context("Ready Focus profile missing")?,
            )?;
            let profile = &execution;
            profile.validate_stage(Stage::Focus)?;
            let limits = FocusProfileLimits {
                max_candidates: profile.max_candidates,
                pair_input_tokens: profile.max_tokens,
                max_input_tokens: 8192,
            };
            limits.validate()?;
            ensure!(
                self.context.read_configuration()?.same_selection(&current),
                "Focus allocation configuration changed"
            );
            Ok(Some(limits))
        }
        #[cfg(not(feature = "local-inference"))]
        {
            Ok(None)
        }
    }
    pub(in crate::service_runtime) async fn import(
        &self,
        request: airk_core::native_offline_import::NativeOfflineImportRequest,
        supervisor: &airk_supervisor::project_supervisor::Supervisor,
    ) -> Result<Status> {
        request.validate()?;
        let configuration = self.configuration()?;
        ensure!(
            configuration.revision == request.expected_revision,
            "Offline import configuration revision changed"
        );
        #[cfg(feature = "local-inference")]
        {
            let execution = self.context.execution_profile(request.profile)?;
            ensure!(
                execution.release_qualified(),
                "Offline import requires an actual compiled qualified release profile"
            );
            let stage = execution
                .description()
                .declared_stage()
                .context("Offline import stage missing")?;
            self.stage(stage).launch_import(
                configuration,
                request.profile,
                std::path::PathBuf::from(request.directory),
                supervisor.arbiter_management(),
            )?;
            self.status()
        }
        #[cfg(not(feature = "local-inference"))]
        {
            let _ = supervisor;
            anyhow::bail!("Native optional inference is unavailable in this image")
        }
    }
    pub(super) async fn manage(
        &self,
        request: ManagementRequest,
        supervisor: &airk_supervisor::project_supervisor::Supervisor,
        effect_check: Option<airk_https::admin_plane::NativeDispatchCheck>,
    ) -> Result<Status> {
        self.context.require_legacy_operation()?;
        let target = match &request {
            ManagementRequest::StatusDevice
            | ManagementRequest::InitializeDevice { .. }
            | ManagementRequest::MigrateDevice { .. }
            | ManagementRequest::ConfigureDevice { .. }
            | ManagementRequest::ProvisionDevice { .. }
            | ManagementRequest::CancelDevice { .. }
            | ManagementRequest::RemoveDevice { .. }
            | ManagementRequest::RecoverDevice { .. } => {
                anyhow::bail!("current device operation requires typed status3 native owner")
            }
            ManagementRequest::Status => return self.status(),
            ManagementRequest::Initialize {
                expected_legacy_configuration,
            } => {
                self.configuration
                    .initialize_checked(*expected_legacy_configuration, || {
                        ensure!(
                            self.context
                                .gate
                                .catalog_clock()
                                .is_some_and(|clock| clock.namespace == self.context.namespace),
                            airk_core::scoped_reads::ScopeReadRefused
                        );
                        super::native_admin::check_effect(effect_check.as_ref())
                    })?;
                return self.status();
            }
            ManagementRequest::Configure { stage, .. }
            | ManagementRequest::Cancel { stage, .. }
            | ManagementRequest::Recover { stage, .. } => *stage,
            ManagementRequest::Provision { profile, .. }
            | ManagementRequest::Remove { profile, .. } => {
                #[cfg(feature = "local-inference")]
                {
                    self.context
                        .execution_profile(*profile)
                        .ok()
                        .and_then(|profile| profile.description().declared_stage())
                        .or_else(|| {
                            self.assets
                                .owned_observation(*profile)
                                .ok()
                                .flatten()
                                .map(|observed| observed.stage)
                        })
                        .context(
                            "Profile has no admitted stage identity or original cleanup custody",
                        )?
                }
                #[cfg(not(feature = "local-inference"))]
                {
                    let _ = profile;
                    anyhow::bail!("Native optional inference is unavailable in this image");
                }
            }
        };
        self.stage(target)
            .manage(request, supervisor, effect_check)
            .await?;
        #[cfg(feature = "local-inference")]
        if let Ok(handle) = supervisor.arbiter_management().existing().await {
            handle.configure(self.configuration()?)?;
        }
        self.status()
    }
    pub(in crate::service_runtime) async fn next_read(
        &self,
        request: NativeNextReadRequest,
        supervisor: &airk_supervisor::project_supervisor::Supervisor,
    ) -> Result<Option<Advice>> {
        self.context.next_read(request, supervisor).await
    }
    pub(in crate::service_runtime) async fn context_action(
        &self,
        request: airk_core::native_context_action::NativeContextActionRequest,
        supervisor: &airk_supervisor::project_supervisor::Supervisor,
    ) -> Result<Option<airk_core::arbiter::context_action::ContextActionAdvice>> {
        self.context.context_action(request, supervisor).await
    }
    #[cfg(feature = "local-inference")]
    pub(in crate::service_runtime) async fn restore_verified(
        &self,
        supervisor: &airk_supervisor::project_supervisor::Supervisor,
    ) -> Result<()> {
        #[cfg(feature = "test-support")]
        if self.context.evaluation.is_some() {
            self.focus.restore_verified(supervisor).await?;
            return self.context.restore_verified(supervisor).await;
        }
        match self.current_device_configuration() {
            Ok(document) => {
                for stage in [Stage::Focus, Stage::Context] {
                    if document.stage(stage).enabled {
                        self.stage(stage).launch_device(
                            document.clone(),
                            supervisor,
                            None,
                            device_management::DeviceAction::Warm,
                        )?;
                    }
                }
                Ok(())
            }
            Err(error) => {
                // Ordinary service remains available; do not reinterpret corrupt,
                // foreign or unknown device state as legacy CPU enablement.
                for owner in [&self.focus, &self.context] {
                    let mut state = owner
                        .lifecycle
                        .lock()
                        .map_err(|_| anyhow::anyhow!("native lifecycle unavailable"))?;
                    state.phase = Availability::Failed;
                    state.reason =
                        bounded_reason(format!("Native device restore refused: {error:#}"));
                }
                Ok(())
            }
        }
    }
}

struct StageOwner {
    stage: Stage,
    #[cfg(feature = "local-inference")]
    devices: Arc<device_management::Shared>,
    #[cfg(feature = "local-inference")]
    joint_ledger: Arc<Mutex<joint_ledger::Ledger>>,
    /// Answer /3 model accounting, shared by the Focus and Context stages.
    #[cfg(feature = "local-inference")]
    loop_ledger: Arc<Mutex<loop_ledger::Ledger>>,
    /// Private evaluation only: the top-score card decision mode, set only
    /// through `EvaluationOwner::with_decision_mode`.
    #[cfg(all(feature = "local-inference", feature = "test-support"))]
    decision_mode: Option<airk_retrieval::arbiter::loop_cards::DecisionMode>,
    #[cfg(all(feature = "local-inference", feature = "test-support"))]
    evaluation: Option<airk_retrieval::arbiter::runtime::EvaluationCatalog>,
    #[cfg(all(feature = "local-inference", feature = "test-support"))]
    context_observer: Option<evaluation::ContextObservationSink>,
    #[cfg(all(feature = "local-inference", feature = "test-support"))]
    execution_observer:
        Option<airk_inference::inference_engine::pool::arbiter::execution_observer::Observer>,
    configuration: Arc<airk_store::service::arbiter::ConfigurationStore>,
    #[cfg(feature = "local-inference")]
    assets: Arc<airk_store::service::arbiter_assets::AssetStore>,
    #[cfg(feature = "local-inference")]
    lifecycle: Arc<Mutex<lifecycle::State>>,
    gate: airk_catalog_control::catalog_admission::NativeCatalogAdmission,
    installation: airk_core::identity::Sha256Digest,
    namespace: airk_core::identity::Incarnation,
    ownership: Arc<airk_store::service::ServiceLease>,
}
impl StageOwner {
    pub(super) fn new(
        ownership: Arc<airk_store::service::ServiceLease>,
        gate: airk_catalog_control::catalog_admission::NativeCatalogAdmission,
        stage: Stage,
    ) -> Result<Self> {
        let clock = gate
            .catalog_clock()
            .context("Arbiter catalog clock unavailable")?;
        let binding =
            airk_endpoint::http_endpoint::installation_binding(ownership.root(), clock.namespace)?;
        let configuration = airk_store::service::arbiter::ConfigurationStore::open(
            ownership.clone(),
            binding.installation,
            clock.namespace,
        )?;
        Ok(Self {
            stage,
            #[cfg(feature = "local-inference")]
            devices: Arc::new(device_management::Shared::default()),
            #[cfg(feature = "local-inference")]
            joint_ledger: Arc::new(Mutex::new(joint_ledger::Ledger::default())),
            #[cfg(feature = "local-inference")]
            loop_ledger: Arc::new(Mutex::new(loop_ledger::Ledger::default())),
            #[cfg(all(feature = "local-inference", feature = "test-support"))]
            decision_mode: None,
            #[cfg(all(feature = "local-inference", feature = "test-support"))]
            evaluation: None,
            #[cfg(all(feature = "local-inference", feature = "test-support"))]
            context_observer: None,
            #[cfg(all(feature = "local-inference", feature = "test-support"))]
            execution_observer: None,
            ownership: ownership.clone(),
            configuration: Arc::new(configuration),
            #[cfg(feature = "local-inference")]
            assets: airk_store::service::arbiter_assets::AssetStore::open(
                ownership,
                binding.installation,
                clock.namespace,
            )?,
            #[cfg(feature = "local-inference")]
            lifecycle: Arc::new(Mutex::new(lifecycle::State::default())),
            gate,
            installation: binding.installation,
            namespace: clock.namespace,
        })
    }
    #[cfg(feature = "local-inference")]
    fn score_request(
        &self,
        handle: &airk_inference::inference_engine::pool::arbiter::Handle,
        request: airk_inference::inference_engine::pool::arbiter::ScoreRequest,
    ) -> Result<airk_inference::inference_engine::pool::arbiter::PendingScore> {
        #[cfg(feature = "test-support")]
        if let Some(observer) = &self.execution_observer {
            ensure!(
                self.evaluation.is_some(),
                "Private score observer requires the actual native evaluation catalog"
            );
            return handle.score_observed(request, observer.clone());
        }
        handle.score(request)
    }
    fn configuration(&self) -> Result<Configuration> {
        ensure!(
            self.gate
                .catalog_clock()
                .is_some_and(|clock| clock.namespace == self.namespace),
            airk_core::scoped_reads::ScopeReadRefused
        );
        self.configuration.load()
    }
    fn validate_scope(&self, worker: WorkerIdentity, authority: ScopedReadRef) -> Result<()> {
        let clock = self
            .gate
            .catalog_clock()
            .ok_or(airk_core::scoped_reads::ScopeReadRefused)?;
        ensure!(
            clock.namespace == self.namespace,
            airk_core::scoped_reads::ScopeReadRefused
        );
        let ticket = self
            .gate
            .capture_current_issue(clock.namespace, clock.revision)?;
        self.gate
            .check_scope_issue(ticket, worker, authority)
            .map_err(|_| airk_core::scoped_reads::ScopeReadRefused.into())
    }
    fn validate_enrollments(&self, configuration: &Configuration) -> Result<()> {
        validate_configuration_enrollments(
            &self.gate,
            &self.ownership,
            self.namespace,
            configuration,
            self.stage,
        )
    }

    pub(super) fn application(
        &self,
        worker: WorkerIdentity,
        authority: ScopedReadRef,
        publication: airk_core::evidence::RevisionEnvelope,
    ) -> Result<IndexApplication> {
        self.validate_scope(worker, authority)?;
        let configuration = match self.read_configuration() {
            Ok(value) => value,
            Err(_) => return self.failed_application(worker, authority, publication),
        };
        let (availability, reason) = if !configuration.enabled(self.stage) {
            (
                Availability::Disabled,
                "Optional local Arbiter is disabled.".to_string(),
            )
        } else if !configuration
            .indexes(self.stage)
            .iter()
            .any(|index| index.index_id == authority.index_id)
        {
            (
                Availability::IndexDisabled,
                "This immutable index is not enrolled.".to_string(),
            )
        } else {
            #[cfg(feature = "local-inference")]
            {
                let catalog = match self.profile_descriptions() {
                    Ok(value) => value,
                    Err(_) => return self.failed_application(worker, authority, publication),
                };
                let enrollment = configuration
                    .indexes(self.stage)
                    .iter()
                    .find(|index| index.index_id == authority.index_id);
                let selected = catalog.iter().find(|profile| {
                    Some(profile.manifest) == configuration.selected_profile(self.stage)
                });
                if let (Some(enrollment), Some(selected)) = (enrollment, selected)
                    && !supports_stage_task(self.stage, &enrollment.tasks, &selected.tasks)
                {
                    (Availability::UnsupportedTask, "No current context task is supported by both this index enrollment and the selected native profile; no answer advice was applied.".to_string())
                } else {
                    let status = match self.read_stage_status() {
                        Ok(value) => value,
                        Err(_) => return self.failed_application(worker, authority, publication),
                    };
                    (status.availability, status.reason)
                }
            }
            #[cfg(not(feature = "local-inference"))]
            {
                (
                    Availability::UnsupportedSystem,
                    "Native optional inference is not present in this image.".to_string(),
                )
            }
        };
        #[cfg(all(feature = "local-inference", feature = "test-support"))]
        let reason = if self.evaluation.is_some() {
            format!("Private evaluation only; candidate is not release-qualified. {reason}")
        } else {
            reason
        };
        let application = IndexApplication {
            scope: ScopeBinding {
                installation: self.installation,
                catalog: self.namespace,
                index_id: authority.index_id,
                authorization: authority.fingerprint,
                configuration_revision: configuration.revision(),
                publication,
            },
            profile: configuration.profile_for_index(self.stage, authority.index_id),
            availability,
            reason: bounded_reason(reason),
        };
        self.validate_scope(worker, authority)?;
        if self
            .read_configuration()
            .is_ok_and(|current| current.same_selection(&configuration))
        {
            Ok(application)
        } else {
            self.failed_application(worker, authority, application.scope.publication)
        }
    }
    fn failed_application(
        &self,
        worker: WorkerIdentity,
        authority: ScopedReadRef,
        publication: airk_core::evidence::RevisionEnvelope,
    ) -> Result<IndexApplication> {
        self.validate_scope(worker, authority)?;
        let application=IndexApplication {scope:ScopeBinding {installation:self.installation,catalog:self.namespace,index_id:authority.index_id,authorization:authority.fingerprint,configuration_revision:0,publication},profile:None,availability:Availability::Failed,
            reason:"Optional Arbiter configuration is unavailable or changed; no advice was applied. The original source authority remains required.".into()};
        self.validate_scope(worker, authority)?;
        Ok(application)
    }
    #[cfg(not(feature = "local-inference"))]
    fn status(&self) -> Result<StageStatus> {
        let configuration = self.configuration()?;
        let availability = if configuration.stage(self.stage).enabled {
            Availability::Unqualified
        } else {
            Availability::Disabled
        };
        Ok(StageStatus { selected_profile:configuration.stage(self.stage).selected_profile,active_profile:None,
            availability,reason:if configuration.stage(self.stage).enabled {"No qualified native Arbiter session is active."} else {"Optional local Arbiter is disabled."}.into(),
            queued:0,running:0,profiles:stage_models(self.stage).into_iter().map(|model| ProfileStatus {cpu_fit:None,qualified_targets:vec![],model,profile:None,availability:Availability::Unqualified,reason:"This candidate requires retained native task, resource and quality qualification.".into(),tasks:Default::default(),download_bytes:None,memory_reservation_bytes:None}).collect()})
    }
    #[cfg(not(feature = "local-inference"))]
    pub(super) async fn manage(
        &self,
        request: ManagementRequest,
        supervisor: &airk_supervisor::project_supervisor::Supervisor,
        effect_check: Option<airk_https::admin_plane::NativeDispatchCheck>,
    ) -> Result<StageStatus> {
        match request {
            ManagementRequest::StatusDevice
            | ManagementRequest::InitializeDevice { .. }
            | ManagementRequest::MigrateDevice { .. }
            | ManagementRequest::ConfigureDevice { .. }
            | ManagementRequest::ProvisionDevice { .. }
            | ManagementRequest::CancelDevice { .. }
            | ManagementRequest::RemoveDevice { .. }
            | ManagementRequest::RecoverDevice { .. } => {
                anyhow::bail!("current device operation requires typed status3 native owner")
            }
            ManagementRequest::Status => {}
            ManagementRequest::Initialize { .. } => {
                anyhow::bail!("Initialization belongs to common native owner")
            }
            ManagementRequest::Configure {
                stage,
                expected_revision,
                enabled,
                selected_profile,
                indexes,
            } => {
                ensure!(stage == self.stage, "Arbiter configuration stage differs");
                let mut configuration = self.configuration()?;
                ensure!(
                    configuration.revision == expected_revision,
                    "Arbiter configuration revision changed"
                );
                configuration.revision = expected_revision
                    .checked_add(1)
                    .context("Arbiter revision exhausted")?;
                configuration.stage_mut(self.stage).enabled = enabled;
                configuration.stage_mut(self.stage).selected_profile = selected_profile;
                configuration.stage_mut(self.stage).indexes = indexes;
                configuration.validate()?;
                self.validate_enrollments(&configuration)?;
                #[cfg(feature = "local-inference")]
                if enabled {
                    let _ = airk_retrieval::arbiter::runtime::ExecutionProfile::from_release(
                        selected_profile.context("Arbiter profile missing")?,
                    )?;
                }
                #[cfg(not(feature = "local-inference"))]
                ensure!(
                    !enabled,
                    "Arbiter native inference is unavailable in this image"
                );
                let configuration =
                    self.configuration
                        .apply_checked(expected_revision, &configuration, || {
                            super::native_admin::check_effect(effect_check.as_ref())
                        })?;
                #[cfg(feature = "local-inference")]
                if let Ok(handle) = supervisor.arbiter().await {
                    handle.configure(configuration)?;
                }
                #[cfg(not(feature = "local-inference"))]
                let _ = (configuration, supervisor);
            }
            ManagementRequest::Provision {
                expected_revision, ..
            }
            | ManagementRequest::Cancel {
                expected_revision, ..
            }
            | ManagementRequest::Remove {
                expected_revision, ..
            }
            | ManagementRequest::Recover {
                expected_revision, ..
            } => {
                ensure!(
                    self.configuration()?.revision == expected_revision,
                    "Arbiter configuration revision changed"
                );
                anyhow::bail!(
                    "Arbiter profile has no release-qualified provisionable asset closure"
                );
            }
        }
        self.status()
    }
}

#[cfg(test)]
fn profile_for_index(
    configuration: &Configuration,
    index_id: airk_core::index_catalog::IndexId,
) -> Option<airk_core::identity::Sha256Digest> {
    stage_profile_for_index(configuration, Stage::Context, index_id)
}
#[cfg(test)]
fn stage_profile_for_index(
    configuration: &Configuration,
    stage: Stage,
    index_id: airk_core::index_catalog::IndexId,
) -> Option<airk_core::identity::Sha256Digest> {
    let selected = configuration.stage(stage);
    selected.selected_profile.filter(|_| {
        selected.enabled
            && selected
                .indexes
                .iter()
                .any(|index| index.index_id == index_id)
    })
}
fn bounded_reason(value: impl AsRef<str>) -> String {
    let value = value.as_ref();
    if value.is_empty() {
        return "Optional native operation returned no diagnostic.".into();
    }
    value[..value.floor_char_boundary(1024)].to_owned()
}

fn stage_models(stage: Stage) -> Vec<ModelChoice> {
    match stage {
        Stage::Focus => vec![ModelChoice::FocusQwenReranker06B],
        Stage::Context => vec![
            ModelChoice::ContextQwenCompact,
            ModelChoice::ContextQwenLarge,
        ],
    }
}
#[cfg(any(feature = "local-inference", test))]
fn supports_stage_task(
    stage: Stage,
    enrollment: &std::collections::BTreeSet<Task>,
    profile: &std::collections::BTreeSet<Task>,
) -> bool {
    let task = match stage {
        Stage::Focus => Task::FocusRank,
        Stage::Context => Task::ContextAction,
    };
    // The Answer loop's card tasks run under the Context enrollment
    // (change-log entry 31): a profile that declares them serves an index
    // enrolled for the Context task, as one that declares that task does.
    enrollment.contains(&task)
        && profile.iter().any(|declared| {
            *declared == task
                || (stage == Stage::Context
                    && airk_core::arbiter::answer_loop::is_card_task(*declared))
        })
}

pub(super) fn failure(error: anyhow::Error) -> ControlResponse {
    ControlResponse::Failure {
        code: if error
            .downcast_ref::<device_management::CurrentDeviceRequired>()
            .is_some()
        {
            "arbiter_device_contract_required"
        } else if error
            .downcast_ref::<airk_core::scoped_reads::ScopeReadRefused>()
            .is_some()
        {
            "scoped_read_unavailable"
        } else {
            "arbiter_unavailable"
        }
        .into(),
    }
}

fn validate_configuration_enrollments(
    gate: &airk_catalog_control::catalog_admission::NativeCatalogAdmission,
    ownership: &airk_store::service::ServiceLease,
    namespace: airk_core::identity::Incarnation,
    configuration: &Configuration,
    stage: Stage,
) -> Result<()> {
    configuration.validate()?;
    let clock = gate
        .catalog_clock()
        .context("Arbiter catalog clock unavailable")?;
    ensure!(
        clock.namespace == namespace,
        "Arbiter catalog identity changed"
    );
    let mut registry =
        airk_store::service::registry::ServiceRegistry::open_saved(ownership.root())?;
    for enrollment in &configuration.stage(stage).indexes {
        let detail = registry.catalog_detail(
            &airk_core::catalog_management::CatalogEntity::Index {
                id: enrollment.index_id,
            },
            Some(clock.revision),
        )?;
        ensure!(
            matches!(detail.item, Some(airk_core::catalog_management::CatalogItem::Index(ref index)) if !index.tombstone && index.source_import == airk_core::index_catalog::SourceImportState::Complete),
            "Arbiter enrollment requires a current immutable index"
        );
    }
    ensure!(
        gate.catalog_clock()
            .is_some_and(|current| current.namespace == clock.namespace
                && current.revision == clock.revision),
        "Arbiter enrollment catalog changed"
    );
    Ok(())
}
