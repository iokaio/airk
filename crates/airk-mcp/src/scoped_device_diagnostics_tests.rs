// SPDX-License-Identifier: Apache-2.0
//! Canonical SDK diagnostics with synthetic native metadata; no model qualification.
use super::*;
use airk_core::{
    arbiter::{device::*, *},
    identity::{Incarnation, Sha256Digest},
    read_attempt::{ReadAttemptBudget, ReadClock},
    service::{ScopedReadRef, WorkerIdentity},
};
use airk_kernel::arbiter::NativeController;
use anyhow::{Context, Result};
use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};
struct SnapshotOwner {
    calls: AtomicUsize,
    refuse: bool,
    foreign: bool,
}
impl NativeController for SnapshotOwner {
    fn application(
        &self,
        worker: WorkerIdentity,
        authority: ScopedReadRef,
        publication: &airk_core::evidence::RevisionEnvelope,
    ) -> Result<IndexApplication> {
        NativeController::application(
            &crate::arbiter_handler_tests::Disabled,
            worker,
            authority,
            publication,
        )
    }
    fn device_read_status(
        &self,
        _: WorkerIdentity,
        authority: ScopedReadRef,
        publication: &airk_core::evidence::RevisionEnvelope,
        attempt: ReadAttemptBudget,
        deadline: Instant,
    ) -> Result<Option<ScopedDeviceStatus>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.refuse {
            return Err(airk_core::scoped_reads::ScopeReadRefused.into());
        }
        anyhow::ensure!(
            deadline > Instant::now() && attempt.request_id == Incarnation::from_bytes([31; 16]),
            "original read clock changed"
        );
        let cases: serde_json::Value = serde_json::from_slice(include_bytes!(
            "../../../fixtures/split/arbiter/device_cases.json"
        ))?;
        let status: DeviceStatus = serde_json::from_value(cases["cases"][4]["status"].clone())?;
        let scope = ScopeBinding {
            installation: status.configuration.installation,
            catalog: status.configuration.catalog,
            index_id: authority.index_id,
            authorization: authority.fingerprint,
            configuration_revision: status.configuration.revision,
            publication: publication.clone(),
        };
        let mut value = ScopedDeviceStatus::from_status(status, scope)?;
        if self.foreign {
            value.scope.authorization = Sha256Digest::from_bytes([99; 32]);
        }
        Ok(Some(value))
    }
}
fn attempt(expired: bool) -> Result<read_attempt::ReadAttemptContext> {
    let now = Instant::now();
    let deadline = if expired {
        now
    } else {
        now + Duration::from_secs(30)
    };
    read_attempt::ReadAttemptContext::new(deadline, Default::default()).with_wire_attempt(
        ReadAttemptBudget {
            schema: 1,
            request_id: Incarnation::from_bytes([31; 16]),
            clock: if cfg!(windows) {
                ReadClock::WindowsPerformanceCounter
            } else {
                ReadClock::UnixMonotonic
            },
            expires_monotonic_ns: 30_000_000_000,
            remaining_ns: 30_000_000_000,
            native_retry_expires_monotonic_ns: None,
        },
    )
}
fn fixture(owner: Arc<dyn NativeController>) -> Result<(tempfile::TempDir, Kernel)> {
    let directory = tempfile::Builder::new().tempdir_in(std::env::temp_dir().canonicalize()?)?;
    let root = directory.path().join("source");
    std::fs::create_dir_all(root.join("public"))?;
    std::fs::write(
        root.join("public/read.txt"),
        "original scoped diagnostic source\n",
    )?;
    let base = Kernel::open(&root, Some(directory.path().join("data")))?;
    base.configure_vectors(false)?;
    base.index()?;
    let mut authority = crate::scoped_observation_tests::authority(&base)?;
    authority.atoms[0].1.path.clear();
    let kernel = base.with_selection_reads(authority.clone())?;
    let reference = ScopedReadRef {
        schema: 1,
        lease_generation: 1,
        index_id: authority.index_id,
        selection: airk_core::scoped_reads::SelectionScopeRef {
            owner: airk_core::scoped_reads::SelectionOwnerKind::Window,
            grant_nonce: airk_core::identity::Incarnation::from_bytes([10; 16]),
            service_instance: airk_core::identity::Incarnation::from_bytes([11; 16]),
            scope_identity: airk_core::identity::Sha256Digest::from_bytes([12; 32]),
            catalog_revision: airk_core::index_catalog::CatalogRevision::try_from(5_u64).unwrap(),
        },
        fingerprint: kernel.scoped_read_fingerprint()?.context("scope absent")?,
    };
    let worker = WorkerIdentity {
        service: Incarnation::from_bytes([8; 16]),
        worker: Incarnation::from_bytes([9; 16]),
        project: authority.backing,
        fencing_generation: 1,
    };
    Ok((
        directory,
        kernel.with_native_arbiter(worker, reference, owner)?,
    ))
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn diagnostics_sdk_keeps_complete_precision_fallback_while_scalar_status_never_calls_owner()
-> Result<()> {
    let owner = Arc::new(SnapshotOwner {
        calls: AtomicUsize::new(0),
        refuse: false,
        foreign: false,
    });
    let (root, kernel) = fixture(owner.clone())?;
    let mut tools = local::LocalTools::new(kernel).await?;
    let args = serde_json::json!({"response_mode":"full"})
        .as_object()
        .unwrap()
        .clone();
    let scalar = tools
        .call_with_read_context("airk_status".into(), args.clone(), attempt(false)?)
        .await?;
    assert!(
        crate::arbiter_handler_tests::body(&scalar)?
            .get("scoped_device_status")
            .is_none()
    );
    assert_eq!(owner.calls.load(Ordering::SeqCst), 0);
    let reply = tools
        .call_with_read_context("airk_diagnostics".into(), args, attempt(false)?)
        .await?;
    let body = crate::arbiter_handler_tests::body(&reply)?;
    let cases: serde_json::Value = serde_json::from_slice(include_bytes!(
        "../../../fixtures/split/arbiter/device_cases.json"
    ))?;
    for stage in ["focus", "context"] {
        assert_eq!(
            body["scoped_device_status"]["observation"][stage]["status"],
            cases["cases"][4]["status"][stage]
        );
    }
    assert_eq!(owner.calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        std::fs::read_to_string(root.path().join("source/public/read.txt"))?,
        "original scoped diagnostic source\n"
    );
    tools.close().await?;
    Ok(())
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn diagnostics_sdk_preserves_scope_refusal_and_rejects_foreign_snapshot() -> Result<()> {
    for (refuse, foreign) in [(true, false), (false, true)] {
        let owner = Arc::new(SnapshotOwner {
            calls: AtomicUsize::new(0),
            refuse,
            foreign,
        });
        let (_root, kernel) = fixture(owner.clone())?;
        let mut tools = local::LocalTools::new(kernel).await?;
        let reply = tools
            .call_with_read_context(
                "airk_diagnostics".into(),
                serde_json::json!({"response_mode":"full"})
                    .as_object()
                    .unwrap()
                    .clone(),
                attempt(false)?,
            )
            .await?;
        assert_eq!(reply.is_error, Some(true));
        let refusal: serde_json::Value = serde_json::from_str(
            &reply.content[0]
                .as_text()
                .context("missing exact native refusal")?
                .text,
        )?;
        assert_eq!(
            refusal["error"],
            serde_json::json!({"schema":1,"code":"scoped_read_unavailable","retryable":false})
        );
        assert_eq!(owner.calls.load(Ordering::SeqCst), 1);
        tools.close().await?;
    }
    Ok(())
}
/// An owner whose device read lets a new publication land under the same
/// authority, as an Index's semantic step does just after its text step.
struct RepublishingOwner {
    snapshot: SnapshotOwner,
    /// How many device reads republish before answering.
    republish: usize,
    root: std::sync::OnceLock<std::path::PathBuf>,
}
impl NativeController for RepublishingOwner {
    fn application(
        &self,
        worker: WorkerIdentity,
        authority: ScopedReadRef,
        publication: &airk_core::evidence::RevisionEnvelope,
    ) -> Result<IndexApplication> {
        self.snapshot.application(worker, authority, publication)
    }
    fn device_read_status(
        &self,
        worker: WorkerIdentity,
        authority: ScopedReadRef,
        publication: &airk_core::evidence::RevisionEnvelope,
        attempt: ReadAttemptBudget,
        deadline: Instant,
    ) -> Result<Option<ScopedDeviceStatus>> {
        let call = self.snapshot.calls.load(Ordering::SeqCst);
        if call < self.republish {
            let root = self.root.get().context("fixture root not set")?;
            std::fs::write(
                root.join("source/public/read.txt"),
                format!("republished scoped diagnostic source {call}\n"),
            )?;
            Kernel::open(root.join("source"), Some(root.join("data")))?.index()?;
        }
        self.snapshot
            .device_read_status(worker, authority, publication, attempt, deadline)
    }
}
fn republishing(republish: usize) -> Arc<RepublishingOwner> {
    Arc::new(RepublishingOwner {
        snapshot: SnapshotOwner {
            calls: AtomicUsize::new(0),
            refuse: false,
            foreign: false,
        },
        republish,
        root: Default::default(),
    })
}
fn diagnostics_args() -> serde_json::Map<String, serde_json::Value> {
    serde_json::json!({"response_mode":"full"})
        .as_object()
        .unwrap()
        .clone()
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn diagnostics_observes_again_when_a_publication_lands_during_the_device_read() -> Result<()>
{
    let owner = republishing(1);
    let (root, kernel) = fixture(owner.clone())?;
    owner.root.set(root.path().to_path_buf()).unwrap();
    let mut tools = local::LocalTools::new(kernel).await?;
    let reply = tools
        .call_with_read_context(
            "airk_diagnostics".into(),
            diagnostics_args(),
            attempt(false)?,
        )
        .await?;
    // Same authority, newer publication: never `scoped_read_unavailable`.
    assert_ne!(reply.is_error, Some(true), "{reply:?}");
    let body = crate::arbiter_handler_tests::body(&reply)?;
    let observed = body["scoped_device_status"]["scope"]["publication"].clone();
    assert!(observed.is_object(), "{body}");
    assert_eq!(owner.snapshot.calls.load(Ordering::SeqCst), 2);
    // The observation is of the publication that landed: an undisturbed
    // read now observes the same one.
    let settled = tools
        .call_with_read_context(
            "airk_diagnostics".into(),
            diagnostics_args(),
            attempt(false)?,
        )
        .await?;
    let settled = crate::arbiter_handler_tests::body(&settled)?;
    assert_eq!(
        settled["scoped_device_status"]["scope"]["publication"],
        observed
    );
    assert_eq!(owner.snapshot.calls.load(Ordering::SeqCst), 3);
    tools.close().await?;
    Ok(())
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn diagnostics_with_a_publication_moving_on_every_read_is_retryable_busy() -> Result<()> {
    let owner = republishing(usize::MAX);
    let (root, kernel) = fixture(owner.clone())?;
    owner.root.set(root.path().to_path_buf()).unwrap();
    let mut tools = local::LocalTools::new(kernel).await?;
    let reply = tools
        .call_with_read_context(
            "airk_diagnostics".into(),
            diagnostics_args(),
            attempt(false)?,
        )
        .await?;
    assert_eq!(reply.is_error, Some(true));
    let refusal: serde_json::Value = serde_json::from_str(
        &reply.content[0]
            .as_text()
            .context("missing typed retry")?
            .text,
    )?;
    assert_eq!(refusal["code"], "publication_busy", "{refusal}");
    assert_eq!(refusal["retryable"], true);
    assert_eq!(refusal["action"], "repeat_same_request");
    assert_eq!(owner.snapshot.calls.load(Ordering::SeqCst), 3);
    tools.close().await?;
    Ok(())
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn diagnostics_expired_original_attempt_never_submits_native_snapshot() -> Result<()> {
    let owner = Arc::new(SnapshotOwner {
        calls: AtomicUsize::new(0),
        refuse: false,
        foreign: false,
    });
    let (_root, kernel) = fixture(owner.clone())?;
    let mut tools = local::LocalTools::new(kernel).await?;
    let reply = tools
        .call_with_read_context(
            "airk_diagnostics".into(),
            Default::default(),
            attempt(true)?,
        )
        .await?;
    assert_eq!(reply.is_error, Some(true));
    assert_eq!(owner.calls.load(Ordering::SeqCst), 0);
    tools.close().await?;
    Ok(())
}
