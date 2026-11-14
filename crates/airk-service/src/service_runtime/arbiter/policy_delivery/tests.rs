// SPDX-License-Identifier: Apache-2.0
use super::*;
use airk_core::{
    arbiter::Enrollment,
    identity::{Incarnation, RevisionToken},
    index_catalog::IndexId,
};
use airk_store::service::{ServiceLease, arbiter::ConfigurationStore};
use airk_mcp::read_attempt::ReadAttemptContext;

fn fixture() -> Result<(
    tempfile::TempDir,
    Arc<ConfigurationStore>,
    ContextDeliveryBinding,
)> {
    let root = tempfile::tempdir()?;
    let installation = Sha256Digest::from_bytes([1; 32]);
    let catalog = Incarnation::from_bytes([2; 16]);
    let index = IndexId::from_identity(Incarnation::from_bytes([3; 16]));
    let profile = Sha256Digest::from_bytes([7; 32]);
    let store = Arc::new(ConfigurationStore::open(
        Arc::new(ServiceLease::acquire(root.path())?),
        installation,
        catalog,
    )?);
    let mut current = Configuration::disabled(installation, catalog);
    current.enabled = true;
    current.revision = 2;
    current.selected_profile = Some(profile);
    current.indexes = vec![Enrollment {
        index_id: index,
        tasks: BTreeSet::from([Task::ContextAction]),
    }];
    store.apply(1, &current)?;
    let binding = ContextDeliveryBinding {
        profile: Some(profile),
        scope: ScopeBinding {
            installation,
            catalog,
            index_id: index,
            authorization: Sha256Digest::from_bytes([4; 32]),
            configuration_revision: 2,
            publication: airk_core::evidence::RevisionEnvelope::new(
                "fixture".into(),
                RevisionToken {
                    incarnation: Incarnation::from_bytes([5; 16]),
                    generation: 1,
                    revision: Sha256Digest::from_bytes([6; 32]),
                },
            ),
        },
    };
    Ok((root, store, binding))
}
fn guard(store: Arc<ConfigurationStore>, binding: ContextDeliveryBinding) -> PolicyDelivery {
    PolicyDelivery {
        configuration: Some(store),
        expected: vec![binding],
        digest: Sha256Digest::from_bytes([9; 32]),
        deadline: Instant::now() + Duration::from_secs(5),
        cancellation: Default::default(),
        service_live: Arc::new(|| true),
    }
}
#[test]
fn disable_or_switch_while_another_member_is_pending_refuses_cached_advice() -> Result<()> {
    for switch in [false, true] {
        let (_root, store, binding) = fixture()?;
        let policy = guard(store.clone(), binding);
        let (entered_tx, entered_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let pending = std::thread::spawn(move || {
            entered_tx.send(()).unwrap();
            release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            let mut polls = 0;
            let result = policy.poll(&mut || {
                polls += 1;
                Poll::Ready(Ok(1))
            });
            (result, polls)
        });
        entered_rx.recv_timeout(Duration::from_secs(5))?;
        let mut changed = store.load()?;
        changed.revision += 1;
        if switch {
            changed.selected_profile = Some(Sha256Digest::from_bytes([8; 32]));
        } else {
            changed.enabled = false;
        }
        store.apply(2, &changed)?;
        release_tx.send(())?;
        let (result, polls) = pending.join().unwrap();
        assert!(matches!(result, Poll::Ready(Err(_))));
        assert_eq!(polls, 0);
    }
    Ok(())
}
#[tokio::test]
async fn actual_outer_pipe_partial_write_closes_after_policy_revocation_without_fallback()
-> Result<()> {
    use tokio::io::AsyncReadExt;
    let (_root, store, binding) = fixture()?;
    let context =
        ReadAttemptContext::new(Instant::now() + Duration::from_secs(5), Default::default());
    context.retain_delivery_guard(Arc::new(guard(store.clone(), binding)))?;
    let (mut reader, mut raw) = tokio::io::duplex(1);
    let writing = tokio::spawn(async move {
        let mut writer = crate::service_runtime::policy_writer::PolicyWriter {
            inner: &mut raw,
            context: &context,
        };
        airk_ipc::endpoint::write_message(
            &mut writer,
            &ControlResponse::Failure {
                code: "synthetic-private-body".into(),
            },
            Duration::from_secs(5),
        )
        .await
    });
    let mut first = [0u8; 1];
    reader.read_exact(&mut first).await?;
    let mut changed = store.load()?;
    changed.revision += 1;
    changed.enabled = false;
    store.apply(2, &changed)?;
    let mut remainder = Vec::new();
    reader.read_to_end(&mut remainder).await?;
    assert!(writing.await?.is_err());
    assert!(
        1 + remainder.len() < 4,
        "a revoked partial frame must not become a complete response"
    );
    Ok(())
}
#[test]
fn original_cancel_or_expiry_refuses_even_without_model_or_optional_state() -> Result<()> {
    let root = tempfile::tempdir()?;
    let mut policy = PolicyDelivery {
        configuration: None,
        expected: Vec::new(),
        digest: Sha256Digest::from_bytes([9; 32]),
        deadline: Instant::now() + Duration::from_secs(5),
        cancellation: Default::default(),
        service_live: Arc::new(|| true),
    };
    let mut calls = 0;
    assert!(matches!(
        policy.poll(&mut || {
            calls += 1;
            Poll::Ready(Ok(1))
        }),
        Poll::Ready(Ok(1))
    ));
    policy.cancellation.cancel();
    assert!(matches!(
        policy.poll(&mut || {
            calls += 1;
            Poll::Ready(Ok(1))
        }),
        Poll::Ready(Err(_))
    ));
    policy.cancellation = Default::default();
    policy.deadline = Instant::now();
    assert!(matches!(
        policy.poll(&mut || {
            calls += 1;
            Poll::Ready(Ok(1))
        }),
        Poll::Ready(Err(_))
    ));
    assert_eq!(calls, 1);
    assert!(!root.path().join("arbiter-v2").exists());
    Ok(())
}
#[test]
fn cancellation_after_outer_policy_check_is_rechecked_inside_actual_source_cut() -> Result<()> {
    use airk_mcp::read_attempt::{ReadSourceDeliveryGuard, ReadSourceGuardKind};
    use std::{
        pin::Pin,
        sync::atomic::{AtomicUsize, Ordering},
        task::{Context as TaskContext, Waker},
    };
    use tokio::io::AsyncWrite;
    struct CancelAtSource(tokio_util::sync::CancellationToken);
    impl ReadSourceDeliveryGuard for CancelAtSource {
        fn kind(&self) -> ReadSourceGuardKind {
            ReadSourceGuardKind::Selection
        }
        fn identity(&self) -> Sha256Digest {
            Sha256Digest::from_bytes([1; 32])
        }
        fn check(&self) -> std::io::Result<()> {
            panic!("source gate must not be reentered by final liveness check")
        }
        fn poll(
            &self,
            effect: &mut dyn FnMut() -> Poll<std::io::Result<usize>>,
        ) -> Poll<std::io::Result<usize>> {
            self.0.cancel();
            effect()
        }
    }
    struct Count(Arc<AtomicUsize>);
    impl AsyncWrite for Count {
        fn poll_write(
            self: Pin<&mut Self>,
            _: &mut TaskContext<'_>,
            bytes: &[u8],
        ) -> Poll<std::io::Result<usize>> {
            self.0.fetch_add(1, Ordering::AcqRel);
            Poll::Ready(Ok(bytes.len()))
        }
        fn poll_flush(self: Pin<&mut Self>, _: &mut TaskContext<'_>) -> Poll<std::io::Result<()>> {
            Poll::Ready(Ok(()))
        }
        fn poll_shutdown(
            self: Pin<&mut Self>,
            _: &mut TaskContext<'_>,
        ) -> Poll<std::io::Result<()>> {
            Poll::Ready(Ok(()))
        }
    }
    let context =
        ReadAttemptContext::new(Instant::now() + Duration::from_secs(5), Default::default());
    context
        .retain_source_delivery_guard(Arc::new(CancelAtSource(context.cancellation().clone())))?;
    let calls = Arc::new(AtomicUsize::new(0));
    let mut raw = Count(calls.clone());
    let mut writer = crate::service_runtime::policy_writer::PolicyWriter {
        inner: &mut raw,
        context: &context,
    };
    let mut cx = TaskContext::from_waker(Waker::noop());
    assert!(matches!(
        Pin::new(&mut writer).poll_write(&mut cx, b"synthetic"),
        Poll::Ready(Err(_))
    ));
    assert_eq!(calls.load(Ordering::Acquire), 0);
    Ok(())
}
