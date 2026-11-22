// SPDX-License-Identifier: Apache-2.0
//! Bounded waiting before the existing broker accepts any query work.
use airk_core::service::AdmissionError;
use std::{future::Future, sync::Arc};
use tokio::sync::{Mutex, Notify, Semaphore};

pub(super) struct QueryAdmission {
    waiting: Arc<Semaphore>,
    turn: Mutex<()>,
    changed: Notify,
    closed: std::sync::atomic::AtomicBool,
}
impl Default for QueryAdmission {
    fn default() -> Self {
        Self {
            waiting: Arc::new(Semaphore::new(128)),
            turn: Mutex::new(()),
            changed: Notify::new(),
            closed: std::sync::atomic::AtomicBool::new(false),
        }
    }
}
impl QueryAdmission {
    pub(super) fn close(&self) {
        self.closed
            .store(true, std::sync::atomic::Ordering::Release);
        self.waiting.close();
        self.changed.notify_waiters();
    }
    pub(super) fn completed(&self) {
        self.changed.notify_waiters();
    }
    pub(super) async fn while_open<T>(
        &self,
        peer: &mut (impl tokio::io::AsyncRead + Unpin),
        expires: tokio::time::Instant,
        attempt: impl FnMut() -> Result<T, AdmissionError>,
    ) -> anyhow::Result<Option<Result<T, AdmissionError>>> {
        use tokio::io::AsyncReadExt;
        let mut unexpected = [0; 1];
        tokio::select! {
            biased;
            received = peer.read(&mut unexpected) => {
                anyhow::ensure!(received? == 0, "native query pipelined before admission completed");
                Ok(None)
            }
            result = self.admit(expires, attempt) => Ok(Some(result)),
        }
    }
    pub(super) async fn admit<T>(
        &self,
        expires: tokio::time::Instant,
        mut attempt: impl FnMut() -> Result<T, AdmissionError>,
    ) -> Result<T, AdmissionError> {
        let waiting = self.waiting.clone().try_acquire_owned().map_err(|error| {
            if error == tokio::sync::TryAcquireError::Closed {
                AdmissionError::Draining
            } else {
                AdmissionError::Busy
            }
        })?;
        // One FIFO waiter retries broker admission; other bounded handlers keep
        // no broker lock. No accepted query is re-executed by this queue.
        let admission = async {
            let _turn = self.turn.lock().await;
            loop {
                // Enable before inspecting capacity to avoid losing a finish
                // notification between a Busy observation and awaiting it.
                let changed = self.changed.notified();
                tokio::pin!(changed);
                changed.as_mut().enable();
                if self.closed.load(std::sync::atomic::Ordering::Acquire) {
                    return Err(AdmissionError::Draining);
                }
                if tokio::time::Instant::now() >= expires {
                    return Err(AdmissionError::Busy);
                }
                match attempt() {
                    Err(AdmissionError::Busy) => changed.await,
                    result => return result,
                }
            }
        };
        let result = before(expires, admission).await;
        drop(waiting);
        result
    }
}

async fn before<T>(
    expires: tokio::time::Instant,
    admission: impl Future<Output = Result<T, AdmissionError>>,
) -> Result<T, AdmissionError> {
    tokio::time::timeout_at(expires, admission)
        .await
        .unwrap_or(Err(AdmissionError::Busy))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    fn poll_pending<T>(future: std::pin::Pin<&mut impl Future<Output = T>>) {
        let mut context = std::task::Context::from_waker(std::task::Waker::noop());
        assert!(future.poll(&mut context).is_pending());
    }

    #[tokio::test]
    async fn released_capacity_admits_one_attempt_without_replaying_execution() {
        let queue = QueryAdmission::default();
        let ready = AtomicBool::new(false);
        let attempts = AtomicUsize::new(0);
        let call = queue.admit(
            tokio::time::Instant::now() + std::time::Duration::from_secs(1),
            || {
                attempts.fetch_add(1, Ordering::SeqCst);
                if ready.load(Ordering::SeqCst) {
                    Ok(17)
                } else {
                    Err(AdmissionError::Busy)
                }
            },
        );
        tokio::pin!(call);
        poll_pending(call.as_mut());
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
        ready.store(true, Ordering::SeqCst);
        queue.completed();
        assert_eq!(call.await.unwrap(), 17);
        assert_eq!(attempts.load(Ordering::SeqCst), 2);
        assert_eq!(queue.waiting.available_permits(), 128);
    }

    #[tokio::test]
    async fn pending_cap_cancel_close_and_expiry_never_accept_work() {
        let queue = QueryAdmission::default();
        let expires = tokio::time::Instant::now() + std::time::Duration::from_secs(1);
        let mut held: Vec<_> = (0..128)
            .map(|_| Box::pin(queue.admit(expires, || Err::<(), _>(AdmissionError::Busy))))
            .collect();
        for call in &mut held {
            poll_pending(call.as_mut());
        }
        let accepted = AtomicUsize::new(0);
        assert_eq!(
            queue
                .admit(expires, || {
                    accepted.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                })
                .await,
            Err(AdmissionError::Busy)
        );
        drop(held.pop());
        assert_eq!(queue.waiting.available_permits(), 1);
        assert_eq!(
            queue
                .admit(tokio::time::Instant::now(), || {
                    accepted.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                })
                .await,
            Err(AdmissionError::Busy)
        );
        queue.close();
        for call in held {
            assert_eq!(call.await, Err(AdmissionError::Draining));
        }
        assert_eq!(accepted.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn queued_turn_revalidates_and_never_retries_permanent_refusals() {
        let queue = QueryAdmission::default();
        let stale = AtomicBool::new(false);
        let expires = tokio::time::Instant::now() + std::time::Duration::from_secs(1);
        let mut first = Box::pin(queue.admit(expires, || Err::<(), _>(AdmissionError::Busy)));
        poll_pending(first.as_mut());
        let mut second = Box::pin(queue.admit(expires, || {
            Err::<(), _>(if stale.load(Ordering::SeqCst) {
                AdmissionError::Expired
            } else {
                AdmissionError::Busy
            })
        }));
        poll_pending(second.as_mut());
        stale.store(true, Ordering::SeqCst);
        drop(first);
        assert_eq!(second.await, Err(AdmissionError::Expired));
        let open = QueryAdmission::default();
        let attempts = AtomicUsize::new(0);
        assert_eq!(
            open.admit(expires, || {
                attempts.fetch_add(1, Ordering::SeqCst);
                Err::<(), _>(AdmissionError::Expired)
            })
            .await,
            Err(AdmissionError::Expired)
        );
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn actual_peer_eof_retires_unsubmitted_wait_without_acceptance() {
        use tokio::io::AsyncWriteExt;
        let queue = QueryAdmission::default();
        let (mut client, mut server) = tokio::io::duplex(8);
        let attempts = AtomicUsize::new(0);
        let mut pending = Box::pin(queue.while_open(
            &mut server,
            tokio::time::Instant::now() + std::time::Duration::from_secs(1),
            || {
                attempts.fetch_add(1, Ordering::SeqCst);
                Err::<(), _>(AdmissionError::Busy)
            },
        ));
        poll_pending(pending.as_mut());
        assert_eq!(queue.waiting.available_permits(), 127);
        client.shutdown().await.unwrap();
        assert!(pending.await.unwrap().is_none());
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
        assert_eq!(queue.waiting.available_permits(), 128);
    }

    #[tokio::test]
    async fn actual_broker_capacity_is_rechecked_and_expired_sessions_never_admit()
    -> anyhow::Result<()> {
        use airk_core::{
            identity::Incarnation,
            service::{QueryAllowance, WorkerIdentity},
        };
        use airk_kernel::service_broker::{Broker, BrokerLimits};
        let identity = WorkerIdentity {
            service: Incarnation::from_bytes([71; 16]),
            worker: Incarnation::from_bytes([72; 16]),
            project: "ab".repeat(32).parse()?,
            fencing_generation: 1,
        };
        let caller = Incarnation::from_bytes([73; 16]);
        let now = std::time::Instant::now();
        let broker = std::sync::Mutex::new(Broker::new(identity.service, BrokerLimits::default())?);
        let (first, second, held) = {
            let mut policy = broker.lock().unwrap();
            policy.register(
                identity,
                QueryAllowance {
                    concurrent_queries: 1,
                    response_bytes: 1024,
                },
                now,
                std::time::Duration::from_secs(300),
            )?;
            let first_grant =
                policy.issue(identity, caller, now, std::time::Duration::from_secs(5))?;
            let first = policy.redeem(
                &first_grant,
                identity,
                caller,
                now,
                std::time::Duration::from_secs(60),
            )?;
            let second_grant =
                policy.issue(identity, caller, now, std::time::Duration::from_secs(5))?;
            let second = policy.redeem(
                &second_grant,
                identity,
                caller,
                now,
                std::time::Duration::from_secs(60),
            )?;
            let held = policy.begin_query(identity, first, 1024, now)?;
            (first, second, held)
        };
        let queue = QueryAdmission::default();
        let observed = std::cell::Cell::new(now);
        let expires = tokio::time::Instant::now() + std::time::Duration::from_secs(1);
        let mut pending = Box::pin(queue.admit(expires, || {
            broker
                .lock()
                .unwrap()
                .begin_query(identity, second, 1024, observed.get())
        }));
        poll_pending(pending.as_mut());
        assert_eq!(broker.lock().unwrap().status(now).active_queries, 1);
        broker.lock().unwrap().finish_query(identity, held)?;
        queue.completed();
        let admitted = pending.await?;
        assert_eq!(broker.lock().unwrap().status(now).active_queries, 1);
        assert_ne!(
            held, admitted,
            "admission creates one new query identity, not a replay"
        );
        let mut expired = Box::pin(queue.admit(expires, || {
            broker
                .lock()
                .unwrap()
                .begin_query(identity, first, 1024, observed.get())
        }));
        poll_pending(expired.as_mut());
        observed.set(now + std::time::Duration::from_secs(61));
        broker.lock().unwrap().finish_query(identity, admitted)?;
        queue.completed();
        assert_eq!(expired.await, Err(AdmissionError::Expired));
        assert_eq!(
            broker.lock().unwrap().status(observed.get()).active_queries,
            0
        );
        Ok(())
    }
}

#[cfg(test)]
mod admitted_lifetime_tests {
    use anyhow::{Context, Result};
    use std::time::{Duration, Instant};
    #[test]
    fn worker_decodes_original_captured_parent_without_connection_admission_narrowing() -> Result<()>
    {
        let now = Instant::now();
        let id = airk_core::identity::Incarnation::from_bytes([85; 16]);
        let parent = airk_mcp::read_attempt::ReadAttemptContext::new(
            now + Duration::from_secs(120),
            Default::default(),
        )
        .with_request_id(id)?;
        let wire = airk_host::read_attempt::encode(&parent, id)?;
        let decoded = super::super::query_read_context("airk_search", Some(wire), None, None)?
            .context("original read missing")?;
        let carried = decoded
            .captured_transport_attempt()
            .context("original absolute parent was narrowed/replaced")?;
        assert_eq!(carried.expires_monotonic_ns, wire.expires_monotonic_ns);
        assert_eq!(carried.request_id, id);
        assert_eq!(decoded.request_id(), Some(id));
        assert!(super::super::query_read_context("airk_search", None, None, None)?.is_none());
        Ok(())
    }
    #[test]
    fn worker_carries_an_answer_attempt_to_its_ceiling_and_narrows_other_tools() -> Result<()> {
        let now = Instant::now();
        let id = airk_core::identity::Incarnation::from_bytes([86; 16]);
        let parent = airk_mcp::read_attempt::ReadAttemptContext::new(
            now + Duration::from_secs(600),
            Default::default(),
        )
        .with_read_ceiling(airk_core::read_attempt::MAX_ANSWER_READ_BUDGET)
        .with_request_id(id)?;
        let wire = airk_host::read_attempt::encode(&parent, id)?;
        let answer = super::super::query_read_context("airk_answer", Some(wire), None, None)?
            .context("original read missing")?;
        assert!(answer.deadline() > now + Duration::from_secs(590));
        assert!(answer.captured_transport_attempt().is_some());
        let search = super::super::query_read_context("airk_search", Some(wire), None, None)?
            .context("original read missing")?;
        assert!(search.deadline() <= Instant::now() + Duration::from_secs(125));
        Ok(())
    }
    #[test]
    fn actual_broker_clock_keeps_accepted_capacity_after_60_second_expiry_and_refuses_new_work()
    -> Result<()> {
        use airk_core::{
            identity::Incarnation,
            service::{AdmissionError, QueryAllowance, WorkerIdentity},
        };
        use airk_kernel::service_broker::{Broker, BrokerLimits};
        let now = Instant::now();
        let identity = WorkerIdentity {
            service: Incarnation::from_bytes([86; 16]),
            worker: Incarnation::from_bytes([87; 16]),
            project: "ab".repeat(32).parse()?,
            fencing_generation: 1,
        };
        let caller = Incarnation::from_bytes([88; 16]);
        let mut broker = Broker::new(identity.service, BrokerLimits::default())?;
        broker.register(
            identity,
            QueryAllowance {
                concurrent_queries: 2,
                response_bytes: 2048,
            },
            now,
            Duration::from_secs(300),
        )?;
        let grant = broker.issue(identity, caller, now, Duration::from_secs(5))?;
        let session = broker.redeem(&grant, identity, caller, now, Duration::from_secs(60))?;
        let accepted = broker.begin_query(identity, session, 1024, now)?;
        let after_admission = now + Duration::from_secs(70);
        assert_eq!(
            broker.begin_query(identity, session, 1024, after_admission),
            Err(AdmissionError::Expired)
        );
        assert_eq!(broker.status(after_admission).active_queries, 1);
        broker.finish_query(identity, accepted)?;
        assert_eq!(broker.status(after_admission).active_queries, 0);
        assert_eq!(
            broker.begin_query(identity, session, 1024, after_admission),
            Err(AdmissionError::Expired)
        );
        Ok(())
    }
}
