// SPDX-License-Identifier: Apache-2.0
//! Typed private evaluation seam. No wire value attaches this catalog.
use super::*;
use airk_core::identity::Sha256Digest;
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom, Write},
};

/// Native test-support observation only. The sink is attached before exact
/// same-lease/same-gate fixture composition; no wire value attaches it.
pub type ContextObservationSink = Arc<
    dyn Fn(&airk_core::native_context_action::NativeContextActionRequest, &Profile) -> Result<()>
        + Send
        + Sync,
>;

#[derive(serde::Serialize)]
pub struct EvaluationResult<T> {
    evaluation: bool,
    release_qualified: bool,
    result: T,
}
impl<T> EvaluationResult<T> {
    fn new(result: T) -> Self {
        Self {
            evaluation: true,
            release_qualified: false,
            result,
        }
    }
    pub fn result(&self) -> &T {
        &self.result
    }
    pub fn into_result(self) -> T {
        self.result
    }
}
/// Only a native fixture with the real private service lease constructs this.
/// The caller must use an isolated control root, never an ordinary installation.
pub struct EvaluationOwner {
    owner: Owner,
}
impl EvaluationOwner {
    pub(in crate::service_runtime) fn into_owner_for(
        self,
        lease: &Arc<airk_store::service::ServiceLease>,
        gate: &airk_catalog_control::catalog_admission::NativeCatalogAdmission,
    ) -> Result<Owner> {
        ensure!(
            Arc::ptr_eq(&self.owner.ownership, lease) && self.owner.gate.same_instance(gate),
            "Private evaluation owner differs from the running service lease or admission gate"
        );
        Ok(self.owner)
    }
    pub fn new(
        ownership: Arc<airk_store::service::ServiceLease>,
        gate: airk_catalog_control::catalog_admission::NativeCatalogAdmission,
        profiles: Vec<Profile>,
    ) -> Result<Self> {
        Self::new_bound(ownership, gate, profiles, None)
    }
    #[cfg(test)]
    pub(in crate::service_runtime) fn reopen_same_configuration(
        ownership: Arc<airk_store::service::ServiceLease>,
        gate: airk_catalog_control::catalog_admission::NativeCatalogAdmission,
        profiles: Vec<Profile>,
        expected: &Configuration,
    ) -> Result<Self> {
        Self::verify_restart_settled(&ownership)?;
        Self::new_bound(ownership, gate, profiles, Some(expected))
    }
    #[cfg(test)]
    pub(in crate::service_runtime) fn verify_restart_settled(
        lease: &airk_store::service::ServiceLease,
    ) -> Result<()> {
        let mut registry =
            airk_store::service::registry::ServiceRegistry::open_saved(lease.root())?;
        ensure!(
            registry.pending_projects(1)?.is_empty(),
            "Restart has native pending/running work"
        );
        let mut after = None;
        let mut revision = None;
        loop {
            let page = registry.list(after, revision, 32)?;
            revision = Some(page.revision);
            for project in page.projects {
                let binding = project.registration.binding;
                ensure!(
                    registry.pending_job_observation(binding)?.is_none()
                        && !registry.has_unsettled_configuration(binding)?,
                    "Restart has paused queued work or unfinished configuration"
                );
            }
            after = page.after;
            if after.is_none() {
                break;
            }
        }
        drop(registry);
        Ok(())
    }
    fn new_bound(
        ownership: Arc<airk_store::service::ServiceLease>,
        gate: airk_catalog_control::catalog_admission::NativeCatalogAdmission,
        profiles: Vec<Profile>,
        expected: Option<&Configuration>,
    ) -> Result<Self> {
        let catalog =
            airk_kernel::inference_owner::evaluation_catalog(ownership.clone(), profiles)?;
        let mut owner = Owner::new(ownership, gate)?;
        let configuration = owner.configuration()?;
        ensure!(
            expected.is_some_and(|saved| saved == &configuration)
                || (expected.is_none()
                    && ![&configuration.focus, &configuration.context]
                        .into_iter()
                        .any(|stage| stage.enabled
                            || !stage.indexes.is_empty()
                            || stage.selected_profile.is_some())),
            "Private evaluation requires an unconfigured owner or exact saved restart configuration"
        );
        owner.focus.evaluation = Some(catalog.clone());
        owner.context.evaluation = Some(catalog);
        Ok(Self { owner })
    }
    /// Observe an already admitted request without dispatching Context scoring.
    /// Source/configuration/deadline postchecks still decide the canonical reply.
    /// Separate compiler census/recording work is not a deployed-model timing.
    pub fn observe_context(mut self, sink: ContextObservationSink) -> Self {
        self.owner.context.context_observer = Some(sink);
        self
    }
    /// Observation only; actual service composition still checks the SAME native lease/gate.
    pub fn observe_execution(
        mut self,
        observer: airk_inference::inference_engine::pool::arbiter::execution_observer::Observer,
    ) -> Self {
        self.owner.focus.execution_observer = Some(observer.clone());
        self.owner.context.execution_observer = Some(observer);
        self
    }
    #[cfg(test)]
    pub(in crate::service_runtime) fn observe_warm(
        self,
        observer: super::warm_observer::Observer,
    ) -> Result<Self> {
        super::lifecycle::observe_warm(&self.owner.focus.lifecycle, observer.clone())?;
        super::lifecycle::observe_warm(&self.owner.context.lifecycle, observer)?;
        Ok(self)
    }
    pub fn configuration(&self) -> Result<EvaluationResult<Configuration>> {
        Ok(EvaluationResult::new(self.owner.configuration()?))
    }
    /// The fixture supplies retained read descriptors for its approved model
    /// assets. No path/URL supplied by a client is opened, and source files are
    /// never written. The ordinary Store transaction verifies exact actual bytes.
    pub fn seed(
        &self,
        manifest: Sha256Digest,
        source: &mut dyn FnMut(&Asset) -> Result<File>,
    ) -> Result<EvaluationResult<()>> {
        let execution = self.owner.context.execution_profile(manifest)?;
        ensure!(
            execution.evaluation() && !execution.release_qualified(),
            "Private seed requires an opaque evaluation capability"
        );
        let profile = execution.description();
        let configuration = self.owner.configuration()?;
        let transaction = self.owner.assets.begin(profile, configuration.revision)?;
        let mut buffer = [0_u8; 65536];
        for asset in &profile.assets {
            if let Some(mut writer) = transaction.asset(asset)? {
                let mut input = source(asset)?;
                ensure!(
                    input.metadata()?.is_file() && input.metadata()?.len() == asset.bytes,
                    "Private evaluation asset descriptor differs"
                );
                input.seek(SeekFrom::Start(0))?;
                let mut remaining = asset.bytes;
                while remaining > 0 {
                    let size = usize::try_from(remaining.min(buffer.len() as u64))?;
                    let bytes = input.read(&mut buffer[..size])?;
                    ensure!(bytes > 0, "Private evaluation source shortened");
                    writer.write_all(&buffer[..bytes])?;
                    remaining -= u64::try_from(bytes)?;
                }
                let mut extra = [0];
                ensure!(
                    input.read(&mut extra)? == 0 && input.metadata()?.len() == asset.bytes,
                    "Private evaluation source changed"
                );
                transaction.publish(writer)?;
            }
        }
        transaction.finish()?;
        Ok(EvaluationResult::new(()))
    }
    pub async fn manage(
        &self,
        request: ManagementRequest,
        supervisor: &airk_supervisor::project_supervisor::Supervisor,
    ) -> Result<EvaluationResult<Status>> {
        let mut status = self.owner.manage(request, supervisor, None).await?;
        for stage in [&mut status.focus, &mut status.context] {
            stage.reason = bounded_reason(format!(
                "Private evaluation only; candidate is not release-qualified. {}",
                stage.reason
            ));
        }
        Ok(EvaluationResult::new(status))
    }
    pub fn application(
        &self,
        worker: WorkerIdentity,
        authority: ScopedReadRef,
        publication: airk_core::evidence::RevisionEnvelope,
    ) -> Result<EvaluationResult<IndexApplication>> {
        Ok(EvaluationResult::new(self.owner.application(
            worker,
            authority,
            publication,
        )?))
    }
    pub async fn next_read(
        &self,
        request: NativeNextReadRequest,
        supervisor: &airk_supervisor::project_supervisor::Supervisor,
    ) -> Result<EvaluationResult<Option<Advice>>> {
        Ok(EvaluationResult::new(
            self.owner.next_read(request, supervisor).await?,
        ))
    }
}

#[cfg(test)]
mod execution_observer_tests {
    use super::*;
    #[test]
    fn private_execution_observer_attachment_still_requires_actual_startup_lease_and_gate()
    -> Result<()> {
        use airk_catalog_control::catalog_admission::NativeCatalogAdmission;
        use airk_core::{identity::Incarnation, index_catalog::CatalogRevision};
        let root = tempfile::tempdir()?;
        let other = tempfile::tempdir()?;
        let lease = Arc::new(airk_store::service::ServiceLease::acquire(root.path())?);
        let foreign = Arc::new(airk_store::service::ServiceLease::acquire(other.path())?);
        let gate = NativeCatalogAdmission::new(
            Incarnation::from_bytes([81; 16]),
            Incarnation::from_bytes([82; 16]),
            CatalogRevision::INITIAL,
            1,
        )?;
        for variant in [0, 1, 2] {
            let(observer,_)=airk_inference::inference_engine::pool::arbiter::execution_observer::Observer::channel(16)?;
            // Attachment itself grants no execution catalog; actual score routing separately requires it.
            let owner = EvaluationOwner {
                owner: Owner::new(lease.clone(), gate.clone())?,
            }
            .observe_execution(observer);
            assert!(
                owner.owner.focus.execution_observer.is_some()
                    && owner.owner.context.execution_observer.is_some()
            );
            let supplied = if variant == 0 { &foreign } else { &lease };
            let other_gate = NativeCatalogAdmission::new(
                Incarnation::from_bytes([81; 16]),
                Incarnation::from_bytes([82; 16]),
                CatalogRevision::INITIAL,
                1,
            )?;
            assert_eq!(
                owner
                    .into_owner_for(supplied, if variant == 1 { &other_gate } else { &gate })
                    .is_ok(),
                variant == 2
            );
        }
        Ok(())
    }
}
