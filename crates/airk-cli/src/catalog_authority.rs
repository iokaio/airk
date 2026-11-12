// SPDX-License-Identifier: Apache-2.0
//! Inert native construction of frozen read authority. This issues no credential.
// Temporary inert-only allowance: remove when native callers are wired, before
// P6f acceptance. Admission remains deferred until contract 1 activation.
#![allow(dead_code)]
use airk_core::{
    catalog_management::{CatalogEntity, CatalogItem, CatalogSource},
    grant_provenance::{GrantProvenanceKind, PersistedGrant},
    identity::{Incarnation, Sha256Digest},
    index_catalog::{
        CatalogCapabilities, CatalogRevision, GrantId, IndexDefinition, IndexId, MAX_SELECTION,
        MAX_SOURCE_ROOTS, Membership, QueryProject, QueryProjectId, SourceImportState,
        unique_bounded,
    },
    scoped_reads::{ScopeReadRefused, SelectionScopeRef, SourceDescriptor},
    service::{ProjectBinding, WindowScopeSelection},
};
use airk_kernel::scoped_reads::{ScopedReadAuthority, SelectionReadAuthority};
use airk_store::service::registry::{ServiceRegistry, catalog};
use anyhow::{Result, ensure};
use std::collections::BTreeSet;

/// Exact native selection captured by the caller. `host` must be authenticated by
/// the native host; a client-supplied host digest is not authentication.
#[derive(Clone)]
pub(crate) struct NativeCatalogAuthoritySelection {
    pub namespace: Incarnation,
    pub host: Sha256Digest,
    pub catalog_revision: CatalogRevision,
    pub grant_id: GrantId,
    pub grant_revision: CatalogRevision,
    pub index_id: IndexId,
    pub project_ids: Vec<QueryProjectId>,
    pub backing: ProjectBinding,
}
impl NativeCatalogAuthoritySelection {
    fn validate(&self) -> Result<()> {
        unique_bounded(&self.project_ids, MAX_SELECTION)?;
        ensure!(!self.project_ids.is_empty(), ScopeReadRefused);
        Ok(())
    }
}

/// Read only persisted reviewed authority at one exact catalog revision. There
/// is no worker installation, renewal, bearer issuance or activation here. The
/// caller retains the namespace/revision fence and must recheck it at admission
/// and delivery; a returned payload alone is not live authorization.
/// NativeBroker must separately validate active backing registration and worker
/// identity at issuance and delivery. Catalog metadata proves no availability.
pub(crate) fn build_catalog_read_authority(
    registry: &mut ServiceRegistry,
    selection: &NativeCatalogAuthoritySelection,
) -> Result<ScopedReadAuthority> {
    build_catalog_read_authority_kind(
        registry,
        selection,
        GrantProvenanceKind::UserReviewedProjectGrant,
    )
}

/// The trusted native legacy-binding route resolves only an existing sealed
/// compatibility record. Absence/revocation never falls back or creates a grant.
pub(crate) fn build_legacy_catalog_read_authority(
    registry: &mut ServiceRegistry,
    namespace: Incarnation,
    host: Sha256Digest,
    expected_catalog_revision: CatalogRevision,
    legacy_binding: ProjectBinding,
) -> Result<ScopedReadAuthority> {
    let persisted =
        registry.legacy_compatibility_snapshot(legacy_binding, host, expected_catalog_revision)?;
    persisted.validate()?;
    ensure!(
        persisted.provenance.imported_binding() == Some(legacy_binding)
            && persisted.grant.memberships.len() == 1,
        ScopeReadRefused
    );
    let member = &persisted.grant.memberships[0].membership;
    let selection = NativeCatalogAuthoritySelection {
        namespace,
        host,
        catalog_revision: expected_catalog_revision,
        grant_id: persisted.grant.id,
        grant_revision: persisted.grant.revision,
        index_id: member.index_id,
        project_ids: vec![member.project_id],
        backing: legacy_binding,
    };
    build_catalog_read_authority_kind(registry, &selection, persisted.provenance.kind())
}

fn build_catalog_read_authority_kind(
    registry: &mut ServiceRegistry,
    selection: &NativeCatalogAuthoritySelection,
    expected_kind: GrantProvenanceKind,
) -> Result<ScopedReadAuthority> {
    selection.validate()?;
    let state = registry.catalog_state()?.ok_or(ScopeReadRefused)?;
    validate_catalog_fence(&state, selection, &catalog::capabilities())?;
    let detail = |registry: &mut ServiceRegistry, entity| -> Result<CatalogItem> {
        let result = registry.catalog_detail(&entity, Some(selection.catalog_revision))?;
        ensure!(
            result.namespace == selection.namespace
                && result.revision == selection.catalog_revision,
            ScopeReadRefused
        );
        result.item.ok_or_else(|| ScopeReadRefused.into())
    };
    let CatalogItem::Grant(persisted) = detail(
        registry,
        CatalogEntity::Grant {
            id: selection.grant_id,
        },
    )?
    else {
        return Err(ScopeReadRefused.into());
    };
    persisted.validate()?;
    ensure!(
        persisted.provenance.kind() == expected_kind,
        ScopeReadRefused
    );
    let provenance = persisted.provenance;
    let grant = persisted.grant;
    let CatalogItem::Index(index) = detail(
        registry,
        CatalogEntity::Index {
            id: selection.index_id,
        },
    )?
    else {
        return Err(ScopeReadRefused.into());
    };
    require_policy_ready(registry, &index)?;
    // Validate the frozen grant before using it to bound any further lookups.
    grant.validate()?;
    ensure!(
        grant.id == selection.grant_id
            && !grant.revoked
            && grant.host == selection.host
            && grant.revision == selection.grant_revision,
        ScopeReadRefused
    );
    let mut projects = Vec::new();
    let mut memberships = Vec::new();
    let mut source_ids = BTreeSet::new();
    for id in &selection.project_ids {
        let frozen = grant
            .memberships
            .iter()
            .find(|m| m.membership.project_id == *id && m.membership.index_id == selection.index_id)
            .ok_or(ScopeReadRefused)?;
        source_ids.extend(frozen.membership.scope.iter().map(|a| a.source_root_id));
        ensure!(source_ids.len() <= MAX_SOURCE_ROOTS, ScopeReadRefused);
        let CatalogItem::Project(project) = detail(registry, CatalogEntity::Project { id: *id })?
        else {
            return Err(ScopeReadRefused.into());
        };
        projects.push(project);
        let CatalogItem::Membership(member) = detail(
            registry,
            CatalogEntity::Membership {
                project_id: *id,
                index_id: selection.index_id,
            },
        )?
        else {
            return Err(ScopeReadRefused.into());
        };
        memberships.push(member);
    }
    let mut sources = Vec::new();
    for id in source_ids {
        let CatalogItem::Source(source) = detail(registry, CatalogEntity::Source { id })? else {
            return Err(ScopeReadRefused.into());
        };
        sources.push(source);
    }
    validate_snapshot(
        selection,
        PersistedGrant { grant, provenance },
        index,
        &projects,
        &memberships,
        sources,
    )
}

/// One index of an in-memory W4/A2 selection grant, as the owning plane
/// resolved it (selection authority spec §3.2 step 4). Carried only on the
/// in-process admission call; never on a `ControlMethod`. The read projects
/// are computed by the builder, never supplied.
#[derive(Clone, Debug)]
pub(crate) struct SelectionConnectRequest {
    pub selection_ref: SelectionScopeRef,
    pub window_selection: WindowScopeSelection,
    pub index_id: IndexId,
}
impl SelectionConnectRequest {
    /// The index is one of the selection's; the selection names at least one
    /// project (§3.5) and was granted at exactly the ref's revision.
    fn validate(&self) -> Result<()> {
        let selection = &self.window_selection;
        unique_bounded(&selection.index_ids, MAX_SELECTION)?;
        unique_bounded(&selection.project_ids, MAX_SELECTION)?;
        ensure!(
            selection.index_ids.contains(&self.index_id)
                && !selection.project_ids.is_empty()
                && selection.expected_catalog_revision == self.selection_ref.catalog_revision,
            ScopeReadRefused
        );
        Ok(())
    }
}

/// Build the read authority of one index of a selection grant at exactly the
/// grant's catalog revision. Read-only (`catalog_state`/`catalog_detail`);
/// it writes nothing and creates no `GrantId` (§3.8). The caller holds the
/// gate's issue ticket at that same revision and rechecks it at delivery.
pub(crate) fn build_selection_read_authority(
    registry: &mut ServiceRegistry,
    request: &SelectionConnectRequest,
) -> Result<SelectionReadAuthority> {
    request.validate()?;
    let namespace = request.window_selection.namespace;
    let revision = request.selection_ref.catalog_revision;
    let state = registry.catalog_state()?.ok_or(ScopeReadRefused)?;
    let capabilities = catalog::capabilities();
    ensure!(
        state.activated
            && state.namespace == namespace
            && state.revision == revision
            && state.authority_contract == 1
            && state.policy_contract
                <= airk_store::service::registry::catalog::SUPPORTED_POLICY_CONTRACT
            && capabilities.supports(state.schema, 1, state.policy_contract),
        ScopeReadRefused
    );
    let detail = |registry: &mut ServiceRegistry, entity| -> Result<Option<CatalogItem>> {
        let result = registry.catalog_detail(&entity, Some(revision))?;
        ensure!(
            result.namespace == namespace && result.revision == revision,
            ScopeReadRefused
        );
        Ok(result.item)
    };
    let Some(CatalogItem::Index(index)) = detail(
        registry,
        CatalogEntity::Index {
            id: request.index_id,
        },
    )?
    else {
        return Err(ScopeReadRefused.into());
    };
    require_policy_ready(registry, &index)?;
    let mut projects = Vec::new();
    let mut memberships = Vec::new();
    let mut source_ids = BTreeSet::new();
    for id in &request.window_selection.project_ids {
        let Some(CatalogItem::Project(project)) =
            detail(registry, CatalogEntity::Project { id: *id })?
        else {
            return Err(ScopeReadRefused.into());
        };
        projects.push(project);
        // A selected project without a membership of this index is not a
        // read project of it (§3.2 step 3); the subset is checked below.
        if let Some(CatalogItem::Membership(member)) = detail(
            registry,
            CatalogEntity::Membership {
                project_id: *id,
                index_id: request.index_id,
            },
        )? {
            source_ids.extend(member.scope.iter().map(|atom| atom.source_root_id));
            ensure!(source_ids.len() <= MAX_SOURCE_ROOTS, ScopeReadRefused);
            memberships.push(member);
        }
    }
    let mut sources = Vec::new();
    for id in source_ids {
        let Some(CatalogItem::Source(source)) = detail(registry, CatalogEntity::Source { id })?
        else {
            return Err(ScopeReadRefused.into());
        };
        sources.push(source);
    }
    validate_selection_snapshot(request, index, &projects, &memberships, sources)
}

/// Pure checks of `build_selection_read_authority` over the observed catalog
/// items: those of `validate_snapshot` minus the grant, plus: every read
/// project has a current membership of the index with a non-empty scope (an
/// empty scope grants nothing, F7/N6), the read-project subset is non-empty,
/// the import is complete and every source is enabled and owned by the index.
fn validate_selection_snapshot(
    request: &SelectionConnectRequest,
    index: IndexDefinition,
    projects: &[QueryProject],
    memberships: &[Membership],
    sources: Vec<CatalogSource>,
) -> Result<SelectionReadAuthority> {
    request.validate()?;
    index.validate()?;
    let selected = &request.window_selection.project_ids;
    ensure!(
        index.id == request.index_id
            && !index.tombstone
            && index.source_import == SourceImportState::Complete
            && projects.len() == selected.len(),
        ScopeReadRefused
    );
    for id in selected {
        projects
            .iter()
            .find(|project| project.id == *id)
            .ok_or(ScopeReadRefused)?
            .validate()?;
    }
    let mut project_ids = Vec::new();
    let mut atoms = Vec::new();
    let mut membership_revisions = Vec::new();
    for id in selected {
        let Some(member) = memberships
            .iter()
            .find(|m| m.project_id == *id && m.index_id == request.index_id)
        else {
            continue;
        };
        member.validate()?;
        if member.scope.is_empty() {
            continue;
        }
        project_ids.push(*id);
        atoms.extend(member.scope.iter().map(|atom| (*id, atom.clone())));
        membership_revisions.push((*id, member.revision));
    }
    ensure!(
        !project_ids.is_empty()
            && memberships.iter().all(|member| {
                member.index_id == request.index_id && selected.contains(&member.project_id)
            }),
        ScopeReadRefused
    );
    let used: BTreeSet<_> = atoms.iter().map(|(_, atom)| atom.source_root_id).collect();
    ensure!(sources.len() <= MAX_SOURCE_ROOTS, ScopeReadRefused);
    let mut descriptors = Vec::new();
    for source in sources {
        ensure!(
            source.index_id == request.index_id && source.enabled,
            ScopeReadRefused
        );
        // Sources of dropped (empty-scope) memberships are not loaded twice;
        // only the used set is described, and it must be complete below.
        if used.contains(&source.id) {
            descriptors.push(SourceDescriptor {
                source_root_id: source.id,
                original_root: source.canonical_path,
            });
        }
    }
    let authority = SelectionReadAuthority {
        schema: 1,
        backing: index.backing.binding,
        index_id: request.index_id,
        project_ids,
        selection: request.selection_ref,
        atoms,
        sources: descriptors,
        membership_revisions,
        source_revision: index.source_revision,
        policy_epoch: index.policy.as_ref().map_or(0, |policy| policy.epoch.get()),
        policy_digest: index.policy.as_ref().map(|policy| policy.digest),
    };
    // Kernel validation: source set equals the atom union, per-project atoms
    // and revisions, nothing newer than the grant's revision, 48 KiB.
    authority.validate()?;
    Ok(authority)
}

/// Empty-policy indexes retain the existing metadata-only fast path. A policy
/// reference is usable only after its native removal report binds the exact
/// saved policy. Physical reclamation may continue without reviving evidence.
pub(crate) fn require_policy_ready(
    registry: &ServiceRegistry,
    index: &IndexDefinition,
) -> Result<()> {
    let Some(reference) = &index.policy else {
        return Ok(());
    };
    let snapshot = registry
        .catalog_policy_status(index.id)?
        .ok_or(ScopeReadRefused)?;
    ensure!(
        snapshot.policy.index_id == index.id
            && snapshot.policy.owner_binding == index.backing.binding
            && snapshot.policy.epoch == reference.epoch
            && snapshot.policy.identity()? == reference.digest
            && snapshot.application.as_ref().is_some_and(|report| {
                report.policy_digest == reference.digest
                    && report.operation_id == snapshot.operation_id
            }),
        ScopeReadRefused
    );
    Ok(())
}

fn validate_catalog_fence(
    state: &catalog::CatalogState,
    selection: &NativeCatalogAuthoritySelection,
    capabilities: &CatalogCapabilities,
) -> Result<()> {
    ensure!(
        state.activated
            && state.namespace == selection.namespace
            && state.revision == selection.catalog_revision
            && state.authority_contract == 1
            && state.policy_contract
                <= airk_store::service::registry::catalog::SUPPORTED_POLICY_CONTRACT
            && capabilities.supports(state.schema, 1, state.policy_contract),
        ScopeReadRefused
    );
    Ok(())
}

fn validate_snapshot(
    selection: &NativeCatalogAuthoritySelection,
    persisted: PersistedGrant,
    index: IndexDefinition,
    projects: &[QueryProject],
    memberships: &[Membership],
    sources: Vec<CatalogSource>,
) -> Result<ScopedReadAuthority> {
    selection.validate()?;
    persisted.validate()?;
    let PersistedGrant { grant, provenance } = persisted;
    index.validate()?;
    ensure!(
        grant.id == selection.grant_id
            && !grant.revoked
            && grant.host == selection.host
            && grant.revision == selection.grant_revision
            && index.id == selection.index_id
            && index.backing.binding == selection.backing
            && !index.tombstone
            && index.source_import == SourceImportState::Complete
            && projects.len() == selection.project_ids.len()
            && memberships.len() == selection.project_ids.len(),
        ScopeReadRefused
    );
    for id in &selection.project_ids {
        let project = projects
            .iter()
            .find(|p| p.id == *id)
            .ok_or(ScopeReadRefused)?;
        project.validate()?;
        let current = memberships
            .iter()
            .find(|m| m.project_id == *id && m.index_id == selection.index_id)
            .ok_or(ScopeReadRefused)?;
        current.validate()?;
        let frozen = grant
            .memberships
            .iter()
            .find(|m| m.membership.project_id == *id && m.membership.index_id == selection.index_id)
            .ok_or(ScopeReadRefused)?;
        ensure!(
            frozen.membership.revision == current.revision
                && frozen.membership.scope_is_subset_of(current)
                && frozen.source_revision == index.source_revision
                && frozen.policy_epoch
                    == index.policy.as_ref().map_or(0, |policy| policy.epoch.get())
                && frozen.policy_digest == index.policy.as_ref().map(|policy| policy.digest),
            ScopeReadRefused
        );
    }
    ensure!(sources.len() <= MAX_SOURCE_ROOTS, ScopeReadRefused);
    let descriptors = sources
        .into_iter()
        .map(|source| {
            // `owner` marks the primary root; external roots belong to the
            // same index through their immutable index/source identities.
            ensure!(
                source.index_id == selection.index_id && source.enabled,
                ScopeReadRefused
            );
            Ok(SourceDescriptor {
                source_root_id: source.id,
                original_root: source.canonical_path,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let authority = ScopedReadAuthority {
        provenance,
        schema: 1,
        backing: selection.backing,
        index_id: selection.index_id,
        project_ids: selection.project_ids.clone(),
        grant,
        sources: descriptors,
    };
    // Kernel validates the exact used descriptor set and the 48 KiB wire budget.
    // Its session constructor separately proves backing and published roots.
    authority.validate()?;
    Ok(authority)
}

#[cfg(test)]
mod tests {
    use super::*;
    use airk_core::{
        grant_provenance::{GrantProvenance, native_imported_grant_id},
        index_catalog::{
            GrantedMembership, PathScope, PolicyReference, ReviewedGrant, ScopeAtom, SourceRootId,
        },
        service::ProjectRegistration,
    };

    #[derive(Clone)]
    struct Fixture {
        selection: NativeCatalogAuthoritySelection,
        grant: ReviewedGrant,
        index: IndexDefinition,
        projects: Vec<QueryProject>,
        members: Vec<Membership>,
        sources: Vec<CatalogSource>,
    }
    impl Fixture {
        fn new() -> Result<Self> {
            let identity = Incarnation::from_bytes([1; 16]);
            let project_id = QueryProjectId::from_identity(identity);
            let index_id = IndexId::from_identity(identity);
            let source_id = SourceRootId::from_identity(identity);
            let grant_id = GrantId::from_identity(identity);
            let host = Sha256Digest::from_bytes([2; 32]);
            let revision = CatalogRevision::INITIAL;
            let root = std::env::current_dir()?.to_string_lossy().into_owned();
            let backing = ProjectRegistration::new(root.clone(), root.clone(), "fixture".into())?;
            let selection = NativeCatalogAuthoritySelection {
                namespace: identity,
                host,
                catalog_revision: revision,
                grant_id,
                grant_revision: revision,
                index_id,
                project_ids: vec![project_id],
                backing: backing.binding,
            };
            let member = Membership {
                project_id,
                index_id,
                revision,
                scope: vec![ScopeAtom {
                    source_root_id: source_id,
                    kind: PathScope::Subtree,
                    path: String::new(),
                }],
            };
            let mut narrowed = member.clone();
            narrowed.scope[0].path = "selected".into();
            let grant = ReviewedGrant {
                id: grant_id,
                host,
                revision,
                revoked: false,
                memberships: vec![GrantedMembership {
                    membership: narrowed,
                    source_revision: revision,
                    policy_epoch: 0,
                    policy_digest: None,
                }],
            };
            Ok(Self {
                selection,
                grant,
                index: IndexDefinition {
                    id: index_id,
                    name: "Fixture".into(),
                    backing,
                    definition_revision: revision,
                    source_revision: revision,
                    policy: None,
                    tombstone: false,
                    source_import: SourceImportState::Complete,
                },
                projects: vec![QueryProject {
                    id: project_id,
                    name: "Project".into(),
                    definition_revision: revision,
                    scope_revision: revision,
                    defaults_revision: revision,
                    default_indexes: Vec::new(),
                }],
                members: vec![member],
                sources: vec![CatalogSource {
                    id: source_id,
                    index_id,
                    canonical_path: root,
                    owner: true,
                    enabled: true,
                    aliases: Vec::new(),
                }],
            })
        }
        fn build(self) -> Result<ScopedReadAuthority> {
            validate_snapshot(
                &self.selection,
                PersistedGrant {
                    grant: self.grant,
                    provenance: GrantProvenance::UserReviewedProjectGrant,
                },
                self.index,
                &self.projects,
                &self.members,
                self.sources,
            )
        }
    }

    #[test]
    fn frozen_narrowing_never_expands_to_current_membership_or_defaults() -> Result<()> {
        let mut f = Fixture::new()?;
        f.projects[0]
            .default_indexes
            .push(IndexId::from_identity(Incarnation::from_bytes([7; 16])));
        let expected = f.grant.clone();
        let authority = f.build()?;
        assert_eq!(authority.grant, expected);
        assert_eq!(
            authority.grant.memberships[0].membership.scope[0].path,
            "selected"
        );
        assert_eq!(authority.sources.len(), 1);
        Ok(())
    }

    #[test]
    fn imported_snapshot_keeps_same_validation_and_explicit_provenance() -> Result<()> {
        let mut f = Fixture::new()?;
        let provenance = GrantProvenance::ImportedLegacySnapshot {
            upgrade_id: f.selection.namespace,
            plan_digest: Sha256Digest::from_bytes([8; 32]),
            legacy_binding: f.selection.backing,
            configuration_digest: Sha256Digest::from_bytes([9; 32]),
        };
        f.grant.id =
            native_imported_grant_id(f.selection.namespace, f.selection.host, &provenance)?;
        f.selection.grant_id = f.grant.id;
        f.grant.memberships[0].membership = f.members[0].clone();
        let persisted = PersistedGrant {
            grant: f.grant.clone(),
            provenance: provenance.clone(),
        };
        let authority = validate_snapshot(
            &f.selection,
            persisted,
            f.index.clone(),
            &f.projects,
            &f.members,
            f.sources.clone(),
        )?;
        assert_eq!(authority.provenance, provenance);
        let mut changed_policy = f.index.clone();
        changed_policy.policy = Some(PolicyReference {
            epoch: CatalogRevision::INITIAL,
            digest: Sha256Digest::from_bytes([7; 32]),
        });
        assert!(
            validate_snapshot(
                &f.selection,
                PersistedGrant {
                    grant: f.grant.clone(),
                    provenance: provenance.clone()
                },
                changed_policy,
                &f.projects,
                &f.members,
                f.sources.clone(),
            )
            .is_err(),
            "an imported epoch-zero snapshot cannot renew itself after trim"
        );
        f.grant.revoked = true;
        assert!(
            validate_snapshot(
                &f.selection,
                PersistedGrant {
                    grant: f.grant,
                    provenance
                },
                f.index,
                &f.projects,
                &f.members,
                f.sources
            )
            .is_err()
        );
        Ok(())
    }

    #[test]
    fn exact_selection_refuses_missing_duplicate_and_ungranted_projects() -> Result<()> {
        let f = Fixture::new()?;
        let mut missing = f.clone();
        missing.projects.clear();
        assert!(missing.build().is_err());
        let mut removed = f.clone();
        removed.members.clear();
        assert!(removed.build().is_err());
        let mut duplicate = f.clone();
        duplicate
            .selection
            .project_ids
            .push(f.selection.project_ids[0]);
        assert!(duplicate.build().is_err());
        let mut empty = f.clone();
        empty.selection.project_ids.clear();
        assert!(empty.build().is_err());
        let mut unknown = f;
        unknown.selection.project_ids[0] =
            QueryProjectId::from_identity(Incarnation::from_bytes([9; 16]));
        assert!(unknown.build().is_err());
        Ok(())
    }

    #[test]
    fn stale_revoked_host_backing_and_policy_fail_closed() -> Result<()> {
        let f = Fixture::new()?;
        let mut revoked = f.clone();
        revoked.grant.revoked = true;
        assert!(revoked.build().is_err());
        let mut stale = f.clone();
        stale.selection.grant_revision = CatalogRevision::INITIAL.next()?;
        assert!(stale.build().is_err());
        let mut host = f.clone();
        host.selection.host = Sha256Digest::from_bytes([8; 32]);
        assert!(host.build().is_err());
        let mut backing = f.clone();
        backing.selection.backing = Sha256Digest::from_bytes([8; 32]);
        assert!(backing.build().is_err());
        let mut source = f.clone();
        source.index.source_revision = CatalogRevision::INITIAL.next()?;
        assert!(source.build().is_err());
        let mut member = f.clone();
        member.members[0].revision = CatalogRevision::INITIAL.next()?;
        assert!(member.build().is_err());
        let mut policy = f;
        policy.index.policy = Some(PolicyReference {
            epoch: CatalogRevision::INITIAL,
            digest: Sha256Digest::from_bytes([8; 32]),
        });
        assert!(policy.build().is_err());
        Ok(())
    }

    #[test]
    fn reviewed_policy_authority_requires_the_exact_frozen_epoch_and_digest() -> Result<()> {
        let mut f = Fixture::new()?;
        let digest = Sha256Digest::from_bytes([8; 32]);
        let epoch = CatalogRevision::INITIAL.next()?;
        f.index.policy = Some(PolicyReference { epoch, digest });
        f.grant.memberships[0].policy_epoch = epoch.get();
        f.grant.memberships[0].policy_digest = Some(digest);
        let expected = f.grant.clone();
        assert_eq!(f.clone().build()?.grant, expected);
        for changed in 0..4 {
            let mut stale = f.clone();
            match changed {
                0 => stale.grant.memberships[0].policy_epoch = 1,
                1 => {
                    stale.grant.memberships[0].policy_digest =
                        Some(Sha256Digest::from_bytes([9; 32]))
                }
                2 => stale.index.policy = None,
                _ => {
                    stale.grant.memberships[0].policy_epoch = 0;
                    stale.grant.memberships[0].policy_digest = None;
                }
            }
            assert!(stale.build().is_err(), "policy case {changed}");
        }
        Ok(())
    }

    #[test]
    fn sibling_scope_and_unready_disabled_or_foreign_source_are_refused() -> Result<()> {
        let f = Fixture::new()?;
        let mut sibling = f.clone();
        sibling.members[0].scope[0].path = "selected-other".into();
        assert!(sibling.build().is_err());
        let mut exact = f.clone();
        exact.members[0].scope[0].kind = PathScope::Exact;
        exact.members[0].scope[0].path = "selected".into();
        assert!(exact.build().is_err());
        let mut pending = f.clone();
        pending.index.source_import = SourceImportState::PendingMetadata;
        assert!(pending.build().is_err());
        let mut tombstone = f.clone();
        tombstone.index.tombstone = true;
        assert!(tombstone.build().is_err());
        let mut disabled = f.clone();
        disabled.sources[0].enabled = false;
        assert!(disabled.build().is_err());
        let mut wrong = f;
        wrong.sources[0].index_id = IndexId::from_identity(Incarnation::from_bytes([8; 16]));
        assert!(wrong.build().is_err());
        Ok(())
    }

    #[test]
    fn enabled_external_root_is_authorized_by_owning_index_not_primary_flag() -> Result<()> {
        let external_directory = tempfile::tempdir()?;
        let external_root = external_directory
            .path()
            .canonicalize()?
            .to_string_lossy()
            .into_owned();
        let mut f = Fixture::new()?;
        let id = SourceRootId::from_identity(Incarnation::from_bytes([8; 16]));
        f.sources.push(CatalogSource {
            id,
            index_id: f.selection.index_id,
            canonical_path: external_root.clone(),
            owner: false,
            enabled: true,
            aliases: vec!["external".into()],
        });
        let atom = ScopeAtom {
            source_root_id: id,
            kind: PathScope::Subtree,
            path: "allowed".into(),
        };
        f.members[0].scope.push(atom.clone());
        f.grant.memberships[0].membership.scope.push(atom);
        let authority = f.build()?;
        assert_eq!(authority.sources.len(), 2);
        assert!(
            authority
                .sources
                .iter()
                .any(|source| source.source_root_id == id && source.original_root == external_root)
        );
        Ok(())
    }

    #[test]
    fn descriptors_must_equal_selected_frozen_atom_sources() -> Result<()> {
        let f = Fixture::new()?;
        let mut missing = f.clone();
        missing.sources.clear();
        assert!(missing.build().is_err());
        let mut extra = f.clone();
        let mut source = extra.sources[0].clone();
        source.id = SourceRootId::from_identity(Incarnation::from_bytes([8; 16]));
        source.canonical_path.push_str("/other");
        extra.sources.push(source);
        assert!(extra.build().is_err());
        let mut duplicate = f.clone();
        duplicate.sources.push(f.sources[0].clone());
        assert!(duplicate.build().is_err());
        // Unselected reviewed memberships do not cause their descriptors to be loaded.
        let mut unselected = f;
        let mut other = unselected.grant.memberships[0].clone();
        other.membership.project_id =
            QueryProjectId::from_identity(Incarnation::from_bytes([7; 16]));
        other.membership.scope[0].source_root_id =
            SourceRootId::from_identity(Incarnation::from_bytes([7; 16]));
        unselected.grant.memberships.push(other);
        assert_eq!(unselected.build()?.sources.len(), 1);
        Ok(())
    }

    #[test]
    fn builder_does_not_create_or_upgrade_a_legacy_catalog() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let f = Fixture::new()?;
        let mut registry = ServiceRegistry::open(directory.path(), f.selection.namespace)?;
        assert!(registry.catalog_state()?.is_none());
        assert!(build_catalog_read_authority(&mut registry, &f.selection).is_err());
        assert!(registry.catalog_state()?.is_none());
        Ok(())
    }

    #[test]
    fn authority_zero_and_changed_catalog_fences_are_refused() -> Result<()> {
        let f = Fixture::new()?;
        // Pure metadata fixture only: no predecessor/upgrade proof is fabricated.
        let state = catalog::CatalogState {
            schema: 12,
            namespace: f.selection.namespace,
            revision: f.selection.catalog_revision,
            activated: true,
            accepted_new_writes: false,
            authority_contract: 1,
            policy_contract: 0,
            upgrade_id: f.selection.namespace,
            legacy_schema_digest: f.selection.host,
            backup_digest: f.selection.host,
        };
        let capabilities = CatalogCapabilities {
            maximum_registry_schema: 12,
            authority_contract: 1,
            policy_contract: 0,
            reference_refresh_contract: None,
            production_activation_enabled: false,
        };
        validate_catalog_fence(&state, &f.selection, &capabilities)?;
        // Unsupported runtime contract 0 refuses an otherwise valid snapshot.
        // Production capabilities and persisted activation remain separate gates.
        let unsupported = CatalogCapabilities {
            authority_contract: 0,
            ..capabilities.clone()
        };
        assert!(validate_catalog_fence(&state, &f.selection, &unsupported).is_err());
        let actual = catalog::capabilities();
        assert_eq!(
            validate_catalog_fence(&state, &f.selection, &actual).is_ok(),
            actual.supports(12, 1, 0)
        );
        let mut authority_zero = state.clone();
        authority_zero.authority_contract = 0;
        assert!(validate_catalog_fence(&authority_zero, &f.selection, &capabilities).is_err());
        let mut namespace = state.clone();
        namespace.namespace = Incarnation::from_bytes([8; 16]);
        assert!(validate_catalog_fence(&namespace, &f.selection, &capabilities).is_err());
        let mut revision = state.clone();
        revision.revision = revision.revision.next()?;
        assert!(validate_catalog_fence(&revision, &f.selection, &capabilities).is_err());
        let mut policy = state.clone();
        policy.policy_contract = 1;
        assert!(validate_catalog_fence(&policy, &f.selection, &capabilities).is_err());
        let policy_capabilities = CatalogCapabilities {
            policy_contract: 1,
            ..capabilities.clone()
        };
        validate_catalog_fence(&policy, &f.selection, &policy_capabilities)?;
        policy.policy_contract = 2;
        assert!(validate_catalog_fence(&policy, &f.selection, &policy_capabilities).is_err());
        let mut inactive = state.clone();
        inactive.activated = false;
        assert!(validate_catalog_fence(&inactive, &f.selection, &capabilities).is_err());
        let mut schema = state;
        schema.schema = 13;
        assert!(validate_catalog_fence(&schema, &f.selection, &capabilities).is_err());
        Ok(())
    }

    #[test]
    fn root_count_and_encoded_authority_budgets_are_enforced() -> Result<()> {
        let mut too_many = Fixture::new()?;
        let source = too_many.sources[0].clone();
        too_many.sources = (1..=18)
            .map(|n| CatalogSource {
                id: SourceRootId::from_identity(Incarnation::from_bytes([n; 16])),
                canonical_path: format!("{}/root{n}", source.canonical_path),
                ..source.clone()
            })
            .collect();
        too_many.members[0].scope = too_many
            .sources
            .iter()
            .map(|s| ScopeAtom {
                source_root_id: s.id,
                kind: PathScope::Subtree,
                path: String::new(),
            })
            .collect();
        too_many.grant.memberships[0].membership = too_many.members[0].clone();
        assert!(too_many.build().is_err());

        let mut oversized = Fixture::new()?;
        let source_root_id = oversized.sources[0].id;
        oversized.members[0].scope = (0..16)
            .map(|n| ScopeAtom {
                source_root_id,
                kind: PathScope::Exact,
                path: format!("{n:02}{}", "x".repeat(4000)),
            })
            .collect();
        oversized.grant.memberships[0].membership = oversized.members[0].clone();
        assert!(oversized.build().is_err());
        Ok(())
    }

    fn selection_request(f: &Fixture) -> SelectionConnectRequest {
        SelectionConnectRequest {
            selection_ref: SelectionScopeRef {
                owner: airk_core::scoped_reads::SelectionOwnerKind::Window,
                grant_nonce: Incarnation::from_bytes([0x5c; 16]),
                service_instance: Incarnation::from_bytes([0xe1; 16]),
                scope_identity: Sha256Digest::from_bytes([0x33; 32]),
                catalog_revision: f.selection.catalog_revision,
            },
            window_selection: WindowScopeSelection {
                namespace: f.selection.namespace,
                expected_catalog_revision: f.selection.catalog_revision,
                index_ids: vec![f.selection.index_id],
                project_ids: f.selection.project_ids.clone(),
            },
            index_id: f.selection.index_id,
        }
    }
    fn build_selection(
        f: &Fixture,
        request: &SelectionConnectRequest,
    ) -> Result<SelectionReadAuthority> {
        validate_selection_snapshot(
            request,
            f.index.clone(),
            &f.projects,
            &f.members,
            f.sources.clone(),
        )
    }

    #[test]
    fn selection_authority_reads_current_membership_atoms_at_the_grant_revision() -> Result<()> {
        let f = Fixture::new()?;
        let request = selection_request(&f);
        let authority = build_selection(&f, &request)?;
        let project = QueryProjectId::from_identity(Incarnation::from_bytes([1; 16]));
        let source = SourceRootId::from_identity(Incarnation::from_bytes([1; 16]));
        // The current membership's atom (path ""), never the reviewed grant's
        // narrowed "selected" atom: a selection has no reviewed grant.
        assert_eq!(authority.project_ids, vec![project]);
        assert_eq!(
            authority.atoms,
            vec![(
                project,
                ScopeAtom {
                    source_root_id: source,
                    kind: PathScope::Subtree,
                    path: String::new(),
                }
            )]
        );
        assert_eq!(
            authority.membership_revisions,
            vec![(project, CatalogRevision::INITIAL)]
        );
        assert_eq!(authority.sources.len(), 1);
        assert_eq!(authority.sources[0].source_root_id, source);
        assert_eq!(authority.backing, f.index.backing.binding);
        assert_eq!(authority.selection, request.selection_ref);
        assert_eq!((authority.policy_epoch, authority.policy_digest), (0, None));
        Ok(())
    }

    #[test]
    fn selection_authority_drops_empty_scope_memberships_and_refuses_an_empty_subset() -> Result<()>
    {
        let mut f = Fixture::new()?;
        let second = QueryProjectId::from_identity(Incarnation::from_bytes([2; 16]));
        let mut project = f.projects[0].clone();
        project.id = second;
        f.projects.push(project);
        let mut empty = f.members[0].clone();
        empty.project_id = second;
        empty.scope.clear();
        f.members.push(empty);
        let mut request = selection_request(&f);
        request.window_selection.project_ids.push(second);
        let first = QueryProjectId::from_identity(Incarnation::from_bytes([1; 16]));
        assert_eq!(build_selection(&f, &request)?.project_ids, vec![first]);
        // A selected project with no membership of the index is not read.
        let mut absent = f.clone();
        absent.members.pop();
        assert_eq!(build_selection(&absent, &request)?.project_ids, vec![first]);
        // With every membership empty or absent, the subset is empty: refused.
        let mut none = f.clone();
        none.members[0].scope.clear();
        assert!(build_selection(&none, &request).is_err());
        let mut gone = f;
        gone.members.clear();
        assert!(build_selection(&gone, &request).is_err());
        Ok(())
    }

    #[test]
    fn selection_authority_refuses_foreign_retired_unready_or_projectless_selections() -> Result<()>
    {
        let f = Fixture::new()?;
        let base = selection_request(&f);
        let mut outside = base.clone();
        outside.index_id = IndexId::from_identity(Incarnation::from_bytes([9; 16]));
        assert!(build_selection(&f, &outside).is_err(), "index outside");
        let mut projectless = base.clone();
        projectless.window_selection.project_ids.clear();
        assert!(build_selection(&f, &projectless).is_err(), "no project");
        let mut drifted = base.clone();
        drifted.selection_ref.catalog_revision = CatalogRevision::INITIAL.next()?;
        assert!(build_selection(&f, &drifted).is_err(), "revision drift");
        let mut retired = f.clone();
        retired.index.tombstone = true;
        assert!(build_selection(&retired, &base).is_err(), "retired");
        let mut pending = f.clone();
        pending.index.source_import = SourceImportState::PendingMetadata;
        assert!(build_selection(&pending, &base).is_err(), "import pending");
        let mut disabled = f.clone();
        disabled.sources[0].enabled = false;
        assert!(build_selection(&disabled, &base).is_err(), "disabled");
        let mut foreign = f.clone();
        foreign.sources[0].index_id = IndexId::from_identity(Incarnation::from_bytes([8; 16]));
        assert!(build_selection(&foreign, &base).is_err(), "foreign source");
        let mut unknown = f.clone();
        unknown.projects.clear();
        assert!(build_selection(&unknown, &base).is_err(), "unknown project");
        let mut missing = f;
        missing.sources.clear();
        assert!(build_selection(&missing, &base).is_err(), "missing source");
        Ok(())
    }
}
