// SPDX-License-Identifier: Apache-2.0
// Retirement extends native pool membership, not the capability authority.
use super::*;
use airk_core::service::WorkerIdentity;

struct Member {
    identity: Option<WorkerIdentity>,
    service: Incarnation,
    project: Sha256Digest,
    alive: Arc<AtomicBool>,
}
struct Drain {
    ticket: protocol::ProviderDrainTicket,
    completed: Arc<AtomicBool>,
    queued: bool,
}
pub(super) enum Control {
    Begin,
    Poll,
    Forget,
}

#[derive(Default)]
pub(super) struct Retirements {
    members: BTreeMap<Incarnation, Member>,
    drains: BTreeMap<Incarnation, Drain>,
}
impl Retirements {
    #[cfg(any(test, feature = "test-support"))]
    pub(super) fn native_identity(&self, worker: Incarnation) -> Option<WorkerIdentity> {
        self.members.get(&worker).and_then(|member| member.identity)
    }
    pub(super) fn register(
        &mut self,
        grants: &mut BTreeMap<Sha256Digest, Authorization>,
        authority: (Incarnation, Incarnation),
        grant: protocol::Grant,
        identity: Option<WorkerIdentity>,
        now: Instant,
    ) -> protocol::Observation {
        if identity.is_some_and(|identity| {
            identity.service != grant.service
                || identity.worker != grant.worker
                || identity.project != grant.project.binding
                || identity.fencing_generation == 0
        }) || self
            .drains
            .values()
            .any(|drain| drain.ticket.worker.worker == grant.worker)
            || (!self.members.contains_key(&grant.worker)
                && self.members.len() >= protocol::MAX_PROVIDER_DRAINS)
        {
            return protocol::Observation::Failed;
        }
        let alive = if let Some(member) = self.members.get(&grant.worker) {
            if member.identity != identity
                || member.service != grant.service
                || member.project != grant.project.binding
                || !member.alive.load(Ordering::Acquire)
            {
                return protocol::Observation::Failed;
            }
            member.alive.clone()
        } else {
            Arc::new(AtomicBool::new(true))
        };
        let token = grant.token;
        let worker = grant.worker;
        let service = grant.service;
        let project = grant.project.binding;
        let observation = register_authorization(grants, authority.0, authority.1, grant, now);
        if matches!(observation, protocol::Observation::Registered) {
            // The authoritative worker flag outlives token TTL, replacement
            // connections, and every already queued or queue-waiting request.
            if let Some(authorization) = grants.get_mut(&token) {
                authorization.worker_alive = alive.clone();
            }
            self.members.insert(
                worker,
                Member {
                    identity,
                    service,
                    project,
                    alive,
                },
            );
        }
        observation
    }

    pub(super) fn revoke(
        &mut self,
        grants: &mut BTreeMap<Sha256Digest, Authorization>,
        worker: Incarnation,
    ) {
        if let Some(member) = self.members.get(&worker) {
            member.alive.store(false, Ordering::Release);
        }
        grants.retain(|_, auth| {
            if auth.grant.worker == worker {
                auth.worker_alive.store(false, Ordering::Release);
                auth.alive.store(false, Ordering::Release);
                false
            } else {
                true
            }
        });
        if !self
            .drains
            .values()
            .any(|drain| drain.ticket.worker.worker == worker)
        {
            self.members.remove(&worker);
        }
    }

    pub(super) fn control(
        &mut self,
        action: Control,
        authority: (Incarnation, Incarnation),
        grants: &mut BTreeMap<Sha256Digest, Authorization>,
        sender: &mpsc::Sender<Work>,
        ticket: protocol::ProviderDrainTicket,
    ) -> protocol::Observation {
        use protocol::ProviderDrainObservation as O;
        let valid = ticket.helper == authority.1 && ticket.worker.service == authority.0;
        let observation = if !valid {
            O::Refused
        } else {
            match action {
                Control::Begin => {
                    if let Some(drain) = self.drains.get(&ticket.ticket) {
                        if drain.ticket != ticket {
                            O::Refused
                        } else if drain.completed.load(Ordering::Acquire) {
                            O::Drained
                        } else {
                            O::Pending
                        }
                    } else if self.drains.len() >= protocol::MAX_PROVIDER_DRAINS
                        || self
                            .drains
                            .values()
                            .any(|drain| drain.ticket.worker.worker == ticket.worker.worker)
                        || self
                            .members
                            .get(&ticket.worker.worker)
                            .is_none_or(|member| member.identity != Some(ticket.worker))
                    {
                        O::Refused
                    } else {
                        self.drains.insert(
                            ticket.ticket,
                            Drain {
                                ticket,
                                completed: Arc::new(AtomicBool::new(false)),
                                queued: false,
                            },
                        );
                        self.revoke(grants, ticket.worker.worker);
                        self.retry(sender);
                        O::Pending
                    }
                }
                Control::Poll => {
                    self.retry(sender);
                    match self.drains.get(&ticket.ticket) {
                        Some(drain) if drain.ticket == ticket => {
                            if drain.completed.load(Ordering::Acquire) {
                                O::Drained
                            } else {
                                O::Pending
                            }
                        }
                        _ => O::Refused,
                    }
                }
                Control::Forget => {
                    if self.drains.get(&ticket.ticket).is_some_and(|drain| {
                        drain.ticket == ticket && drain.completed.load(Ordering::Acquire)
                    }) {
                        // Native caller records combined child/descendant and
                        // provider proof before explicitly releasing this slot.
                        // Retained queued requests still hold the OLD false flag.
                        self.drains.remove(&ticket.ticket);
                        self.members.remove(&ticket.worker.worker);
                        O::Forgotten
                    } else {
                        O::Refused
                    }
                }
            }
        };
        protocol::Observation::Drain {
            ticket,
            observation,
        }
    }

    pub(super) fn retry(&mut self, sender: &mpsc::Sender<Work>) {
        for drain in self.drains.values_mut().filter(|drain| !drain.queued) {
            if sender
                .try_send(Work::DrainRelease {
                    worker: drain.ticket.worker.worker,
                    completed: drain.completed.clone(),
                })
                .is_ok()
            {
                drain.queued = true;
            } else {
                break;
            }
        }
    }
}
#[cfg(test)]
mod tests;
