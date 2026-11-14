// SPDX-License-Identifier: Apache-2.0
//! Typed private evaluation seam. No wire value attaches this catalog.
use super::*;
use airk_core::identity::Sha256Digest;
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom, Write},
};

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
        let catalog =
            airk_kernel::inference_owner::evaluation_catalog(ownership.clone(), profiles)?;
        let mut owner = Owner::new(ownership, gate)?;
        let configuration = owner.configuration()?;
        ensure!(
            !configuration.enabled
                && configuration.indexes.is_empty()
                && configuration.selected_profile.is_none(),
            "Private evaluation requires an isolated unconfigured owner"
        );
        owner.evaluation = Some(catalog);
        Ok(Self { owner })
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
        let execution = self.owner.execution_profile(manifest)?;
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
        status.reason = format!(
            "Private evaluation only; candidate is not release-qualified. {}",
            status.reason
        );
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
