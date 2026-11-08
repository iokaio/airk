// SPDX-License-Identifier: Apache-2.0
use super::*;

fn imported_member(seed: u8, root: String) -> Result<NativeImportedHttpSelection> {
    use crate::{
        grant_provenance::{GrantProvenance, PersistedGrant, native_imported_grant_id},
        index_catalog::*,
        service::ProjectRegistration,
    };
    let identity = Incarnation::from_bytes([seed; 16]);
    let namespace = Incarnation::from_bytes([1; 16]);
    let host = Sha256Digest::from_bytes([2; 32]);
    let registration = ProjectRegistration::new(root, "/data".into(), "original".into())?;
    let provenance = GrantProvenance::ImportedLegacySnapshot {
        upgrade_id: identity,
        plan_digest: host,
        legacy_binding: registration.binding,
        configuration_digest: host,
    };
    Ok(NativeImportedHttpSelection {
        namespace,
        expected_catalog_revision: CatalogRevision::INITIAL,
        registration,
        configuration_receipt: host,
        grant: PersistedGrant {
            grant: ReviewedGrant {
                id: native_imported_grant_id(namespace, host, &provenance)?,
                host,
                revision: CatalogRevision::INITIAL,
                revoked: false,
                memberships: vec![GrantedMembership {
                    membership: Membership {
                        project_id: QueryProjectId::from_identity(identity),
                        index_id: IndexId::from_identity(identity),
                        revision: CatalogRevision::INITIAL,
                        scope: vec![ScopeAtom {
                            source_root_id: SourceRootId::from_identity(identity),
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
    })
}

#[test]
fn imported_union_is_exact_nondefault_and_bounded_without_truncating_members() -> Result<()> {
    let union = NativeImportedHttpUnion {
        namespace: Incarnation::from_bytes([1; 16]),
        selections: vec![
            imported_member(3, "/first".into())?,
            imported_member(4, "/second".into())?,
        ],
        no_default_project: true,
    };
    let selected = HttpSelection::ImportedLegacyUnion {
        selection: union.clone(),
    };
    selected.validate()?;
    let roundtrip: HttpSelection = serde_json::from_slice(&serde_json::to_vec(&selected)?)?;
    assert_eq!(roundtrip, selected);
    let mut invalid = union.clone();
    invalid.selections.clear();
    assert!(invalid.validate().is_err());
    let mut invalid = union.clone();
    invalid.no_default_project = false;
    assert!(invalid.validate().is_err());
    let mut invalid = union.clone();
    invalid.selections[1] = invalid.selections[0].clone();
    assert!(invalid.validate().is_err());
    let mut invalid = union;
    invalid.namespace = Incarnation::from_bytes([9; 16]);
    assert!(invalid.validate().is_err());
    let selections = (3..35)
        .map(|id| imported_member(id, format!("/{id}/{}", "x".repeat(3000))))
        .collect::<Result<Vec<_>>>()?;
    for member in &selections {
        member.validate()?;
    }
    let oversized = NativeImportedHttpUnion {
        namespace: Incarnation::from_bytes([1; 16]),
        selections,
        no_default_project: true,
    };
    assert!(oversized.validate().is_err());
    assert_eq!(oversized.selections.len(), 32);
    Ok(())
}

fn endpoint() -> EndpointDescriptor {
    EndpointDescriptor {
        schema: HTTP_CONTRACT_VERSION,
        installation: Sha256Digest::from_bytes([1; 32]),
        user: "501".into(),
        namespace: Incarnation::from_bytes([2; 16]),
        generation: 1,
        service_instance: Incarnation::from_bytes([3; 16]),
        executable_sha256: Sha256Digest::from_bytes([4; 32]),
        url: LoopbackMcpUrl::new(PREFERRED_HTTP_PORT).unwrap(),
        ca_sha256: Sha256Digest::from_bytes([5; 32]),
        certificate_sha256: Sha256Digest::from_bytes([6; 32]),
        state: EndpointState::Ready,
        predecessor_digest: None,
    }
}

#[test]
fn loopback_urls_reject_redirect_and_authority_ambiguities() {
    for invalid in [
        "http://127.0.0.1:57421/mcp",
        "https://localhost:57421/mcp",
        "https://127.0.0.2:57421/mcp",
        "https://[::1]:57421/mcp",
        "https://127.0.0.1:0/mcp",
        "https://127.0.0.1:057421/mcp",
        "https://127.0.0.1:65536/mcp",
        "https://127.0.0.1:+443/mcp",
        "https://127.0.0.1:443/mcp?token=secret",
        "https://127.0.0.1:443/mcp#fragment",
        "https://user@127.0.0.1:443/mcp",
        "https://127.0.0.1:443/mcp/../api",
    ] {
        assert!(invalid.parse::<LoopbackMcpUrl>().is_err(), "{invalid}");
    }
    for port in [1, 443, PREFERRED_HTTP_PORT, u16::MAX] {
        let url = LoopbackMcpUrl::new(port).unwrap();
        assert_eq!(url.to_string().parse::<LoopbackMcpUrl>().unwrap(), url);
        assert_eq!(url.port(), port);
    }
}

#[test]
fn descriptor_rejects_unknown_versions_fields_and_oversize_before_use() -> Result<()> {
    let original = endpoint();
    assert_eq!(
        EndpointDescriptor::decode(&serde_json::to_vec(&original)?)?,
        original
    );
    let mut invalid = serde_json::to_value(&original)?;
    invalid["schema"] = 2.into();
    assert!(EndpointDescriptor::decode(&serde_json::to_vec(&invalid)?).is_err());
    invalid["schema"] = 1.into();
    invalid["token"] = "must-not-be-in-a-descriptor".into();
    assert!(EndpointDescriptor::decode(&serde_json::to_vec(&invalid)?).is_err());
    assert!(EndpointDescriptor::decode(&vec![b' '; MAX_ENDPOINT_BYTES + 1]).is_err());
    let mut relocated = original.clone();
    relocated.url = LoopbackMcpUrl::new(57422)?;
    assert_ne!(original.digest()?, relocated.digest()?);
    relocated.generation = 0;
    assert!(relocated.validate().is_err());
    Ok(())
}
