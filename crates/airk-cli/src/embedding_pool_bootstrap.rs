// SPDX-License-Identifier: Apache-2.0
//! Fixed, guarded shared-helper startup. No runtime/model work is permitted
//! until the exact durable native container binding has been validated.
use airk_core::identity::Incarnation;
use airk_store::service::registry::SharedHelperContainment;
use airk_store::service::registry::{ServiceRegistry, SharedHelperIdentity, SharedHelperPhase};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct GuardPermit {
    pub version: u32,
    pub runtime: PathBuf,
    pub namespace: Incarnation,
    pub owner: SharedHelperIdentity,
}

pub struct GuardedBootstrap {
    runtime: PathBuf,
    namespace: Incarnation,
    owner: SharedHelperIdentity,
    #[cfg(unix)]
    guardian: Option<(std::os::unix::net::UnixStream, u32)>,
}
impl GuardedBootstrap {
    pub(crate) fn from_permit(permit: GuardPermit) -> Result<Self> {
        ensure!(
            permit.version == 2
                && permit.runtime.is_absolute()
                && permit.runtime.canonicalize()? == permit.runtime,
            "invalid shared helper startup permit"
        );
        Ok(Self {
            runtime: permit.runtime,
            namespace: permit.namespace,
            owner: permit.owner,
            #[cfg(unix)]
            guardian: None,
        })
    }
    #[cfg(unix)]
    pub(crate) fn from_guardian(
        permit: GuardPermit,
        guardian: std::os::unix::net::UnixStream,
        group: u32,
    ) -> Result<Self> {
        let record = validate_permit(&permit)?;
        ensure!(
            matches!(record.containment, SharedHelperContainment::Unix { group: Some(bound), .. } if bound == group)
            && group == std::process::id()
            // SAFETY: scalar native group query before arming ownership.
            && unsafe { libc::getpgrp() } == group as i32,
            "guarded helper cannot arm another native group"
        );
        let mut proof = Self::from_permit(permit)?;
        proof.guardian = Some((guardian, group));
        proof.enforce_native_owner()?;
        Ok(proof)
    }

    /// The existing helper timer observes the original private guardian peer.
    /// Only positive peer loss activates containment of this verified group;
    /// an uncertain observation never becomes a completion witness.
    pub fn enforce_native_owner(&self) -> Result<()> {
        #[cfg(unix)]
        {
            use std::os::fd::AsRawFd;
            let (guardian, group) = self
                .guardian
                .as_ref()
                .context("guarded helper native peer missing")?;
            let mut byte = 0u8;
            // SAFETY: exact retained private socket and one writable byte; no
            // consumption or blocking and no process lookup by name/PID scan.
            let count = unsafe {
                libc::recv(
                    guardian.as_raw_fd(),
                    (&mut byte as *mut u8).cast(),
                    1,
                    libc::MSG_PEEK | libc::MSG_DONTWAIT,
                )
            };
            let error = std::io::Error::last_os_error();
            if count == 0 || count < 0 && error.raw_os_error() == Some(libc::ECONNRESET) {
                let pid = std::process::id();
                // SAFETY: scalar native group query. Startup bound this exact
                // helper PID/group before permitting any provider work.
                ensure!(
                    *group == pid && unsafe { libc::getpgrp() } == pid as i32,
                    "guarded helper native group changed"
                );
                // The guardian is positively gone. Kill only our actual own
                // verified group, including its descendants, as a failed
                // provider containment path. Healthy helpers never enter here.
                unsafe {
                    libc::kill(-(pid as i32), libc::SIGKILL);
                    // A denied native signal remains unresolved at the parent;
                    // exit this failed helper without generating a core dump.
                    libc::_exit(125);
                }
            }
            if count < 0 {
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                ) {
                    return Ok(());
                }
                return Err(error.into());
            }
            anyhow::bail!("unexpected guarded helper native peer traffic");
        }
        #[cfg(windows)]
        Ok(())
    }
    pub fn validate_init(&self, init: &crate::embedding_pool_protocol::Init) -> Result<()> {
        self.enforce_native_owner()?;
        ensure!(
            init.runtime.canonicalize()? == self.runtime
                && init.service == self.owner.service
                && init.pool == self.owner.helper,
            "guarded helper initialization changed native ownership"
        );
        let registry = ServiceRegistry::open_saved(&self.runtime)?;
        ensure!(
            registry.incarnation()? == self.namespace
                && registry.recorded_service_instance()? == Some(self.owner.service),
            "guarded helper service ownership changed"
        );
        Ok(())
    }
    fn ready(&self) -> crate::embedding_pool_protocol::Observation {
        crate::embedding_pool_protocol::Observation::GuardedReady {
            namespace: self.namespace,
            service: self.owner.service,
            pool: self.owner.helper,
        }
    }
}

#[cfg(unix)]
impl Drop for GuardedBootstrap {
    fn drop(&mut self) {
        let Some((_, group)) = self.guardian.as_ref() else {
            return;
        };
        let pid = std::process::id();
        // This field is armed ONLY by from_guardian after canonical Bound and
        // actual self-group validation. Recheck before any destructive action;
        // an identity mismatch authorizes no numerical group signal.
        if *group != pid || unsafe { libc::getpgrp() } != pid as i32 {
            return;
        }
        // Normal run_guarded completion reaches here after joining its engine;
        // errors/cancellation are failed-provider containment paths. Keep pin
        // and private peer live until this whole exact group is contained.
        unsafe {
            libc::kill(-(pid as i32), libc::SIGKILL);
            libc::_exit(125);
        }
    }
}

pub(crate) fn validate_permit(
    permit: &GuardPermit,
) -> Result<airk_store::service::registry::SharedHelperDrain> {
    ensure!(
        permit.version == 2
            && permit.runtime.is_absolute()
            && permit.runtime.canonicalize()? == permit.runtime,
        "invalid shared helper startup permit"
    );
    crate::local_ipc::require_existing_root(&permit.runtime)?;
    let registry = ServiceRegistry::open_saved(&permit.runtime)?;
    ensure!(
        registry.incarnation()? == permit.namespace
            && registry.recorded_service_instance()? == Some(permit.owner.service),
        "shared helper startup crossed native service ownership"
    );
    let record = registry
        .shared_helper_drain(permit.owner)?
        .context("shared helper reservation is missing")?;
    ensure!(
        record.phase == SharedHelperPhase::Bound,
        "shared helper has no durable native binding"
    );
    Ok(record)
}

/// The opaque result cannot be deserialized or synthesized by an RPC caller.
/// A fixed helper entrypoint calls this before creating its async runtime.
pub fn accept_guarded_bootstrap() -> Result<GuardedBootstrap> {
    #[cfg(unix)]
    let proof = crate::unix_guardian::shared::accept_bootstrap()?;
    #[cfg(windows)]
    let proof = accept_windows()?;
    crate::service_ipc::write_frame(&mut std::io::stdout().lock(), &proof.ready())?;
    Ok(proof)
}

/// Missing standalone legacy metadata is supported only when BOTH native
/// owner and registry are absent. Existing/corrupt authority never falls back.
pub fn shared_mode(root: &Path) -> Result<bool> {
    if !root.join("service-1.4.sqlite3").try_exists()? {
        ensure!(
            !root.join("service-1.4.owner.json").try_exists()?,
            "native registry is missing under an existing owner"
        );
        return Ok(false);
    }
    match ServiceRegistry::open_saved(root)?.catalog_state()? {
        None => Ok(false),
        Some(state) if state.authority_contract == 0 && state.policy_contract == 0 => Ok(false),
        Some(state)
            if state.authority_contract == 1 && state.policy_contract <= 1 && state.activated =>
        {
            Ok(true)
        }
        Some(_) => anyhow::bail!("shared helper catalog contract is unqualified or unsupported"),
    }
}

#[cfg(windows)]
fn accept_windows() -> Result<GuardedBootstrap> {
    use std::os::windows::io::{AsHandle, AsRawHandle, FromRawHandle, OwnedHandle};
    use windows_sys::Win32::System::{
        JobObjects::{IsProcessInJob, OpenJobObjectW},
        Threading::{GetCurrentProcess, GetCurrentProcessId},
    };
    let permit: GuardPermit = crate::service_ipc::read_frame(&mut std::io::stdin().lock())?
        .context("guarded helper permit missing")?;
    let record = validate_permit(&permit)?;
    let SharedHelperContainment::Windows {
        job_name,
        user_sid,
        created,
        leader_pid,
        ..
    } = record.containment
    else {
        anyhow::bail!("shared helper native platform changed");
    };
    // SAFETY: scalar native identity query with no pointer arguments.
    let current_pid = unsafe { GetCurrentProcessId() };
    ensure!(
        created && leader_pid == Some(current_pid),
        "shared helper native assignment is unbound"
    );
    let wide = job_name.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
    // QUERY | READ_CONTROL only. This observation MUST NOT become a cleanup
    // owner or keep a job handle alive after guarded startup completes.
    let raw = unsafe { OpenJobObjectW(0x0004 | 0x0002_0000, 0, wide.as_ptr()) };
    ensure!(!raw.is_null(), std::io::Error::last_os_error());
    // SAFETY: successful open transfers this real observational handle.
    let job = unsafe { OwnedHandle::from_raw_handle(raw) };
    crate::local_ipc::require_job_user(job.as_handle(), &user_sid)?;
    let mut assigned = 0;
    // SAFETY: actual current process and retained native job handles.
    ensure!(
        unsafe { IsProcessInJob(GetCurrentProcess(), job.as_raw_handle(), &mut assigned) } != 0,
        std::io::Error::last_os_error()
    );
    ensure!(
        assigned != 0,
        "guarded helper is outside its exact named job"
    );
    drop(job);
    GuardedBootstrap::from_permit(permit)
}
