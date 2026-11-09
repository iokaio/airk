// SPDX-License-Identifier: Apache-2.0
//! Installer phase tests use the real Store Genesis/setup/job owners. The test
//! producer substitutes only artifact qualification; installed native acceptance
//! must still execute the candidate's nonce-bound probe.
use super::*;
#[path = "runtime_rebuild_continuation_tests.rs"]
mod continuation_tests;
use airk_store::service::registry::catalog::genesis::{
    CatalogGenesis,
    activation::{
        AuthorityV1CandidateObservation, AuthorityV1NativeQualification,
        AuthorityV1ReadinessProducer,
    },
};

struct ComponentProducer;
impl AuthorityV1ReadinessProducer for ComponentProducer {
    fn required_predecessor_identities(&self) -> Result<Vec<Sha256Digest>> {
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

fn fixture(paused: bool) -> Result<(tempfile::TempDir, PathBuf, Journal)> {
    let owner = tempfile::tempdir()?;
    let base = owner.path().canonicalize()?;
    let root = base.join("install");
    local_ipc::prepare_root(&root)?;
    let original_state = base.join("original-state");
    layout::initialize(&root, Some(&original_state), RuntimeChannel::Stable)?;
    let old_control = layout::control(&root)?;
    local_ipc::prepare_root(&old_control)?;
    let service = ServiceLease::acquire(&old_control)?;
    layout::record_control(&service)?;
    let namespace = Incarnation::from_bytes([11; 16]);
    drop(ServiceRegistry::open(&old_control, namespace)?);
    layout::record_registry(&service, namespace)?;
    drop(service);
    let source = base.join("source");
    fs::create_dir(&source)?;
    fs::write(source.join("original.txt"), b"Original rebuild fixture.\n")?;
    let registration = ProjectRegistration::new(
        source.to_str().unwrap().into(),
        base.join("original-data").to_str().unwrap().into(),
        "fixture".into(),
    )?;
    fs::create_dir(&registration.data_root)?;
    airk_store::service::data_scope::bind(
        Path::new(&registration.data_root),
        &old_control,
        namespace,
    )?;
    let mut original = ServiceRegistry::open(&old_control, namespace)?;
    original.register(&registration)?;
    original.pause(registration.binding, paused)?;
    let original_revision = original.list(None, None, 32)?.revision;
    drop(original);
    let mut saved_files = std::collections::BTreeMap::new();
    for name in [
        "roots.json",
        "extraction-policy.json",
        "vector-policy.json",
        "compression-policy.json",
        "scan-limits.json",
    ] {
        saved_files.insert(name.into(), None);
    }
    saved_files.insert(
        "vector-policy.json".into(),
        Some(serde_json::to_vec(&serde_json::json!({
            "version":1,"owner_root":registration.root,"policy":{"enabled":false}
        }))?),
    );
    let saved = settings::SavedIndex {
        registration,
        paused,
        watch: false,
        roots: Default::default(),
        vectors_enabled: false,
        index_detail: airk_core::extraction::IndexDetail::Detailed,
        settings_available: true,
        discarded_preferences: Vec::new(),
        incarnation: Some(Incarnation::from_bytes([12; 16])),
        settings_digest: Sha256Digest::from_bytes([13; 32]),
        default_model: None,
        saved_files,
    };
    let prior = Selection {
        schema: 1,
        active: "a".repeat(64),
        previous: None,
    };
    replace_json(&root, "selected.json", &prior)?;
    let operation = Incarnation::from_bytes([14; 16]);
    let journal = Journal {
        schema: 1,
        operation,
        install: root.clone(),
        old_control,
        state: root.join(format!("rebuild-state-{operation}")),
        candidate: "b".repeat(64),
        prior,
        manager: false,
        roster: settings::SavedRoster {
            namespace,
            revision: original_revision,
            indexes: vec![saved],
        },
        resources: Default::default(),
        clean_break: false,
        capture_complete: None,
        expected_indexes: None,
        cleanup_complete: false,
        superseded_activation: None,
        cleanup_observation: None,
        cleanup_failure: None,
        client_registrations: None,
        client_metadata: None,
        http_ownership_snapshot: None,
        predecessor_stopped: None,
        predecessor_lifecycle_sequence: None,
        layout_sha256: Sha256Digest::from_bytes(
            Sha256::digest(fs::read(root.join("runtime-layout.json"))?).into(),
        ),
        phase: Phase::Accepted,
        indexes: vec![IndexProgress {
            request: None,
            job: None,
            restored: false,
            preferences_restored: false,
            completed: None,
            initial_job: None,
            failure: None,
        }],
        failure: None,
    };
    journal.save(&root)?;
    Ok((owner, root, journal))
}

fn successor_fixture(paused: bool) -> Result<(tempfile::TempDir, PathBuf, Journal)> {
    successor_fixture_for_input(paused, false)
}

fn ordinary_input_root(root: &Path) -> Result<PathBuf> {
    #[cfg(windows)]
    let input = PathBuf::from(root.to_str().unwrap().strip_prefix(r"\\?\").unwrap());
    #[cfg(unix)]
    let input = {
        let input = root.parent().unwrap().join("install-input-alias");
        std::os::unix::fs::symlink(root, &input)?;
        input
    };
    #[cfg(not(any(windows, unix)))]
    let input = root.to_path_buf();
    Ok(input)
}

fn successor_fixture_for_input(
    paused: bool,
    ordinary_input: bool,
) -> Result<(tempfile::TempDir, PathBuf, Journal)> {
    let (owner, root, mut journal) = fixture(paused)?;
    let root = if ordinary_input {
        let input = ordinary_input_root(&root)?;
        layout::preflight_clean_break(&input, journal.operation)?;
        journal.state = rebuild_state(&input, journal.operation)?;
        journal.save(&input)?;
        input
    } else {
        root
    };
    finish_successor_fixture(owner, root, journal)
}

fn finish_successor_fixture(
    owner: tempfile::TempDir,
    root: PathBuf,
    mut journal: Journal,
) -> Result<(tempfile::TempDir, PathBuf, Journal)> {
    let exclusion = OldExclusion::acquire(&journal)?;
    layout::admit_successor(
        &root,
        &SuccessorPermit {
            journal: &journal,
            _exclusion: &exclusion,
        },
    )?;
    drop(exclusion);
    journal.phase = Phase::Switched;
    journal.save(&root)?;
    local_ipc::prepare_root(&journal.control())?;
    let service = ServiceLease::acquire(&journal.control())?;
    layout::record_control(&service)?;
    let namespace = Incarnation::from_bytes([15; 16]);
    drop(ServiceRegistry::open(&journal.control(), namespace)?);
    layout::record_registry(&service, namespace)?;
    let mut genesis = CatalogGenesis::begin(
        service,
        Sha256Digest::from_bytes([16; 32]),
        journal.operation,
    )?;
    genesis.backup()?;
    genesis.migrate()?;
    genesis.promote_authority_v1()?;
    genesis.record_authority_v1_readiness(Incarnation::from_bytes([17; 16]), &ComponentProducer)?;
    state_contract::initialize(&root, &genesis)?;
    genesis.settle_authority_v1()?;
    drop(genesis);
    let lease = ServiceLease::acquire(&journal.control())?;
    let mut registry = ServiceRegistry::open(&journal.control(), namespace)?;
    registry.preserve_private_paths(&lease, &layout::retained_private_paths(&root)?)?;
    let malicious = ProjectRegistration::new(
        journal.roster.indexes[0].registration.data_root.clone(),
        root.parent()
            .unwrap()
            .join("must-remain-uncreated")
            .to_str()
            .unwrap()
            .into(),
        "unsafe-source".into(),
    )?;
    assert!(
        registry
            .preflight_source_isolation(&malicious, &[])
            .is_err(),
        "preserved private data became a source"
    );
    drop(registry);
    drop(lease);
    journal.phase = Phase::Genesis;
    journal.save(&root)?;
    Ok((owner, root, journal))
}

/// Reuse actual Genesis/handoff without exposing the private installer journal
/// or allowing another test to construct its opaque exclusion proof.
pub(in crate::runtime_install) fn storage_cleanup_fixture(
    initialize: impl FnOnce(&Path, Incarnation, &[ProjectRegistration]) -> Result<()>,
) -> Result<(
    tempfile::TempDir,
    PathBuf,
    PathBuf,
    Incarnation,
    Vec<ProjectRegistration>,
)> {
    let (owner, root, journal) = fixture(false)?;
    let registrations: Vec<_> = journal
        .roster
        .indexes
        .iter()
        .map(|index| index.registration.clone())
        .collect();
    initialize(
        &journal.old_control,
        journal.roster.namespace,
        &registrations,
    )?;
    let old_control = journal.old_control.clone();
    let namespace = journal.roster.namespace;
    let (owner, root, _) = finish_successor_fixture(owner, root, journal)?;
    Ok((owner, root, old_control, namespace, registrations))
}

#[test]
fn rebuild_unused_accepted_leaf_reconciles_only_exact_alias_without_read_mutation() -> Result<()> {
    let (_owner, root, mut journal) = fixture(false)?;
    let input = ordinary_input_root(&root)?;
    journal.state = input.join(format!("rebuild-state-{}", journal.operation));
    let original_operation = journal.operation;
    let envelope = Envelope {
        sha256: Sha256Digest::from_bytes(Sha256::digest(serde_json::to_vec(&journal)?).into()),
        journal: journal.clone(),
    };
    replace_json(&root, JOURNAL, &envelope)?;
    let before = fs::read(root.join(JOURNAL))?;
    for spelling in [&root, &input] {
        let observed = read(spelling)?.unwrap();
        assert_eq!(observed.state, rebuild_state(&root, original_operation)?);
        assert_eq!(observed.operation, original_operation);
        assert_eq!(fs::read(root.join(JOURNAL))?, before);
    }
    journal.state = root.join("unrelated-state");
    let envelope = Envelope {
        sha256: Sha256Digest::from_bytes(Sha256::digest(serde_json::to_vec(&journal)?).into()),
        journal,
    };
    replace_json(&root, JOURNAL, &envelope)?;
    assert!(read(&input).is_err());
    Ok(())
}

fn install_probe_fixture(root: &Path, historical: bool) -> Result<String> {
    local_ipc::prepare_root(&root.join("runtimes"))?;
    let stage = root.join(if historical { "probe-old" } else { "probe-new" });
    local_ipc::prepare_root(&stage)?;
    let identity = if historical {
        crate::runtime_install::tests::historical_pair_fixture(&stage)?
    } else {
        crate::runtime_install::tests::fixture(&stage)?
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(stage.join("notices"), fs::Permissions::from_mode(0o700))?;
    }
    let manifest: InstallManifest =
        serde_json::from_slice(&fs::read(stage.join("airk-install.json"))?)?;
    replace_json(
        &stage,
        "approval.json",
        &Approval {
            schema: 1,
            manifest_sha256: identity.clone(),
            version: manifest.version,
            verification_basis: VerificationBasis::OrganizationDigest,
        },
    )?;
    fs::rename(&stage, root.join("runtimes").join(&identity))?;
    Ok(identity)
}

#[tokio::test]
async fn clean_break_bootstrap_epoch_uses_exact_native_transport_before_selection() -> Result<()> {
    use airk_core::service::ControlRequest;
    let (_owner, root, mut journal) = successor_fixture(false)?;
    journal.prior.active = install_probe_fixture(&root, true)?;
    journal.candidate = install_probe_fixture(&root, false)?;
    journal.clean_break = true;
    journal.capture_complete = Some(true);
    journal.phase = Phase::Starting;
    replace_json(&root, "selected.json", &journal.prior)?;
    journal.save(&root)?;
    journal.intent().save(&root)?;
    assert!(state_contract::required(&root)?);
    let control = journal.control();
    let error = service_runtime::request(&control, ControlMethod::RequestEpoch)
        .await
        .err()
        .context("ordinary epoch read bypassed baseline")?;
    assert!(error.to_string().contains("clean_break_required"));
    let (main, approval) = verify_runtime(&root, &journal.candidate)?;
    let instance = Incarnation::from_bytes([111; 16]);
    let permit = state_contract::initialized_probe(
        &root,
        &journal.candidate,
        &approval.version,
        instance,
        &runtime_pair::digest_file(&main)?,
    )?
    .context("exact bootstrap was not admitted")?;
    assert!(
        state_contract::initialized_probe(
            &root,
            &"c".repeat(64),
            &approval.version,
            instance,
            &runtime_pair::digest_file(&main)?,
        )?
        .is_none()
    );
    assert!(
        state_contract::initialized_probe(
            &root,
            &journal.candidate,
            &approval.version,
            instance,
            &"d".repeat(64),
        )
        .is_err()
    );
    let mut mismatched = journal.intent();
    mismatched.candidate = "c".repeat(64);
    mismatched.save(&root)?;
    assert!(permit.request_identity().is_err());
    journal.intent().save(&root)?;
    let mut listener = local_ipc::Listener::bind(&local_ipc::endpoint(&control, "control")?)?;
    let expected_namespace = ServiceRegistry::open_saved(&control)?.incarnation()?;
    let service_control = control.clone();
    let server = tokio::spawn(async move {
        let mut stream = listener.accept().await?;
        let request: ControlRequest = local_ipc::read_message(&mut stream, Duration::from_secs(2))
            .await?
            .context("missing bootstrap request")?;
        ensure!(
            request.protocol == airk_core::service::PROTOCOL_VERSION,
            "bootstrap changed protocol"
        );
        ensure!(
            request.expected_instance == Some(instance),
            "bootstrap lost exact instance"
        );
        ensure!(
            matches!(request.method, ControlMethod::RequestEpoch),
            "bootstrap broadened request"
        );
        let mut registry = ServiceRegistry::open(&service_control, expected_namespace)?;
        let (namespace, epoch) = registry.request_epoch()?;
        local_ipc::write_message(
            &mut stream,
            &ControlResponse::RequestEpoch { namespace, epoch },
            Duration::from_secs(2),
        )
        .await?;
        Ok::<_, anyhow::Error>(())
    });
    let response = service_runtime::request_initialized_probe(&permit).await?;
    server.await??;
    assert!(
        matches!(response, ControlResponse::RequestEpoch { namespace, .. } if namespace == expected_namespace)
    );
    assert!(selection_record(&root)? == journal.prior);
    // No ordinary client is authorized by a valid native bootstrap receipt.
    assert!(
        service_runtime::request(&control, ControlMethod::RequestEpoch)
            .await
            .is_err()
    );
    assert!(
        service_runtime::request(&control, ControlMethod::HttpEndpoint)
            .await
            .is_err()
    );
    let marker = control.join("airk-state-contract.json");
    let marker_bytes = fs::read(&marker)?;
    fs::remove_file(&marker)?;
    assert!(
        service_runtime::request_initialized_probe(&permit)
            .await
            .is_err()
    );
    fs::write(&marker, marker_bytes)?;
    journal.phase = Phase::Complete;
    journal.save(&root)?;
    assert!(
        service_runtime::request_initialized_probe(&permit)
            .await
            .is_err()
    );
    Ok(())
}

fn predecessor_http_fixture() -> Result<(tempfile::TempDir, PathBuf, Journal, Vec<Incarnation>)> {
    use airk_core::{
        index_catalog::{CatalogRevision, GrantId, IndexId, QueryProjectId},
        service::ScopedConnectRequest,
        service_http::{EndpointDescriptor, EndpointState, HttpSelection, LoopbackMcpUrl},
    };
    let (owner, root, mut journal) = fixture(false)?;
    drop(install_lock(&root)?);
    journal.prior.active = install_probe_fixture(&root, true)?;
    journal.candidate = install_probe_fixture(&root, false)?;
    replace_json(&root, "selected.json", &journal.prior)?;
    journal.clean_break = true;
    journal.capture_complete = Some(true);
    journal.client_registrations = Some(
        journal
            .roster
            .indexes
            .iter()
            .map(|index| index.registration.clone())
            .collect(),
    );
    // The canonical ownership collector also observes saved rebuild protection.
    // Publish the real fixture identities before it reads that same journal.
    journal.save(&root)?;
    journal.client_metadata = Some(capture_client_metadata(
        &root,
        &journal.prior.active,
        &journal.candidate,
        &journal.old_control,
        journal.roster.namespace,
    )?);
    journal.save(&root)?;
    let namespace = journal.roster.namespace;
    let binding = crate::http_endpoint::installation_binding(&journal.old_control, namespace)?;
    let private = journal.old_control.join("http-1");
    let mut store = crate::http_endpoint::CredentialStore::open(&private, binding.clone())?;
    let ids: Vec<_> = (201..205)
        .map(|byte| Incarnation::from_bytes([byte; 16]))
        .collect();
    let selection = HttpSelection::Scoped {
        selection: ScopedConnectRequest {
            schema: 1,
            context_key: Sha256Digest::from_bytes([205; 32]),
            namespace,
            expected_catalog_revision: CatalogRevision::INITIAL,
            grant_id: GrantId::from_identity(ids[0]),
            expected_grant_revision: CatalogRevision::INITIAL,
            index_id: IndexId::from_identity(ids[0]),
            project_ids: vec![QueryProjectId::from_identity(ids[0])],
        },
    };
    let now = unix_ms()?;
    let issued = store.enroll(ids[0], selection.clone(), now + 3_600_000, now)?;
    store.enroll(ids[1], selection.clone(), 2, 1)?;
    store.enroll(ids[2], selection.clone(), now + 3_600_000, now)?;
    store.revoke(ids[2])?;
    store.compact_revoked()?;
    store.enroll(ids[3], selection, now + 3_600_000, now)?;
    store.revoke(ids[3])?;
    // Native issuance metadata proves ownership even after a secret is removed.
    fs::remove_file(issued.credential_file)?;
    let (main, _) = verify_runtime(&root, &journal.prior.active)?;
    let endpoint = EndpointDescriptor {
        schema: 1,
        installation: binding.installation,
        user: binding.user.clone(),
        namespace,
        generation: 1,
        service_instance: Incarnation::from_bytes([206; 16]),
        executable_sha256: runtime_pair::digest_file(&main)?.parse()?,
        url: LoopbackMcpUrl::new(57421)?,
        ca_sha256: Sha256Digest::from_bytes([207; 32]),
        certificate_sha256: Sha256Digest::from_bytes([208; 32]),
        state: EndpointState::Stopping,
        predecessor_digest: None,
    };
    crate::http_endpoint::publish_descriptor(&private, &binding, &endpoint, None)?;
    Ok((owner, root, journal, ids))
}

fn initialize_fixture_http_owner(control: &Path) -> Result<()> {
    let lease = ServiceLease::acquire(control)?;
    let namespace = ServiceRegistry::open_saved(control)?.incarnation()?;
    let binding = crate::http_endpoint::installation_binding(control, namespace)?;
    let mut store = crate::http_endpoint::CredentialStore::open(&control.join("http-1"), binding)?;
    store.initialize_empty_owned(&lease)?;
    Ok(())
}

#[test]
fn clean_break_native_http_lookup_owns_stopped_expired_revoked_and_retired_without_tokens()
-> Result<()> {
    use airk_core::service_http_cleanup::{HttpCleanupOwnership, PredecessorHttpLookup};
    let (_owner, root, journal, ids) = predecessor_http_fixture()?;
    for (index, state) in [
        HttpCleanupOwnership::Active,
        HttpCleanupOwnership::Expired,
        HttpCleanupOwnership::Revoked,
        HttpCleanupOwnership::Revoked,
    ]
    .into_iter()
    .enumerate()
    {
        let raw = http_cleanup::lookup(
            &root,
            &journal.prior.active,
            &journal.prior.active,
            ids[index],
        )?;
        let lookup: PredecessorHttpLookup = serde_json::from_value(raw)?;
        lookup.validate()?;
        assert_eq!(lookup.status, "owned");
        assert_eq!(lookup.ownership, Some(state));
        assert!(!lookup.qualification.live_authority);
        assert_eq!(lookup.operation_id, None);
        assert_eq!(lookup.snapshot.owners.len(), 4);
        assert_eq!(lookup.credential_sha256.is_some(), index != 2);
    }
    let lookup: PredecessorHttpLookup = serde_json::from_value(http_cleanup::lookup(
        &root,
        &journal.prior.active,
        &journal.prior.active,
        Incarnation::from_bytes([209; 16]),
    )?)?;
    assert_eq!(lookup.status, "not_owned");
    Ok(())
}

#[test]
fn clean_break_http_snapshot_survives_headless_old_control_and_payload_retirement() -> Result<()> {
    use airk_core::service_http_cleanup::PredecessorHttpLookup;
    let (owner, root, mut journal, ids) = predecessor_http_fixture()?;
    let snapshot = http_cleanup::capture(&root, &journal.prior.active, &journal.old_control);
    snapshot.validate()?;
    assert!(snapshot.complete);
    journal.http_ownership_snapshot = Some(snapshot.clone());
    journal.save(&root)?;
    let metadata = journal.client_metadata.clone().unwrap();
    assert!(
        metadata["command_images"]
            .as_array()
            .unwrap()
            .iter()
            .all(|image| {
                !Path::new(image["path"].as_str().unwrap())
                    .starts_with(root.join("runtimes").join(&journal.candidate))
            })
    );
    let (_owner, root, mut journal) = finish_successor_fixture(owner, root, journal)?;
    replace_json(&root, "selected.json", &journal.selected())?;
    journal.phase = Phase::Complete;
    journal.cleanup_complete = true;
    journal.save(&root)?;
    initialize_fixture_http_owner(&journal.control())?;
    let source = Path::new(&journal.roster.indexes[0].registration.root).join("original.txt");
    let source_before = fs::read(&source)?;
    fs::remove_dir_all(&journal.old_control)?;
    fs::remove_dir_all(root.join("runtimes").join(&journal.prior.active))?;
    let raw = http_cleanup::lookup(&root, &journal.candidate, &journal.prior.active, ids[2])?;
    let lookup: PredecessorHttpLookup = serde_json::from_value(raw)?;
    lookup.validate()?;
    assert_eq!(lookup.status, "owned");
    assert_eq!(
        lookup.qualification.current_namespace_ownership,
        "not_owned"
    );
    assert_eq!(lookup.operation_id, Some(journal.operation));
    assert_eq!(lookup.snapshot, snapshot);
    assert_eq!(lookup.control_dir, journal.old_control.to_str().unwrap());
    assert_eq!(lookup.namespace, Some(journal.roster.namespace));
    assert_eq!(http_snapshot_report(&root)?, Some(snapshot));
    assert_eq!(
        client_metadata(&root)?.unwrap()["predecessor_executable"],
        metadata["predecessor_executable"]
    );
    assert_eq!(fs::read(source)?, source_before);
    // A fresh namespace may legitimately issue the same explicit ID and URL.
    // Original ownership must never authorize deleting its newer configuration.
    let control = journal.control();
    let namespace = ServiceRegistry::open_saved(&control)?.incarnation()?;
    let binding = crate::http_endpoint::installation_binding(&control, namespace)?;
    let mut store =
        crate::http_endpoint::CredentialStore::read_existing(&control.join("http-1"), binding)?;
    use airk_core::{
        index_catalog::{CatalogRevision, GrantId, IndexId, QueryProjectId},
        service::ScopedConnectRequest,
        service_http::HttpSelection,
    };
    store.enroll_default(
        ids[2],
        HttpSelection::Scoped {
            selection: ScopedConnectRequest {
                schema: 1,
                context_key: Sha256Digest::from_bytes([212; 32]),
                namespace,
                expected_catalog_revision: CatalogRevision::INITIAL,
                grant_id: GrantId::from_identity(ids[2]),
                expected_grant_revision: CatalogRevision::INITIAL,
                index_id: IndexId::from_identity(ids[2]),
                project_ids: vec![QueryProjectId::from_identity(ids[2])],
            },
        },
        unix_ms()?,
    )?;
    let collision: PredecessorHttpLookup = serde_json::from_value(http_cleanup::lookup(
        &root,
        &journal.candidate,
        &journal.prior.active,
        ids[2],
    )?)?;
    collision.validate()?;
    assert_eq!(collision.status, "unavailable");
    assert_eq!(collision.qualification.current_namespace_ownership, "owned");
    assert!(collision.credential_file.is_none() && collision.ownership.is_none());
    assert_eq!(collision.snapshot, lookup.snapshot);
    fs::remove_file(control.join("http-1/credentials.json"))?;
    let unknown: PredecessorHttpLookup = serde_json::from_value(http_cleanup::lookup(
        &root,
        &journal.candidate,
        &journal.prior.active,
        ids[2],
    )?)?;
    assert_eq!(unknown.status, "unavailable");
    assert_eq!(
        unknown.qualification.current_namespace_ownership,
        "unavailable"
    );
    assert!(unknown.qualification.current_namespace_failure.is_some());
    Ok(())
}

#[test]
fn clean_break_native_http_lookup_refuses_missing_foreign_or_orphan_metadata() -> Result<()> {
    for fault in [
        "missing_ledger",
        "wrong_namespace",
        "wrong_executable",
        "orphan_token",
    ] {
        let (_owner, root, journal, ids) = predecessor_http_fixture()?;
        let private = journal.old_control.join("http-1");
        match fault {
            "missing_ledger" => fs::remove_file(private.join("credentials.json"))?,
            "wrong_namespace" | "wrong_executable" => {
                let path = private.join("endpoint.json");
                let mut endpoint: serde_json::Value = serde_json::from_slice(&fs::read(&path)?)?;
                if fault == "wrong_namespace" {
                    endpoint["namespace"] = serde_json::json!(Incarnation::from_bytes([210; 16]));
                } else {
                    endpoint["executable_sha256"] =
                        serde_json::json!(Sha256Digest::from_bytes([210; 32]));
                }
                fs::write(path, serde_json::to_vec(&endpoint)?)?;
            }
            "orphan_token" => {
                let path = private.join(format!(
                    "credential-{}.token",
                    Incarnation::from_bytes([211; 16])
                ));
                fs::write(
                    &path,
                    format!(
                        "airk1.{}.{}",
                        Incarnation::from_bytes([211; 16]),
                        "a".repeat(64)
                    ),
                )?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
                }
            }
            _ => unreachable!(),
        }
        let raw =
            http_cleanup::lookup(&root, &journal.prior.active, &journal.prior.active, ids[0])?;
        let lookup: airk_core::service_http_cleanup::PredecessorHttpLookup =
            serde_json::from_value(raw)?;
        assert_eq!(lookup.status, "unavailable", "{fault}");
        assert!(!lookup.snapshot.complete && lookup.snapshot.owners.is_empty());
        assert!(lookup.ownership.is_none());
    }
    Ok(())
}

#[test]
fn clean_break_http_disposal_cli_requires_explicit_purpose_and_predecessor() -> Result<()> {
    use clap::Parser;
    #[derive(Parser)]
    struct Arguments {
        #[command(subcommand)]
        runtime: RuntimeCommand,
    }
    let base = [
        "airk",
        "lookup-http-connection",
        "--install-dir",
        "fixture",
        "--expected-active",
        "current",
        "--connection-id",
        "01010101010101010101010101010101",
    ];
    assert!(Arguments::try_parse_from(base).is_ok());
    let mut args = base.to_vec();
    args.extend(["--purpose", "predecessor-cleanup"]);
    assert!(Arguments::try_parse_from(&args).is_err());
    args.extend(["--predecessor", "original"]);
    assert!(
        matches!(Arguments::try_parse_from(args)?.runtime, RuntimeCommand::LookupHttpConnection { purpose:Some(purpose), predecessor:Some(predecessor), .. } if purpose == "predecessor-cleanup" && predecessor == "original")
    );
    let mut args = base.to_vec();
    args.extend(["--predecessor", "original"]);
    assert!(Arguments::try_parse_from(args).is_err());
    let launchers = [
        "airk",
        "inspect-legacy-launchers",
        "--install-dir",
        "fixture",
        "--expected-active",
        "original",
    ];
    assert!(matches!(
        Arguments::try_parse_from(launchers)?.runtime,
        RuntimeCommand::InspectLegacyLaunchers { purpose: None, .. }
    ));
    let mut args = launchers.to_vec();
    args.extend(["--purpose", "predecessor-cleanup"]);
    assert!(
        matches!(Arguments::try_parse_from(args)?.runtime, RuntimeCommand::InspectLegacyLaunchers { purpose:Some(purpose), .. } if purpose == "predecessor-cleanup")
    );
    Ok(())
}

#[test]
fn clean_break_explicit_launcher_inspection_qualifies_old_pair_without_relaxing_normal_gate()
-> Result<()> {
    let (_owner, root, _journal) = fixture(false)?;
    drop(install_lock(&root)?);
    fs::remove_file(root.join(JOURNAL))?;
    let predecessor = install_probe_fixture(&root, true)?;
    let candidate = install_probe_fixture(&root, false)?;
    replace_json(
        &root,
        "selected.json",
        &Selection {
            schema: 1,
            active: predecessor.clone(),
            previous: None,
        },
    )?;
    let bytes = b"Original receipt-owned predecessor launcher.";
    fs::write(launcher_path(&root), bytes)?;
    replace_json(
        &root,
        "launcher.json",
        &LauncherReceipt {
            schema: 1,
            sha256: format!("{:x}", Sha256::digest(bytes)),
        },
    )?;
    let before = fs::read(root.join("selected.json"))?;
    assert!(legacy_launchers::inspect(&root, &predecessor).is_err());
    let observed = legacy_launchers::inspect_predecessor_cleanup(&root, &predecessor)?;
    assert_eq!(observed["active"], predecessor);
    assert_eq!(
        observed["launcher"]["path"],
        serde_json::to_value(launcher_path(&root))?
    );
    assert_eq!(
        observed["launcher"]["sha256"],
        format!("{:x}", Sha256::digest(bytes))
    );
    assert!(observed.get("legacy_runtime_images").is_some());
    assert!(legacy_launchers::inspect_predecessor_cleanup(&root, &candidate).is_err());
    assert_eq!(fs::read(root.join("selected.json"))?, before);
    assert_eq!(fs::read(launcher_path(&root))?, bytes);
    Ok(())
}

#[test]
fn rebuild_ordinary_input_keeps_canonical_state_through_genesis_and_setup() -> Result<()> {
    for boundary in [
        IndexBoundary::RequestSaved,
        IndexBoundary::SetupComplete,
        IndexBoundary::IndexSaved,
    ] {
        let (_owner, input, mut journal) = successor_fixture_for_input(false, true)?;
        let canonical = input.canonicalize()?;
        assert_eq!(journal.state, rebuild_state(&canonical, journal.operation)?);
        assert_eq!(layout::saved_control(&input)?, journal.control());
        assert_eq!(read(&canonical)?.unwrap().operation, journal.operation);
        let namespace = ServiceRegistry::open_saved(&journal.control())?.incarnation()?;
        let mut registry = ServiceRegistry::open(&journal.control(), namespace)?;
        let mut observed = 0;
        assert!(
            prepare_index_observing(&input, &mut journal, 0, &mut registry, |at| {
                if at == boundary {
                    observed += 1;
                    anyhow::bail!("input spelling interruption");
                }
                Ok(())
            })
            .is_err()
        );
        assert_eq!(observed, 1);
        let mut saved = read(&canonical)?.unwrap();
        let accepted = saved.indexes[0].job;
        prepare_index(&canonical, &mut saved, 0, &mut registry)?;
        assert!(accepted.is_none_or(|id| saved.indexes[0].job == Some(id)));
        assert_eq!(read(&input)?.unwrap().operation, journal.operation);
        assert_eq!(registry.incarnation()?, namespace);
        assert_eq!(
            saved.indexes[0]
                .request
                .as_ref()
                .unwrap()
                .registration
                .data_root,
            replacement_data(&canonical, journal.operation, 0)?
                .to_str()
                .unwrap()
        );
        assert_eq!(
            successor_default_data(&input)?,
            successor_default_data(&canonical)?
        );
    }
    Ok(())
}

#[test]
fn rebuild_resume_each_setup_boundary_reuses_exact_native_receipts_and_index_job() -> Result<()> {
    for paused in [false, true] {
        for boundary in [
            IndexBoundary::RequestSaved,
            IndexBoundary::SetupAccepted,
            IndexBoundary::StoragePrepared,
            IndexBoundary::SetupComplete,
            IndexBoundary::IndexAccepted,
            IndexBoundary::IndexSaved,
            IndexBoundary::PolicyRestored,
        ] {
            let (_owner, root, mut journal) = successor_fixture(paused)?;
            let original_source =
                Path::new(&journal.roster.indexes[0].registration.root).join("original.txt");
            let original_bytes = fs::read(&original_source)?;
            let preserved_database = fs::read(journal.old_control.join("service-1.4.sqlite3"))?;
            let namespace = ServiceRegistry::open_saved(&journal.control())?.incarnation()?;
            let mut registry = ServiceRegistry::open(&journal.control(), namespace)?;
            let mut selected = 0;
            let result =
                prepare_index_observing(&root, &mut journal, 0, &mut registry, |observed| {
                    if observed == boundary {
                        selected += 1;
                        anyhow::bail!("simulated interruption");
                    }
                    Ok(())
                });
            assert!(result.is_err());
            assert_eq!(selected, 1, "interruption boundary must be exercised once");
            drop(registry);
            let mut resumed = read(&root)?.context("interrupted operation lost its journal")?;
            assert_eq!(resumed.operation, journal.operation);
            let mut registry = ServiceRegistry::open(&resumed.control(), namespace)?;
            prepare_index(&root, &mut resumed, 0, &mut registry)?;
            let progress = resumed.indexes[0].clone();
            assert!(progress.restored);
            let request = progress.request.context("resumed setup missing")?;
            assert_eq!(
                registry
                    .index_setup_receipt(request.caller, &request.key)?
                    .unwrap()
                    .stage,
                SetupStage::Ready
            );
            let jobs = registry.queued_jobs_observation(request.registration.binding, 32)?;
            assert_eq!(
                jobs.iter().filter(|job| job.kind == JobKind::Index).count(),
                1
            );
            assert_eq!(
                Some(
                    jobs.iter()
                        .find(|job| job.kind == JobKind::Index)
                        .unwrap()
                        .id
                ),
                progress.job
            );
            assert_eq!(registry.get(request.registration.binding)?.paused, paused);
            let stored = registry.job(progress.job.unwrap())?;
            prepare_index(&root, &mut resumed, 0, &mut registry)?;
            assert_eq!(registry.job(progress.job.unwrap())?.id, stored.id);
            assert_eq!(fs::read(&original_source)?, original_bytes);
            assert_eq!(
                fs::read(resumed.old_control.join("service-1.4.sqlite3"))?,
                preserved_database
            );
        }
    }
    Ok(())
}

#[test]
fn accepted_rebuild_observation_survives_incomplete_genesis_and_detects_journal_corruption()
-> Result<()> {
    let (_owner, root, mut journal) = fixture(true)?;
    let initial = observation(&root)?.unwrap();
    assert_eq!(initial.stage, RebuildStage::Accepted);
    assert!(initial.successor_namespace.is_none());
    fs::create_dir(&journal.state)?;
    fs::create_dir(journal.control())?;
    journal.phase = Phase::Switched;
    journal.save(&root)?;
    let preparing = observation(&root)?.unwrap();
    assert_eq!(preparing.stage, RebuildStage::Preparing);
    assert!(preparing.successor_namespace.is_none());
    journal.failure = Some(RebuildFailure {
        code: "fixture_interrupted".into(),
        message: "Genesis has not completed".into(),
    });
    journal.save(&root)?;
    let incomplete = observation(&root)?.unwrap();
    assert_eq!(incomplete.stage, RebuildStage::Failed);
    assert!(incomplete.successor_namespace.is_none());
    assert_eq!(
        incomplete.preserved_control,
        journal.old_control.to_str().unwrap()
    );
    let mut value: serde_json::Value = serde_json::from_slice(&fs::read(root.join(JOURNAL))?)?;
    value["journal"]["candidate"] = serde_json::json!("c".repeat(64));
    fs::write(root.join(JOURNAL), serde_json::to_vec(&value)?)?;
    assert!(read(&root).is_err());
    Ok(())
}

#[tokio::test]
async fn rebuild_boundaries_reject_scope_drift_and_keep_historical_preservation_after_later_selection()
-> Result<()> {
    let (_owner, root, mut journal) = successor_fixture(false)?;
    let namespace = ServiceRegistry::open_saved(&journal.control())?.incarnation()?;
    let mut registry = ServiceRegistry::open(&journal.control(), namespace)?;
    prepare_index(&root, &mut journal, 0, &mut registry)?;
    let mut changed = journal.clone();
    changed.indexes[0]
        .request
        .as_mut()
        .unwrap()
        .sources
        .push(SetupSource {
            canonical_path: root
                .parent()
                .unwrap()
                .join("unreviewed")
                .to_str()
                .unwrap()
                .into(),
            aliases: vec!["widened".into()],
            owner: false,
        });
    assert!(changed.validate(&root).is_err());
    journal.phase = Phase::Complete;
    journal.save(&root)?;
    replace_json(&root, "selected.json", &journal.intent().selected())?;
    assert!(require_retirement_allowed(&root).is_err());
    let previous = retirement::retire_previous(&root, &journal.candidate)
        .await
        .unwrap_err();
    assert!(
        previous
            .to_string()
            .contains("rebuild recovery evidence is preserved")
    );
    let legacy = legacy_launchers::retire(&root, &journal.candidate)
        .await
        .unwrap_err();
    assert!(
        legacy
            .to_string()
            .contains("rebuild recovery evidence is preserved")
    );
    replace_json(
        &root,
        "selected.json",
        &Selection {
            schema: 1,
            active: "c".repeat(64),
            previous: Some(journal.candidate.clone()),
        },
    )?;
    assert!(!pending(&root)?);
    assert!(preserved_runtime(&root, &journal.prior.active)?);
    require_retirement_allowed(&root)?;
    let preserved = root
        .join("runtimes")
        .join(&journal.prior.active)
        .join("preserved.bin");
    fs::create_dir_all(preserved.parent().unwrap())?;
    fs::write(&preserved, b"preserved predecessor payload")?;
    let refused = retirement::remove(&root, &journal.prior.active).unwrap_err();
    assert!(
        refused
            .to_string()
            .contains("preserved as recovery evidence")
    );
    assert_eq!(fs::read(preserved)?, b"preserved predecessor payload");
    assert!(observation(&root)?.is_some());
    Ok(())
}

#[test]
fn rebuild_failure_messages_are_utf8_bounded_with_disclosed_truncation() {
    let message = bounded_message(&"界".repeat(3000));
    assert!(message.len() <= 4096);
    assert!(message.ends_with(" [truncated]"));
    assert_eq!(
        nonce(Incarnation::from_bytes([1; 16]), 0, b"index"),
        nonce(Incarnation::from_bytes([1; 16]), 0, b"index")
    );
    assert_ne!(
        nonce(Incarnation::from_bytes([1; 16]), 0, b"index"),
        nonce(Incarnation::from_bytes([1; 16]), 0, b"setup")
    );
}

#[test]
fn rebuild_readiness_observes_resume_and_terminal_jobs_before_original_pause() {
    let mut job = airk_core::service::JobRecord {
        id: Sha256Digest::from_bytes([22; 32]),
        project: Sha256Digest::from_bytes([23; 32]),
        kind: JobKind::Index,
        state: JobState::Pending,
        fencing_generation: 0,
        attempts: 0,
        outcome: None,
    };
    assert_eq!(
        index_readiness(Some(&job), true, false),
        RebuildReadiness::Paused
    );
    assert_eq!(
        index_readiness(Some(&job), false, false),
        RebuildReadiness::Pending
    );
    job.state = JobState::Succeeded;
    assert_eq!(
        index_readiness(Some(&job), false, false),
        RebuildReadiness::Ready
    );
    assert_eq!(
        index_readiness(Some(&job), true, false),
        RebuildReadiness::Ready
    );
    job.state = JobState::Failed;
    assert_eq!(
        index_readiness(Some(&job), true, false),
        RebuildReadiness::Failed
    );
    job.state = JobState::Canceled;
    assert_eq!(
        index_readiness(Some(&job), false, false),
        RebuildReadiness::Failed
    );
    assert_eq!(
        index_readiness(None, false, true),
        RebuildReadiness::Unavailable
    );
}

#[test]
fn unfinished_rebuild_never_admits_retirement() -> Result<()> {
    let (_owner, root, mut journal) = fixture(false)?;
    for phase in [
        Phase::Accepted,
        Phase::Switched,
        Phase::Genesis,
        Phase::Setup,
        Phase::Starting,
    ] {
        journal.phase = phase;
        journal.save(&root)?;
        assert!(require_retirement_allowed(&root).is_err());
        assert!(preserved_runtime(&root, &journal.prior.active)?);
    }
    Ok(())
}

#[tokio::test]
async fn reviewed_plain_activate_resumes_only_the_exact_saved_rebuild() -> Result<()> {
    let (_owner, root, journal) = successor_fixture(false)?;
    let namespace_before = ServiceRegistry::open_saved(&journal.control())?.incarnation()?;
    journal.intent().save(&root)?;
    let before = fs::read(root.join(JOURNAL))?;
    for (candidate, expected) in [
        ("c".repeat(64), journal.prior.active.clone()),
        (journal.candidate.clone(), "d".repeat(64)),
    ] {
        assert!(
            crate::runtime_install::activate_reviewed(&root, &candidate, &expected)
                .await
                .is_err()
        );
        assert_eq!(fs::read(root.join(JOURNAL))?, before);
    }
    assert!(
        crate::runtime_install::activate(&root, &journal.candidate)
            .await
            .is_err()
    );
    assert_eq!(fs::read(root.join(JOURNAL))?, before);
    let error =
        crate::runtime_install::activate_reviewed(&root, &journal.candidate, &journal.prior.active)
            .await
            .err()
            .context("inert candidate unexpectedly activated")?;
    assert!(
        !error
            .to_string()
            .contains("requires its exact reviewed predecessor")
    );
    // This inert fixture has no approved candidate executable. Reaching the
    // existing native candidate check proves dispatch entered the same owner,
    // rather than rejecting the GUI's reviewed Resume or creating a new state.
    let retained = read(&root)?.unwrap();
    assert_eq!(retained.operation, journal.operation);
    assert_eq!(retained.control(), journal.control());
    assert!(retained.failure.is_some());
    assert_eq!(
        namespace_before,
        ServiceRegistry::open_saved(&retained.control())?.incarnation()?
    );
    Ok(())
}

#[test]
fn only_mandatory_clean_break_infers_unreviewed_saved_predecessor() -> Result<()> {
    let (_owner, root, mut journal) = fixture(false)?;
    let before = fs::read(root.join(JOURNAL))?;
    assert!(activation_predecessor(&root, None).is_err());
    assert_eq!(fs::read(root.join(JOURNAL))?, before);
    assert_eq!(
        activation_predecessor(&root, Some(&journal.prior.active))?,
        journal.prior.active
    );
    journal.clean_break = true;
    journal.capture_complete = Some(true);
    journal.save(&root)?;
    let before = fs::read(root.join(JOURNAL))?;
    assert_eq!(activation_predecessor(&root, None)?, journal.prior.active);
    assert_eq!(fs::read(root.join(JOURNAL))?, before);
    Ok(())
}

#[test]
fn pruned_native_rebuild_job_is_unavailable_without_inventing_execution_failure() -> Result<()> {
    let (_owner, root, mut journal) = successor_fixture(false)?;
    let namespace = ServiceRegistry::open_saved(&journal.control())?.incarnation()?;
    let mut registry = ServiceRegistry::open(&journal.control(), namespace)?;
    prepare_index(&root, &mut journal, 0, &mut registry)?;
    let request = journal.indexes[0].request.clone().unwrap();
    let job = journal.indexes[0].job.unwrap();
    let mut worker = ProjectLease::acquire(&journal.control(), request.registration.binding)?;
    registry.admit_worker(&mut worker)?;
    assert_eq!(registry.claim_job(&worker)?.unwrap().id, job);
    registry.complete_job(
        &worker,
        job,
        airk_core::service::JobOutcome::Succeeded { revision: None },
    )?;
    drop(worker);
    journal.phase = Phase::Complete;
    journal.save(&root)?;
    assert_eq!(observation(&root)?.unwrap().stage, RebuildStage::Ready);
    let mut connection = rusqlite::Connection::open(journal.control().join("service-1.4.sqlite3"))?;
    let transaction = connection.transaction()?;
    for number in 0..1023u32 {
        let id = framed_digest(b"rebuild-pruning-fixture/1", &[&number.to_le_bytes()]);
        transaction.execute("INSERT INTO service_jobs(id,project,kind,state,epoch,attempts) VALUES(?1,?2,'verify','pending',0,0)",
            rusqlite::params![id.to_string(), request.registration.binding.to_string()])?;
    }
    transaction.commit()?;
    drop(connection);
    let (namespace, epoch) = registry.saved_request_epoch()?;
    let key = RequestKey {
        namespace,
        epoch,
        nonce: Incarnation::from_bytes([78; 16]),
    };
    let refusal = registry
        .enqueue(
            request.registration.binding,
            request.caller,
            &key,
            JobKind::Inspect,
        )
        .unwrap_err();
    assert!(matches!(
        refusal.downcast_ref(),
        Some(airk_core::service::JobError::KeyExpired)
    ));
    assert!(registry.job(job).is_err());
    let unavailable = observation(&root)?.unwrap();
    assert_eq!(unavailable.stage, RebuildStage::Unavailable);
    assert_eq!(
        unavailable.indexes[0].readiness,
        RebuildReadiness::Unavailable
    );
    assert_eq!(
        unavailable.indexes[0].failure.as_ref().unwrap().code,
        "rebuild_job_unavailable"
    );
    assert!(unavailable.indexes[0].job.is_none());
    assert_eq!(
        unavailable.indexes[0].original,
        journal.roster.indexes[0].registration
    );
    journal.failure = Some(RebuildFailure {
        code: "fixture_execution_failed".into(),
        message: "Retained native failure".into(),
    });
    journal.save(&root)?;
    assert_eq!(observation(&root)?.unwrap().stage, RebuildStage::Failed);
    Ok(())
}

pub(in crate::runtime_install) fn historical_retirement_fixture()
-> Result<(tempfile::TempDir, PathBuf, Vec<String>, String, String)> {
    fn payload(root: &Path, label: &str) -> Result<String> {
        let source = root.join(format!("retirement-fixture-{label}"));
        local_ipc::prepare_root(&source)?;
        crate::runtime_install::tests::fixture(&source)?;
        fs::write(source.join("LICENSE"), label.as_bytes())?;
        let mut manifest: InstallManifest =
            serde_json::from_slice(&fs::read(source.join("airk-install.json"))?)?;
        manifest.files.insert(
            "LICENSE".into(),
            runtime_pair::digest_file(&source.join("LICENSE"))?,
        );
        let bytes = serde_json::to_vec(&manifest)?;
        let identity = format!("{:x}", Sha256::digest(&bytes));
        fs::write(source.join("airk-install.json"), bytes)?;
        replace_json(
            &source,
            "approval.json",
            &Approval {
                schema: 1,
                manifest_sha256: identity.clone(),
                version: manifest.version,
                verification_basis: VerificationBasis::OrganizationDigest,
            },
        )?;
        fs::rename(source, root.join("runtimes").join(&identity))?;
        Ok(identity)
    }
    let (owner, root, mut journal) = successor_fixture(false)?;
    local_ipc::prepare_root(&root.join("runtimes"))?;
    let original = payload(&root, "preserved-original")?;
    let original_previous = payload(&root, "preserved-original-previous")?;
    let rebuilt = payload(&root, "rebuilt")?;
    let later = payload(&root, "ordinary-later-upgrade")?;
    journal.prior.active = original.clone();
    journal.prior.previous = Some(original_previous.clone());
    journal.candidate = rebuilt.clone();
    journal.phase = Phase::Complete;
    journal.save(&root)?;
    replace_json(
        &root,
        "selected.json",
        &Selection {
            schema: 1,
            active: later.clone(),
            previous: Some(rebuilt.clone()),
        },
    )?;
    let mut preserved = vec![original, original_previous];
    preserved.sort();
    Ok((owner, root, preserved, rebuilt, later))
}

#[test]
fn clean_break_builds_saved_paused_index_once_before_restoring_preferences() -> Result<()> {
    let (_owner, root, mut journal) = successor_fixture(true)?;
    journal.clean_break = true;
    journal.capture_complete = Some(true);
    journal.expected_indexes = Some(journal.roster.indexes.len());
    journal.roster.indexes[0].watch = true;
    journal.save(&root)?;
    let namespace = ServiceRegistry::open_saved(&journal.control())?.incarnation()?;
    let mut registry = ServiceRegistry::open(&journal.control(), namespace)?;
    prepare_index(&root, &mut journal, 0, &mut registry)?;
    let request = journal.indexes[0]
        .request
        .as_ref()
        .context("setup missing")?;
    assert!(!registry.get(request.registration.binding)?.paused);
    assert!(
        !registry
            .watch(request.registration.binding)?
            .observation
            .enabled
    );
    assert!(journal.roster.indexes[0].paused);
    assert!(journal.roster.indexes[0].watch);
    assert_eq!(
        registry
            .job(journal.indexes[0].job.context("initial job missing")?)?
            .kind,
        JobKind::Index
    );
    let observed = observation(&root)?.context("observation missing")?;
    assert_eq!(
        observed.mode,
        Some(airk_core::rebuild::RebuildMode::CleanBreak)
    );
    assert!(!observed.cleanup.context("cleanup missing")?.complete);
    Ok(())
}

#[test]
fn clean_break_zero_index_readiness_requires_completed_fresh_genesis_phase() -> Result<()> {
    let (_owner, root, mut journal) = successor_fixture(false)?;
    journal.clean_break = true;
    journal.capture_complete = Some(true);
    journal.expected_indexes = Some(journal.roster.indexes.len());
    journal.roster.indexes.clear();
    journal.indexes.clear();
    journal.expected_indexes = Some(0);
    journal.save(&root)?;
    let preparing = observation(&root)?.context("preparing missing")?;
    assert_eq!(preparing.stage, RebuildStage::Preparing);
    assert!(preparing.indexes.is_empty());
    journal.phase = Phase::Complete;
    journal.save(&root)?;
    let ready = observation(&root)?.context("ready missing")?;
    assert_eq!(ready.stage, RebuildStage::Ready);
    assert_ne!(ready.successor_namespace, Some(ready.original_namespace));
    assert!(!ready.cleanup.context("cleanup missing")?.complete);
    assert_eq!(journal.selected().previous, None);
    Ok(())
}

#[test]
fn clean_break_discards_only_predecessor_stop_and_preserves_new_stop() {
    assert!(predecessor_stop_allows_reset(true, 10, true, 10));
    assert!(predecessor_stop_allows_reset(false, 10, false, 10));
    assert!(!predecessor_stop_allows_reset(false, 10, true, 11));
    assert!(!predecessor_stop_allows_reset(true, 10, true, 11));
    assert!(predecessor_stop_allows_reset(true, 10, false, 11));
}

#[test]
fn clean_break_failed_setup_retry_is_explicit_and_candidate_bound() -> Result<()> {
    let (_owner, root, mut journal) = fixture(false)?;
    journal.clean_break = true;
    journal.capture_complete = Some(true);
    journal.phase = Phase::Complete;
    journal.indexes[0].failure = Some(RebuildFailure {
        code: "native_rebuild_index_failed".into(),
        message: "source unavailable before setup acceptance".into(),
    });
    assert!(setup_repair_pending(&journal));
    // A completed reset is never made globally pending: reads and ensure must
    // not turn a source repair into an implicit service restart.
    journal.save(&root)?;
    assert!(!pending(&root)?);
    assert!(repair_pending(&root, &journal.candidate)?);
    assert!(!repair_pending(&root, &"c".repeat(64))?);
    journal.indexes[0].restored = true;
    assert!(!setup_repair_pending(&journal));
    journal.indexes[0].restored = false;
    journal.cleanup_complete = true;
    assert!(!setup_repair_pending(&journal));
    Ok(())
}

#[test]
fn clean_break_exact_success_clears_only_transient_receipt_failure() -> Result<()> {
    let (_owner, _root, mut journal) = fixture(false)?;
    let progress = &mut journal.indexes[0];
    let job = JobRecord {
        id: Sha256Digest::from_bytes([91; 32]),
        project: journal.roster.indexes[0].registration.binding,
        kind: JobKind::Index,
        state: JobState::Succeeded,
        fencing_generation: 0,
        attempts: 1,
        outcome: None,
    };
    progress.failure = Some(RebuildFailure {
        code: "clean_break_index_receipt_unavailable".into(),
        message: "temporary native read failure".into(),
    });
    assert!(record_completed_job(progress, job.clone()).is_err());
    assert!(progress.failure.is_some());
    progress.job = Some(job.id);
    record_completed_job(progress, job.clone())?;
    assert!(progress.failure.is_none());
    assert!(progress.completed.is_some());
    progress.failure = Some(RebuildFailure {
        code: "independent_failure".into(),
        message: "must remain visible".into(),
    });
    record_completed_job(progress, job)?;
    assert_eq!(
        progress.failure.as_ref().unwrap().code,
        "independent_failure"
    );
    Ok(())
}
