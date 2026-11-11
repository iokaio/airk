// SPDX-License-Identifier: Apache-2.0
//! Unwired next qualification stage: kill only the owned controller, then use
//! the actual Supervisor predecessor/drain owner to settle its original WDO.
use super::*;
use airk_core::{
    catalog_fences::FenceIntent, catalog_management::CatalogRequest, identity::Sha256Digest,
};
use std::io::{BufRead, Read, Write};
use std::process::ChildStdin;

const PREFIX: &str = "AIRK_GUARDED_PREPARED:";
const CONTROLLER_ROLE: &str = "AIRK_GUARDED_CONTROLLER";

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Prepared {
    root: PathBuf,
    service: Incarnation,
    worker: WorkerIdentity,
    sibling: WorkerIdentity,
    sibling_definition: airk_core::index_catalog::IndexDefinition,
    helper: Incarnation,
    request: CatalogRequest,
    preview: Sha256Digest,
    intent: Box<FenceIntent>,
}

#[test]
#[ignore = "owned qualification subprocess; invoked only by restored_native_worker_obligation"]
fn controller_process() -> Result<()> {
    ensure!(
        std::env::var(CONTROLLER_ROLE).as_deref() == Ok("prepare"),
        "private controller role required"
    );
    let runtime = executor()?;
    let mut fixture = NativeFixture::build()?;
    runtime.block_on(async {
        let mut a = connect_scope(&fixture.control.owner, fixture.selection(0)?).await?;
        let mut b = connect_scope(&fixture.control.owner, fixture.selection(1)?).await?;
        require_semantic_ready(&mut a).await?;
        require_semantic_ready(&mut b).await?;
        search_scope(&mut a, "vector", "cobalt harbor").await?;
        search_scope(&mut b, "vector", "cobalt harbor").await?;
        let (worker, sibling, helper, root, service) = {
            let owner = fixture.control.owner.borrow();
            (
                owner.workers[&fixture.indices[0].registration.binding]
                    .identity
                    .context("A missing")?,
                owner.workers[&fixture.indices[1].registration.binding]
                    .identity
                    .context("B missing")?,
                owner
                    .pool
                    .as_ref()
                    .context("native helper missing")?
                    .identity(),
                owner.root.clone(),
                owner.instance,
            )
        };
        let sibling_definition = fixture
            .registry
            .catalog_index_for_binding(sibling.project)?
            .context("full native sibling definition missing before controller crash")?;
        let nonce = fixture.gate.arm(worker, helper)?;
        let _accepted =
            tokio::spawn(
                async move { search_scope(&mut a, "vector", "harbor cobalt cobalt").await },
            );
        let entered = fixture.gate.entered(nonce)?;
        ensure!(
            entered.worker == worker && entered.helper == helper,
            "foreign native held work"
        );
        let key = fixture.key()?;
        let state = fixture
            .registry
            .catalog_state()?
            .context("native catalog absent")?;
        let draft = crate::catalog_source_change_control::draft(
            &mut fixture.registry,
            &root,
            &airk_core::catalog_source_draft::SourceMutationDraftSelection {
                key,
                expected_catalog_revision: state.revision,
                index_id: fixture.indices[0].definition,
                change: airk_core::catalog_source_draft::SourceMutationSelection::RetireIndex,
            },
        )?;
        let mut coordinator = crate::catalog_coordinator::CatalogCoordinator::new(
            fixture.control.gate.clone(),
            fixture.control.clone(),
        )?;
        let request = draft.request.clone();
        let preview = draft.preview_digest;
        // This production hook is AFTER durable Prepare, before operation-issued
        // Stop/revoke. The accepted owner and its native inherited guards remain
        // alive until the parent reaps this exact owned controller process.
        coordinator.apply_fenced_with_prepared_test_hook(
            &mut fixture.registry,
            &draft.request,
            preview,
            || {
                let intent = ServiceRegistry::open_saved(&root)
                    .unwrap()
                    .catalog_pending_fence()
                    .unwrap()
                    .expect("prepared WDO missing");
                assert_eq!(intent.request, request);
                let event = Prepared {
                    root: root.clone(),
                    service,
                    worker,
                    sibling,
                    sibling_definition: sibling_definition.clone(),
                    helper,
                    request: request.clone(),
                    preview,
                    intent: Box::new(intent),
                };
                let bytes = serde_json::to_vec(&event).unwrap();
                assert!(bytes.len() <= MAX_FRAME_BYTES);
                let mut output = std::io::stdout().lock();
                writeln!(output, "{PREFIX}{}", std::str::from_utf8(&bytes).unwrap()).unwrap();
                output.flush().unwrap();
                let mut byte = [0];
                let _ = std::io::stdin().read(&mut byte);
                panic!("owned crash controller continued without being killed");
            },
        )?;
        anyhow::bail!("crash injector failed to retain the accepted owner")
    })
}

struct Controller {
    child: Child,
    _input: ChildStdin,
    event: Receiver<Result<Prepared>>,
    reader: Option<std::thread::JoinHandle<()>>,
}

impl Controller {
    fn spawn(directory: &Path) -> Result<Self> {
        let name = module_path!()
            .strip_prefix(concat!(env!("CARGO_CRATE_NAME"), "::"))
            .unwrap_or(module_path!());
        let mut command = std::process::Command::new(std::env::current_exe()?);
        command
            .args([
                "--ignored",
                "--exact",
                &format!("{name}::controller_process"),
                "--nocapture",
            ])
            .env(CONTROLLER_ROLE, "prepare")
            .env("TMPDIR", directory)
            .env("TMP", directory)
            .env("TEMP", directory)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::fs::File::create(directory.join("controller.stderr"))?);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        let mut child = command.spawn()?;
        let input = child.stdin.take().context("controller input missing")?;
        let output = child.stdout.take().context("controller output missing")?;
        let (send, event) = mpsc::sync_channel(1);
        let reader = std::thread::spawn(move || {
            let result = (|| -> Result<Prepared> {
                let mut reader = std::io::BufReader::new(output);
                loop {
                    let mut bytes = Vec::new();
                    // Bound each local test-harness line before JSON decoding.
                    let mut bounded = reader
                        .by_ref()
                        .take((MAX_FRAME_BYTES + PREFIX.len() + 2) as u64);
                    ensure!(
                        bounded.read_until(b'\n', &mut bytes)? != 0,
                        "controller EOF before native Prepare"
                    );
                    ensure!(
                        bytes.len() <= MAX_FRAME_BYTES + PREFIX.len() + 1 && bytes.ends_with(b"\n"),
                        "controller observation exceeded bound"
                    );
                    if let Some(json) = bytes.strip_prefix(PREFIX.as_bytes()) {
                        return Ok(serde_json::from_slice(json)?);
                    }
                }
            })();
            let _ = send.send(result);
        });
        Ok(Self {
            child,
            _input: input,
            event,
            reader: Some(reader),
        })
    }

    fn prepared(&self) -> Result<Prepared> {
        self.event
            .recv_timeout(Duration::from_secs(120))
            .context("native controller preparation deadline")?
    }

    fn kill_and_reap(&mut self) -> Result<()> {
        self.child
            .kill()
            .context("kill exact owned qualification controller")?;
        let _ = self
            .child
            .wait()
            .context("reap exact owned qualification controller")?;
        if let Some(reader) = self.reader.take() {
            reader
                .join()
                .map_err(|_| anyhow::anyhow!("controller reader panicked"))?;
        }
        Ok(())
    }
}
impl Drop for Controller {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

#[test]
#[ignore = "requires separately pinned AIRK_TEST_GUARDED_RUNTIME and real predecessor guardian recovery"]
fn restored_native_worker_obligation() -> Result<()> {
    let directory = tempfile::Builder::new()
        .prefix("airk-guarded-restart-")
        .tempdir_in(std::env::temp_dir().canonicalize()?)?;
    let base = directory.path().canonicalize()?;
    let mut controller = Controller::spawn(&base)?;
    let prepared = controller.prepared()?;
    ensure!(
        prepared.root.canonicalize()?.starts_with(&base),
        "foreign qualification root"
    );
    prepared.intent.validate()?;
    ensure!(
        prepared.intent.request == prepared.request
            && prepared.intent.preview_digest == prepared.preview,
        "controller changed its original native request"
    );
    let obligation = prepared
        .intent
        .plan
        .worker_obligation
        .as_ref()
        .context("durable worker census missing")?;
    assert_eq!(obligation.workers, vec![prepared.worker]);
    assert_eq!(
        obligation.providers,
        vec![crate::catalog_admission::native_provider_obligation(
            prepared.worker,
            vec![prepared.helper]
        )]
    );
    ensure!(
        prepared.intent.worker_confirmations.is_empty(),
        "controller reported premature provider completion"
    );
    controller.kill_and_reap()?;
    // Controller exit is NOT a worker/helper proof. Strict Attach below must
    // recover actual predecessor roles and acquire their native exclusions.
    let lease = Arc::new(airk_store::service::ServiceLease::acquire(&prepared.root)?);
    let instance = new_instance_id()?;
    let mut registry = ServiceRegistry::open(&prepared.root, instance)?;
    registry.record_service_instance(&lease, instance)?;
    let state = registry
        .catalog_state()?
        .context("native restarted catalog absent")?;
    let gate = NativeCatalogAdmission::new(instance, state.namespace, state.revision, 1)?;
    let (log, _guard) = crate::service_logs::Log::start(prepared.root.clone(), instance);
    let supervisor = Supervisor::start(
        prepared.root.clone(),
        instance,
        Arc::new(Mutex::new(Broker::new(
            instance,
            airk_kernel::service_broker::BrokerLimits::service(),
        )?)),
        Arc::new(std::sync::atomic::AtomicU32::new(0)),
        Arc::new(std::sync::atomic::AtomicBool::new(false)),
        log,
        lease,
    )?;
    let control = supervisor.catalog_control(gate.clone())?;
    let mut coordinator = crate::catalog_coordinator::CatalogCoordinator::new(gate, control)?;
    let restored = coordinator
        .restore_pending(&registry)?
        .context("restart lost durable native WDO")?;
    assert_eq!(
        restored.plan, prepared.intent.plan,
        "restart replaced original census/provider history"
    );
    assert_eq!(restored.request, prepared.request);
    let deadline = Instant::now() + Duration::from_secs(30);
    let receipt = loop {
        match coordinator.apply_fenced(&mut registry, &prepared.request, prepared.preview) {
            Ok(receipt) => break receipt,
            Err(error) => {
                ensure!(
                    error.is::<crate::catalog_coordinator::CatalogApplyOutcomeUnknown>(),
                    "restored accepted operation lost its uncertain classification"
                );
                ensure!(
                    Instant::now() < deadline,
                    "actual predecessor/resource proof never completed"
                );
                std::thread::sleep(Duration::from_millis(100));
            }
        }
    };
    receipt.validate_for(
        &prepared.request,
        prepared.preview,
        restored.prepared_digest,
        &restored.plan,
    )?;
    assert_eq!(
        receipt.proof.worker_obligation,
        restored.plan.worker_obligation
    );
    assert_eq!(receipt.proof.worker_confirmations.len(), 1);
    assert_eq!(
        receipt.proof.worker_confirmations[0].worker,
        prepared.worker
    );
    assert_eq!(
        coordinator.apply_fenced(&mut registry, &prepared.request, prepared.preview)?,
        receipt
    );
    let airk_core::catalog_management::CatalogOperation::RetireIndex { id, .. } =
        prepared.request.operation
    else {
        anyhow::bail!("prepared restart request changed operation")
    };
    let detail = registry.catalog_detail(
        &airk_core::catalog_management::CatalogEntity::Index { id },
        None,
    )?;
    let Some(airk_core::catalog_management::CatalogItem::Index(retired)) = detail.item else {
        anyhow::bail!("native retired definition missing")
    };
    ensure!(
        retired.tombstone,
        "restored native retirement did not commit"
    );
    let sibling = registry
        .catalog_index_for_binding(prepared.sibling.project)?
        .context("native sibling definition lost")?;
    assert_eq!(
        sibling, prepared.sibling_definition,
        "restored native retirement changed the full unrelated index definition"
    );
    for index in [&retired, &sibling] {
        ensure!(
            std::fs::read(Path::new(&index.backing.root).join("memo.md"))?
                == b"# Cobalt harbor\nCobalt harbor navigation memo.\n",
            "controller restart changed native source bytes"
        );
    }
    ensure!(
        prepared.worker.service != instance && prepared.sibling.service == prepared.service,
        "restart did not exercise old native identities"
    );
    Ok(())
}
