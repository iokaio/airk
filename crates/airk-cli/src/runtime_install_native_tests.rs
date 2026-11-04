// SPDX-License-Identifier: Apache-2.0
//! Opt-in real-pair execution gate. Never uses a user's installed service.
use super::*;
use airk_core::{
    identity::Incarnation,
    service::RequestKey,
    service_lifecycle::{LifecycleAction, LifecycleRequest},
};

pub(super) struct NativeFixture {
    _owner: tempfile::TempDir,
    pub(super) root: PathBuf,
    pub(super) control: PathBuf,
    pub(super) identity: String,
}
impl NativeFixture {
    pub(super) async fn new() -> Result<Self> {
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
        let owner = match std::env::var_os("AIRK_PRIVATE_CENSUS_BASE") {
            Some(base) => tempfile::Builder::new()
                .prefix("airk-native-install-")
                .tempdir_in(base)?,
            None => tempfile::tempdir()?,
        };
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
    pub(super) async fn cleanup(&self) -> Result<()> {
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

#[tokio::test]
#[ignore = "requires a freshly built sealed pair and explicit approved digest; run serially"]
async fn native_mcp_eof_joins_installer_wait_without_late_startup() -> Result<()> {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
    for mode in [
        "unregistered_source",
        "registered_source",
        "installed_pinned",
    ] {
        eprintln!("cold frontend scenario: {mode}");
        let fixture = NativeFixture::new().await?;
        let result = async {
        let source = fixture._owner.path().join("source");
        fs::create_dir(&source)?;
        fs::write(source.join("README.md"), b"Original isolated EOF fixture.\n")?;
        let data = fixture._owner.path().join("data");
        local_ipc::prepare_root(&data)?;
        let registration = airk_core::service::ProjectRegistration::new(
            source.canonicalize()?.to_string_lossy().into_owned(),
            data.canonicalize()?.to_string_lossy().into_owned(),
            "eof-fixture".into(),
        )?;
        if mode != "unregistered_source" {
            let registered = service_runtime::request_for_version(
                &fixture.control, ControlMethod::Register { project: registration.clone() },
                env!("CARGO_PKG_VERSION"),
            ).await?;
            ensure!(matches!(registered, ControlResponse::Registered { .. }), "original project registration failed");
        }
        fixture.cleanup().await?;
        let registry = airk_store::service::registry::ServiceRegistry::open(
            &fixture.control, airk_kernel::service_broker::new_instance_id()?,
        )?;
        ensure!(!registry.service_explicitly_stopped()?, "fixture STOP barrier would mask late startup");
        drop(registry);
        let receipt = fixture.root.join(format!("startup-{}.json", fixture.identity));
        let bootstrap = fixture.root.join("bootstrap.json");
        let receipt_before = fs::read(&receipt).ok();
        let bootstrap_before = fs::read(&bootstrap).ok();
        let lock = install_lock(&fixture.root)?;
        let executable = fixture.root.join("runtimes").join(&fixture.identity).join(main_name());
        let diagnostics = PathBuf::from(std::env::var_os("AIRK_NATIVE_EOF_DIAGNOSTIC_DIR")
            .context("AIRK_NATIVE_EOF_DIAGNOSTIC_DIR must name owned external diagnostics")?).join(mode);
        fs::create_dir(&diagnostics)?;
        let stderr = OpenOptions::new().write(true).create_new(true)
            .open(diagnostics.join("eof-child-stderr.log"))?;
        let mut command = tokio::process::Command::new(executable);
        command.arg("--service-install-dir").arg(&fixture.root).arg("--root").arg(&source);
        if mode == "installed_pinned" {
            command.arg("--data-dir").arg(&data).arg("--collection").arg(&registration.collection)
                .arg("--service-project").arg(registration.binding.to_string());
        }
        let mut child = command.arg("mcp")
            .stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::from(stderr)).kill_on_drop(true).spawn()?;
        let exchange = async {
            let mut input = child.stdin.take().context("child input missing")?;
            let mut lines = tokio::io::BufReader::new(child.stdout.take().context("child output missing")?).lines();
            input.write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{\"protocolVersion\":\"2024-11-05\",\"capabilities\":{},\"clientInfo\":{\"name\":\"fixture\",\"version\":\"1\"}}}\n").await?;
            let first = tokio::time::timeout(Duration::from_secs(10), lines.next_line()).await.context("EOF fixture initialize deadline")??.context("initialize missing")?;
            ensure!(serde_json::from_str::<serde_json::Value>(&first)?["result"].is_object(), "initialize rejected");
            input.write_all(b"{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/list\"}\n").await?;
            let listed = tokio::time::timeout(Duration::from_secs(10), lines.next_line()).await.context("EOF fixture catalog deadline")??.context("catalog missing")?;
            ensure!(serde_json::from_str::<serde_json::Value>(&listed)?["result"]["tools"].is_array(), "catalog unavailable while installer lock held");
            input.write_all(b"{\"jsonrpc\":\"2.0\",\"id\":3,\"method\":\"tools/call\",\"params\":{\"name\":\"help\",\"arguments\":{}}}\n").await?;
            let help = tokio::time::timeout(Duration::from_secs(10), lines.next_line()).await
                .context("EOF fixture local help deadline")??.context("local help missing")?;
            let help: serde_json::Value = serde_json::from_str(&help)?;
            let help: serde_json::Value = serde_json::from_str(help["result"]["content"][0]["text"].as_str().context("local help content missing")?)?;
            ensure!(help["topics"].is_array(), "bundled help unavailable while lock held");
            ensure!(fs::read(&receipt).ok() == receipt_before && fs::read(&bootstrap).ok() == bootstrap_before,
                "static initialization/catalog/help changed startup receipts");
            input.write_all(b"{\"jsonrpc\":\"2.0\",\"id\":4,\"method\":\"tools/call\",\"params\":{\"name\":\"status\",\"arguments\":{}}}\n").await?;
            let status = tokio::time::timeout(Duration::from_secs(15), lines.next_line())
                .await.context("EOF fixture first native response deadline")??.context("first native deadline response missing")?;
            fs::write(diagnostics.join("eof-first-status.json"), &status)?;
            let status: serde_json::Value = serde_json::from_str(&status)?;
            let value: serde_json::Value = serde_json::from_str(
                status["result"]["content"][0]["text"].as_str().context("status content missing")?
            )?;
            ensure!(value["index_error"]["code"] == "status_timeout" && value["index_error"]["stage"] == "setup", "native request did not reach original setup deadline: {value}");
            ensure!(value["publication"].is_null(), "deadline fabricated index publication");
            input.shutdown().await?;
            drop(input);
            let status = tokio::time::timeout(Duration::from_secs(10), child.wait()).await.context("EOF fixture child drain deadline")??;
            ensure!(status.success(), "normal EOF failed to drain owned setup");
            Ok::<(), anyhow::Error>(())
        }.await;
        if exchange.is_err() {
            let _ = child.kill().await;
            let _ = child.wait().await;
        }
        // The installer lock remains held through child exit and observations.
        ensure!(fs::read(&receipt).ok() == receipt_before, "EOF changed startup receipt");
        ensure!(fs::read(&bootstrap).ok() == bootstrap_before, "EOF changed bootstrap receipt");
        ensure!(fixture.health().await.is_err(), "EOF admitted a late service start");
        wait_quiescent(&fixture.root).await?;
        drop(lock);
        ensure!(fs::read(source.join("README.md"))? == b"Original isolated EOF fixture.\n", "source bytes changed");
        exchange
    }.await;
        let cleanup = fixture.cleanup().await;
        result
            .and(cleanup)
            .with_context(|| format!("cold frontend scenario {mode}"))?;
    }
    Ok(())
}

#[tokio::test]
#[ignore = "requires a freshly built sealed pair and explicit approved digest; run serially"]
async fn native_mcp_pending_health_eof_drains_without_late_startup() -> Result<()> {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
    let fixture = NativeFixture::new().await?;
    let result=async{
        let mut health=fixture.health().await?;
        // A valid, authenticated draining observation follows the ordinary
        // health-unavailable continuation; it is not malformed metadata.
        health.policy.accepting=false;
        fixture.cleanup().await?;
        let source=fixture._owner.path().join("pending-health-source");
        fs::create_dir(&source)?;let content=b"Original pending-health EOF fixture.\n";
        fs::write(source.join("README.md"),content)?;
        let registry=airk_store::service::registry::ServiceRegistry::open_saved(&fixture.control)?;
        ensure!(!registry.service_explicitly_stopped()?,"STOP barrier masks pending EOF admission");drop(registry);
        layout::validate_control(&fixture.control)?;
        let mut listener=local_ipc::Listener::bind(&local_ipc::endpoint(&fixture.control,"control")?)?;
        let (entered,began)=tokio::sync::oneshot::channel();
        let (release,released)=tokio::sync::oneshot::channel();let mut release=Some(release);
        let peer_body=async move{
            let mut stream=listener.accept().await?;
            let request:airk_core::service::ControlRequest=local_ipc::read_message(&mut stream,Duration::from_secs(15)).await?
                .context("pending EOF health request missing")?;
            ensure!(matches!(request.method,ControlMethod::Health),"pending EOF peer received non-health operation");
            let _=entered.send(());released.await.context("pending EOF health release dropped")?;
            local_ipc::write_message(&mut stream,&ControlResponse::Health{observation:Box::new(health)},Duration::from_secs(5)).await?;
            Ok::<(),anyhow::Error>(())
        };
        let receipt=fixture.root.join(format!("startup-{}.json",fixture.identity));let bootstrap=fixture.root.join("bootstrap.json");
        let before=(fs::read(&receipt).ok(),fs::read(&bootstrap).ok());
        let diagnostics=PathBuf::from(std::env::var_os("AIRK_NATIVE_EOF_DIAGNOSTIC_DIR")
            .context("AIRK_NATIVE_EOF_DIAGNOSTIC_DIR must name owned external diagnostics")?).join("pending-health");
        fs::create_dir(&diagnostics)?;
        let stderr=OpenOptions::new().write(true).create_new(true).open(diagnostics.join("child-stderr.log"))?;
        let executable=fixture.root.join("runtimes").join(&fixture.identity).join(main_name());
        let mut child=tokio::process::Command::new(executable).arg("--service-install-dir").arg(&fixture.root)
            .arg("--root").arg(&source).arg("mcp").stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::from(stderr)).kill_on_drop(true).spawn()?;
        let mut peer=tokio::spawn(peer_body);
        let exchange=async{
            let mut input=child.stdin.take().context("pending EOF child input missing")?;
            let mut lines=tokio::io::BufReader::new(child.stdout.take().context("pending EOF child output missing")?).lines();
            input.write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{\"protocolVersion\":\"2024-11-05\",\"capabilities\":{},\"clientInfo\":{\"name\":\"pending-eof-fixture\",\"version\":\"1\"}}}\n").await?;
            let initialized=tokio::time::timeout(Duration::from_secs(10),lines.next_line()).await??.context("pending EOF initialize missing")?;
            ensure!(serde_json::from_str::<serde_json::Value>(&initialized)?["result"].is_object(),"pending EOF initialize rejected");
            input.write_all(b"{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/call\",\"params\":{\"name\":\"status\",\"arguments\":{}}}\n").await?;
            tokio::time::timeout(Duration::from_secs(8),began).await.context("pending EOF native setup did not submit Health before setup expiry")??;
            ensure!(child.try_wait()?.is_none(),"pending EOF child exited while Health held");
            ensure!((fs::read(&receipt).ok(),fs::read(&bootstrap).ok())==before,"pending Health already changed startup receipts");
            input.shutdown().await?;drop(input);
            // Real Health reception proves accepted setup. The bounded hold now
            // checks retained drain ownership, rather than inferring admission
            // solely from an unanswered request or a sleep.
            ensure!(tokio::time::timeout(Duration::from_millis(250),child.wait()).await.is_err(),"EOF detached accepted Health work");
            release.take().context("pending EOF release already consumed")?.send(()).map_err(|_|anyhow::anyhow!("pending health owner dropped"))?;
            let exited=tokio::time::timeout(Duration::from_secs(10),child.wait()).await.context("pending EOF owned child drain deadline")??;
            ensure!(exited.success(),"pending EOF child failed owned drain");
            Ok::<(),anyhow::Error>(())
        }.await;
        if exchange.is_err(){let _=child.kill().await;let _=child.wait().await;}
        if let Some(release)=release.take(){let _=release.send(());}
        let peer_result=match tokio::time::timeout(Duration::from_secs(10),&mut peer).await{
            Ok(result)=>result.context("pending EOF peer failed").and_then(|value|value),
            Err(error)=>{peer.abort();let _=peer.await;Err(anyhow::Error::new(error).context("pending EOF peer drain deadline"))}
        };
        match(exchange,peer_result){
            (Err(error),Err(peer))=>return Err(error.context(format!("pending EOF peer cleanup also failed: {peer:#}"))),
            (Err(error),_)|(_,Err(error))=>return Err(error),_=>{}
        }
        ensure!((fs::read(&receipt).ok(),fs::read(&bootstrap).ok())==before,"EOF permitted a late election/bootstrap effect");
        wait_quiescent(&fixture.root).await?;
        ensure!(fs::read(source.join("README.md"))?==content,"pending EOF changed source bytes");
        Ok(())
    }.await;
    let cleanup = fixture.cleanup().await;
    match (result, cleanup) {
        (Err(error), Err(cleanup)) => {
            Err(error.context(format!("pending EOF cleanup also failed: {cleanup:#}")))
        }
        (Err(error), _) | (_, Err(error)) => Err(error),
        _ => Ok(()),
    }
}

#[tokio::test]
#[ignore = "requires fresh sealed AIRK_F03_RUNTIME and explicit approval; run serially"]
async fn actual_copied_first_install_qualifies_genesis_before_first_service() -> Result<()> {
    use airk_store::service::registry::{ServiceRegistry, catalog::genesis};
    let source = PathBuf::from(
        std::env::var_os("AIRK_F03_RUNTIME").context("fresh sealed runtime required")?,
    )
    .canonicalize()?;
    let approved =
        std::env::var("AIRK_F03_APPROVED_SHA256").context("explicit runtime approval required")?;
    ensure!(
        runtime_pair::digest_file(&source.join("airk-install.json"))? == approved,
        "approved runtime differs"
    );
    for separate_state in [false, true] {
        let directory = tempfile::tempdir()?;
        let base = directory.path().canonicalize()?;
        let root = base.join("install");
        let state = base.join("state");
        let requested = separate_state.then_some(state.as_path());
        staging::stage_with_layout(
            &source,
            &root,
            &approved,
            VerificationBasis::OrganizationDigest,
            requested,
            RuntimeChannel::Stable,
        )?;
        let control = layout::control(&root)?;
        assert!(!root.join("selected.json").exists());
        assert!(!control.join("service-1.4.sqlite3").exists());
        let result = async {
            activate(&root, &approved).await?;
            let receipt =
                genesis::pending(&control)?.context("copied installer skipped Genesis")?;
            assert_eq!(receipt.stage, genesis::GenesisStage::Settled);
            let qualification = receipt
                .authority_v1
                .context("actual native qualification absent")?;
            assert!(qualification.readiness.required_predecessors.is_empty());
            let registry = ServiceRegistry::open_saved(&control)?;
            let catalog = registry
                .catalog_state()?
                .context("copied catalog missing")?;
            assert!(catalog.activated);
            assert_eq!(catalog.authority_contract, 1);
            let health = ensure_running(&root).await?;
            assert_eq!(registry.recorded_service_instance()?, Some(health.instance));
            assert_eq!(selection_record(&root)?.active, approved);
            assert!(!root.join("activation.json").exists());
            activate(&root, &approved).await?;
            assert_eq!(
                ensure_running(&root).await?.instance,
                health.instance,
                "completed replay restarted service"
            );
            Ok::<_, anyhow::Error>(())
        }
        .await;
        let _ = service_runtime::request_for_version(
            &control,
            ControlMethod::Shutdown,
            env!("CARGO_PKG_VERSION"),
        )
        .await;
        let cleanup = wait_quiescent(&root).await;
        result.and(cleanup)?;
    }
    Ok(())
}
