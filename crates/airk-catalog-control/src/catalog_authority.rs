// SPDX-License-Identifier: Apache-2.0
//! Inert native construction of frozen read authority. This issues no credential.
//! The two authority sources are an in-memory W4/A2 selection and a U105 open
//! agent read; reviewed catalog grants were removed (change-log entry 68 part 3).
use airk_core::{
    catalog_management::{CatalogEntity, CatalogItem, CatalogSource},
    identity::Incarnation,
    index_catalog::{
        IndexDefinition, IndexId, MAX_SELECTION, MAX_SOURCE_ROOTS, Membership, QueryProject,
        SourceImportState, unique_bounded,
    },
    scoped_reads::{ScopeReadRefused, SelectionScopeRef, SourceDescriptor},
    service::WindowScopeSelection,
};
use airk_kernel::scoped_reads::{OpenReadAuthority, SelectionReadAuthority};
use airk_store::service::registry::{ServiceRegistry, catalog};
use anyhow::{Result, ensure};
use std::collections::BTreeSet;

/// One index of an in-memory W4/A2 selection grant, as the owning plane
/// resolved it (selection authority spec §3.2 step 4). Carried only on the
/// in-process admission call; never on a `ControlMethod`. The read projects
/// are computed by the builder, never supplied.
#[derive(Clone, Debug)]
pub struct SelectionConnectRequest {
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
pub fn build_selection_read_authority(
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

/// One named index of a U105 open agent read (U106(d)), as the service's
/// agent plane resolved it: the agent-owned selection ref (its live nonce
/// was registered with the gate) and the namespace it was resolved in.
/// Carried only on the in-process admission call; never on a `ControlMethod`.
#[derive(Clone, Debug)]
pub struct OpenConnectRequest {
    pub selection_ref: SelectionScopeRef,
    pub namespace: Incarnation,
    pub index_id: IndexId,
}
impl OpenConnectRequest {
    fn validate(&self) -> Result<()> {
        ensure!(
            self.selection_ref.owner == airk_core::scoped_reads::SelectionOwnerKind::Agent,
            ScopeReadRefused
        );
        Ok(())
    }
}

/// Bound on source pages read for one index: 17 retained sources plus the
/// page that proves completion.
const MAX_OPEN_SOURCE_PAGES: usize = MAX_SOURCE_ROOTS + 1;

/// Build the read authority of one whole index for an open agent read at
/// exactly the selection's catalog revision: the index is live, its import
/// is complete, its policy is ready and every enabled source is read whole
/// (one Subtree `""` atom each). Read-only (`catalog_state`,
/// `catalog_detail`, `catalog_page`); it writes nothing and creates no grant.
/// The caller holds the gate's issue ticket at that same revision.
pub fn build_open_read_authority(
    registry: &mut ServiceRegistry,
    request: &OpenConnectRequest,
) -> Result<OpenReadAuthority> {
    request.validate()?;
    let namespace = request.namespace;
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
    let detail = registry.catalog_detail(
        &CatalogEntity::Index {
            id: request.index_id,
        },
        Some(revision),
    )?;
    ensure!(
        detail.namespace == namespace && detail.revision == revision,
        ScopeReadRefused
    );
    let Some(CatalogItem::Index(index)) = detail.item else {
        return Err(ScopeReadRefused.into());
    };
    require_policy_ready(registry, &index)?;
    let mut sources = Vec::new();
    let mut after = None;
    for _ in 0..MAX_OPEN_SOURCE_PAGES {
        let page = registry.catalog_page(&airk_core::catalog_management::CatalogPageRequest {
            kind: airk_core::catalog_management::CatalogKind::Sources,
            index_id: Some(request.index_id),
            project_id: None,
            after: after.take(),
            expected_catalog_revision: Some(revision),
            limit: MAX_SELECTION,
            max_bytes: airk_core::catalog_management::MAX_PAGE_BYTES,
        })?;
        ensure!(
            page.namespace == namespace && page.revision == revision,
            ScopeReadRefused
        );
        for item in page.items {
            let CatalogItem::Source(source) = item else {
                return Err(ScopeReadRefused.into());
            };
            sources.push(source);
            ensure!(sources.len() <= MAX_SOURCE_ROOTS, ScopeReadRefused);
        }
        match page.continuation {
            Some(next) => after = Some(next),
            None => return validate_open_snapshot(request, index, sources),
        }
    }
    Err(ScopeReadRefused.into())
}

/// Pure checks of `build_open_read_authority`: the index is live and
/// complete, a foreign source refuses, disabled sources are not read, and at
/// least one enabled source remains.
fn validate_open_snapshot(
    request: &OpenConnectRequest,
    index: IndexDefinition,
    sources: Vec<CatalogSource>,
) -> Result<OpenReadAuthority> {
    request.validate()?;
    index.validate()?;
    ensure!(
        index.id == request.index_id
            && !index.tombstone
            && index.source_import == SourceImportState::Complete
            && sources.len() <= MAX_SOURCE_ROOTS,
        ScopeReadRefused
    );
    let mut descriptors = Vec::new();
    for source in sources {
        ensure!(source.index_id == request.index_id, ScopeReadRefused);
        if source.enabled {
            descriptors.push(SourceDescriptor {
                source_root_id: source.id,
                original_root: source.canonical_path,
            });
        }
    }
    descriptors.sort_by_key(|source| source.source_root_id);
    let authority = OpenReadAuthority {
        schema: 1,
        backing: index.backing.binding,
        index_id: request.index_id,
        selection: request.selection_ref,
        sources: descriptors,
        source_revision: index.source_revision,
        policy_epoch: index.policy.as_ref().map_or(0, |policy| policy.epoch.get()),
        policy_digest: index.policy.as_ref().map(|policy| policy.digest),
    };
    // Kernel validation: agent owner, at least one source, unique roots,
    // nothing newer than the selection's revision, 48 KiB.
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

#[cfg(test)]
mod tests {
    use super::*;
    use airk_core::{
        identity::Sha256Digest,
        index_catalog::{CatalogRevision, PathScope, QueryProjectId, ScopeAtom, SourceRootId},
        service::ProjectRegistration,
    };

    #[derive(Clone)]
    struct Fixture {
        namespace: Incarnation,
        catalog_revision: CatalogRevision,
        index_id: IndexId,
        project_ids: Vec<QueryProjectId>,
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
            let revision = CatalogRevision::INITIAL;
            let root = std::env::current_dir()?.to_string_lossy().into_owned();
            let backing = ProjectRegistration::new(root.clone(), root.clone(), "fixture".into())?;
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
            Ok(Self {
                namespace: identity,
                catalog_revision: revision,
                index_id,
                project_ids: vec![project_id],
                index: IndexDefinition {
                    id: index_id,
                    name: "Fixture".into(),
                    backing,
                    definition_revision: revision,
                    source_revision: revision,
                    policy: None,
                    tombstone: false,
                    source_import: SourceImportState::Complete,
                    description: None,
                },
                projects: vec![QueryProject {
                    id: project_id,
                    name: "Project".into(),
                    definition_revision: revision,
                    scope_revision: revision,
                    defaults_revision: revision,
                    default_indexes: Vec::new(),
                    description: None,
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
    }

    fn selection_request(f: &Fixture) -> SelectionConnectRequest {
        SelectionConnectRequest {
            selection_ref: SelectionScopeRef {
                owner: airk_core::scoped_reads::SelectionOwnerKind::Window,
                grant_nonce: Incarnation::from_bytes([0x5c; 16]),
                service_instance: Incarnation::from_bytes([0xe1; 16]),
                scope_identity: Sha256Digest::from_bytes([0x33; 32]),
                catalog_revision: f.catalog_revision,
            },
            window_selection: WindowScopeSelection {
                namespace: f.namespace,
                expected_catalog_revision: f.catalog_revision,
                index_ids: vec![f.index_id],
                project_ids: f.project_ids.clone(),
            },
            index_id: f.index_id,
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

    fn open_request(f: &Fixture) -> OpenConnectRequest {
        OpenConnectRequest {
            selection_ref: SelectionScopeRef {
                owner: airk_core::scoped_reads::SelectionOwnerKind::Agent,
                grant_nonce: Incarnation::from_bytes([0x6a; 16]),
                service_instance: Incarnation::from_bytes([0xe1; 16]),
                scope_identity: Sha256Digest::from_bytes([0x44; 32]),
                catalog_revision: f.catalog_revision,
            },
            namespace: f.namespace,
            index_id: f.index_id,
        }
    }
    fn source(f: &Fixture, byte: u8, enabled: bool) -> CatalogSource {
        let mut source = f.sources[0].clone();
        source.id = SourceRootId::from_identity(Incarnation::from_bytes([byte; 16]));
        source.canonical_path = format!("{}/extra-{byte}", f.sources[0].canonical_path);
        source.owner = false;
        source.enabled = enabled;
        source
    }

    /// U106(d): one Subtree "" atom per enabled source; memberships, grants
    /// and project defaults play no part.
    #[test]
    fn open_authority_reads_every_enabled_source_whole() -> Result<()> {
        let f = Fixture::new()?;
        let request = open_request(&f);
        let mut sources = vec![source(&f, 9, true), f.sources[0].clone()];
        sources.push(source(&f, 8, false));
        let authority = validate_open_snapshot(&request, f.index.clone(), sources)?;
        let mut expected = vec![
            f.sources[0].id,
            SourceRootId::from_identity(Incarnation::from_bytes([9; 16])),
        ];
        expected.sort();
        assert_eq!(
            authority
                .sources
                .iter()
                .map(|source| source.source_root_id)
                .collect::<Vec<_>>(),
            expected,
            "every enabled source, the disabled one excluded"
        );
        assert_eq!(authority.selection, request.selection_ref);
        assert_eq!(authority.backing, f.index.backing.binding);
        assert_eq!(authority.source_revision, f.index.source_revision);
        assert_eq!((authority.policy_epoch, authority.policy_digest), (0, None));
        authority.fingerprint()?;
        Ok(())
    }

    #[test]
    fn open_authority_refuses_foreign_retired_incomplete_and_non_agent_reads() -> Result<()> {
        let f = Fixture::new()?;
        let request = open_request(&f);
        let build = |index: IndexDefinition, sources: Vec<CatalogSource>| {
            validate_open_snapshot(&request, index, sources)
        };
        let mut foreign = f.sources[0].clone();
        foreign.index_id = IndexId::from_identity(Incarnation::from_bytes([7; 16]));
        assert!(build(f.index.clone(), vec![f.sources[0].clone(), foreign]).is_err());
        let mut retired = f.index.clone();
        retired.tombstone = true;
        assert!(build(retired, f.sources.clone()).is_err(), "tombstoned");
        let mut pending = f.index.clone();
        pending.source_import = SourceImportState::PendingMetadata;
        assert!(build(pending, f.sources.clone()).is_err(), "incomplete");
        let mut other = f.index.clone();
        other.id = IndexId::from_identity(Incarnation::from_bytes([7; 16]));
        assert!(build(other, f.sources.clone()).is_err(), "other index");
        assert!(
            build(f.index.clone(), vec![source(&f, 9, false)]).is_err(),
            "no enabled source"
        );
        assert!(build(f.index.clone(), Vec::new()).is_err(), "no source");
        for owner in [
            airk_core::scoped_reads::SelectionOwnerKind::Window,
            airk_core::scoped_reads::SelectionOwnerKind::Admin,
        ] {
            let mut window = open_request(&f);
            window.selection_ref.owner = owner;
            assert!(
                validate_open_snapshot(&window, f.index.clone(), f.sources.clone()).is_err(),
                "{owner:?}"
            );
        }
        // A source revision newer than the selection's revision refuses.
        let mut newer = f.index.clone();
        newer.source_revision = CatalogRevision::INITIAL.next()?;
        assert!(
            build(newer, f.sources.clone()).is_err(),
            "newer source revision"
        );
        Ok(())
    }

    #[test]
    fn open_builder_does_not_create_or_upgrade_a_legacy_catalog() -> Result<()> {
        let f = Fixture::new()?;
        let directory = tempfile::tempdir()?;
        let mut registry = ServiceRegistry::open(directory.path(), f.namespace)?;
        assert!(build_open_read_authority(&mut registry, &open_request(&f)).is_err());
        assert!(registry.catalog_state()?.is_none());
        Ok(())
    }
}
