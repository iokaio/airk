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
#[cfg(feature = "local-inference")]
mod download;
#[cfg(all(feature = "local-inference", feature = "test-support"))]
pub(super) mod evaluation;
#[cfg(test)]
mod failure_tests;
#[cfg(feature = "local-inference")]
mod lifecycle;
mod scoring;

pub(super) struct Owner {
    #[cfg(all(feature = "local-inference", feature = "test-support"))]
    evaluation: Option<airk_retrieval::arbiter::runtime::EvaluationCatalog>,
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
impl Owner {
    pub(super) fn new(
        ownership: Arc<airk_store::service::ServiceLease>,
        gate: airk_catalog_control::catalog_admission::NativeCatalogAdmission,
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
            #[cfg(all(feature = "local-inference", feature = "test-support"))]
            evaluation: None,
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
        )
    }

    pub(super) fn application(
        &self,
        worker: WorkerIdentity,
        authority: ScopedReadRef,
        publication: airk_core::evidence::RevisionEnvelope,
    ) -> Result<IndexApplication> {
        self.validate_scope(worker, authority)?;
        let configuration = match self.configuration() {
            Ok(value) => value,
            Err(_) => return self.failed_application(worker, authority, publication),
        };
        let (availability, reason) = if !configuration.enabled {
            (
                Availability::Disabled,
                "Optional local Arbiter is disabled.".to_string(),
            )
        } else if !configuration
            .indexes
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
                    .indexes
                    .iter()
                    .find(|index| index.index_id == authority.index_id);
                let selected = catalog
                    .iter()
                    .find(|profile| Some(profile.manifest) == configuration.selected_profile);
                if let (Some(enrollment), Some(selected)) = (enrollment, selected)
                    && !supports_next_read(&enrollment.tasks, &selected.tasks)
                {
                    (Availability::UnsupportedTask, "ContextNextRead is not supported by both this index enrollment and the selected native profile; no answer advice was applied.".to_string())
                } else {
                    let status = match self.status() {
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
                configuration_revision: configuration.revision,
                publication,
            },
            profile: profile_for_index(&configuration, authority.index_id),
            availability,
            reason,
        };
        self.validate_scope(worker, authority)?;
        if self
            .configuration()
            .is_ok_and(|current| current == configuration)
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
    fn status(&self) -> Result<Status> {
        let configuration = self.configuration()?;
        let availability = if configuration.enabled {
            Availability::Unqualified
        } else {
            Availability::Disabled
        };
        Ok(Status {schema:STATUS_SCHEMA.into(), selected_profile:configuration.selected_profile,active_profile:None,
            availability,reason:if configuration.enabled {"No qualified native Arbiter session is active."} else {"Optional local Arbiter is disabled."}.into(),
            configuration,queued:0,running:0,profiles:[ModelChoice::SmallIntern,ModelChoice::SmallStrands,ModelChoice::LargeIntern,ModelChoice::LargeKev].into_iter().map(|model| ProfileStatus {model,profile:None,availability:Availability::Unqualified,reason:"This candidate requires retained native task, resource and quality qualification.".into(),tasks:Default::default(),download_bytes:None,memory_reservation_bytes:None}).collect()})
    }
    #[cfg(not(feature = "local-inference"))]
    pub(super) async fn manage(
        &self,
        request: ManagementRequest,
        supervisor: &airk_supervisor::project_supervisor::Supervisor,
        effect_check: Option<airk_https::admin_plane::NativeDispatchCheck>,
    ) -> Result<Status> {
        match request {
            ManagementRequest::Status => {}
            ManagementRequest::Configure {
                expected_revision,
                enabled,
                selected_profile,
                indexes,
            } => {
                let mut configuration = self.configuration()?;
                ensure!(
                    configuration.revision == expected_revision,
                    "Arbiter configuration revision changed"
                );
                configuration.revision = expected_revision
                    .checked_add(1)
                    .context("Arbiter revision exhausted")?;
                configuration.enabled = enabled;
                configuration.selected_profile = selected_profile;
                configuration.indexes = indexes;
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
            | ManagementRequest::Cancel { expected_revision }
            | ManagementRequest::Remove {
                expected_revision, ..
            }
            | ManagementRequest::Recover { expected_revision } => {
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

fn profile_for_index(
    configuration: &Configuration,
    index_id: airk_core::index_catalog::IndexId,
) -> Option<airk_core::identity::Sha256Digest> {
    configuration.selected_profile.filter(|_| {
        configuration.enabled
            && configuration
                .indexes
                .iter()
                .any(|index| index.index_id == index_id)
    })
}

#[cfg(any(feature = "local-inference", test))]
fn supports_next_read(
    enrollment: &std::collections::BTreeSet<Task>,
    profile: &std::collections::BTreeSet<Task>,
) -> bool {
    enrollment.contains(&Task::ContextNextRead) && profile.contains(&Task::ContextNextRead)
}

pub(super) fn failure(error: anyhow::Error) -> ControlResponse {
    ControlResponse::Failure {
        code: if error
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
    for enrollment in &configuration.indexes {
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
