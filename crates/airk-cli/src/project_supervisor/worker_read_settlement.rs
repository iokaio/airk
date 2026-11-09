// SPDX-License-Identifier: Apache-2.0
//! A failed read channel retires only its exact worker, after accepted work.
use super::*;

pub(super) struct ReadSettlement {
    operation_id: Incarnation,
    phase: Phase,
}

#[derive(Clone, Copy)]
enum Phase {
    Ready(Instant),
    Awaiting,
    Unknown,
    Sealed,
}

impl ReadSettlement {
    pub(super) fn new(operation_id: Incarnation) -> Self {
        Self {
            operation_id,
            phase: Phase::Ready(Instant::now()),
        }
    }

    pub(super) fn control_pending(&self) -> bool {
        matches!(self.phase, Phase::Awaiting | Phase::Unknown)
    }

    pub(super) fn sealed(&self) -> bool {
        matches!(self.phase, Phase::Sealed)
    }

    fn accept(
        &mut self,
        identity: WorkerIdentity,
        engine: airk_core::identity::Sha256Digest,
        observation: protocol::WorkerCheckpointObservation,
    ) -> Result<bool> {
        self.phase = Phase::Unknown;
        ensure!(
            observation.schema == 1
                && observation.identity == identity
                && observation.operation_id == self.operation_id
                && observation.engine_sha256 == engine
                && observation.admission_closed,
            "crossed read settlement acknowledgment"
        );
        let complete = observation.sealed
            && observation.durable_boundary
            && observation.active_queries == 0
            && observation.active_work_grant.is_none();
        self.phase = if complete {
            Phase::Sealed
        } else {
            Phase::Ready(Instant::now() + Duration::from_secs(1))
        };
        Ok(complete)
    }
}

pub(super) fn requester(
    sender: Option<std::sync::Weak<SyncSender<Envelope>>>,
) -> airk_mcp::read_attempt::WorkerSettlementRequester {
    Arc::new(move |identity| {
        let sender = sender
            .as_ref()
            .and_then(std::sync::Weak::upgrade)
            .context("read settlement supervisor closed")?;
        sender
            .try_send(Envelope::SettleRead {
                identity,
                operation_id: new_instance_id()?,
            })
            .map_err(|_| anyhow::anyhow!("read settlement queue unavailable"))
    })
}

impl ChildWorker {
    /// A checkpoint response is the existing native work/query census, not an
    /// inferred idle timer. Unknown control outcomes retain ownership and never
    /// authorize Stop or a replacement worker.
    pub(super) fn poll_read_settlement(&mut self) -> Result<bool> {
        let Some(settlement) = self.read_settlement.as_ref() else {
            return Ok(false);
        };
        match settlement.phase {
            Phase::Sealed => return Ok(true),
            Phase::Unknown => return Ok(false),
            Phase::Ready(at) => {
                if Instant::now() < at || !self.catalog_poll_ready()? {
                    return Ok(false);
                }
                let identity = self.identity.context("read settlement identity missing")?;
                let settlement = self
                    .read_settlement
                    .as_mut()
                    .context("read settlement missing")?;
                // Set uncertainty before any control bytes may be submitted.
                settlement.phase = Phase::Unknown;
                service_ipc::write_frame(
                    self.input
                        .as_mut()
                        .context("read settlement input missing")?,
                    &WorkerCommand::Checkpoint {
                        identity,
                        operation_id: settlement.operation_id,
                        seal: true,
                    },
                )?;
                settlement.phase = Phase::Awaiting;
                return Ok(false);
            }
            Phase::Awaiting => {}
        }
        let reply = match self.output.try_recv() {
            Ok(reply) => reply,
            Err(mpsc::TryRecvError::Empty) => return Ok(false),
            Err(mpsc::TryRecvError::Disconnected) => {
                self.read_settlement
                    .as_mut()
                    .expect("read settlement exists")
                    .phase = Phase::Unknown;
                return Ok(false);
            }
        };
        let settlement = self
            .read_settlement
            .as_mut()
            .context("read settlement missing")?;
        settlement.phase = Phase::Unknown;
        let Reply::Checkpoint { observation } = reply else {
            anyhow::bail!("read settlement acknowledgment differs from checkpoint");
        };
        settlement.accept(
            self.identity.context("read settlement identity missing")?,
            crate::runtime_pair::engine_identity()?,
            observation,
        )
    }
}

#[cfg(test)]
mod tests;
