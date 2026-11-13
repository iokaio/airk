// SPDX-License-Identifier: Apache-2.0
//! Bounded correlation state owned by the existing helper reply reader.
use super::*;
use std::collections::BTreeMap;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Exchange {
    Observe,
    Forget,
}
struct Slot {
    ticket: protocol::ProviderDrainTicket,
    state: ProviderDrainState,
    exchange: Option<(Exchange, Instant)>,
}

pub(super) struct Drains {
    service: Incarnation,
    helper: Incarnation,
    members: BTreeMap<Incarnation, airk_core::service::WorkerIdentity>,
    slots: BTreeMap<Incarnation, Slot>,
    closed: bool,
}
impl Drains {
    pub(super) fn new(service: Incarnation, helper: Incarnation) -> Self {
        Self {
            service,
            helper,
            members: BTreeMap::new(),
            slots: BTreeMap::new(),
            closed: false,
        }
    }
    pub(super) fn validate_member(
        &self,
        identity: airk_core::service::WorkerIdentity,
    ) -> Result<()> {
        ensure!(
            identity.service == self.service && identity.fencing_generation > 0,
            "provider worker identity does not belong to this service"
        );
        ensure!(
            !self
                .slots
                .values()
                .any(|slot| slot.ticket.worker.worker == identity.worker),
            "provider worker has an outstanding retirement obligation"
        );
        ensure!(
            self.members
                .get(&identity.worker)
                .is_none_or(|member| *member == identity),
            "provider worker fencing identity changed"
        );
        ensure!(
            self.members.contains_key(&identity.worker)
                || self.members.len() < protocol::MAX_PROVIDER_DRAINS,
            "provider membership capacity exhausted"
        );
        Ok(())
    }
    pub(super) fn registered(&mut self, identity: airk_core::service::WorkerIdentity) {
        self.members.insert(identity.worker, identity);
    }
    #[cfg(any(test, feature = "test-support"))]
    pub(super) fn revoked(&mut self, worker: Incarnation) {
        if !self
            .slots
            .values()
            .any(|slot| slot.ticket.worker.worker == worker)
        {
            self.members.remove(&worker);
        }
    }
    pub(super) fn ticket(
        &self,
        identity: airk_core::service::WorkerIdentity,
    ) -> Result<Option<protocol::ProviderDrainTicket>> {
        ensure!(
            identity.service == self.service,
            "provider retirement worker belongs to another service"
        );
        Ok(self
            .slots
            .values()
            .find(|slot| slot.ticket.worker == identity)
            .map(|slot| slot.ticket))
    }
    pub(super) fn begin(
        &mut self,
        identity: airk_core::service::WorkerIdentity,
        nonce: Incarnation,
    ) -> Result<(protocol::ProviderDrainTicket, bool)> {
        ensure!(
            identity.service == self.service
                && self.members.get(&identity.worker) == Some(&identity),
            "provider retirement worker is not the exact registered native owner"
        );
        if let Some(slot) = self
            .slots
            .values()
            .find(|slot| slot.ticket.worker == identity)
        {
            return Ok((slot.ticket, false));
        }
        ensure!(
            self.slots.len() < protocol::MAX_PROVIDER_DRAINS && !self.slots.contains_key(&nonce),
            "provider drain capacity or duplicate ticket"
        );
        let ticket = protocol::ProviderDrainTicket {
            helper: self.helper,
            worker: identity,
            ticket: nonce,
        };
        self.slots.insert(
            nonce,
            Slot {
                ticket,
                state: if self.closed {
                    ProviderDrainState::Unknown
                } else {
                    ProviderDrainState::Pending
                },
                exchange: Some((Exchange::Observe, Instant::now())),
            },
        );
        Ok((ticket, !self.closed))
    }
    fn slot(&mut self, ticket: &protocol::ProviderDrainTicket) -> Result<&mut Slot> {
        ensure!(
            ticket.helper == self.helper && ticket.worker.service == self.service,
            "provider retirement helper identity changed"
        );
        let slot = self
            .slots
            .get_mut(&ticket.ticket)
            .context("provider retirement ticket is not owned")?;
        ensure!(
            slot.ticket == *ticket,
            "provider retirement ticket identity mismatch"
        );
        Ok(slot)
    }
    pub(super) fn poll(
        &mut self,
        ticket: &protocol::ProviderDrainTicket,
    ) -> Result<(ProviderDrainState, bool)> {
        let closed = self.closed;
        let slot = self.slot(ticket)?;
        if slot.exchange.is_some_and(|(kind, sent)| {
            kind == Exchange::Observe && sent.elapsed() >= Duration::from_secs(5)
        }) {
            slot.state = ProviderDrainState::Unknown;
        }
        let send = !closed && slot.state == ProviderDrainState::Pending && slot.exchange.is_none();
        if send {
            slot.exchange = Some((Exchange::Observe, Instant::now()));
        }
        Ok((slot.state, send))
    }
    pub(super) fn forget(&mut self, ticket: &protocol::ProviderDrainTicket) -> Result<bool> {
        let slot = self.slot(ticket)?;
        ensure!(
            slot.state == ProviderDrainState::Drained,
            "provider retirement has no engine completion acknowledgment"
        );
        if slot.exchange.is_some() {
            return Ok(false);
        }
        slot.exchange = Some((Exchange::Forget, Instant::now()));
        Ok(true)
    }
    pub(super) fn failed_write(&mut self, ticket: &protocol::ProviderDrainTicket) {
        if let Ok(slot) = self.slot(ticket)
            && slot.state != ProviderDrainState::Drained
        {
            slot.state = ProviderDrainState::Unknown;
        }
    }
    pub(super) fn observe(
        &mut self,
        ticket: protocol::ProviderDrainTicket,
        observation: protocol::ProviderDrainObservation,
    ) {
        let Ok(slot) = self.slot(&ticket) else {
            return;
        };
        let Some((exchange, _)) = slot.exchange else {
            return;
        };
        use protocol::ProviderDrainObservation as O;
        match (exchange, observation) {
            (Exchange::Observe, O::Pending) => {
                slot.exchange = None;
                slot.state = ProviderDrainState::Pending;
            }
            (Exchange::Observe, O::Drained) => {
                slot.exchange = None;
                slot.state = ProviderDrainState::Drained;
            }
            (Exchange::Forget, O::Forgotten) => {
                self.slots.remove(&ticket.ticket);
                self.members.remove(&ticket.worker.worker);
            }
            (_, O::Refused) => {
                slot.exchange = None;
                if slot.state != ProviderDrainState::Drained {
                    slot.state = ProviderDrainState::Unknown;
                }
            }
            _ => { /* An unsolicited or wrong-phase reply is never a proof. */ }
        }
    }
    pub(super) fn closed(&mut self) {
        self.closed = true;
        for slot in self.slots.values_mut() {
            if slot.state != ProviderDrainState::Drained {
                slot.state = ProviderDrainState::Unknown;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use airk_core::service::WorkerIdentity;
    fn identity(member: u8) -> WorkerIdentity {
        WorkerIdentity {
            service: Incarnation::from_bytes([1; 16]),
            worker: Incarnation::from_bytes([member; 16]),
            project: Sha256Digest::from_bytes([member; 32]),
            fencing_generation: 1,
        }
    }
    fn registry() -> Drains {
        Drains::new(identity(3).service, Incarnation::from_bytes([2; 16]))
    }

    #[test]
    fn canceled_or_expired_waiter_retains_ticket_until_exact_engine_ack() -> Result<()> {
        let mut drains = registry();
        let worker = identity(3);
        drains.registered(worker);
        let (ticket, send) = drains.begin(worker, Incarnation::from_bytes([4; 16]))?;
        assert!(send);
        assert_eq!(drains.ticket(worker)?, Some(ticket));
        assert!(drains.forget(&ticket).is_err());
        let slot = drains.slots.get_mut(&ticket.ticket).unwrap();
        slot.exchange = Some((Exchange::Observe, Instant::now() - Duration::from_secs(6)));
        assert_eq!(drains.poll(&ticket)?, (ProviderDrainState::Unknown, false));
        assert_eq!(
            drains.begin(worker, Incarnation::from_bytes([5; 16]))?,
            (ticket, false)
        );
        assert_eq!(drains.slots.len(), 1);
        drains.observe(
            protocol::ProviderDrainTicket {
                helper: Incarnation::from_bytes([9; 16]),
                ..ticket
            },
            protocol::ProviderDrainObservation::Drained,
        );
        assert_eq!(drains.poll(&ticket)?.0, ProviderDrainState::Unknown);
        drains.observe(ticket, protocol::ProviderDrainObservation::Drained);
        assert_eq!(drains.poll(&ticket)?, (ProviderDrainState::Drained, false));
        assert!(drains.forget(&ticket)?);
        assert_eq!(drains.slots.len(), 1, "writing Forget is not a receipt");
        drains.observe(ticket, protocol::ProviderDrainObservation::Forgotten);
        assert!(drains.ticket(worker)?.is_none());
        Ok(())
    }

    #[test]
    fn helper_eof_write_failure_and_foreign_identities_never_prove_drain() -> Result<()> {
        let mut drains = registry();
        let worker = identity(3);
        drains.registered(worker);
        let (ticket, _) = drains.begin(worker, Incarnation::from_bytes([4; 16]))?;
        let foreign = protocol::ProviderDrainTicket {
            worker: WorkerIdentity {
                fencing_generation: 2,
                ..worker
            },
            ..ticket
        };
        drains.observe(foreign, protocol::ProviderDrainObservation::Drained);
        assert!(drains.poll(&foreign).is_err());
        assert!(
            drains
                .begin(foreign.worker, Incarnation::from_bytes([5; 16]))
                .is_err()
        );
        drains.failed_write(&ticket);
        assert_eq!(drains.poll(&ticket)?, (ProviderDrainState::Unknown, false));
        drains.closed();
        assert_eq!(drains.poll(&ticket)?, (ProviderDrainState::Unknown, false));
        assert!(drains.forget(&ticket).is_err());
        assert_eq!(drains.slots.len(), 1);
        Ok(())
    }

    #[test]
    fn pending_ticket_capacity_is_fixed_and_only_explicit_acknowledged_forget_reclaims()
    -> Result<()> {
        let mut drains = registry();
        let mut tickets = Vec::new();
        for member in 3..35 {
            let worker = identity(member);
            drains.validate_member(worker)?;
            drains.registered(worker);
            tickets.push(
                drains
                    .begin(worker, Incarnation::from_bytes([member; 16]))?
                    .0,
            );
        }
        assert_eq!(drains.slots.len(), protocol::MAX_PROVIDER_DRAINS);
        assert!(drains.validate_member(identity(35)).is_err());
        let ticket = tickets[0];
        assert!(drains.validate_member(ticket.worker).is_err());
        drains.observe(ticket, protocol::ProviderDrainObservation::Drained);
        drains.forget(&ticket)?;
        assert!(drains.validate_member(identity(35)).is_err());
        drains.observe(ticket, protocol::ProviderDrainObservation::Forgotten);
        drains.validate_member(identity(35))?;
        assert_eq!(drains.slots.len(), protocol::MAX_PROVIDER_DRAINS - 1);
        Ok(())
    }
}
