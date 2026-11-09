// SPDX-License-Identifier: Apache-2.0
//! Baseline admission with real policy, registry pins and Genesis owners. Inert
//! provider bytes qualify metadata only; these are not process lifecycle tests.
use super::*;
use airk_store::service::registry::catalog::genesis::{
    CatalogGenesis,
    activation::{
        AuthorityV1CandidateObservation, AuthorityV1NativeQualification,
        AuthorityV1ReadinessProducer,
    },
};
use airk_store::service::{ServiceLease, registry::ServiceRegistry};

struct ComponentReadiness;
impl AuthorityV1ReadinessProducer for ComponentReadiness {
    fn required_predecessor_identities(&self) -> Result<Vec<airk_core::identity::Sha256Digest>> {
        Ok(Vec::new())
    }
    fn qualify(
        &self,
        expected: &AuthorityV1CandidateObservation,
    ) -> Result<AuthorityV1NativeQualification> {
        Ok(AuthorityV1NativeQualification {
            candidate: expected.clone(),
            predecessor_refusals: Vec::new(),
        })
    }
}

struct Fixture {
    _temporary: tempfile::TempDir,
    root: PathBuf,
    control: PathBuf,
    provider: PathBuf,
    approval: Approval,
    namespace: Incarnation,
}

fn fixture(lifecycle: managed::ManagedLifecycle, historical: bool) -> Result<Fixture> {
    let temporary = tempfile::tempdir()?;
    let base = temporary.path().canonicalize()?;
    let root = base.join("install");
    let provider = base.join("provider");
    local_ipc::prepare_root(&root)?;
    local_ipc::prepare_root(&provider)?;
    let identity = if historical {
        crate::runtime_install::tests::historical_pair_fixture(&provider)?
    } else {
        crate::runtime_install::tests::fixture(&provider)?
    };
    let manifest: InstallManifest =
        serde_json::from_slice(&fs::read(provider.join("airk-install.json"))?)?;
    let approval = Approval {
        schema: 1,
        manifest_sha256: identity,
        version: manifest.version,
        verification_basis: VerificationBasis::OrganizationDigest,
    };
    replace_json(
        &root,
        "managed-runtime.json",
        &serde_json::json!({
            "schema":1,"install":root,"source":provider,"approval":approval,
            "executable_sha256":runtime_pair::digest_file(&provider.join(main_name()))?,"lifecycle":lifecycle,
        }),
    )?;
    layout::initialize(&root, Some(&base.join("state")), RuntimeChannel::Stable)?;
    let control = layout::control(&root)?;
    local_ipc::prepare_root(&control)?;
    let lease = ServiceLease::acquire(&control)?;
    layout::record_control(&lease)?;
    let namespace = Incarnation::from_bytes([151; 16]);
    drop(ServiceRegistry::open(&control, namespace)?);
    layout::record_registry(&lease, namespace)?;
    let mut genesis = CatalogGenesis::begin(
        lease,
        approval.manifest_sha256.parse()?,
        Incarnation::from_bytes([152; 16]),
    )?;
    genesis.backup()?;
    genesis.migrate()?;
    genesis.promote_authority_v1()?;
    genesis
        .record_authority_v1_readiness(Incarnation::from_bytes([153; 16]), &ComponentReadiness)?;
    initialize(&root, &genesis)?;
    genesis.settle_authority_v1()?;
    drop(genesis);
    assert!(managed::read(&root)?.is_some());
    assert!(!root.join("runtimes").exists());
    Ok(Fixture {
        _temporary: temporary,
        root,
        control,
        provider,
        approval,
        namespace,
    })
}

fn publish_selection(fixture: &Fixture, active: String, previous: Option<String>) -> Result<()> {
    replace_json(
        &fixture.root,
        "selected.json",
        &Selection {
            schema: 1,
            active,
            previous,
        },
    )
}

#[test]
fn clean_break_managed_published_selection_uses_external_provider_baseline() -> Result<()> {
    for lifecycle in [
        managed::ManagedLifecycle::UserOnDemand,
        managed::ManagedLifecycle::External,
    ] {
        let fixture = fixture(lifecycle, false)?;
        assert!(!required(&fixture.root)?);
        publish_selection(&fixture, fixture.approval.manifest_sha256.clone(), None)?;
        let policy = fs::read(fixture.root.join("managed-runtime.json"))?;
        let selection = fs::read(fixture.root.join("selected.json"))?;
        let marker = fs::read(fixture.control.join(MARKER))?;
        let provider = fs::read(fixture.provider.join(main_name()))?;
        assert!(
            !required(&fixture.root)?,
            "published managed selection must keep external provider provenance"
        );
        require_control(&fixture.control)?;
        let observed = report(&fixture.root)?;
        assert_eq!(observed["required"], false);
        assert_eq!(observed["installed_contract"], CONTRACT);
        assert_eq!(observed["registrations_complete"], true);
        assert_eq!(observed["registrations_scope"], "current_registry");
        assert_eq!(fs::read(fixture.root.join("managed-runtime.json"))?, policy);
        assert_eq!(fs::read(fixture.root.join("selected.json"))?, selection);
        assert_eq!(fs::read(fixture.control.join(MARKER))?, marker);
        assert_eq!(fs::read(fixture.provider.join(main_name()))?, provider);
        assert!(!fixture.root.join("runtimes").exists());
    }
    Ok(())
}

#[tokio::test]
async fn clean_break_managed_epoch_read_and_probe_share_initialized_source_guard() -> Result<()> {
    use airk_core::service::ControlRequest;
    let fixture = fixture(managed::ManagedLifecycle::UserOnDemand, false)?;
    publish_selection(&fixture, fixture.approval.manifest_sha256.clone(), None)?;
    let instance = Incarnation::from_bytes([154; 16]);
    let mut listener =
        local_ipc::Listener::bind(&local_ipc::endpoint(&fixture.control, "control")?)?;
    let control = fixture.control.clone();
    let namespace = fixture.namespace;
    let server = tokio::spawn(async move {
        for _ in 0..2 {
            let mut stream = listener.accept().await?;
            let request: ControlRequest =
                local_ipc::read_message(&mut stream, Duration::from_secs(2))
                    .await?
                    .context("missing managed epoch request")?;
            ensure!(
                request.expected_instance == Some(instance)
                    && matches!(request.method, ControlMethod::RequestEpoch),
                "managed epoch request lost native scope"
            );
            let mut registry = ServiceRegistry::open(&control, namespace)?;
            let (namespace, epoch) = registry.request_epoch()?;
            local_ipc::write_message(
                &mut stream,
                &ControlResponse::RequestEpoch { namespace, epoch },
                Duration::from_secs(2),
            )
            .await?;
        }
        Ok::<_, anyhow::Error>(())
    });
    let result = async {
        let ordinary = service_runtime::request_for_instance(&fixture.control, ControlMethod::RequestEpoch, instance).await?;
        ensure!(matches!(ordinary, ControlResponse::RequestEpoch { namespace, .. } if namespace == fixture.namespace), "managed ordinary read crossed namespace");
        let probe = service_runtime::request_for_version_and_instance(&fixture.control, ControlMethod::RequestEpoch, &fixture.approval.version, instance).await?;
        ensure!(matches!(probe, ControlResponse::RequestEpoch { namespace, .. } if namespace == fixture.namespace), "managed readiness probe crossed namespace");
        Ok::<_, anyhow::Error>(())
    }.await;
    if result.is_err() {
        server.abort();
    }
    result?;
    server.await??;
    assert!(!fixture.root.join("runtimes").exists());
    Ok(())
}

#[test]
fn clean_break_managed_baseline_refuses_crossed_or_previous_public_selection() -> Result<()> {
    let fixture = fixture(managed::ManagedLifecycle::UserOnDemand, false)?;
    for (active, previous) in [
        ("c".repeat(64), None),
        (
            fixture.approval.manifest_sha256.clone(),
            Some("d".repeat(64)),
        ),
    ] {
        publish_selection(&fixture, active, previous)?;
        assert!(required(&fixture.root).is_err());
        assert!(require_control(&fixture.control).is_err());
        let refused = require_service(&fixture.control)
            .err()
            .context("crossed managed selection was admitted for service startup")?;
        assert!(
            refused
                .to_string()
                .contains("managed selection differs from its approved pin")
        );
    }
    fs::write(
        fixture.root.join("selected.json"),
        b"malformed public selection",
    )?;
    assert!(required(&fixture.root).is_err());
    Ok(())
}

#[test]
fn clean_break_managed_old_versions_and_missing_or_future_contract_stay_fenced() -> Result<()> {
    let old = fixture(managed::ManagedLifecycle::UserOnDemand, true)?;
    publish_selection(&old, old.approval.manifest_sha256.clone(), None)?;
    assert!(required(&old.root)?);
    assert!(require_control(&old.control).is_err());
    let current = fixture(managed::ManagedLifecycle::UserOnDemand, false)?;
    publish_selection(&current, current.approval.manifest_sha256.clone(), None)?;
    fs::remove_file(current.control.join(MARKER))?;
    assert!(required(&current.root)?);
    assert!(!current.control.join(MARKER).exists());
    assert!(require_control(&current.control).is_err());
    let marker = Marker {
        schema: 1,
        state_contract: "airk-state/2".into(),
        baseline_version: BASELINE.into(),
        install: current.root.clone(),
        control: current.control.clone(),
        namespace: current.namespace,
    };
    replace_json(&current.control, MARKER, &marker)?;
    let before = fs::read(current.control.join(MARKER))?;
    assert!(required(&current.root).is_err());
    assert_eq!(fs::read(current.control.join(MARKER))?, before);
    Ok(())
}

#[test]
fn clean_break_malformed_managed_policy_cannot_fall_back_to_valid_owned_payload() -> Result<()> {
    let fixture = fixture(managed::ManagedLifecycle::UserOnDemand, false)?;
    let directory = fixture
        .root
        .join("runtimes")
        .join(&fixture.approval.manifest_sha256);
    local_ipc::prepare_root(&fixture.root.join("runtimes"))?;
    local_ipc::prepare_root(&directory)?;
    let inventory: InstallManifest =
        serde_json::from_slice(&fs::read(fixture.provider.join("airk-install.json"))?)?;
    for name in inventory
        .files
        .keys()
        .chain(std::iter::once(&"airk-install.json".to_string()))
    {
        let target = directory.join(name);
        fs::create_dir_all(target.parent().context("owned fixture parent missing")?)?;
        fs::copy(fixture.provider.join(name), target)?;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(directory.join("notices"), fs::Permissions::from_mode(0o700))?;
    }
    replace_json(&directory, "approval.json", &fixture.approval)?;
    publish_selection(&fixture, fixture.approval.manifest_sha256.clone(), None)?;
    fs::remove_file(fixture.root.join("managed-runtime.json"))?;
    assert!(
        !required(&fixture.root)?,
        "unmanaged source remains supported when no managed policy exists"
    );
    fs::write(
        fixture.root.join("managed-runtime.json"),
        b"malformed managed policy",
    )?;
    assert!(
        required(&fixture.root).is_err(),
        "invalid managed policy must not be ignored because an owned inventory exists"
    );
    Ok(())
}
