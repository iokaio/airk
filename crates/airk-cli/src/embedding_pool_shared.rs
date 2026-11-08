// SPDX-License-Identifier: Apache-2.0
//! Native shared-helper admission and predecessor recovery. Durable metadata is
//! combined with actual OS completion; metadata/EOF alone never mint a witness.
use super::*;
#[cfg(windows)]
use airk_store::service::registry::SharedHelperContainment;
#[cfg(all(windows, any(not(feature = "local-inference"), test)))]
use airk_store::service::registry::shared_helper_job_name;
use airk_store::service::registry::{
    ServiceRegistry, SharedHelperDrain, SharedHelperIdentity, SharedHelperPhase,
};

pub struct SharedHelperPredecessorProof {
    namespace: Incarnation,
    service: Incarnation,
    witnesses: Vec<ProviderReapWitness>,
}
impl SharedHelperPredecessorProof {
    pub fn matches(&self, namespace: Incarnation, service: Incarnation) -> bool {
        self.namespace == namespace && self.service == service
    }
    pub fn witnesses(&self) -> &[ProviderReapWitness] {
        &self.witnesses
    }
}

pub(crate) fn registry(root: &Path) -> Result<ServiceRegistry> {
    let saved = ServiceRegistry::open_saved(root)?;
    let state = saved
        .catalog_state()?
        .context("shared helper coverage is unavailable")?;
    ensure!(
        state.activated && state.authority_contract == 1,
        "shared helper coverage requires actual authority-1 activation"
    );
    ServiceRegistry::open(root, state.namespace)
}

pub(crate) fn known_transition(
    registry: &ServiceRegistry,
    result: Result<SharedHelperDrain>,
    expected: &SharedHelperDrain,
) -> Result<SharedHelperDrain> {
    match result {
        Ok(record) => {
            ensure!(
                record == *expected,
                "shared helper transition returned another identity"
            );
            Ok(record)
        }
        Err(error) => match registry.shared_helper_drain(expected.owner) {
            Ok(Some(record)) if record == *expected => Ok(record),
            _ => Err(error.context("shared helper transition outcome is not durably known")),
        },
    }
}

#[cfg(any(unix, not(feature = "local-inference"), test))]
pub(crate) fn complete(root: &Path, owner: SharedHelperIdentity) -> Result<SharedHelperDrain> {
    let mut registry = registry(root)?;
    let record = registry
        .shared_helper_drain(owner)?
        .context("shared helper completion record missing")?;
    let mut expected = record.clone();
    expected.phase = SharedHelperPhase::Drained;
    let result = registry.complete_shared_helper_drain(&record);
    known_transition(&registry, result, &expected)
}

/// Called only under the current native service owner/start barrier. Current
/// healthy helpers are never killed to satisfy one worker's retirement.
pub fn recover_shared_helper(
    root: &Path,
    owner: SharedHelperIdentity,
) -> Result<Option<ProviderReapWitness>> {
    let mut registry = registry(root)?;
    let current = registry
        .recorded_service_instance()?
        .context("current native service is not recorded")?;
    let record = registry
        .shared_helper_drain(owner)?
        .context("shared helper has no canonical admission record")?;
    if owner.service == current {
        return Ok(None);
    }
    recover_record(root, &mut registry, &record)
}

/// Installer maintenance holds native service exclusion rather than inventing
/// a replacement service instance. All recorded helpers are now excluded owners,
/// including the last recorded instance. Never prune history here: worker drain
/// obligations may still need these terminal observations on the next startup.
pub(crate) fn quiesce_for_maintenance(lease: &airk_store::service::ServiceLease) -> Result<bool> {
    let root = lease.root();
    let saved = ServiceRegistry::open_saved(root)?;
    let Some(state) = saved.catalog_state()? else {
        return Ok(true); // Legacy helper lifetime remains covered by its lease.
    };
    if state.authority_contract == 0 {
        return Ok(true);
    }
    ensure!(
        state.activated && state.authority_contract == 1,
        "installer shared helper coverage requires a supported active catalog"
    );
    let records = saved.shared_helper_drains()?;
    drop(saved);
    let mut registry = registry(root)?;
    for record in records {
        if recover_record(root, &mut registry, &record)?.is_none() {
            return Ok(false);
        }
    }
    Ok(true)
}

fn recover_record(
    _root: &Path,
    registry: &mut ServiceRegistry,
    record: &SharedHelperDrain,
) -> Result<Option<ProviderReapWitness>> {
    let owner = record.owner;
    // The strict canonical row records a prior native completion. Never open
    // a newly recreated job name or probe a reused numerical group after that
    // durable terminal transition; those resources are no longer ours.
    if record.phase == SharedHelperPhase::Drained {
        return Ok(Some(ProviderReapWitness {
            service: owner.service,
            helper: owner.helper,
        }));
    }
    #[cfg(unix)]
    let done = crate::unix_guardian::shared::recover(record, _root)?;
    #[cfg(windows)]
    let done = recover_windows(record)?;
    if !done {
        return Ok(None);
    }
    let mut expected = record.clone();
    expected.phase = SharedHelperPhase::Drained;
    let result = registry.complete_shared_helper_drain(record);
    known_transition(registry, result, &expected)?;
    // The predecessor service is excluded by the current native owner; its
    // reader threads ended with that process. Native container/guardian proof
    // above is additional, never inferred from service or row absence alone.
    Ok(Some(ProviderReapWitness {
        service: owner.service,
        helper: owner.helper,
    }))
}

pub fn recover_predecessor_shared_helpers(
    root: &Path,
    current: Incarnation,
) -> Result<Option<SharedHelperPredecessorProof>> {
    let registry = registry(root)?;
    ensure!(
        registry.recorded_service_instance()? == Some(current),
        "predecessor helper recovery has another current native owner"
    );
    let namespace = registry.incarnation()?;
    let records = registry.shared_helper_drains()?;
    drop(registry);
    let mut witnesses = Vec::new();
    for record in records
        .into_iter()
        .filter(|record| record.owner.service != current)
    {
        let Some(witness) = recover_shared_helper(root, record.owner)? else {
            return Ok(None);
        };
        witnesses.push(witness);
    }
    Ok(Some(SharedHelperPredecessorProof {
        namespace,
        service: current,
        witnesses,
    }))
}

/// Called by the existing native owner only after strict attach and retained
/// resource/WDO reconciliation. The opaque proof is never a frontend boolean.
pub fn prune_predecessor_shared_helpers(
    root: &Path,
    current: Incarnation,
    lease: &airk_store::service::ServiceLease,
    proof: &SharedHelperPredecessorProof,
) -> Result<usize> {
    let mut registry = registry(root)?;
    ensure!(
        proof.matches(registry.incarnation()?, current),
        "shared helper predecessor cleanup proof differs"
    );
    let records = registry
        .shared_helper_drains()?
        .into_iter()
        .filter(|record| record.owner.service != current)
        .collect::<Vec<_>>();
    ensure!(
        records.len() <= proof.witnesses.len()
            && records
                .iter()
                .all(|record| record.phase == SharedHelperPhase::Drained
                    && proof
                        .witnesses
                        .iter()
                        .any(|witness| witness.matches(record.owner.service, record.owner.helper))),
        "shared helper predecessor cleanup has uncaptured ownership"
    );
    registry.prune_drained_shared_helpers(lease, current, &records)
}

#[cfg(test)]
#[path = "embedding_pool_shared_tests.rs"]
mod tests;

#[cfg(windows)]
fn recover_windows(record: &SharedHelperDrain) -> Result<bool> {
    let SharedHelperContainment::Windows {
        job_name,
        user_sid,
        created,
        leader_pid,
        ..
    } = &record.containment
    else {
        anyhow::bail!("shared helper native platform changed");
    };
    ensure!(
        crate::local_ipc::current_user()? == *user_sid,
        "shared helper belongs to another native user"
    );
    if !created {
        // The canonical producer never spawns before Created is durably known.
        // A collided/foreign object with this name MUST NOT be opened or killed.
        ensure!(
            leader_pid.is_none() && record.phase != SharedHelperPhase::Bound,
            "uncreated shared helper has a process binding"
        );
        return Ok(true);
    }
    match airk_process::ChildJob::open_named(job_name, |handle| {
        crate::local_ipc::require_job_user(handle, user_sid)
    })? {
        airk_process::NamedJobOpen::Missing => Ok(true),
        airk_process::NamedJobOpen::Owned(job) => {
            job.terminate_and_wait(Duration::from_secs(2))?;
            Ok(true)
        }
    }
}

#[cfg(all(windows, any(not(feature = "local-inference"), test)))]
pub(super) struct ManagedWindows {
    pub child: Option<airk_process::AtomicJobChild>,
    job: airk_process::ChildJob,
    root: PathBuf,
    owner: SharedHelperIdentity,
    finished: bool,
}
#[cfg(all(windows, any(not(feature = "local-inference"), test)))]
impl ManagedWindows {
    pub(super) fn member_pids(&self) -> Result<Vec<u32>> {
        self.job.member_pids()
    }
    pub(super) fn spawn(
        root: &Path,
        service: Incarnation,
        helper: Incarnation,
        program: &Path,
    ) -> Result<Self> {
        let root = root.canonicalize()?;
        let null = std::fs::OpenOptions::new().write(true).open("NUL")?;
        let owner = SharedHelperIdentity { service, helper };
        let mut registry = registry(&root)?;
        let namespace = registry.incarnation()?;
        let user = crate::local_ipc::current_user()?;
        let job_name = shared_helper_job_name(namespace, owner, &user)?;
        let record = registry.begin_shared_helper_drain(
            owner,
            SharedHelperContainment::Windows {
                job_name: job_name.clone(),
                user_sid: user.clone(),
                session: session()?,
                created: false,
                leader_pid: None,
            },
            boot(&user)?,
        )?;
        let job =
            match crate::local_ipc::with_current_user_job_security(|attributes, native_user| {
                ensure!(native_user == user, "shared helper native user changed");
                airk_process::ChildJob::unassigned_named(&job_name, attributes)
            }) {
                Ok(job) => job,
                Err(error) => {
                    let _ = complete(&root, owner);
                    return Err(error);
                }
            };
        let mut managed = Self {
            child: None,
            job,
            root,
            owner,
            finished: false,
        };
        let mut expected = record.clone();
        let SharedHelperContainment::Windows { created, .. } = &mut expected.containment else {
            unreachable!()
        };
        *created = true;
        let result = registry.mark_shared_job_created(&record);
        let created = known_transition(&registry, result, &expected)?;
        managed.child = Some(managed.job.spawn_guarded_with_stderr(
            program,
            "pool-guarded",
            &null,
        )?);
        ensure!(
            managed
                .child
                .as_ref()
                .expect("created guarded helper")
                .assigned_to(&managed.job)?,
            "shared helper atomic job assignment is missing"
        );
        let mut expected = created.clone();
        let SharedHelperContainment::Windows { leader_pid, .. } = &mut expected.containment else {
            unreachable!()
        };
        let child = managed.child.as_mut().expect("created guarded helper");
        *leader_pid = Some(child.id());
        expected.phase = SharedHelperPhase::Bound;
        let result = registry.bind_shared_helper(&created, child.id());
        known_transition(&registry, result, &expected)?;
        let permit = crate::embedding_pool_bootstrap::GuardPermit {
            version: 2,
            runtime: managed.root.clone(),
            namespace,
            owner,
        };
        service_ipc::write_frame(
            child
                .stdin
                .as_mut()
                .context("guarded helper input missing")?,
            &permit,
        )?;
        Ok(managed)
    }
    pub(super) fn drain(&mut self, timeout: Duration) -> Result<()> {
        if self.finished {
            return Ok(());
        }
        self.job.terminate_and_wait(timeout)?;
        if let Some(child) = self.child.as_mut() {
            child.wait()?;
        }
        complete(&self.root, self.owner)?;
        self.finished = true;
        Ok(())
    }
}
#[cfg(all(windows, any(not(feature = "local-inference"), test)))]
impl Drop for ManagedWindows {
    fn drop(&mut self) {
        while self.drain(Duration::from_secs(2)).is_err() {
            std::thread::sleep(Duration::from_millis(25));
        }
    }
}

#[cfg(all(windows, any(not(feature = "local-inference"), test)))]
fn session() -> Result<u32> {
    let mut session = 0;
    // SAFETY: current process identity and correctly sized native output.
    ensure!(
        unsafe {
            windows_sys::Win32::System::RemoteDesktop::ProcessIdToSessionId(
                std::process::id(),
                &mut session,
            )
        } != 0,
        std::io::Error::last_os_error()
    );
    Ok(session)
}
#[cfg(all(windows, any(not(feature = "local-inference"), test)))]
fn boot(user: &str) -> Result<airk_store::service::registry::ProviderBoot> {
    use sha2::{Digest, Sha256};
    // SAFETY: scalar monotonic native uptime query with no pointer arguments.
    let elapsed = unsafe { windows_sys::Win32::System::SystemInformation::GetTickCount64() };
    Ok(airk_store::service::registry::ProviderBoot {
        boot_id: None,
        process_scope: format!("windows-global-user-{:x}", Sha256::digest(user.as_bytes())),
        elapsed_ns: elapsed
            .checked_mul(1_000_000)
            .context("native Windows uptime overflow")?,
        monotonic_reset_proves_boot: true,
    })
}
