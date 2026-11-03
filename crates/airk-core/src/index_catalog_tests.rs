// SPDX-License-Identifier: Apache-2.0
use super::*;

#[test]
fn encoded_budget_counts_escaped_json_bytes_before_allocating_the_output() -> Result<()> {
    let escaped = "\0".repeat(16);
    assert_eq!(encoded_size(&escaped, 98)?, 98);
    assert!(encoded_size(&escaped, 97).is_err());
    let unicode = "é".repeat(16);
    assert_eq!(encoded_size(&unicode, 34)?, 34);
    assert!(encoded_size(&unicode, 33).is_err());
    Ok(())
}

#[test]
fn identifiers_and_revisions_are_canonical_and_do_not_cross_families() -> Result<()> {
    let identity = Incarnation::from_bytes([17; 16]);
    let project = QueryProjectId::from_identity(identity);
    let index = IndexId::from_identity(identity);
    assert_eq!(project.to_string(), "prj_11111111111111111111111111111111");
    assert_eq!(project.to_string().parse::<QueryProjectId>()?, project);
    assert!(project.to_string().parse::<IndexId>().is_err());
    assert!(index.to_string().parse::<QueryProjectId>().is_err());
    for invalid in [
        "idx_ABCDEFABCDEFABCDEFABCDEFABCDEFAB",
        "idx_11",
        "idx_11111111111111111111111111111111\n",
    ] {
        assert!(invalid.parse::<IndexId>().is_err());
    }
    assert!(serde_json::from_str::<CatalogRevision>("0").is_err());
    assert!(CatalogRevision::try_from(i64::MAX as u64)?.next().is_err());
    assert_eq!(CatalogRevision::INITIAL.next()?.get(), 2);
    Ok(())
}

#[test]
fn scope_atoms_keep_component_and_root_boundaries_and_exact_files() -> Result<()> {
    let root = SourceRootId::from_identity(Incarnation::from_bytes([1; 16]));
    let other = SourceRootId::from_identity(Incarnation::from_bytes([2; 16]));
    let mut atom = ScopeAtom {
        source_root_id: root,
        kind: PathScope::Subtree,
        path: "src/api".into(),
    };
    atom.validate()?;
    assert!(atom.matches_canonical(root, "src/api/queue.rs"));
    assert!(!atom.matches_canonical(root, "src/api-other/queue.rs"));
    assert!(!atom.matches_canonical(other, "src/api/queue.rs"));
    atom.kind = PathScope::Exact;
    assert!(atom.matches_canonical(root, "src/api"));
    assert!(!atom.matches_canonical(root, "src/api/queue.rs"));
    for path in [
        "",
        "/absolute",
        "src/../private",
        "src//api",
        "src/",
        "src\\api",
        "C:stream",
        "nul\0",
    ] {
        atom.path = path.into();
        assert!(atom.validate().is_err(), "{path:?}");
    }
    atom.kind = PathScope::Subtree;
    atom.path.clear();
    atom.validate()?;
    assert!(atom.matches_canonical(root, "any/file.rs"));
    atom.path = "é".repeat(2049);
    assert!(atom.validate().is_err(), "scope bounds count UTF-8 bytes");
    Ok(())
}

#[test]
fn context_preferences_cannot_infer_pending_authority_and_selections_are_bounded() -> Result<()> {
    let mut context = ContextChoices {
        id: ContextId::from_identity(Incarnation::from_bytes([1; 16])),
        host: Sha256Digest::from_bytes([2; 32]),
        key: Sha256Digest::from_bytes([3; 32]),
        kind: ContextKind::LegacyPending,
        name: "Pending legacy context".into(),
        revision: CatalogRevision::INITIAL,
        project_ids: vec![],
        index_ids: vec![],
    };
    context.validate()?;
    context
        .index_ids
        .push(IndexId::from_identity(Incarnation::from_bytes([4; 16])));
    assert!(context.validate().is_err());
    context
        .project_ids
        .push(QueryProjectId::from_identity(Incarnation::from_bytes(
            [5; 16],
        )));
    assert!(context.validate().is_err());
    context.kind = ContextKind::Workspace;
    context.validate()?;
    context.project_ids.push(context.project_ids[0]);
    assert!(context.validate().is_err());
    context.project_ids = (0..33)
        .map(|n| QueryProjectId::from_identity(Incarnation::from_bytes([n; 16])))
        .collect();
    assert!(context.validate().is_err());
    assert!(label(&"é".repeat(129)).is_err());
    Ok(())
}

#[test]
fn frozen_grants_bind_a_policy_digest_and_cannot_mix_empty_and_active_epochs() -> Result<()> {
    let membership = Membership {
        project_id: QueryProjectId::from_identity(Incarnation::from_bytes([1; 16])),
        index_id: IndexId::from_identity(Incarnation::from_bytes([2; 16])),
        revision: CatalogRevision::INITIAL,
        scope: vec![],
    };
    let mut grant = ReviewedGrant {
        id: GrantId::from_identity(Incarnation::from_bytes([3; 16])),
        host: Sha256Digest::from_bytes([4; 32]),
        revision: CatalogRevision::INITIAL,
        memberships: vec![GrantedMembership {
            membership,
            source_revision: CatalogRevision::INITIAL,
            policy_epoch: 0,
            policy_digest: None,
        }],
        revoked: false,
    };
    grant.validate()?;
    grant.memberships[0].policy_epoch = 1;
    assert!(grant.validate().is_err());
    grant.memberships[0].policy_digest = Some(Sha256Digest::from_bytes([5; 32]));
    grant.validate()?;
    grant.memberships[0].policy_epoch = 0;
    assert!(grant.validate().is_err());
    grant.memberships[0].policy_epoch = 1;
    grant.memberships.push(grant.memberships[0].clone());
    assert!(grant.validate().is_err());
    Ok(())
}
