// SPDX-License-Identifier: Apache-2.0
use super::*;
use airk_core::{
    index_catalog::{CatalogRevision, GrantId, IndexId, QueryProjectId},
    service::ScopedConnectRequest,
    service_http::HttpSelection,
};
use std::{io::Cursor, sync::Arc};

fn binding() -> InstallationBinding {
    InstallationBinding {
        installation: Sha256Digest::from_bytes([1; 32]),
        user: crate::local_ipc::current_user().unwrap(),
        namespace: Incarnation::from_bytes([2; 16]),
    }
}
fn selection() -> HttpSelection {
    let id = Incarnation::from_bytes([3; 16]);
    HttpSelection::Scoped {
        selection: ScopedConnectRequest {
            schema: 1,
            context_key: Sha256Digest::from_bytes([4; 32]),
            namespace: binding().namespace,
            expected_catalog_revision: CatalogRevision::INITIAL,
            grant_id: GrantId::from_identity(id),
            expected_grant_revision: CatalogRevision::INITIAL,
            index_id: IndexId::from_identity(id),
            project_ids: vec![QueryProjectId::from_identity(id)],
        },
    }
}

fn standalone_selection() -> anyhow::Result<HttpSelection> {
    use airk_core::{
        catalog_legacy_adoption::{
            LegacyAdoptionPlan, LegacyAdoptionPublication, source_inventory_digest,
        },
        catalog_legacy_authority::{StandaloneAdoptionProvenance, StandaloneConfigurationClaim},
        catalog_setup::{IndexSetupRequest, SetupSource},
        grant_provenance::{GrantProvenance, PersistedGrant, native_imported_grant_id},
        identity::RevisionToken,
        index_catalog::{
            GrantedMembership, Membership, PathScope, ReviewedGrant, ScopeAtom, SourceRootId,
        },
        service::{ProjectRegistration, RequestKey},
        service_http::NativeImportedHttpSelection,
    };
    let digest = Sha256Digest::from_bytes([42; 32]);
    let id = Incarnation::from_bytes([43; 16]);
    let registration =
        ProjectRegistration::new("/source".into(), "/adopted".into(), "saved".into())?;
    let setup = IndexSetupRequest {
        caller: id,
        key: RequestKey {
            namespace: binding().namespace,
            epoch: 1,
            nonce: id,
        },
        expected_catalog_revision: CatalogRevision::INITIAL,
        name: "Saved evidence".into(),
        registration: registration.clone(),
        sources: vec![SetupSource {
            canonical_path: "/source".into(),
            aliases: vec![],
            owner: true,
        }],
        setup_watch: false,
    };
    let operation_id =
        airk_core::catalog_management::catalog_operation_id(setup.caller, &setup.key)?;
    let plan = LegacyAdoptionPlan {
        schema: 1,
        setup,
        original_registration: ProjectRegistration::new(
            "/source".into(),
            "/original".into(),
            "saved".into(),
        )?,
        origin_sha256: digest,
        config_sha256: digest,
        executable_sha256: digest,
        quiescence_sha256: digest,
        archive_inventory_sha256: digest,
        original_publication_sha256: digest,
        source_inventory_sha256: source_inventory_digest("/source", "saved", &Default::default())?,
    };
    let publication = LegacyAdoptionPublication {
        schema: 1,
        plan_digest: plan.identity()?,
        archive_inventory_sha256: digest,
        original_publication_sha256: digest,
        current: RevisionToken {
            incarnation: id,
            generation: 1,
            revision: digest,
        },
        resource_sha256: digest,
        compatibility_sha256: digest,
    };
    let provenance = GrantProvenance::ImportedStandaloneAdoption {
        adoption: Box::new(StandaloneAdoptionProvenance {
            schema: 1,
            operation_id,
            plan: Box::new(plan),
            publication,
            original_server: "saved_alias".into(),
            no_default_project: false,
            configuration_claims: vec![StandaloneConfigurationClaim {
                schema: 1,
                origin_sha256: digest,
                config_sha256: digest,
                executable_sha256: digest,
                selected_entry_sha256: digest,
                container_path: vec!["mcp_servers".into()],
                original_server: "saved_alias".into(),
                no_default_project: false,
            }],
        }),
    };
    let selected = HttpSelection::ImportedLegacy {
        selection: Box::new(NativeImportedHttpSelection {
            namespace: binding().namespace,
            expected_catalog_revision: CatalogRevision::INITIAL,
            registration,
            configuration_receipt: digest,
            grant: PersistedGrant {
                grant: ReviewedGrant {
                    id: native_imported_grant_id(binding().namespace, digest, &provenance)?,
                    host: digest,
                    revision: CatalogRevision::INITIAL,
                    revoked: false,
                    memberships: vec![GrantedMembership {
                        membership: Membership {
                            project_id: QueryProjectId::from_identity(id),
                            index_id: IndexId::from_identity(id),
                            revision: CatalogRevision::INITIAL,
                            scope: vec![ScopeAtom {
                                source_root_id: SourceRootId::from_identity(id),
                                kind: PathScope::Subtree,
                                path: String::new(),
                            }],
                        },
                        source_revision: CatalogRevision::INITIAL,
                        policy_epoch: 0,
                        policy_digest: None,
                    }],
                },
                provenance,
            },
        }),
    };
    selected.validate()?;
    Ok(selected)
}

fn attachment(root: &Path, session: &str) -> anyhow::Result<std::path::PathBuf> {
    use sha2::{Digest, Sha256};
    let assistance = root.join("explorer-assistance");
    let windows = assistance.join("windows");
    crate::local_ipc::prepare_root(&assistance)?;
    crate::local_ipc::prepare_root(&windows)?;
    let identity = format!("{session}\0{}", root.display());
    let fingerprint = format!("{:x}", Sha256::digest(identity.as_bytes()));
    let file = windows.join(format!("{session}.json"));
    write_private(
        &windows,
        file.file_name().unwrap().to_str().unwrap(),
        &serde_json::to_vec(&serde_json::json!({
            "schema":"airk-explorer-window/1", "session_id":session,"endpoint":"fixture", "credential":"a".repeat(64),
            "roots":[root],"binding_fingerprint":&fingerprint[..16], "extension_version":"1.8.14","expires_unix_ms":1000,
        }))?,
        16384,
        false,
    )?;
    Ok(file)
}

#[test]
fn authority_digest_matches_shared_cross_language_golden_vectors() -> anyhow::Result<()> {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../packages/vscode/src/test/fixtures/httpAuthorityDigest.json"
    ))?;
    for vector in fixture["vectors"].as_array().unwrap() {
        let selected: HttpSelection = serde_json::from_value(vector["selection"].clone())?;
        assert_eq!(
            authority_digest(&selected)?.to_string(),
            vector["authority_sha256"].as_str().unwrap()
        );
    }
    Ok(())
}

#[test]
fn historical_attached_ids_require_native_provenance_and_mutation_time_revocation_check()
-> anyhow::Result<()> {
    use airk_core::usage::ClientFamily;
    let root = private_root();
    let mut store = CredentialStore::open(root.path(), binding())?;
    let selected = standalone_selection()?;
    let old = attachment(root.path(), &"1".repeat(32))?;
    let new = attachment(root.path(), &"2".repeat(32))?;
    // The predecessor ID did not incorporate its attachment. Native metadata,
    // never ID shape, must distinguish this attached owner from a manual one.
    let id = Incarnation::from_bytes([71; 16]);
    let next = Incarnation::from_bytes([72; 16]);
    store.enroll_default_for_client(
        id,
        selected.clone(),
        Some(old.clone()),
        ClientFamily::Codex,
        1,
    )?;
    let token = read_client_token(root.path(), id)?;
    assert!(store.attachment_retirement(id, 2)?.is_none());
    std::fs::remove_file(&old)?;
    let initial = store.attachment_retirement(id, 2)?.unwrap();
    assert_eq!(initial.client, ClientFamily::Codex);
    assert_eq!(initial.authority_sha256, authority_digest(&selected)?);
    store.validate_reattachment(id, next, &selected, ClientFamily::Codex, Some(&new), 2)?;
    assert!(
        store
            .validate_reattachment(id, next, &selected, ClientFamily::Codex, None, 2)
            .is_err()
    );
    assert!(
        store
            .validate_reattachment(id, next, &selected, ClientFamily::Codex, Some(&old), 2)
            .is_err()
    );
    assert!(
        store
            .validate_reattachment(id, next, &selected, ClientFamily::ClaudeCode, Some(&new), 2)
            .is_err()
    );
    let (retired, result) = store.revoke_inactive(2);
    result?;
    assert_eq!(retired, vec![id]);
    store.compact_revoked()?;
    let marker = std::fs::read(root.path().join(format!("credential-{id}.token")))?;
    assert!(marker.len() <= 2048);
    assert!(!String::from_utf8(marker)?.contains(&token));
    store = CredentialStore::open(root.path(), binding())?;
    assert_eq!(
        store
            .attachment_retirement(id, 2)?
            .unwrap()
            .authority_sha256,
        initial.authority_sha256
    );
    store.validate_reattachment(id, next, &selected, ClientFamily::Codex, Some(&new), 2)?;
    let same_old = attachment(root.path(), &"1".repeat(32))?;
    assert!(
        store
            .validate_reattachment(id, next, &selected, ClientFamily::Codex, Some(&same_old), 2)
            .is_err()
    );
    let mut crossed = selected.clone();
    if let HttpSelection::ImportedLegacy { selection } = &mut crossed {
        selection.grant.grant.revision = CatalogRevision::INITIAL.next()?;
    }
    assert!(
        store
            .validate_reattachment(id, next, &crossed, ClientFamily::Codex, Some(&new), 2)
            .is_err()
    );
    // Lookup already succeeded; an explicit revoke before issuance must win,
    // even after the original full credential record has compacted away.
    store.revoke(id)?;
    assert!(store.attachment_retirement(id, 2)?.is_none());
    assert!(
        store
            .validate_reattachment(id, next, &selected, ClientFamily::Codex, Some(&new), 2)
            .is_err()
    );
    Ok(())
}

#[test]
fn retirement_proof_refuses_manual_expired_unknown_and_recovers_compaction_crash()
-> anyhow::Result<()> {
    let root = private_root();
    let mut store = CredentialStore::open(root.path(), binding())?;
    let manual = Incarnation::from_bytes([73; 16]);
    store.enroll_default(manual, selection(), 1)?;
    assert!(store.attachment_retirement(manual, 2)?.is_none());
    let expired = Incarnation::from_bytes([74; 16]);
    let window = attachment(root.path(), &"3".repeat(32))?;
    store.enroll_attached(expired, selection(), Some(window.clone()), 2, 1)?;
    std::fs::remove_file(window)?;
    assert!(store.attachment_retirement(expired, 2)?.is_none());
    let (ids, result) = store.revoke_inactive(2);
    result?;
    assert_eq!(ids, vec![expired]);
    store.compact_revoked()?;
    assert!(store.attachment_retirement(expired, 2)?.is_none());
    let unknown = Incarnation::from_bytes([75; 16]);
    let unknown_file = format!("credential-{unknown}.token");
    write_private(
        root.path(),
        &unknown_file,
        &serde_json::to_vec(
            &serde_json::json!({"schema":1,"binding":binding(),"id":unknown,"revoked":true}),
        )?,
        2048,
        false,
    )?;
    assert_eq!(store.ownership(unknown, 2)?, ConnectionOwnership::Revoked);
    assert!(store.attachment_retirement(unknown, 2)?.is_none());
    // Model auto marker publication preceding its full-record compaction.
    let crash = Incarnation::from_bytes([76; 16]);
    let old = attachment(root.path(), &"4".repeat(32))?;
    store.enroll_default_attached(crash, selection(), Some(old.clone()), 1)?;
    std::fs::remove_file(old)?;
    store.revoke_inactive(2).1?;
    let full_record = std::fs::read(root.path().join("credentials.json"))?;
    store.compact_revoked()?;
    std::fs::write(root.path().join("credentials.json"), full_record)?;
    store.revoke(crash)?;
    store.compact_revoked()?;
    assert!(store.attachment_retirement(crash, 2)?.is_none());
    // An earlier full revoked record and an ownership-only marker must never
    // acquire an inferred automatic-retirement reason during crash recovery.
    for (number, explicit_after) in [(77, false), (78, true)] {
        let unknown_full = Incarnation::from_bytes([number; 16]);
        store.enroll_default(unknown_full, selection(), 1)?;
        store.revoke(unknown_full)?;
        let ledger_path = root.path().join("credentials.json");
        let mut ledger: serde_json::Value = serde_json::from_slice(&std::fs::read(&ledger_path)?)?;
        for record in ledger["records"].as_array_mut().unwrap() {
            if record["id"] == serde_json::json!(unknown_full) {
                record.as_object_mut().unwrap().remove("revocation_reason");
            }
        }
        std::fs::write(&ledger_path, serde_json::to_vec(&ledger)?)?;
        write_private(
            root.path(),
            &format!("credential-{unknown_full}.token"),
            &serde_json::to_vec(
                &serde_json::json!({"schema":1,"binding":binding(),"id":unknown_full,"revoked":true}),
            )?,
            2048,
            true,
        )?;
        if explicit_after {
            store.revoke(unknown_full)?;
        }
        store.compact_revoked()?;
        assert!(store.attachment_retirement(unknown_full, 2)?.is_none());
        let marker: serde_json::Value = serde_json::from_slice(&std::fs::read(
            root.path().join(format!("credential-{unknown_full}.token")),
        )?)?;
        assert_eq!(
            marker["retirement"]["reason"],
            if explicit_after {
                "explicit"
            } else {
                "unknown"
            }
        );
    }
    Ok(())
}
struct PrivateRoot {
    _parent: tempfile::TempDir,
    path: std::path::PathBuf,
}
impl PrivateRoot {
    fn path(&self) -> &Path {
        &self.path
    }
}
fn private_root() -> PrivateRoot {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("http");
    crate::local_ipc::prepare_root(&path).unwrap();
    PrivateRoot {
        _parent: directory,
        path,
    }
}
fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis()
        .try_into()
        .unwrap()
}

#[test]
fn cleanup_owner_census_retains_complete_metadata_without_reading_active_token_bodies()
-> anyhow::Result<()> {
    use airk_core::service_http_cleanup::HttpCleanupOwnership;
    let root = private_root();
    let mut store = CredentialStore::open(root.path(), binding())?;
    let active = Incarnation::from_bytes([201; 16]);
    let expired = Incarnation::from_bytes([202; 16]);
    let revoked = Incarnation::from_bytes([203; 16]);
    let issued = store.enroll(active, selection(), 100, 1)?;
    store.enroll(expired, selection(), 2, 1)?;
    store.enroll(revoked, selection(), 100, 1)?;
    let token = read_client_token(root.path(), active)?;
    let verifier = digest(token.as_bytes());
    // The cleanup read uses the already committed verifier, not the current
    // secret bytes and not live authentication. A corrupt body is never read.
    write_private(
        root.path(),
        issued
            .credential_file
            .file_name()
            .unwrap()
            .to_str()
            .unwrap(),
        &[b'x'; 103],
        103,
        true,
    )?;
    store.revoke(revoked)?;
    let before = std::fs::read(root.path().join("credentials.json"))?;
    let owners = store.cleanup_owners(3)?;
    assert_eq!(owners.len(), 3);
    assert_eq!(
        owners
            .iter()
            .find(|owner| owner.connection_id == active)
            .unwrap()
            .ownership,
        HttpCleanupOwnership::Active
    );
    assert_eq!(
        owners
            .iter()
            .find(|owner| owner.connection_id == active)
            .unwrap()
            .credential_sha256,
        Some(verifier)
    );
    assert_eq!(
        owners
            .iter()
            .find(|owner| owner.connection_id == expired)
            .unwrap()
            .ownership,
        HttpCleanupOwnership::Expired
    );
    assert_eq!(
        owners
            .iter()
            .find(|owner| owner.connection_id == revoked)
            .unwrap()
            .ownership,
        HttpCleanupOwnership::Revoked
    );
    assert!(owners.iter().all(|owner| owner.credential_sha256.is_some()));
    let encoded = serde_json::to_string(&owners)?;
    assert!(!encoded.contains(&token));
    assert!(!encoded.contains("airk1."));
    assert_eq!(std::fs::read(root.path().join("credentials.json"))?, before);
    std::fs::remove_file(issued.credential_file)?;
    assert_eq!(
        store
            .cleanup_owners(3)?
            .iter()
            .find(|owner| owner.connection_id == active)
            .unwrap()
            .credential_sha256,
        Some(verifier),
        "a missing secret cannot erase committed ownership"
    );
    store.compact_revoked()?;
    let compacted = store.cleanup_owners(3)?;
    assert_eq!(compacted.len(), 3);
    let retired = compacted
        .iter()
        .find(|owner| owner.connection_id == revoked)
        .unwrap();
    assert_eq!(retired.ownership, HttpCleanupOwnership::Revoked);
    assert_eq!(
        retired.credential_sha256, None,
        "a marker without a verifier must not invent a credential hash"
    );
    Ok(())
}

#[test]
fn cleanup_owner_census_refuses_orphans_and_missing_or_corrupt_authority_without_mutation()
-> anyhow::Result<()> {
    let root = private_root();
    let id = Incarnation::from_bytes([204; 16]);
    let mut store = CredentialStore::open(root.path(), binding())?;
    assert!(
        store.cleanup_owners(1).is_err(),
        "an unpersisted ledger is not a complete empty inventory"
    );
    store.enroll(id, selection(), 100, 1)?;
    let original_ledger = std::fs::read(root.path().join("credentials.json"))?;
    let orphan = root.path().join(format!(
        "credential-{}.token",
        Incarnation::from_bytes([205; 16])
    ));
    write_private(
        root.path(),
        orphan.file_name().unwrap().to_str().unwrap(),
        &[b'x'; 103],
        103,
        false,
    )?;
    assert!(
        store.cleanup_owners(1).is_err(),
        "a native-sized orphan token is unobserved, never foreign"
    );
    assert_eq!(std::fs::read(&orphan)?, vec![b'x'; 103]);
    assert_eq!(
        std::fs::read(root.path().join("credentials.json"))?,
        original_ledger
    );
    std::fs::remove_file(&orphan)?;
    write_private(
        root.path(),
        orphan.file_name().unwrap().to_str().unwrap(),
        &[b'{'; 200],
        2048,
        false,
    )?;
    assert!(
        store.cleanup_owners(1).is_err(),
        "unknown marker bytes do not prove retired ownership"
    );
    std::fs::remove_file(&orphan)?;
    std::fs::remove_file(root.path().join("credentials.json"))?;
    assert!(
        store.cleanup_owners(1).is_err(),
        "a stale in-memory ledger cannot replace missing native authority"
    );
    assert!(!root.path().join("credentials.json").exists());
    write_private(root.path(), "credentials.json", b"{}", 1_048_576, false)?;
    assert!(store.cleanup_owners(1).is_err());
    assert_eq!(std::fs::read(root.path().join("credentials.json"))?, b"{}");
    Ok(())
}

#[test]
fn cleanup_owner_census_refuses_a_roster_over_128_instead_of_returning_a_subset()
-> anyhow::Result<()> {
    use airk_core::service_http_cleanup::{HttpCleanupOwnership, MAX_HTTP_CLEANUP_OWNERS};
    let root = private_root();
    let mut store = CredentialStore::open(root.path(), binding())?;
    let first = Incarnation::from_bytes(1u128.to_be_bytes());
    store.enroll(first, selection(), 100, 1)?;
    store.revoke(first)?;
    store.compact_revoked()?;
    for value in 2..=MAX_HTTP_CLEANUP_OWNERS as u128 {
        let id = Incarnation::from_bytes(value.to_be_bytes());
        let marker = serde_json::to_vec(
            &serde_json::json!({"schema":1,"binding":binding(),"id":id,"revoked":true}),
        )?;
        write_private(
            root.path(),
            &format!("credential-{id}.token"),
            &marker,
            2048,
            false,
        )?;
    }
    let owners = store.cleanup_owners(1)?;
    assert_eq!(owners.len(), MAX_HTTP_CLEANUP_OWNERS);
    assert!(
        owners
            .iter()
            .all(|owner| owner.ownership == HttpCleanupOwnership::Revoked
                && owner.credential_sha256.is_none())
    );
    let id = Incarnation::from_bytes(129u128.to_be_bytes());
    let marker = serde_json::to_vec(
        &serde_json::json!({"schema":1,"binding":binding(),"id":id,"revoked":true}),
    )?;
    write_private(
        root.path(),
        &format!("credential-{id}.token"),
        &marker,
        2048,
        false,
    )?;
    assert!(store.cleanup_owners(1).is_err());
    assert!(
        root.path()
            .join(format!("credential-{first}.token"))
            .exists()
    );
    assert!(root.path().join(format!("credential-{id}.token")).exists());
    Ok(())
}

fn owned_http_initialization_fixture() -> anyhow::Result<(
    PrivateRoot,
    airk_store::service::ServiceLease,
    std::path::PathBuf,
    InstallationBinding,
)> {
    let root = private_root();
    let control = root.path().join("control");
    crate::local_ipc::prepare_root(&control)?;
    airk_store::service::registry::ServiceRegistry::open(&control, binding().namespace)?;
    let owner = airk_store::service::ServiceLease::acquire(&control)?;
    let http = owner.root().join("http-1");
    let binding = installation_binding(owner.root(), binding().namespace)?;
    Ok((root, owner, http, binding))
}

#[test]
fn service_owned_empty_ledger_is_durable_without_repairing_later_missing_authority()
-> anyhow::Result<()> {
    use airk_core::service_http::{EndpointDescriptor, EndpointState, LoopbackMcpUrl};
    let (_root, owner, http, binding) = owned_http_initialization_fixture()?;
    let mut store = CredentialStore::open(&http, binding.clone())?;
    assert!(CredentialStore::read_existing(&http, binding.clone()).is_err());
    store.initialize_empty_owned(&owner)?;
    assert!(store.cleanup_owners(1)?.is_empty());
    assert!(
        CredentialStore::read_existing(&http, binding.clone())?
            .cleanup_owners(1)?
            .is_empty()
    );
    let empty = std::fs::read(http.join("credentials.json"))?;
    store.initialize_empty_owned(&owner)?;
    assert_eq!(std::fs::read(http.join("credentials.json"))?, empty);
    let id = Incarnation::from_bytes([206; 16]);
    store.enroll(id, selection(), 100, 1)?;
    let issued = std::fs::read(http.join("credentials.json"))?;
    store.initialize_empty_owned(&owner)?;
    assert_eq!(
        std::fs::read(http.join("credentials.json"))?,
        issued,
        "service startup must not overwrite a valid ledger"
    );
    let descriptor = EndpointDescriptor {
        schema: 1,
        installation: binding.installation,
        user: binding.user.clone(),
        namespace: binding.namespace,
        generation: 1,
        service_instance: Incarnation::from_bytes([207; 16]),
        executable_sha256: Sha256Digest::from_bytes([208; 32]),
        url: LoopbackMcpUrl::new(57421)?,
        ca_sha256: Sha256Digest::from_bytes([209; 32]),
        certificate_sha256: Sha256Digest::from_bytes([210; 32]),
        state: EndpointState::Ready,
        predecessor_digest: None,
    };
    publish_descriptor(&http, &binding, &descriptor, None)?;
    std::fs::remove_file(http.join("credentials.json"))?;
    std::fs::remove_file(http.join(format!("credential-{id}.token")))?;
    // Even an empty in-memory open cannot turn a historical publication into
    // fresh installation evidence after its durable ledger disappears.
    let mut reopened = CredentialStore::open(&http, binding.clone())?;
    let error = reopened.initialize_empty_owned(&owner).unwrap_err();
    assert!(error.to_string().contains("published HTTP endpoint"));
    assert!(!http.join("credentials.json").exists());
    assert!(CredentialStore::read_existing(&http, binding.clone()).is_err());
    assert_eq!(load_descriptor(&http, &binding)?, Some(descriptor));
    Ok(())
}

#[test]
fn service_owned_empty_ledger_refuses_foreign_owner_or_orphan_or_corrupt_history()
-> anyhow::Result<()> {
    let (_root, owner, http, binding) = owned_http_initialization_fixture()?;
    let (_other_root, other_owner, _, _) = owned_http_initialization_fixture()?;
    let mut store = CredentialStore::open(&http, binding.clone())?;
    assert!(store.initialize_empty_owned(&other_owner).is_err());
    assert!(!http.join("credentials.json").exists());
    let foreign_binding = installation_binding(owner.root(), Incarnation::from_bytes([212; 16]))?;
    let mut foreign = CredentialStore::open(&http, foreign_binding)?;
    assert!(foreign.initialize_empty_owned(&owner).is_err());
    assert!(!http.join("credentials.json").exists());
    for name in ["tls-identity.json", "ca.pem", "issuer-rotation.json"] {
        write_private(
            &http,
            name,
            b"retained native initialization history",
            1024,
            false,
        )?;
        assert!(
            store.initialize_empty_owned(&owner).is_err(),
            "a missing ledger cannot erase interrupted or damaged issuer history"
        );
        assert!(!http.join("credentials.json").exists());
        assert_eq!(
            std::fs::read(http.join(name))?,
            b"retained native initialization history"
        );
        std::fs::remove_file(http.join(name))?;
    }
    let orphan = format!("credential-{}.token", Incarnation::from_bytes([211; 16]));
    write_private(&http, &orphan, &[b'x'; 103], 103, false)?;
    assert!(store.initialize_empty_owned(&owner).is_err());
    assert!(!http.join("credentials.json").exists());
    assert_eq!(std::fs::read(http.join(&orphan))?, [b'x'; 103]);
    std::fs::remove_file(http.join(orphan))?;
    store.initialize_empty_owned(&owner)?;
    write_private(&http, "credentials.json", b"{}", 1_048_576, true)?;
    assert!(store.initialize_empty_owned(&owner).is_err());
    assert_eq!(std::fs::read(http.join("credentials.json"))?, b"{}");
    Ok(())
}

#[test]
fn retired_credentials_reclaim_capacity_preserve_manual_authority_and_never_reissue_ids()
-> anyhow::Result<()> {
    let root = private_root();
    let mut store = CredentialStore::open(root.path(), binding())?;
    let manual_id = Incarnation::from_bytes([250; 16]);
    store.enroll_default(manual_id, selection(), 1)?;
    let manual_token = read_client_token(root.path(), manual_id)?;
    // More than the former lifetime limit, including reopened readers.
    for number in 1..=140u128 {
        let id = Incarnation::from_bytes(number.to_be_bytes());
        store.enroll_default(id, selection(), 1)?;
        let token = read_client_token(root.path(), id)?;
        store.revoke(id)?;
        let (retired, result) = store.revoke_inactive(2);
        result?;
        assert_eq!(retired, vec![id]);
        store.compact_revoked()?;
        store = CredentialStore::open(root.path(), binding())?;
        assert!(store.authenticate(&token, 2).is_err());
        assert_eq!(store.ownership(id, 2)?, ConnectionOwnership::Revoked);
        assert!(store.enroll_default(id, selection(), 2).is_err());
        store.revoke(id)?; // Idempotent after compaction.
        let marker = std::fs::read(root.path().join(format!("credential-{id}.token")))?;
        assert!(marker.len() <= 1024);
        assert!(!String::from_utf8(marker)?.contains(&token));
    }
    assert_eq!(
        store.authenticate(&manual_token, 2)?.connection_id,
        manual_id
    );
    let ledger: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.path().join("credentials.json"))?)?;
    assert_eq!(ledger["records"].as_array().unwrap().len(), 1);
    Ok(())
}

#[test]
fn retirement_preserves_crash_recovery_and_does_not_invent_orphan_ownership() -> anyhow::Result<()>
{
    let root = private_root();
    let mut store = CredentialStore::open(root.path(), binding())?;
    let id = Incarnation::from_bytes([63; 16]);
    let issued = store.enroll(id, selection(), 2, 1)?;
    let original_token = read_client_token(root.path(), id)?;
    let (ids, result) = store.revoke_inactive(2);
    result?;
    assert_eq!(ids, vec![id]);
    assert!(store.authenticate(&original_token, 1).is_err());
    let revoked_ledger = std::fs::read(root.path().join("credentials.json"))?;
    // A missing client secret cannot erase accepted native issuance.
    std::fs::remove_file(&issued.credential_file)?;
    store.compact_revoked()?;
    assert_eq!(store.ownership(id, 3)?, ConnectionOwnership::Revoked);
    // Simulate marker publication followed by failed ledger compaction.
    std::fs::write(root.path().join("credentials.json"), &revoked_ledger)?;
    store.compact_revoked()?;
    assert_eq!(store.ownership(id, 3)?, ConnectionOwnership::Revoked);
    assert!(store.enroll_default(id, selection(), 3).is_err());
    // An immutable token left by failed issuance is unknown, not proof of a
    // committed connection and not a foreign declaration eligible for cleanup.
    std::fs::write(&issued.credential_file, original_token.as_bytes())?;
    assert!(store.ownership(id, 3).is_err());
    assert!(store.enroll_default(id, selection(), 3).is_err());
    Ok(())
}

#[test]
fn retirement_rejects_crossed_marker_and_preserves_unproven_attachment() -> anyhow::Result<()> {
    let root = private_root();
    let mut store = CredentialStore::open(root.path(), binding())?;
    let id = Incarnation::from_bytes([64; 16]);
    let issued = store.enroll_default_attached(
        id,
        selection(),
        Some(root.path().join("unknown.json")),
        1,
    )?;
    let token = read_client_token(root.path(), id)?;
    let (ids, result) = store.revoke_inactive(2);
    result?;
    assert!(ids.is_empty());
    assert!(store.authenticate(&token, 2).is_ok());
    store.revoke(id)?;
    store.compact_revoked()?;
    let mut marker: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&issued.credential_file)?)?;
    marker["binding"]["installation"] = serde_json::to_value(Sha256Digest::from_bytes([99; 32]))?;
    std::fs::write(&issued.credential_file, serde_json::to_vec(&marker)?)?;
    assert!(store.ownership(id, 2).is_err());
    assert!(store.enroll_default(id, selection(), 2).is_err());
    Ok(())
}

#[test]
fn only_withdrawn_exact_private_window_namespace_retires_attached_credential() -> anyhow::Result<()>
{
    let root = private_root();
    let assistance = root.path().join("explorer-assistance");
    let windows = assistance.join("windows");
    crate::local_ipc::prepare_root(&assistance)?;
    crate::local_ipc::prepare_root(&windows)?;
    let path = windows.join("11111111111111111111111111111111.json");
    let id = Incarnation::from_bytes([65; 16]);
    let mut store = CredentialStore::open(root.path(), binding())?;
    store.enroll_default_attached(id, selection(), Some(path.clone()), 1)?;
    // Malformed existing metadata is unknown, even in the correct directory.
    super::write_private(
        &windows,
        path.file_name().unwrap().to_str().unwrap(),
        b"{}",
        100,
        false,
    )?;
    let (ids, result) = store.revoke_inactive(2);
    result?;
    assert!(ids.is_empty());
    std::fs::remove_file(path)?;
    let (ids, result) = store.revoke_inactive(2);
    result?;
    assert_eq!(ids, vec![id]);
    store.compact_revoked()?;
    assert_eq!(store.ownership(id, 2)?, ConnectionOwnership::Revoked);
    Ok(())
}

#[test]
fn readonly_connection_ownership_never_turns_missing_ledger_revocation_or_missing_token_into_absence()
 {
    let root = private_root();
    let id = Incarnation::from_bytes([44; 16]);
    assert!(CredentialStore::read_existing(root.path(), binding()).is_err());
    assert!(!root.path().join("credentials.json").exists());
    let mut store = CredentialStore::open(root.path(), binding()).unwrap();
    assert!(
        store.ownership(id, now()).is_err(),
        "an unpersisted empty ledger invented authenticated absence"
    );
    let time = now();
    let issued = store.enroll_default(id, selection(), time).unwrap();
    let before = std::fs::read(root.path().join("credentials.json")).unwrap();
    let reader = CredentialStore::read_existing(root.path(), binding()).unwrap();
    assert_eq!(
        reader.ownership(id, time).unwrap(),
        ConnectionOwnership::Active
    );
    assert_eq!(
        reader
            .ownership(Incarnation::from_bytes([45; 16]), time)
            .unwrap(),
        ConnectionOwnership::NotOwned
    );
    assert_eq!(
        std::fs::read(root.path().join("credentials.json")).unwrap(),
        before
    );
    std::fs::remove_file(&issued.credential_file).unwrap();
    assert_eq!(
        reader.ownership(id, time).unwrap(),
        ConnectionOwnership::Active,
        "a missing secret erased native ownership"
    );
    assert_eq!(
        reader.ownership(id, u64::MAX).unwrap(),
        ConnectionOwnership::Expired
    );
    store.revoke(id).unwrap();
    assert_eq!(
        reader.ownership(id, time).unwrap(),
        ConnectionOwnership::Revoked,
        "a stale reader retained old revocation state"
    );
    assert!(
        !issued.credential_file.exists(),
        "readonly lookup recreated a deleted token"
    );
    assert!(
        store.enroll_default(id, selection(), time).is_err(),
        "revoked connection was automatically reissued"
    );
    std::fs::write(root.path().join("credentials.json"), b"{}").unwrap();
    assert!(CredentialStore::read_existing(root.path(), binding()).is_err());
    assert!(reader.ownership(id, time).is_err());
    assert_eq!(
        std::fs::read(root.path().join("credentials.json")).unwrap(),
        b"{}"
    );
}

#[test]
fn descriptor_cas_preserves_identity_and_requires_generation_for_relocation() {
    use airk_core::service_http::{EndpointDescriptor, EndpointState, LoopbackMcpUrl};
    let root = private_root();
    let binding = binding();
    let original = EndpointDescriptor {
        schema: 1,
        installation: binding.installation,
        user: binding.user.clone(),
        namespace: binding.namespace,
        generation: 1,
        service_instance: Incarnation::from_bytes([12; 16]),
        executable_sha256: Sha256Digest::from_bytes([13; 32]),
        url: LoopbackMcpUrl::new(57421).unwrap(),
        ca_sha256: Sha256Digest::from_bytes([14; 32]),
        certificate_sha256: Sha256Digest::from_bytes([15; 32]),
        state: EndpointState::Ready,
        predecessor_digest: None,
    };
    publish_descriptor(root.path(), &binding, &original, None).unwrap();
    let expected = original.digest().unwrap();
    let mut next = original.clone();
    next.predecessor_digest = Some(expected);
    next.url = LoopbackMcpUrl::new(57422).unwrap();
    assert!(publish_descriptor(root.path(), &binding, &next, Some(expected)).is_err());
    assert_eq!(
        load_descriptor(root.path(), &binding).unwrap(),
        Some(original.clone())
    );
    next.generation = 2;
    publish_descriptor(root.path(), &binding, &next, Some(expected)).unwrap();
    assert!(publish_descriptor(root.path(), &binding, &next, Some(expected)).is_err());
    assert_eq!(
        load_descriptor(root.path(), &binding).unwrap(),
        Some(next.clone())
    );
    let expected = next.digest().unwrap();
    next.predecessor_digest = Some(expected);
    next.certificate_sha256 = Sha256Digest::from_bytes([16; 32]);
    next.state = EndpointState::Stopping;
    publish_descriptor(root.path(), &binding, &next, Some(expected)).unwrap();
    assert_eq!(load_descriptor(root.path(), &binding).unwrap(), Some(next));
    let mut foreign = binding;
    foreign.namespace = Incarnation::from_bytes([17; 16]);
    assert!(load_descriptor(root.path(), &foreign).is_err());
}

#[test]
fn occupied_saved_port_falls_back_without_releasing_foreign_listener() {
    let foreign = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let port = foreign.local_addr().unwrap().port();
    let endpoint = BoundEndpoint::bind(Some(port)).unwrap();
    assert_ne!(endpoint.port().unwrap(), port);
    assert!(TcpListener::bind((Ipv4Addr::LOCALHOST, endpoint.port().unwrap())).is_err());
    assert!(TcpListener::bind((Ipv4Addr::LOCALHOST, port)).is_err());
    assert!(BoundEndpoint::bind(Some(0)).is_err());
    foreign.set_nonblocking(true).unwrap();
    assert_eq!(
        foreign.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}

#[test]
fn concurrent_port_fallback_retains_distinct_sockets_without_contacting_occupant() {
    let foreign = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let occupied = foreign.local_addr().unwrap().port();
    let barrier = Arc::new(std::sync::Barrier::new(16));
    let endpoints = std::thread::scope(|scope| {
        let threads: Vec<_> = (0..16)
            .map(|_| {
                let barrier = barrier.clone();
                scope.spawn(move || {
                    barrier.wait();
                    BoundEndpoint::bind(Some(occupied)).unwrap()
                })
            })
            .collect();
        threads
            .into_iter()
            .map(|thread| thread.join().unwrap())
            .collect::<Vec<_>>()
    });
    let ports: std::collections::BTreeSet<_> = endpoints
        .iter()
        .map(|endpoint| endpoint.port().unwrap())
        .collect();
    assert_eq!(ports.len(), endpoints.len());
    assert!(!ports.contains(&occupied));
    for port in ports {
        assert!(TcpListener::bind((Ipv4Addr::LOCALHOST, port)).is_err());
    }
    foreign.set_nonblocking(true).unwrap();
    assert_eq!(
        foreign.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}

#[test]
fn enrollment_restart_recovery_revocation_and_expiry_are_fail_closed() {
    let root = private_root();
    let id = Incarnation::from_bytes([8; 16]);
    let mut store = CredentialStore::open(root.path(), binding()).unwrap();
    let issued = store.enroll(id, selection(), 1000, 1).unwrap();
    assert_eq!(issued.generation, 1);
    let token = read_client_token(root.path(), id).unwrap();
    assert_eq!(store.authenticate(&token, 999).unwrap().connection_id, id);
    assert!(store.authenticate(&token, 1000).is_err());
    assert!(store.authenticate(&format!("{token}0"), 2).is_err());
    let bytes = std::fs::read(root.path().join("credentials.json")).unwrap();
    assert!(!String::from_utf8(bytes).unwrap().contains(&token));
    let mut reopened = CredentialStore::open(root.path(), binding()).unwrap();
    reopened.enroll(id, selection(), 1000, 2).unwrap();
    assert_eq!(read_client_token(root.path(), id).unwrap(), token);
    assert!(reopened.enroll(id, selection(), 1001, 2).is_err());
    reopened.revoke(id).unwrap();
    assert!(store.authenticate(&token, 2).is_err());
    assert!(
        CredentialStore::open(root.path(), binding())
            .unwrap()
            .authenticate(&token, 2)
            .is_err()
    );
    assert!(reopened.enroll(id, selection(), 1000, 2).is_err());
}

#[test]
fn credential_retains_its_enrolled_client_family_across_restarts() {
    use airk_core::usage::ClientFamily;
    let root = private_root();
    let id = Incarnation::from_bytes([26; 16]);
    let mut store = CredentialStore::open(root.path(), binding()).unwrap();
    store
        .enroll_default_for_client(id, selection(), None, ClientFamily::Vscode, 1)
        .unwrap();
    let token = read_client_token(root.path(), id).unwrap();
    let mut reopened = CredentialStore::open(root.path(), binding()).unwrap();
    assert_eq!(
        reopened.authenticate(&token, 2).unwrap().client,
        ClientFamily::Vscode
    );
    reopened
        .enroll_default_for_client(id, selection(), None, ClientFamily::Vscode, 2)
        .unwrap();
    assert!(
        reopened
            .enroll_default_for_client(id, selection(), None, ClientFamily::Codex, 3)
            .is_err()
    );
    assert_eq!(read_client_token(root.path(), id).unwrap(), token);
}

#[test]
fn imported_enrollment_retry_preserves_token_and_only_advances_catalog_clock() -> anyhow::Result<()>
{
    use airk_core::{
        grant_provenance::{GrantProvenance, PersistedGrant, native_imported_grant_id},
        index_catalog::{
            GrantedMembership, Membership, PathScope, ReviewedGrant, ScopeAtom, SourceRootId,
        },
        service::ProjectRegistration,
        service_http::NativeImportedHttpSelection,
    };
    let root = private_root();
    let id = Incarnation::from_bytes([18; 16]);
    let host = Sha256Digest::from_bytes([19; 32]);
    let registration = ProjectRegistration::new(
        "/original/source".into(),
        "/original/data".into(),
        "original".into(),
    )?;
    let provenance = GrantProvenance::ImportedLegacySnapshot {
        upgrade_id: id,
        plan_digest: host,
        legacy_binding: registration.binding,
        configuration_digest: host,
    };
    let original = NativeImportedHttpSelection {
        namespace: binding().namespace,
        expected_catalog_revision: CatalogRevision::INITIAL,
        registration,
        configuration_receipt: Sha256Digest::from_bytes([20; 32]),
        grant: PersistedGrant {
            grant: ReviewedGrant {
                id: native_imported_grant_id(binding().namespace, host, &provenance)?,
                host,
                revision: CatalogRevision::INITIAL,
                revoked: false,
                memberships: vec![GrantedMembership {
                    membership: Membership {
                        project_id: QueryProjectId::from_identity(id),
                        index_id: IndexId::from_identity(id),
                        revision: CatalogRevision::INITIAL,
                        scope: vec![ScopeAtom {
                            source_root_id: SourceRootId::from_identity(id),
                            kind: PathScope::Subtree,
                            path: String::new(),
                        }],
                    },
                    source_revision: CatalogRevision::INITIAL,
                    policy_epoch: 0,
                    policy_digest: None,
                }],
            },
            provenance,
        },
    };
    let selected = |value: NativeImportedHttpSelection| HttpSelection::ImportedLegacy {
        selection: Box::new(value),
    };
    // Inert storage-capacity fixture: no native grant is issued by this test.
    // Large exact unions must fail before leaving an immutable orphan token.
    let mut members = Vec::new();
    for number in 0..2 {
        let mut member = original.clone();
        member.registration = ProjectRegistration::new(
            format!("/{number}/{}", "s".repeat(3000)),
            format!("/{}", "d".repeat(3000)),
            "original".into(),
        )?;
        if let GrantProvenance::ImportedLegacySnapshot { legacy_binding, .. } =
            &mut member.grant.provenance
        {
            *legacy_binding = member.registration.binding;
        }
        member.grant.grant.id =
            native_imported_grant_id(member.namespace, host, &member.grant.provenance)?;
        members.push(member);
    }
    let union = HttpSelection::ImportedLegacyUnion {
        selection: airk_core::service_http::NativeImportedHttpUnion {
            namespace: binding().namespace,
            selections: members,
            no_default_project: true,
        },
    };
    union.validate()?;
    let capacity_root = private_root();
    let mut capacity = CredentialStore::open(capacity_root.path(), binding())?;
    let mut refused = false;
    for number in 1..=128 {
        let connection_id = Incarnation::from_bytes([number; 16]);
        if capacity
            .enroll_default(connection_id, union.clone(), 1)
            .is_err()
        {
            assert!(
                !capacity_root
                    .path()
                    .join(format!("credential-{connection_id}.token"))
                    .exists()
            );
            assert!(
                number < 128,
                "aggregate byte bound should trigger before entry count"
            );
            refused = true;
            break;
        }
    }
    assert!(refused);
    let mut store = CredentialStore::open(root.path(), binding())?;
    store.enroll_default(id, selected(original.clone()), 1)?;
    let token = read_client_token(root.path(), id)?;
    // A lost setup reply retries through a reopened store and exact connection ID.
    let mut store = CredentialStore::open(root.path(), binding())?;
    store.enroll_default(id, selected(original.clone()), 2)?;
    assert_eq!(read_client_token(root.path(), id)?, token);
    let mut advanced = original.clone();
    advanced.expected_catalog_revision = serde_json::from_value(serde_json::json!(2))?;
    store.enroll_default(id, selected(advanced.clone()), 3)?;
    assert_eq!(
        store.authenticate(&token, 4)?.selection,
        selected(advanced.clone())
    );
    assert!(store.enroll_default(id, selected(original), 5).is_err());
    let mut changed = advanced.clone();
    changed.configuration_receipt = Sha256Digest::from_bytes([21; 32]);
    assert!(store.enroll_default(id, selected(changed), 5).is_err());
    let mut changed = advanced;
    if let GrantProvenance::ImportedLegacySnapshot {
        configuration_digest,
        ..
    } = &mut changed.grant.provenance
    {
        *configuration_digest = Sha256Digest::from_bytes([22; 32]);
    }
    changed.grant.grant.id =
        native_imported_grant_id(changed.namespace, host, &changed.grant.provenance)?;
    assert!(store.enroll_default(id, selected(changed), 5).is_err());
    assert_eq!(read_client_token(root.path(), id)?, token);
    Ok(())
}

#[test]
fn orphan_token_and_cross_installation_never_authorize() {
    let root = private_root();
    let id = Incarnation::from_bytes([8; 16]);
    write_private(
        root.path(),
        &format!("credential-{id}.token"),
        b"orphan",
        103,
        false,
    )
    .unwrap();
    let mut store = CredentialStore::open(root.path(), binding()).unwrap();
    assert!(store.enroll(id, selection(), 1000, 1).is_err());
    assert!(!root.path().join("credentials.json").exists());
    let id2 = Incarnation::from_bytes([9; 16]);
    store.enroll(id2, selection(), 1000, 1).unwrap();
    let mut other = binding();
    other.installation = Sha256Digest::from_bytes([5; 32]);
    assert!(CredentialStore::open(root.path(), other).is_err());
}

#[test]
fn default_enrollment_keeps_expiry_and_token_on_later_retry() {
    let root = private_root();
    let id = Incarnation::from_bytes([10; 16]);
    let mut store = CredentialStore::open(root.path(), binding()).unwrap();
    store.enroll_default(id, selection(), 1).unwrap();
    let token = read_client_token(root.path(), id).unwrap();
    let expiry = store.authenticate(&token, 2).unwrap().expires_unix_ms;
    store.enroll_default(id, selection(), 86_400_000).unwrap();
    assert_eq!(read_client_token(root.path(), id).unwrap(), token);
    assert_eq!(
        store
            .authenticate(&token, 86_400_000)
            .unwrap()
            .expires_unix_ms,
        expiry
    );
    let mut advanced = selection();
    let HttpSelection::Scoped { selection: scope } = &mut advanced else {
        unreachable!()
    };
    scope.expected_catalog_revision = CatalogRevision::INITIAL.next().unwrap();
    store
        .enroll_default(id, advanced.clone(), 86_400_001)
        .unwrap();
    assert_eq!(
        store.authenticate(&token, 86_400_002).unwrap().selection,
        advanced
    );
    assert!(store.enroll_default(id, selection(), 86_400_003).is_err());
    assert!(store.enroll_default(id, advanced, expiry).is_err());
}

#[test]
fn default_enrollment_rejects_authority_change_and_revocation() {
    let root = private_root();
    let id = Incarnation::from_bytes([10; 16]);
    let mut store = CredentialStore::open(root.path(), binding()).unwrap();
    store.enroll_default(id, selection(), 1).unwrap();
    let mut different = selection();
    let HttpSelection::Scoped { selection: scope } = &mut different else {
        unreachable!()
    };
    scope.expected_grant_revision = CatalogRevision::INITIAL.next().unwrap();
    assert!(store.enroll_default(id, different, 2).is_err());
    let mut different = selection();
    let HttpSelection::Scoped { selection: scope } = &mut different else {
        unreachable!()
    };
    scope
        .project_ids
        .push(QueryProjectId::from_identity(Incarnation::from_bytes(
            [7; 16],
        )));
    assert!(store.enroll_default(id, different, 2).is_err());
    store.revoke(id).unwrap();
    assert!(store.enroll_default(id, selection(), 2).is_err());
}

#[test]
fn explorer_attachment_is_optional_immutable_connection_metadata() {
    let root = private_root();
    let id = Incarnation::from_bytes([11; 16]);
    let mut store = CredentialStore::open(root.path(), binding()).unwrap();
    let window = root
        .path()
        .join("explorer-assistance/windows/11111111111111111111111111111111.json");
    // Store only retains the caller-reviewed reference; actual window ownership
    // and expiry verification belong to native enrollment and request wrapper.
    let issued = store
        .enroll_default_attached(id, selection(), Some(window.clone()), 1)
        .unwrap();
    assert_eq!(issued.explorer_descriptor, Some(window.clone()));
    let token = read_client_token(root.path(), id).unwrap();
    assert_eq!(
        store.authenticate(&token, 2).unwrap().explorer_descriptor,
        Some(window.clone())
    );
    store
        .enroll_default_attached(id, selection(), Some(window), 2)
        .unwrap();
    assert!(store.enroll_default(id, selection(), 3).is_err());
    assert!(
        store
            .enroll_default_attached(
                id,
                selection(),
                Some(root.path().join("other-window.json")),
                3
            )
            .is_err()
    );
    assert!(
        store
            .enroll_default_attached(
                Incarnation::from_bytes([12; 16]),
                selection(),
                Some("relative.json".into()),
                3
            )
            .is_err()
    );
}

#[test]
fn issuer_persists_leaf_renews_and_mismatched_public_export_fails() {
    let root = private_root();
    let now = now();
    let original = TlsIdentity::load_or_create(root.path(), binding(), now).unwrap();
    let restarted = TlsIdentity::load_or_create(root.path(), binding(), now + 1000).unwrap();
    assert_eq!(original.ca_sha256(), restarted.ca_sha256());
    assert_eq!(
        original.certificate_sha256(),
        restarted.certificate_sha256()
    );
    let renewed =
        TlsIdentity::load_or_create(root.path(), binding(), now + 24 * 86_400_000).unwrap();
    assert_eq!(original.ca_sha256(), renewed.ca_sha256());
    assert_ne!(original.certificate_sha256(), renewed.certificate_sha256());
    write_private(root.path(), "ca.pem", b"different issuer", 8192, true).unwrap();
    assert!(TlsIdentity::load_or_create(root.path(), binding(), now).is_err());
}

#[test]
fn expiring_issuer_fails_without_silent_trust_replacement() {
    let root = private_root();
    let now = now();
    TlsIdentity::load_or_create(root.path(), binding(), now).unwrap();
    let bundle = std::fs::read(root.path().join("tls-identity.json")).unwrap();
    let ca = std::fs::read(root.path().join("ca.pem")).unwrap();
    assert!(TlsIdentity::load_or_create(root.path(), binding(), now + 3620 * 86_400_000).is_err());
    assert_eq!(
        std::fs::read(root.path().join("tls-identity.json")).unwrap(),
        bundle
    );
    assert_eq!(std::fs::read(root.path().join("ca.pem")).unwrap(), ca);
}

fn handshake(identity: &TlsIdentity, ca: Vec<u8>, name: &str) -> anyhow::Result<()> {
    let mut roots = rustls::RootCertStore::empty();
    roots.add(rustls::pki_types::CertificateDer::from(ca))?;
    let config = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()?
    .with_root_certificates(roots)
    .with_no_client_auth();
    let mut client = rustls::ClientConnection::new(
        Arc::new(config),
        rustls::pki_types::ServerName::try_from(name.to_owned())?,
    )?;
    let mut server = rustls::ServerConnection::new(identity.server_config())?;
    for _ in 0..8 {
        let mut wire = Vec::new();
        client.write_tls(&mut wire)?;
        server.read_tls(&mut Cursor::new(wire))?;
        server.process_new_packets()?;
        let mut wire = Vec::new();
        server.write_tls(&mut wire)?;
        client.read_tls(&mut Cursor::new(wire))?;
        client.process_new_packets()?;
        if !client.is_handshaking() && !server.is_handshaking() {
            return Ok(());
        }
    }
    anyhow::bail!("handshake did not complete")
}
fn ca_der(root: &Path) -> Vec<u8> {
    let bundle: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("tls-identity.json")).unwrap()).unwrap();
    serde_json::from_value(bundle["ca_der"].clone()).unwrap()
}
#[test]
fn real_rustls_handshake_validates_ip_and_issuer() {
    let root = private_root();
    let identity = TlsIdentity::load_or_create(root.path(), binding(), now()).unwrap();
    assert!(handshake(&identity, ca_der(root.path()), "127.0.0.1").is_ok());
    assert!(handshake(&identity, ca_der(root.path()), "localhost").is_err());
    assert!(handshake(&identity, ca_der(root.path()), "127.0.0.2").is_err());
    let other = private_root();
    TlsIdentity::load_or_create(other.path(), binding(), now()).unwrap();
    assert!(handshake(&identity, ca_der(other.path()), "127.0.0.1").is_err());
}

#[cfg(unix)]
#[test]
fn private_record_rejects_links_and_public_permissions() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let root = private_root();
    write_private(root.path(), "record", b"private", 32, false).unwrap();
    std::fs::hard_link(root.path().join("record"), root.path().join("hardlink")).unwrap();
    assert!(read_private(root.path(), "record", 32).is_err());
    std::fs::remove_file(root.path().join("hardlink")).unwrap();
    symlink(root.path().join("record"), root.path().join("symlink")).unwrap();
    assert!(read_private(root.path(), "symlink", 32).is_err());
    std::fs::set_permissions(
        root.path().join("record"),
        std::fs::Permissions::from_mode(0o644),
    )
    .unwrap();
    assert!(read_private(root.path(), "record", 32).is_err());
}
