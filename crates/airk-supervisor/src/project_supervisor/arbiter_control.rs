// SPDX-License-Identifier: Apache-2.0
//! Weak native management port; no independent factory or memory capability.
use super::*;
use airk_core::arbiter::Configuration;
use airk_inference::inference_engine::pool::arbiter::Handle;
use std::sync::Weak;

#[derive(Clone)]
pub struct ArbiterManagement {
    sender: Weak<SyncSender<Envelope>>,
    closed: Weak<std::sync::atomic::AtomicBool>,
}
impl ArbiterManagement {
    fn require_open(&self) -> Result<()> {
        ensure!(
            self.closed
                .upgrade()
                .is_some_and(|closed| !closed.load(std::sync::atomic::Ordering::Acquire)),
            "Arbiter management owner closed"
        );
        Ok(())
    }
    async fn handle(&self, configuration: Option<Configuration>) -> Result<Handle> {
        self.require_open()?;
        let (reply, receive) = oneshot::channel();
        let deadline = Instant::now() + Duration::from_secs(15);
        // The temporary strong sender ends at this statement. A background
        // download cannot keep the actor alive while Supervisor::drop joins it.
        self.sender
            .upgrade()
            .context("Arbiter supervisor closed")?
            .try_send(Envelope::ArbiterHandle {
                configuration,
                deadline,
                reply,
            })?;
        tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), receive)
            .await
            .context("Arbiter owner readiness remains pending")?
            .context("Arbiter owner closed")?
    }
    pub async fn existing(&self) -> Result<Handle> {
        self.handle(None).await
    }
    pub async fn ensure(&self, configuration: Configuration) -> Result<Handle> {
        configuration.validate()?;
        self.handle(Some(configuration)).await
    }
    pub async fn reserve(
        &self,
        bytes: u64,
    ) -> Result<airk_kernel::service_resources::OptionalReservation> {
        self.require_open()?;
        let (reply, receive) = oneshot::channel();
        self.sender
            .upgrade()
            .context("Arbiter supervisor closed")?
            .try_send(Envelope::ReserveOptional {
                id: new_instance_id()?,
                bytes,
                reply,
            })?;
        tokio::time::timeout(Duration::from_secs(5), receive)
            .await
            .context("Arbiter resource admission remains pending")?
            .context("Arbiter resource owner closed")?
    }
}
pub(super) struct Waiting {
    pub configuration: Configuration,
    pub deadline: Instant,
    pub reply: oneshot::Sender<Result<Handle>>,
}
pub(super) fn settle(
    waiting: &mut Vec<Waiting>,
    pool: Option<&Pool>,
    closed: bool,
    suppressed: bool,
) {
    let mut retained = Vec::new();
    for entry in waiting.drain(..) {
        if entry.reply.is_closed() {
            continue;
        }
        let result = if closed || suppressed {
            Some(Err(anyhow::anyhow!(
                "Arbiter inference admission unavailable"
            )))
        } else if Instant::now() >= entry.deadline {
            Some(Err(anyhow::anyhow!(
                "Arbiter inference readiness deadline exhausted"
            )))
        } else {
            pool.map(|pool| {
                pool.arbiter().and_then(|handle| {
                    handle.configure(entry.configuration.clone())?;
                    Ok(handle)
                })
            })
        };
        if let Some(result) = result {
            let _ = entry.reply.send(result);
        } else {
            retained.push(entry);
        }
    }
    *waiting = retained;
}
impl Supervisor {
    pub fn arbiter_management(&self) -> ArbiterManagement {
        ArbiterManagement {
            sender: self.sender.as_ref().map_or_else(Weak::new, Arc::downgrade),
            closed: Arc::downgrade(&self.checkpoint_closed),
        }
    }
}
