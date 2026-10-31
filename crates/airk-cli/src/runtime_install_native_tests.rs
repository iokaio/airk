// SPDX-License-Identifier: Apache-2.0
//! Opt-in real-pair execution gate. Never uses a user's installed service.
use super::*;
use airk_core::{
    identity::Incarnation,
    service::RequestKey,
    service_lifecycle::{LifecycleAction, LifecycleRequest},
};

struct NativeFixture {
    _owner: tempfile::TempDir,
    root: PathBuf,
    control: PathBuf,
    identity: String,
}
impl NativeFixture {
    async fn new() -> Result<Self> {
        let source = PathBuf::from(
            std::env::var_os("AIRK_F03_RUNTIME")
                .context("AIRK_F03_RUNTIME must name a freshly built approved sealed pair")?,
        )
        .canonicalize()?;
        let approved = std::env::var("AIRK_F03_APPROVED_SHA256")
            .context("AIRK_F03_APPROVED_SHA256 must come from the reviewed fresh build")?;
        digest(&approved)?;
        ensure!(
            runtime_pair::digest_file(&source.join("airk-install.json"))? == approved,
            "fresh approved pair identity mismatch"
        );
        let owner = tempfile::tempdir()?;
        let root = owner.path().canonicalize()?.join("install");
        local_ipc::prepare_root(&root)?;
        layout::initialize(
            &root,
            Some(&owner.path().canonicalize()?.join("state")),
            RuntimeChannel::Stable,
        )?;
        stage(
            &source,
            &root,
            &approved,
            VerificationBasis::OrganizationDigest,
        )?;
        let control = layout::control(&root)?;
        let fixture = Self {
            _owner: owner,
            root,
            control,
            identity: approved,
        };
        // Keep ownership available for cleanup even if activation fails.
        if let Err(error) = activate(&fixture.root, &fixture.identity).await {
            return Err(match fixture.cleanup().await {
                Ok(()) => error,
                Err(cleanup) => {
                    error.context(format!("owned fixture cleanup also failed: {cleanup:#}"))
                }
            });
        }
        Ok(fixture)
    }
    async fn health(&self) -> Result<ServiceHealth> {
        let ControlResponse::Health { observation } = service_runtime::request_for_version(
            &self.control,
            ControlMethod::Health,
            env!("CARGO_PKG_VERSION"),
        )
        .await?
        else {
            bail!("native fixture returned another health response")
        };
        Ok(*observation)
    }
    fn begin(&self, action: LifecycleAction, instance: Incarnation) -> Result<()> {
        let mut ledger = airk_store::service::registry::ServiceRegistry::open(
            &self.control,
            airk_kernel::service_broker::new_instance_id()?,
        )?;
        let (namespace, epoch) = ledger.saved_request_epoch()?;
        ledger.begin_lifecycle(&LifecycleRequest {
            caller: airk_kernel::service_broker::new_instance_id()?,
            key: RequestKey {
                namespace,
                epoch,
                nonce: airk_kernel::service_broker::new_instance_id()?,
            },
            action,
            expected_instance: instance,
            runtime: self.identity.parse()?,
            manager: false,
        })?;
        Ok(())
    }
    async fn cleanup(&self) -> Result<()> {
        // Version/namespace are the freshly owned fixture's, never installed defaults.
        let _ = service_runtime::request_for_version(
            &self.control,
            ControlMethod::Shutdown,
            env!("CARGO_PKG_VERSION"),
        )
        .await;
        wait_quiescent(&self.root).await
    }
}

#[tokio::test]
#[ignore = "requires a freshly built sealed pair and explicit approved digest; run serially"]
async fn native_pending_same_selection_restart_uses_public_connection_and_preserves_incarnation()
-> Result<()> {
    let fixture = NativeFixture::new().await?;
    let result = async {
        let first = fixture.health().await?;
        fixture.begin(LifecycleAction::Restart, first.instance)?;
        ensure!(
            lifecycle::inspect(&fixture.root)?["pending"].is_object(),
            "pending record was not created"
        );
        let control = installed_control(&fixture.root).await?;
        ensure!(
            control == fixture.control,
            "connection changed saved control"
        );
        let second = fixture.health().await?;
        ensure!(
            second.instance != first.instance,
            "pending restart effects did not actually run"
        );
        ensure!(
            second.runtime_sha256.as_deref() == Some(fixture.identity.as_str()),
            "runtime changed"
        );
        ensure!(
            selection_record(&fixture.root)?.active == fixture.identity,
            "selection changed"
        );
        ensure!(
            lifecycle::inspect(&fixture.root)?["pending"].is_null(),
            "restart did not settle"
        );
        installed_control(&fixture.root).await?;
        ensure!(
            fixture.health().await?.instance == second.instance,
            "healthy reconnect restarted again"
        );
        Ok(())
    }
    .await;
    let cleanup = fixture.cleanup().await;
    result.and(cleanup)
}

#[tokio::test]
#[ignore = "requires a freshly built sealed pair and explicit approved digest; run serially"]
async fn native_pending_effects_requalify_ancillary_bytes_after_proof_discard() -> Result<()> {
    for activation in [false, true] {
        let fixture = NativeFixture::new().await?;
        let result = async {
            let first = fixture.health().await?;
            if activation {
                // A real, valid Probe checkpoint already committed to the same selection.
                // Prior=None is valid for interrupted first activation; replay must settle it.
                replace_json(
                    &fixture.root,
                    "activation.json",
                    &serde_json::json!({
                        "schema":1,"install":fixture.root.canonicalize()?,"control":fixture.control,
                        "candidate":fixture.identity,"prior":null,"manager":false,"phase":"probe"
                    }),
                )?;
                ensure!(
                    activation::pending(&fixture.root)?.is_some(),
                    "activation checkpoint invalid"
                );
            } else {
                fixture.begin(LifecycleAction::Restart, first.instance)?;
            }
            let preflight = selected_preflight(&fixture.root)?;
            let lock = install_lock(&fixture.root)?;
            let authority = InstallerAuthority {
                root: &fixture.root,
                _lock: &lock,
            };
            ensure!(
                matches!(
                    prepare_connection(&authority, &preflight)?,
                    PreparedConnection::SettlementRequired
                ),
                "pending work retained an execution proof"
            );
            let license = fixture
                .root
                .join("runtimes")
                .join(&fixture.identity)
                .join("LICENSE");
            let original = fs::read(&license)?;
            let mut boundary_reached = false;
            let operation = ensure_running_locked_after_settlement(&authority, || {
                ensure!(
                    lifecycle::inspect(&fixture.root)?["pending"].is_null(),
                    "lifecycle effects remain pending"
                );
                ensure!(
                    activation::pending(&fixture.root)?.is_none(),
                    "activation effects remain pending"
                );
                ensure!(
                    selection_record(&fixture.root)?.active == fixture.identity,
                    "same-selection fixture changed"
                );
                boundary_reached = true;
                fs::write(
                    &license,
                    b"Original native fixture damage AFTER real settlement effects",
                )?;
                Ok(())
            })
            .await;
            // Restore before propagating assertions so owned service cleanup retains approved bytes.
            fs::write(&license, &original)?;
            ensure!(
                boundary_reached,
                "failed before post-effects verification boundary"
            );
            let error = operation
                .err()
                .context("post-effects damage escaped replacement verification")?;
            ensure!(
                format!("{error:#}").contains("LICENSE"),
                "unexpected post-effects failure: {error:#}"
            );
            let current = fixture.health().await?;
            if !activation {
                ensure!(
                    current.instance != first.instance,
                    "restart did not run before injected damage"
                );
            }
            ensure!(
                current.runtime_sha256.as_deref() == Some(fixture.identity.as_str()),
                "settlement changed runtime"
            );
            // Authority is no longer borrowed here; release its actual held lock
            // before a new public operation acquires that lock again.
            drop(lock);
            // Restored bytes now permit the actual public operation, preserving the settled instance.
            installed_control(&fixture.root).await?;
            ensure!(
                fixture.health().await?.instance == current.instance,
                "recovery restarted settled service"
            );
            Ok(())
        }
        .await;
        let cleanup = fixture.cleanup().await;
        result.and(cleanup)?;
    }
    Ok(())
}

#[tokio::test]
#[ignore = "requires a freshly built sealed pair and explicit approved digest; run serially"]
async fn native_direct_ensure_settles_stop_with_missing_ancillary_bytes() -> Result<()> {
    let fixture = NativeFixture::new().await?;
    let result = async {
        let first = fixture.health().await?;
        fixture.begin(LifecycleAction::Stop, first.instance)?;
        let license = fixture
            .root
            .join("runtimes")
            .join(&fixture.identity)
            .join("LICENSE");
        let original = fs::read(&license)?;
        fs::remove_file(&license)?;
        let operation = ensure_running(&fixture.root).await;
        fs::write(&license, &original)?;
        let error = operation
            .err()
            .context("direct ensure restarted an explicitly stopped service")?;
        ensure!(
            format!("{error:#}").contains("service_explicitly_stopped"),
            "STOP incorrectly needed ancillary data: {error:#}"
        );
        let state = lifecycle::inspect(&fixture.root)?;
        ensure!(
            state["pending"].is_null() && state["explicitly_stopped"] == true,
            "missing ancillary bytes prevented durable STOP settlement"
        );
        wait_quiescent(&fixture.root).await?;
        ensure!(
            fixture.health().await.is_err(),
            "direct STOP left a live service"
        );
        ensure!(
            selection_record(&fixture.root)?.active == fixture.identity,
            "STOP changed saved selection"
        );
        Ok(())
    }
    .await;
    let cleanup = fixture.cleanup().await;
    result.and(cleanup)
}
