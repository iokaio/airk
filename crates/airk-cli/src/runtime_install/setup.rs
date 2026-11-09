// SPDX-License-Identifier: Apache-2.0
//! Session-owned blocking setup; cancellation cannot detach accepted lifecycle work.
use super::*;
use std::sync::{
    Arc,
    atomic::{AtomicU8, Ordering},
};

type Worker = tokio::task::JoinHandle<Result<PathBuf>>;

/// Session EOF closes this gate without waiting for an attempt-state lock.
/// It admits only the same exact operations as the attempt gate, never a pipeline.
#[derive(Default)]
pub(crate) struct SetupAdmission(AtomicU8);
impl SetupAdmission {
    pub(crate) fn close(&self) {
        self.0.fetch_or(CLOSED, Ordering::AcqRel);
    }
    fn checkpoint(&self) -> Result<()> {
        ensure!(
            self.0.load(Ordering::Acquire) & CLOSED == 0,
            "runtime setup cancelled"
        );
        Ok(())
    }
    fn admit(&self) -> Result<SessionPermit<'_>> {
        self.0
            .compare_exchange(0, ACTIVE, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| {
                anyhow::anyhow!("runtime setup cancelled or operation already admitted")
            })?;
        Ok(SessionPermit(self))
    }
}
struct SessionPermit<'a>(&'a SetupAdmission);
impl Drop for SessionPermit<'_> {
    fn drop(&mut self) {
        self.0.0.fetch_and(!ACTIVE, Ordering::AcqRel);
    }
}

#[derive(Default)]
pub(crate) struct SetupCancellation {
    admission: AtomicU8,
    session: Option<Arc<SetupAdmission>>,
    worker: tokio::sync::Mutex<Option<Worker>>,
}
impl SetupCancellation {
    pub(crate) fn for_session(session: Arc<SetupAdmission>) -> Self {
        Self {
            session: Some(session),
            ..Self::default()
        }
    }
    pub(crate) fn close(&self) {
        // Never wait for an OS operation already admitted on the blocking owner.
        self.admission.fetch_or(CLOSED, Ordering::AcqRel);
    }
    pub(crate) fn expire(&self) {
        // Keep deadline cancellation typed after accepted work has drained, so
        // a later status waiter cannot mistake it for a genuine setup failure.
        self.admission.fetch_or(CLOSED | EXPIRED, Ordering::AcqRel);
    }
    pub(super) fn checkpoint(&self) -> Result<()> {
        if let Some(session) = &self.session {
            session.checkpoint()?;
        }
        let admission = self.admission.load(Ordering::Acquire);
        if admission & EXPIRED != 0 {
            return Err(airk_mcp::remote::PreparationExpired.into());
        }
        ensure!(admission & CLOSED == 0, "runtime setup cancelled");
        Ok(())
    }
    fn admit(&self) -> Result<OperationPermit<'_>> {
        // Session CAS orders actual EOF against this exact operation; the local
        // CAS also rejects an individually cancelled/failed attempt.
        let session = self.session.as_ref().map(|gate| gate.admit()).transpose()?;
        self.admission
            .compare_exchange(0, ACTIVE, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|admission| {
                if admission & EXPIRED != 0 {
                    anyhow::Error::new(airk_mcp::remote::PreparationExpired)
                } else {
                    anyhow::anyhow!("runtime setup cancelled or operation already admitted")
                }
            })?;
        Ok(OperationPermit {
            owner: self,
            _session: session,
        })
    }
    pub(super) fn effect<T>(&self, operation: impl FnOnce() -> Result<T>) -> Result<T> {
        let _permit = self.admit()?;
        operation()
    }
    #[cfg(test)]
    fn admit_effect(&self) -> Result<()> {
        self.effect(|| Ok(()))
    }
    async fn run(self: &Arc<Self>, root: PathBuf) -> Result<PathBuf> {
        let runtime = tokio::runtime::Handle::current();
        self.run_worker(move |control| prepare_and_start(root, control, runtime))
            .await
    }
    pub(crate) async fn run_worker<F>(self: &Arc<Self>, operation: F) -> Result<PathBuf>
    where
        F: FnOnce(Arc<Self>) -> Result<PathBuf> + Send + 'static,
    {
        let mut slot = self.worker.lock().await;
        self.checkpoint()?;
        ensure!(slot.is_none(), "runtime setup worker already owned");
        let control = self.clone();
        // The slot retains the actual handle while awaiting by mutable reference.
        // A dropped waiter never takes the worker away from its session owner.
        *slot = Some(tokio::task::spawn_blocking(move || operation(control)));
        let outcome = slot
            .as_mut()
            .expect("owned setup worker")
            .await
            .context("owned runtime setup worker failed");
        *slot = None;
        outcome?
    }
    #[cfg(test)]
    pub(crate) fn admission_is_closed(&self) -> bool {
        self.checkpoint().is_err()
    }
    #[cfg(test)]
    pub(crate) fn worker_is_owned(&self) -> bool {
        match self.worker.try_lock() {
            Ok(slot) => slot.is_some(),
            // run_worker holds this slot while awaiting its retained real handle.
            Err(_) => true,
        }
    }
    pub(crate) async fn drain(&self) -> Result<()> {
        self.close();
        let mut slot = self.worker.lock().await;
        if let Some(worker) = slot.as_mut() {
            // spawn_blocking cannot be stopped by abort. Join even after close;
            // an OS call already blocked has no claimed hard shutdown deadline.
            let outcome = worker.await.context("owned runtime setup drain failed");
            *slot = None;
            drop(outcome?);
        }
        Ok(())
    }
}

const ACTIVE: u8 = 1;
const CLOSED: u8 = 2;
const EXPIRED: u8 = 4;
/// Non-Clone and tied to this owner. This is specific operation admission, not
/// a durable receipt or permission for a subsequent pipeline operation.
pub(super) struct OperationPermit<'a> {
    owner: &'a SetupCancellation,
    _session: Option<SessionPermit<'a>>,
}
impl Drop for OperationPermit<'_> {
    fn drop(&mut self) {
        self.owner.admission.fetch_and(!ACTIVE, Ordering::AcqRel);
    }
}

/// The production health-to-election boundary. An existing healthy observation
/// performs no election; a miss serializes the exact first effect with EOF.
pub(super) async fn observe_or_effect<'a, T>(
    observation: impl std::future::Future<Output = Result<T>>,
    cancel: Option<&'a SetupCancellation>,
    effect: impl FnOnce() -> Result<()>,
) -> Result<(Option<T>, Option<OperationPermit<'a>>)> {
    if let Ok(value) = observation.await {
        return Ok((Some(value), None));
    }
    // CAS is the admission linearization point, immediately before this exact
    // operation's first effect. EOF wins => no callback. Admission wins => this
    // operation remains owner-retained through its receipt/election settlement.
    let permit = cancel.map(SetupCancellation::admit).transpose()?;
    effect()?;
    Ok((None, permit))
}

/// Exact saved operations already have durable authority. Their own result
/// keeps priority; success never authorizes a subsequent ordinary election.
pub(super) async fn settle_then_checkpoint(
    operation: impl std::future::Future<Output = Result<()>>,
    cancel: Option<&SetupCancellation>,
) -> Result<()> {
    operation.await?;
    if let Some(cancel) = cancel {
        cancel.checkpoint()?;
    }
    Ok(())
}

pub(crate) async fn installed_control_cooperative(
    root: PathBuf,
    control: Arc<SetupCancellation>,
) -> Result<PathBuf> {
    control.run(root).await
}

/// The File and root move together into the blocking owner. Borrowed F03 views
/// never escape this owner's stack or survive releasing this exact File.
struct OwnedInstallerAuthority {
    root: PathBuf,
    lock: File,
}
impl OwnedInstallerAuthority {
    fn acquire(root: PathBuf, cancel: &SetupCancellation) -> Result<Self> {
        cancel.checkpoint()?;
        let lock = open_install_lock(&root)?;
        let deadline = std::time::Instant::now() + Duration::from_secs(60);
        loop {
            cancel.checkpoint()?;
            match lock.try_lock() {
                Ok(()) => return Ok(Self { root, lock }),
                Err(std::fs::TryLockError::WouldBlock) if std::time::Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(50));
                }
                Err(error) => {
                    return Err(error)
                        .context("service connection installer wait expired or failed");
                }
            }
        }
    }
    fn view(&self) -> InstallerAuthority<'_> {
        InstallerAuthority {
            root: &self.root,
            _lock: &self.lock,
        }
    }
}

fn prepare_and_start(
    root: PathBuf,
    cancel: Arc<SetupCancellation>,
    runtime: tokio::runtime::Handle,
) -> Result<PathBuf> {
    cancel.checkpoint()?;
    if let Some(policy) = managed::read(&root)? {
        // Managed pins keep their existing full preflight and legacy settlement.
        // Both verification and synchronous manager work run on this owned thread.
        // The common held-lock path authenticates pending STOP from saved
        // identities before it requires a runnable approved payload.
        let approval = policy.approval.clone();
        ensure!(
            approval.version == env!("CARGO_PKG_VERSION"),
            "matching runtime version required"
        );
        let owned = OwnedInstallerAuthority::acquire(root, &cancel)?;
        let authority = owned.view();
        cancel.checkpoint()?;
        let health = runtime.block_on(ensure_running_locked_checked(&authority, Some(&cancel)))?;
        require_matching_health(&approval, &health)?;
        return layout::control(&owned.root);
    }
    if layout::managed_origin(&root)?.is_some() {
        let owned = OwnedInstallerAuthority::acquire(root, &cancel)?;
        // Authenticate any saved STOP first, then return the explicit
        // same-configure recovery reason; never use copied selection fallback.
        runtime.block_on(ensure_running_locked_checked(&owned.view(), Some(&cancel)))?;
        anyhow::bail!("repeat_same_approved_managed_configure_to_resume");
    }
    ensure!(
        !state_contract::required(&root)?,
        "clean_break_required: install the baseline before opening an evidence connection"
    );
    let check = || cancel.checkpoint();
    let preflight = selected_preflight(&root)?;
    cancel.checkpoint()?;
    let owned = OwnedInstallerAuthority::acquire(root, &cancel)?;
    let authority = owned.view();
    let prepared = prepare_connection_with(&authority, &preflight, |authority| {
        selected_verified_checked(authority, &check)
    })?;
    cancel.checkpoint()?;
    let health = match prepared {
        PreparedConnection::NoEffects(verified) => {
            runtime.block_on(activation::start_verified_checked(&verified, Some(&cancel)))?
        }
        PreparedConnection::SettlementRequired => {
            runtime.block_on(ensure_running_locked_checked(&authority, Some(&cancel)))?
        }
    };
    require_matching_health(&preflight.approval, &health)?;
    let control = layout::control(&owned.root)?;
    ensure!(
        control == preflight.control,
        "selected service control changed"
    );
    Ok(control)
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    #[test]
    fn session_close_fences_new_attempts_and_preserves_accepted_operation() -> Result<()> {
        let session = Arc::new(SetupAdmission::default());
        let first = SetupCancellation::for_session(session.clone());
        first.close();
        let replacement = SetupCancellation::for_session(session.clone());
        replacement.checkpoint()?;
        let accepted = replacement.admit()?;
        session.close();
        assert!(replacement.checkpoint().is_err());
        assert_eq!(session.0.load(Ordering::Acquire), ACTIVE | CLOSED);
        drop(accepted);
        assert_eq!(session.0.load(Ordering::Acquire), CLOSED);
        let later = SetupCancellation::for_session(session);
        assert!(later.admit().is_err());
        assert!(later.checkpoint().is_err());
        Ok(())
    }

    #[test]
    fn expired_attempt_refuses_later_effects_without_poisoning_session() -> Result<()> {
        let session = Arc::new(SetupAdmission::default());
        let setup = SetupCancellation::for_session(session.clone());
        let accepted = setup.admit()?;
        setup.expire();
        assert!(
            setup
                .checkpoint()
                .unwrap_err()
                .is::<airk_mcp::remote::PreparationExpired>()
        );
        drop(accepted);
        let calls = AtomicUsize::new(0);
        let error = setup
            .effect(|| {
                calls.fetch_add(1, Ordering::SeqCst);
                Ok(())
            })
            .unwrap_err();
        assert!(error.is::<airk_mcp::remote::PreparationExpired>());
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        let replacement = SetupCancellation::for_session(session.clone());
        replacement.effect(|| {
            calls.fetch_add(1, Ordering::SeqCst);
            Ok(())
        })?;
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        session.close();
        for control in [&setup, &replacement] {
            assert!(
                !control
                    .checkpoint()
                    .unwrap_err()
                    .is::<airk_mcp::remote::PreparationExpired>()
            );
        }
        Ok(())
    }

    #[tokio::test]
    async fn actual_sdk_eof_during_health_prevents_startup_receipt() -> Result<()> {
        use airk_mcp::remote::{RemoteFuture, RemoteTools};
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
        struct HealthProbe {
            root: PathBuf,
            identity: String,
            cancel: Arc<SetupCancellation>,
            began: Arc<tokio::sync::Notify>,
            release: Arc<tokio::sync::Notify>,
        }
        impl RemoteTools for HealthProbe {
            fn call(
                &self,
                _: String,
                _: serde_json::Map<String, serde_json::Value>,
            ) -> RemoteFuture<'_> {
                Box::pin(async move {
                    let observation = async {
                        self.began.notify_one();
                        self.release.notified().await;
                        anyhow::bail!("inert fixture reports no healthy service")
                    };
                    let result: Result<(Option<()>, Option<OperationPermit<'_>>)> =
                        observe_or_effect(observation, Some(&self.cancel), || {
                            startup_attempt(&self.root, &self.identity, 10_000)
                        })
                        .await;
                    assert!(result.is_err(), "actual EOF must win startup admission");
                    Ok(serde_json::json!({"content": []}))
                })
            }
        }
        let (_owner, root, identity) = super::super::tests::unselected_fixture()?;
        let session = Arc::new(SetupAdmission::default());
        let began = Arc::new(tokio::sync::Notify::new());
        let release = Arc::new(tokio::sync::Notify::new());
        let closed = Arc::new(tokio::sync::Notify::new());
        let observed = closed.clone();
        let closing = session.clone();
        let (client, server) = tokio::io::duplex(65536);
        let (input, output) = tokio::io::split(server);
        let task = tokio::spawn(airk_mcp::remote::serve_io_with_input_lifecycle(
            Arc::new(HealthProbe {
                root: root.clone(),
                identity: identity.clone(),
                cancel: Arc::new(SetupCancellation::for_session(session)),
                began: began.clone(),
                release: release.clone(),
            }),
            input,
            output,
            Arc::new(move || {
                closing.close();
                observed.notify_one();
            }),
        ));
        let (input, mut output) = tokio::io::split(client);
        let mut lines = tokio::io::BufReader::new(input).lines();
        output.write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{\"protocolVersion\":\"2024-11-05\",\"capabilities\":{},\"clientInfo\":{\"name\":\"fixture\",\"version\":\"1\"}}}\n").await?;
        let initialized = lines
            .next_line()
            .await?
            .context("initialize response missing")?;
        ensure!(
            serde_json::from_str::<serde_json::Value>(&initialized)?["result"].is_object(),
            "initialize failed"
        );
        output.write_all(b"{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/call\",\"params\":{\"name\":\"airk_search\",\"arguments\":{}}}\n").await?;
        began.notified().await;
        output.shutdown().await?;
        tokio::time::timeout(std::time::Duration::from_secs(1), closed.notified()).await?;
        assert!(
            !task.is_finished(),
            "health is still blocked during SDK drain"
        );
        release.notify_one();
        task.await??;
        assert!(!root.join(format!("startup-{identity}.json")).exists());
        assert!(!root.join("bootstrap.json").exists());
        Ok(())
    }

    #[tokio::test]
    async fn blocked_verifier_keeps_executor_responsive_and_close_prevents_effect_admission()
    -> Result<()> {
        let control = Arc::new(SetupCancellation::default());
        let effects = Arc::new(AtomicUsize::new(0));
        let counted = effects.clone();
        let (started, began) = tokio::sync::oneshot::channel();
        let (release, released) = std::sync::mpsc::channel();
        let mut work = Box::pin(control.run_worker(move |control| {
            let _ = started.send(());
            released.recv().context("owned test release missing")?;
            control.checkpoint()?;
            control.admit_effect()?;
            counted.fetch_add(1, Ordering::SeqCst);
            Ok(PathBuf::from("owned-result"))
        }));
        tokio::select! {
            _ = &mut work => anyhow::bail!("worker must remain blocked"),
            result = began => result?,
        }
        // This current-thread runtime can continue polling while the worker is
        // blocked in synchronous work. No elapsed-time threshold is asserted.
        assert_eq!(tokio::spawn(async { 42 }).await?, 42);
        control.close();
        release.send(())?;
        assert!(work.await.is_err());
        control.drain().await?;
        assert_eq!(effects.load(Ordering::SeqCst), 0);
        assert!(control.worker.lock().await.is_none());
        Ok(())
    }

    #[tokio::test]
    async fn accepted_setup_is_joined_after_close_without_replay() -> Result<()> {
        let control = Arc::new(SetupCancellation::default());
        let effects = Arc::new(AtomicUsize::new(0));
        let counted = effects.clone();
        let (started, began) = tokio::sync::oneshot::channel();
        let (release, released) = std::sync::mpsc::channel();
        let mut work = Box::pin(control.run_worker(move |control| {
            control.admit_effect()?;
            counted.fetch_add(1, Ordering::SeqCst);
            let _ = started.send(());
            released.recv().context("owned test release missing")?;
            Ok(PathBuf::from("accepted-result"))
        }));
        tokio::select! {
            _ = &mut work => anyhow::bail!("accepted operation must remain owned"),
            result = began => result?,
        }
        let mut drain = Box::pin(control.drain());
        std::future::poll_fn(|cx| {
            assert!(std::future::Future::poll(drain.as_mut(), cx).is_pending());
            std::task::Poll::Ready(())
        })
        .await;
        drop(drain);
        release.send(())?;
        assert_eq!(work.await?, PathBuf::from("accepted-result"));
        control.drain().await?;
        assert_eq!(effects.load(Ordering::SeqCst), 1);
        assert!(control.worker.lock().await.is_none());
        Ok(())
    }

    #[tokio::test]
    async fn closed_during_actual_health_boundary_writes_no_startup_receipt() -> Result<()> {
        let (_owner, root, identity) = super::super::tests::unselected_fixture()?;
        let control = SetupCancellation::default();
        let (release, released) = tokio::sync::oneshot::channel::<()>();
        let mut election = Box::pin(observe_or_effect(
            async {
                released.await?;
                anyhow::bail!("fixture reports no healthy service")
            },
            Some(&control),
            || startup_attempt(&root, &identity, 10_000),
        ));
        std::future::poll_fn(|cx| {
            assert!(std::future::Future::poll(election.as_mut(), cx).is_pending());
            std::task::Poll::Ready(())
        })
        .await;
        control.close();
        release.send(()).expect("health waiter retained");
        let result: Result<(Option<()>, Option<OperationPermit<'_>>)> = election.await;
        assert!(result.is_err());
        assert!(!root.join(format!("startup-{identity}.json")).exists());
        assert!(!root.join("bootstrap.json").exists());
        Ok(())
    }

    #[tokio::test]
    async fn closed_during_manager_health_boundary_does_not_elect() -> Result<()> {
        let control = SetupCancellation::default();
        let elections = AtomicUsize::new(0);
        let (release, released) = tokio::sync::oneshot::channel::<()>();
        let mut election = Box::pin(observe_or_effect(
            async {
                released.await?;
                anyhow::bail!("fixture reports no healthy manager service")
            },
            Some(&control),
            || {
                elections.fetch_add(1, Ordering::SeqCst);
                Ok(())
            },
        ));
        std::future::poll_fn(|cx| {
            assert!(std::future::Future::poll(election.as_mut(), cx).is_pending());
            std::task::Poll::Ready(())
        })
        .await;
        control.close();
        release.send(()).expect("health waiter retained");
        let result: Result<(Option<()>, Option<OperationPermit<'_>>)> = election.await;
        assert!(result.is_err());
        assert_eq!(elections.load(Ordering::SeqCst), 0);
        Ok(())
    }

    #[tokio::test]
    async fn accepted_exact_startup_retains_permit_after_close_without_replay() -> Result<()> {
        let (_owner, root, identity) = super::super::tests::unselected_fixture()?;
        let control = SetupCancellation::default();
        let (healthy, attempt) = observe_or_effect(
            async { anyhow::bail!("inert fixture has no service") },
            Some(&control),
            || startup_attempt(&root, &identity, 10_000),
        )
        .await?;
        let _: Option<()> = healthy;
        assert!(attempt.is_some());
        assert!(root.join(format!("startup-{identity}.json")).exists());
        control.close(); // Must return while the exact operation permit is held.
        assert!(control.admit().is_err());
        // Dropping the settled operation cannot reopen the closed owner.
        drop(attempt);
        assert!(control.admit().is_err());
        Ok(())
    }

    #[tokio::test]
    async fn user_on_demand_native_boundary_closed_during_health_never_launches() -> Result<()> {
        let (_owner, root, identity) = super::super::tests::unselected_fixture()?;
        let (main, approval) = verify_runtime(&root, &identity)?;
        let control_root = layout::control(&root)?;
        let digest = runtime_pair::digest_file(&main)?;
        let cancel = SetupCancellation::default();
        let (release, released) = tokio::sync::oneshot::channel::<()>();
        // This is the actual native adapter used by managed UserOnDemand,
        // replacing only its read-only health observation, never the election.
        let mut start = Box::pin(start_with_observation(
            &root,
            &approval,
            &main,
            true,
            Some(&cancel),
            &control_root,
            &digest,
            async {
                released.await?;
                anyhow::bail!("inert fixture reports no service")
            },
        ));
        std::future::poll_fn(|cx| {
            assert!(std::future::Future::poll(start.as_mut(), cx).is_pending());
            std::task::Poll::Ready(())
        })
        .await;
        cancel.close();
        release.send(()).expect("native health waiter retained");
        assert!(start.await.is_err());
        assert!(!root.join(format!("startup-{identity}.json")).exists());
        assert!(!root.join("bootstrap.json").exists());
        Ok(())
    }

    #[test]
    fn digest_checkpoint_cancels_between_real_file_chunks() -> Result<()> {
        use std::cell::Cell;
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("original-image");
        fs::write(&path, vec![7; 256 * 1024])?;
        let checks = Cell::new(0);
        let result = runtime_pair::digest_file_checked(&path, &|| {
            checks.set(checks.get() + 1);
            ensure!(checks.get() < 3, "owned fixture cancellation");
            Ok(())
        });
        assert!(result.is_err());
        assert_eq!(checks.get(), 3);
        assert_eq!(fs::metadata(path)?.len(), 256 * 1024);
        Ok(())
    }

    #[test]
    fn owned_installer_file_moves_without_releasing_its_lock() -> Result<()> {
        let (_owner, root, _) = super::super::tests::unselected_fixture()?;
        let cancel = SetupCancellation::default();
        let owned = OwnedInstallerAuthority::acquire(root.clone(), &cancel)?;
        assert!(install_lock(&root).is_err());
        let moved = std::thread::spawn(move || {
            let view = owned.view();
            assert!(install_lock(view.root).is_err());
            owned
        })
        .join()
        .expect("owned authority thread");
        assert!(install_lock(&root).is_err());
        drop(moved);
        install_lock(&root)?;
        Ok(())
    }
}
