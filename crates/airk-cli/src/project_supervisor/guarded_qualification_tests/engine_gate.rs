// SPDX-License-Identifier: Apache-2.0
//! Private, bounded qualification latch. Its messages are observations, never
//! authority, and the actual helper engine supplies every entered correlation.
use super::*;
#[path = "../../../tests/fixtures/guarded_provider_gate_protocol.rs"]
mod wire;

struct Arm {
    worker: WorkerIdentity,
    helper: Incarnation,
    nonce: Incarnation,
    release: tokio::sync::oneshot::Receiver<()>,
}

pub(super) struct EngineGate {
    arm: Arc<Mutex<Option<Arm>>>,
    entered: Receiver<Result<(wire::Context, Incarnation)>>,
    release: Option<tokio::sync::oneshot::Sender<()>>,
    stop: Option<tokio::sync::oneshot::Sender<()>>,
    thread: Option<std::thread::JoinHandle<Result<()>>>,
}

impl EngineGate {
    pub fn bind(root: &Path) -> Result<Self> {
        let address = local_ipc::endpoint(root, wire::ROLE)?;
        let arm: Arc<Mutex<Option<Arm>>> = Arc::new(Mutex::new(None));
        let shared = arm.clone();
        let (ready, waiting) = mpsc::sync_channel(1);
        let (entered, observations) = mpsc::sync_channel(1);
        let (stop, mut stopped) = tokio::sync::oneshot::channel();
        let thread = std::thread::Builder::new()
            .name("qualification-engine-gate".into())
            .spawn(move || -> Result<()> {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()?;
                runtime.block_on(async {
                    let mut listener = match local_ipc::Listener::bind(&address) {
                        Ok(listener) => listener,
                        Err(error) => {
                            let _ = ready.send(Err(error));
                            return Ok(());
                        }
                    };
                    let _ = ready.send(Ok(()));
                    loop {
                        let peer = tokio::select! {
                            _ = &mut stopped => return Ok(()),
                            peer = listener.accept() => peer?,
                        };
                        if let Err(error) = exchange(peer, &shared, &entered).await {
                            let _ = entered
                                .try_send(Err(anyhow::anyhow!("qualification gate: {error:#}")));
                            return Err(error);
                        }
                    }
                })
            })?;
        waiting
            .recv_timeout(Duration::from_secs(5))
            .context("native gate bind deadline")??;
        Ok(Self {
            arm,
            entered: observations,
            release: None,
            stop: Some(stop),
            thread: Some(thread),
        })
    }

    pub fn arm(&mut self, worker: WorkerIdentity, helper: Incarnation) -> Result<Incarnation> {
        ensure!(self.release.is_none(), "qualification hold already armed");
        let nonce = new_instance_id()?;
        let (release, released) = tokio::sync::oneshot::channel();
        let mut arm = self
            .arm
            .lock()
            .map_err(|_| anyhow::anyhow!("qualification gate poisoned"))?;
        ensure!(arm.is_none(), "qualification arm already pending");
        *arm = Some(Arm {
            worker,
            helper,
            nonce,
            release: released,
        });
        self.release = Some(release);
        Ok(nonce)
    }

    pub fn entered(&self, nonce: Incarnation) -> Result<wire::Context> {
        let (context, echoed) = self
            .entered
            .recv_timeout(Duration::from_secs(wire::DEADLINE_SECONDS))
            .context("actual before-compute entry deadline")??;
        ensure!(echoed == nonce, "qualification hold nonce crossed");
        Ok(context)
    }

    pub fn resume(&mut self) -> Result<()> {
        self.release
            .take()
            .context("qualification hold is not armed")?
            .send(())
            .map_err(|_| anyhow::anyhow!("native qualification peer already lost"))
    }
}

impl Drop for EngineGate {
    fn drop(&mut self) {
        // Release before stopping the endpoint. A held peer is never silently
        // detached while this test still owns its helper and descendants.
        if let Some(release) = self.release.take() {
            let _ = release.send(());
        }
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

async fn exchange(
    mut peer: local_ipc::Stream,
    armed: &Mutex<Option<Arm>>,
    entered: &SyncSender<Result<(wire::Context, Incarnation)>>,
) -> Result<()> {
    let budget = Duration::from_secs(wire::DEADLINE_SECONDS);
    let event: wire::Event =
        local_ipc::read_message_limited(&mut peer, budget, wire::MAX_FRAME_BYTES)
            .await?
            .context("native gate Reached EOF")?;
    let wire::Event::Reached { context } = event else {
        anyhow::bail!("native gate entered without reaching compute")
    };
    let arm = {
        let mut slot = armed
            .lock()
            .map_err(|_| anyhow::anyhow!("qualification arm poisoned"))?;
        if slot
            .as_ref()
            .is_some_and(|arm| arm.worker == context.worker && arm.helper == context.helper)
        {
            slot.take()
        } else {
            None
        }
    };
    let Some(arm) = arm else {
        local_ipc::write_message_limited(
            &mut peer,
            &wire::Action::Resume {
                context,
                nonce: None,
            },
            budget,
            wire::MAX_FRAME_BYTES,
        )
        .await?;
        return Ok(());
    };
    local_ipc::write_message_limited(
        &mut peer,
        &wire::Action::Hold {
            context,
            nonce: arm.nonce,
        },
        budget,
        wire::MAX_FRAME_BYTES,
    )
    .await?;
    let event: wire::Event =
        local_ipc::read_message_limited(&mut peer, budget, wire::MAX_FRAME_BYTES)
            .await?
            .context("native gate Entered EOF")?;
    ensure!(
        matches!(event, wire::Event::Entered { context: echoed, nonce } if echoed == context && nonce == arm.nonce),
        "native gate entered correlations crossed"
    );
    entered
        .try_send(Ok((context, arm.nonce)))
        .map_err(|_| anyhow::anyhow!("native gate observation queue unavailable"))?;
    // Even a failed/canceled driver releases the exact held peer. A timeout is
    // reported as failure, never interpreted as a provider drain completion.
    let released = tokio::time::timeout(budget, arm.release).await;
    local_ipc::write_message_limited(
        &mut peer,
        &wire::Action::Resume {
            context,
            nonce: Some(arm.nonce),
        },
        budget,
        wire::MAX_FRAME_BYTES,
    )
    .await?;
    released
        .context("qualification hold exceeded native deadline")?
        .context("qualification controller dropped its release owner")?;
    Ok(())
}
