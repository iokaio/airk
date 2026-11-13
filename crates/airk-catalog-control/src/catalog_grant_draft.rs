// SPDX-License-Identifier: Apache-2.0
//! Native reviewed grant preparation; no persistence, issuance or activation.
#[cfg(test)]
use airk_core::catalog_management::CatalogPreview;
#[cfg(test)]
use airk_core::index_catalog::QueryProjectId;
use airk_core::{
    catalog_management::{
        CatalogEntity, CatalogItem, CatalogKind, CatalogOperation, CatalogPageRequest,
        CatalogRequest, CatalogSource, MAX_PAGE_BYTES,
    },
    grant_provenance::native_reviewed_grant_id,
    identity::{Incarnation, Sha256Digest, framed_digest},
    index_catalog::{
        CatalogCapabilities, CatalogRevision, GrantId, GrantedMembership, IndexDefinition, IndexId,
        MAX_SELECTION, MAX_SOURCE_ROOTS, Membership, QueryProject, ReviewedGrant,
        SourceImportState, encoded_size,
    },
    scoped_reads::ScopeReadRefused,
    service::RequestKey,
};
use airk_store::service::registry::{ServiceRegistry, catalog};
use anyhow::{Result, ensure};
use std::collections::{BTreeMap, BTreeSet};

/// Implemented only by the native coordinator's authenticated administrative
/// connection. Host identity comes from the native namespace and authenticated
/// local OS caller, never a supplied wire host or open-folder preference.
pub trait NativeGrantDraftContext {
    fn namespace(&self) -> Incarnation;
    fn host(&self) -> Sha256Digest;
    fn context_key(&self) -> Sha256Digest;
    fn caller(&self) -> Incarnation;
    fn authorize_selection(&self, selection: &GrantDraftSelection) -> Result<()>;
}

pub(crate) use airk_core::catalog_host::{GrantDraftSelection, ReviewedGrantDraft};

#[derive(Clone)]
struct HostContext {
    namespace: Incarnation,
    host: Sha256Digest,
    key: Sha256Digest,
    caller: Incarnation,
}
impl HostContext {
    fn capture(
        context: &impl NativeGrantDraftContext,
        selection: &GrantDraftSelection,
    ) -> Result<Self> {
        selection.validate()?;
        context.authorize_selection(selection)?;
        let captured = Self {
            namespace: context.namespace(),
            host: context.host(),
            key: context.context_key(),
            caller: context.caller(),
        };
        ensure!(
            captured.namespace == selection.key.namespace,
            ScopeReadRefused
        );
        Ok(captured)
    }
    fn grant_id(&self, key: &RequestKey) -> Result<GrantId> {
        let identity = framed_digest(
            b"airk/reviewed-grant-draft-id/1",
            &[&serde_json::to_vec(&(
                self.namespace,
                self.host,
                self.key,
                self.caller,
                key,
            ))?],
        );
        Ok(native_reviewed_grant_id(identity))
    }
}

pub fn build_reviewed_grant_draft(
    registry: &mut ServiceRegistry,
    context: &impl NativeGrantDraftContext,
    selection: &GrantDraftSelection,
) -> Result<ReviewedGrantDraft> {
    let host = HostContext::capture(context, selection)?;
    let state = registry.catalog_state()?.ok_or(ScopeReadRefused)?;
    validate_fence(&state, &catalog::capabilities(), &host, selection)?;
    let detail = |registry: &mut ServiceRegistry, entity| -> Result<Option<CatalogItem>> {
        let result = registry.catalog_detail(&entity, Some(selection.expected_catalog_revision))?;
        ensure!(
            result.namespace == host.namespace
                && result.revision == selection.expected_catalog_revision,
            ScopeReadRefused
        );
        Ok(result.item)
    };
    let mut snapshot = Snapshot::default();
    for id in &selection.project_ids {
        let Some(CatalogItem::Project(project)) =
            detail(registry, CatalogEntity::Project { id: *id })?
        else {
            return Err(ScopeReadRefused.into());
        };
        snapshot.projects.push(project);
        if let Some(indexes) = &selection.index_ids {
            for index in indexes {
                match detail(
                    registry,
                    CatalogEntity::Membership {
                        project_id: *id,
                        index_id: *index,
                    },
                )? {
                    Some(CatalogItem::Membership(member)) => snapshot.memberships.push(member),
                    None => {}
                    _ => return Err(ScopeReadRefused.into()),
                }
            }
        } else {
            let mut request = CatalogPageRequest {
                kind: CatalogKind::Memberships,
                project_id: Some(*id),
                index_id: None,
                after: None,
                expected_catalog_revision: Some(selection.expected_catalog_revision),
                limit: MAX_SELECTION,
                max_bytes: MAX_PAGE_BYTES,
            };
            let mut count = 0;
            loop {
                let page = registry.catalog_page(&request)?;
                ensure!(
                    page.namespace == host.namespace
                        && page.revision == selection.expected_catalog_revision,
                    ScopeReadRefused
                );
                count += page.items.len();
                ensure!(count <= MAX_SELECTION, ScopeReadRefused);
                for item in page.items {
                    let CatalogItem::Membership(member) = item else {
                        return Err(ScopeReadRefused.into());
                    };
                    snapshot.memberships.push(member);
                }
                let Some(next) = page.continuation else {
                    break;
                };
                ensure!(
                    request
                        .after
                        .as_ref()
                        .is_none_or(|old| old.after < next.after)
                        && count < MAX_SELECTION,
                    ScopeReadRefused
                );
                request.after = Some(next);
            }
        }
        encoded_size(&snapshot.memberships, MAX_PAGE_BYTES)?;
    }
    let index_ids = snapshot
        .memberships
        .iter()
        .map(|m| m.index_id)
        .collect::<BTreeSet<_>>();
    ensure!(index_ids.len() <= MAX_SELECTION, ScopeReadRefused);
    for id in index_ids {
        let Some(CatalogItem::Index(index)) = detail(registry, CatalogEntity::Index { id })? else {
            return Err(ScopeReadRefused.into());
        };
        crate::catalog_authority::require_policy_ready(registry, &index)?;
        snapshot.indexes.push(index);
    }
    let source_ids = snapshot
        .memberships
        .iter()
        .flat_map(|m| m.scope.iter().map(|a| a.source_root_id))
        .collect::<BTreeSet<_>>();
    ensure!(
        source_ids.len() <= MAX_SELECTION * MAX_SOURCE_ROOTS,
        ScopeReadRefused
    );
    for id in source_ids {
        let Some(CatalogItem::Source(source)) = detail(registry, CatalogEntity::Source { id })?
        else {
            return Err(ScopeReadRefused.into());
        };
        snapshot.sources.push(source);
    }
    let request = snapshot.request(&host, selection)?;
    let preview = registry.catalog_preview(&request)?;
    ReviewedGrantDraft::new(request, preview)
}

fn validate_fence(
    state: &catalog::CatalogState,
    capabilities: &CatalogCapabilities,
    host: &HostContext,
    selection: &GrantDraftSelection,
) -> Result<()> {
    ensure!(
        state.activated
            && state.namespace == host.namespace
            && state.revision == selection.expected_catalog_revision
            && state.authority_contract == 1
            && state.policy_contract
                <= airk_store::service::registry::catalog::SUPPORTED_POLICY_CONTRACT
            && capabilities.supports(state.schema, 1, state.policy_contract),
        ScopeReadRefused
    );
    Ok(())
}

#[derive(Clone, Default)]
struct Snapshot {
    projects: Vec<QueryProject>,
    memberships: Vec<Membership>,
    indexes: Vec<IndexDefinition>,
    sources: Vec<CatalogSource>,
}
impl Snapshot {
    fn request(
        &self,
        host: &HostContext,
        selection: &GrantDraftSelection,
    ) -> Result<CatalogRequest> {
        selection.validate()?;
        ensure!(host.namespace == selection.key.namespace, ScopeReadRefused);
        let projects = self.projects.iter().map(|p| p.id).collect::<BTreeSet<_>>();
        ensure!(
            projects.len() == self.projects.len()
                && projects == selection.project_ids.iter().copied().collect()
                && self.memberships.len() <= MAX_SELECTION * MAX_SELECTION,
            ScopeReadRefused
        );
        for project in &self.projects {
            project.validate()?;
        }
        let indexes = self
            .indexes
            .iter()
            .map(|i| (i.id, i))
            .collect::<BTreeMap<_, _>>();
        ensure!(
            indexes.len() == self.indexes.len() && indexes.len() <= MAX_SELECTION,
            ScopeReadRefused
        );
        let sources = self
            .sources
            .iter()
            .map(|s| (s.id, s))
            .collect::<BTreeMap<_, _>>();
        ensure!(sources.len() == self.sources.len(), ScopeReadRefused);
        let mut contributed = BTreeSet::new();
        let mut used_indexes = BTreeSet::new();
        let mut used_sources = BTreeSet::new();
        let mut index_roots = BTreeMap::<IndexId, BTreeSet<_>>::new();
        let mut memberships = Vec::new();
        for member in &self.memberships {
            member.validate()?;
            ensure!(
                projects.contains(&member.project_id)
                    && !member.scope.is_empty()
                    && selection
                        .index_ids
                        .as_ref()
                        .is_none_or(|ids| ids.contains(&member.index_id)),
                ScopeReadRefused
            );
            let index = indexes.get(&member.index_id).ok_or(ScopeReadRefused)?;
            index.validate()?;
            ensure!(
                !index.tombstone && index.source_import == SourceImportState::Complete,
                ScopeReadRefused
            );
            let mut roots = BTreeSet::new();
            for atom in &member.scope {
                let source = sources.get(&atom.source_root_id).ok_or(ScopeReadRefused)?;
                // `owner` is the primary-root role, not authorization.
                ensure!(
                    source.index_id == member.index_id && source.enabled,
                    ScopeReadRefused
                );
                roots.insert(source.id);
                used_sources.insert(source.id);
                index_roots
                    .entry(member.index_id)
                    .or_default()
                    .insert(source.id);
            }
            ensure!(roots.len() <= MAX_SOURCE_ROOTS, ScopeReadRefused);
            contributed.insert(member.project_id);
            used_indexes.insert(member.index_id);
            memberships.push(GrantedMembership {
                membership: member.clone(),
                source_revision: index.source_revision,
                policy_epoch: index.policy.as_ref().map_or(0, |policy| policy.epoch.get()),
                policy_digest: index.policy.as_ref().map(|policy| policy.digest),
            });
        }
        ensure!(
            contributed == projects
                && used_indexes == indexes.keys().copied().collect()
                && used_sources == sources.keys().copied().collect(),
            ScopeReadRefused
        );
        ensure!(
            index_roots
                .values()
                .all(|roots| roots.len() <= MAX_SOURCE_ROOTS),
            ScopeReadRefused
        );
        if let Some(selected) = &selection.index_ids {
            ensure!(
                used_indexes == selected.iter().copied().collect(),
                ScopeReadRefused
            );
        }
        // Canonical order makes reconstruction independent of UI selector order.
        memberships.sort_by_key(|m| (m.membership.project_id, m.membership.index_id));
        let request = CatalogRequest {
            caller: host.caller,
            key: selection.key.clone(),
            expected_catalog_revision: selection.expected_catalog_revision,
            operation: CatalogOperation::CreateGrant {
                grant: ReviewedGrant {
                    id: host.grant_id(&selection.key)?,
                    host: host.host,
                    revision: CatalogRevision::INITIAL,
                    memberships,
                    revoked: false,
                },
            },
        };
        request.validate()?;
        Ok(request)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use airk_core::{
        index_catalog::{PathScope, ScopeAtom, SourceRootId},
        service::ProjectRegistration,
    };

    #[derive(Clone)]
    struct Fixture {
        host: HostContext,
        selection: GrantDraftSelection,
        snapshot: Snapshot,
    }
    impl Fixture {
        fn new() -> Result<Self> {
            let id = Incarnation::from_bytes([1; 16]);
            let project_id = QueryProjectId::from_identity(id);
            let index_id = IndexId::from_identity(id);
            let source = SourceRootId::from_identity(id);
            let rev = CatalogRevision::INITIAL;
            let host = HostContext {
                namespace: id,
                host: Sha256Digest::from_bytes([2; 32]),
                key: Sha256Digest::from_bytes([3; 32]),
                caller: Incarnation::from_bytes([4; 16]),
            };
            let selection = GrantDraftSelection {
                key: RequestKey {
                    namespace: id,
                    epoch: 1,
                    nonce: Incarnation::from_bytes([5; 16]),
                },
                expected_catalog_revision: rev,
                project_ids: vec![project_id],
                index_ids: None,
            };
            let root = std::env::current_dir()?.to_string_lossy().into_owned();
            let snapshot = Snapshot {
                projects: vec![QueryProject {
                    id: project_id,
                    name: "Fixture".into(),
                    definition_revision: rev,
                    scope_revision: rev,
                    defaults_revision: rev,
                    default_indexes: Vec::new(),
                }],
                memberships: vec![Membership {
                    project_id,
                    index_id,
                    revision: rev,
                    scope: vec![ScopeAtom {
                        source_root_id: source,
                        kind: PathScope::Subtree,
                        path: "reviewed/subtree".into(),
                    }],
                }],
                indexes: vec![IndexDefinition {
                    id: index_id,
                    name: "Fixture".into(),
                    backing: ProjectRegistration::new(
                        root.clone(),
                        root.clone(),
                        "fixture".into(),
                    )?,
                    definition_revision: rev,
                    source_revision: rev.next()?,
                    policy: None,
                    tombstone: false,
                    source_import: SourceImportState::Complete,
                }],
                sources: vec![CatalogSource {
                    id: source,
                    index_id,
                    canonical_path: root,
                    owner: true,
                    enabled: true,
                    aliases: Vec::new(),
                }],
            };
            Ok(Self {
                host,
                selection,
                snapshot,
            })
        }
        fn request(&self) -> Result<CatalogRequest> {
            self.snapshot.request(&self.host, &self.selection)
        }
    }
    struct Context {
        host: HostContext,
        permitted: bool,
    }
    impl NativeGrantDraftContext for Context {
        fn namespace(&self) -> Incarnation {
            self.host.namespace
        }
        fn host(&self) -> Sha256Digest {
            self.host.host
        }
        fn context_key(&self) -> Sha256Digest {
            self.host.key
        }
        fn caller(&self) -> Incarnation {
            self.host.caller
        }
        fn authorize_selection(&self, _: &GrantDraftSelection) -> Result<()> {
            ensure!(self.permitted, ScopeReadRefused);
            Ok(())
        }
    }
    fn grant(request: &CatalogRequest) -> &ReviewedGrant {
        let CatalogOperation::CreateGrant { grant } = &request.operation else {
            panic!("grant operation required")
        };
        grant
    }

    #[test]
    fn exact_native_snapshot_and_identity_reconstruct_without_expansion() -> Result<()> {
        let f = Fixture::new()?;
        let request = f.request()?;
        assert_eq!(
            grant(&request).memberships[0].membership,
            f.snapshot.memberships[0]
        );
        assert_eq!(
            grant(&request).memberships[0].source_revision,
            f.snapshot.indexes[0].source_revision
        );
        assert_eq!(request, f.request()?);
        let serialized = serde_json::to_vec(&request)?;
        assert_eq!(
            serde_json::from_slice::<CatalogRequest>(&serialized)?,
            request
        );
        let mut changed = f.clone();
        changed.snapshot.memberships[0].scope[0]
            .path
            .push_str("/narrower");
        let different = changed.request()?;
        assert_eq!(grant(&request).id, grant(&different).id);
        assert_ne!(request.identity()?, different.identity()?);
        for n in 0..5 {
            let mut alternate = f.clone();
            match n {
                0 => alternate.host.host = Sha256Digest::from_bytes([8; 32]),
                1 => alternate.host.key = Sha256Digest::from_bytes([8; 32]),
                2 => alternate.host.caller = Incarnation::from_bytes([8; 16]),
                3 => alternate.selection.key.nonce = Incarnation::from_bytes([8; 16]),
                _ => {
                    alternate.host.namespace = Incarnation::from_bytes([8; 16]);
                    alternate.selection.key.namespace = alternate.host.namespace;
                }
            }
            assert_ne!(grant(&alternate.request()?).id, grant(&request).id);
        }
        Ok(())
    }

    #[test]
    fn selected_projects_sharing_index_retain_distinct_membership_atoms() -> Result<()> {
        let mut f = Fixture::new()?;
        let other = QueryProjectId::from_identity(Incarnation::from_bytes([8; 16]));
        let mut project = f.snapshot.projects[0].clone();
        project.id = other;
        f.snapshot.projects.push(project);
        f.selection.project_ids.push(other);
        let mut member = f.snapshot.memberships[0].clone();
        member.project_id = other;
        member.scope[0].path = "separate".into();
        f.snapshot.memberships.push(member);
        let request = f.request()?;
        assert_eq!(grant(&request).memberships.len(), 2);
        assert_ne!(
            grant(&request).memberships[0].membership.scope,
            grant(&request).memberships[1].membership.scope
        );
        f.selection.project_ids.reverse();
        f.snapshot.memberships.reverse();
        assert_eq!(request, f.request()?);
        Ok(())
    }

    #[test]
    fn unknown_empty_duplicate_and_unrepresented_selection_refuse() -> Result<()> {
        let f = Fixture::new()?;
        for n in 0..9 {
            let mut invalid = f.clone();
            match n {
                0 => invalid.selection.project_ids.clear(),
                1 => invalid
                    .selection
                    .project_ids
                    .push(invalid.selection.project_ids[0]),
                2 => {
                    invalid.selection.project_ids[0] =
                        QueryProjectId::from_identity(Incarnation::from_bytes([8; 16]))
                }
                3 => invalid.selection.index_ids = Some(Vec::new()),
                4 => {
                    invalid.selection.index_ids = Some(vec![IndexId::from_identity(
                        Incarnation::from_bytes([8; 16]),
                    )])
                }
                5 => invalid.snapshot.memberships.clear(),
                6 => invalid.snapshot.memberships[0].scope.clear(),
                7 => invalid.selection.key.namespace = Incarnation::from_bytes([8; 16]),
                _ => invalid
                    .snapshot
                    .memberships
                    .push(invalid.snapshot.memberships[0].clone()),
            }
            assert!(invalid.request().is_err(), "case {n}");
        }
        let mut subset = f;
        subset.selection.index_ids = Some(vec![subset.snapshot.indexes[0].id]);
        subset.request()?;
        Ok(())
    }

    #[test]
    fn index_readiness_policy_and_source_ownership_fail_closed() -> Result<()> {
        let f = Fixture::new()?;
        for n in 0..6 {
            let mut invalid = f.clone();
            match n {
                0 => invalid.snapshot.indexes[0].tombstone = true,
                1 => invalid.snapshot.indexes[0].source_import = SourceImportState::PendingMetadata,
                2 => invalid.snapshot.sources[0].enabled = false,
                3 => {
                    invalid.snapshot.sources[0].index_id =
                        IndexId::from_identity(Incarnation::from_bytes([8; 16]))
                }
                4 => invalid.snapshot.sources.clear(),
                _ => invalid.snapshot.indexes.clear(),
            }
            assert!(invalid.request().is_err(), "case {n}");
        }
        Ok(())
    }

    #[test]
    fn reviewed_draft_freezes_exact_policy_without_changing_membership_scope() -> Result<()> {
        let mut f = Fixture::new()?;
        let baseline = f.request()?;
        let epoch = CatalogRevision::INITIAL.next()?;
        let digest = Sha256Digest::from_bytes([8; 32]);
        f.snapshot.indexes[0].policy =
            Some(airk_core::index_catalog::PolicyReference { epoch, digest });
        let reviewed = f.request()?;
        let member = &grant(&reviewed).memberships[0];
        assert_eq!(member.policy_epoch, epoch.get());
        assert_eq!(member.policy_digest, Some(digest));
        assert_eq!(
            member.membership,
            grant(&baseline).memberships[0].membership
        );
        assert_eq!(
            member.source_revision,
            grant(&baseline).memberships[0].source_revision
        );
        assert_ne!(reviewed.identity()?, baseline.identity()?);
        Ok(())
    }

    #[test]
    fn exact_snapshot_retains_enabled_external_root_with_primary_flag_false() -> Result<()> {
        let external_directory = tempfile::tempdir()?;
        let external_root = external_directory
            .path()
            .canonicalize()?
            .to_string_lossy()
            .into_owned();
        let mut f = Fixture::new()?;
        let id = SourceRootId::from_identity(Incarnation::from_bytes([8; 16]));
        f.snapshot.sources.push(CatalogSource {
            id,
            index_id: f.snapshot.indexes[0].id,
            canonical_path: external_root,
            owner: false,
            enabled: true,
            aliases: vec!["external".into()],
        });
        f.snapshot.memberships[0].scope.push(ScopeAtom {
            source_root_id: id,
            kind: PathScope::Exact,
            path: "allowed.rs".into(),
        });
        let request = f.request()?;
        assert_eq!(
            grant(&request).memberships[0].membership,
            f.snapshot.memberships[0]
        );
        assert!(
            grant(&request).memberships[0]
                .membership
                .scope
                .iter()
                .any(|atom| atom.source_root_id == id && atom.path == "allowed.rs")
        );
        Ok(())
    }

    #[test]
    fn pure_contract_one_baseline_and_independent_fences() -> Result<()> {
        let f = Fixture::new()?;
        let state = catalog::CatalogState {
            schema: 12,
            namespace: f.host.namespace,
            revision: f.selection.expected_catalog_revision,
            activated: true,
            accepted_new_writes: false,
            authority_contract: 1,
            policy_contract: 0,
            upgrade_id: f.host.namespace,
            legacy_schema_digest: f.host.host,
            backup_digest: f.host.host,
        };
        let capabilities = CatalogCapabilities {
            maximum_registry_schema: 12,
            authority_contract: 1,
            policy_contract: 0,
            reference_refresh_contract: None,
            production_activation_enabled: false,
        };
        validate_fence(&state, &capabilities, &f.host, &f.selection)?;
        for n in 0..6 {
            let mut invalid = state.clone();
            match n {
                0 => invalid.namespace = Incarnation::from_bytes([8; 16]),
                1 => invalid.revision = invalid.revision.next()?,
                2 => invalid.activated = false,
                3 => invalid.authority_contract = 0,
                4 => invalid.policy_contract = 1,
                _ => invalid.schema = 13,
            }
            assert!(
                validate_fence(&invalid, &capabilities, &f.host, &f.selection).is_err(),
                "case {n}"
            );
        }
        let mut unsupported = capabilities;
        unsupported.authority_contract = 0;
        assert!(validate_fence(&state, &unsupported, &f.host, &f.selection).is_err());
        let mut policy = state;
        policy.policy_contract = 1;
        let qualified = CatalogCapabilities {
            authority_contract: 1,
            policy_contract: 1,
            ..unsupported
        };
        validate_fence(&policy, &qualified, &f.host, &f.selection)?;
        policy.policy_contract = 2;
        assert!(validate_fence(&policy, &qualified, &f.host, &f.selection).is_err());
        Ok(())
    }

    #[test]
    fn native_administrative_admission_is_required_and_legacy_reads_do_not_upgrade() -> Result<()> {
        let f = Fixture::new()?;
        let denied = Context {
            host: f.host.clone(),
            permitted: false,
        };
        assert!(HostContext::capture(&denied, &f.selection).is_err());
        let permitted = Context {
            host: f.host.clone(),
            permitted: true,
        };
        HostContext::capture(&permitted, &f.selection)?;
        let directory = tempfile::tempdir()?;
        let mut registry = ServiceRegistry::open(directory.path(), f.host.namespace)?;
        assert!(build_reviewed_grant_draft(&mut registry, &permitted, &f.selection).is_err());
        assert!(registry.catalog_state()?.is_none());
        Ok(())
    }

    #[test]
    fn aggregate_roots_and_exact_request_bytes_are_bounded() -> Result<()> {
        let mut f = Fixture::new()?;
        f.snapshot.memberships[0].scope = (0..16)
            .map(|n| ScopeAtom {
                source_root_id: f.snapshot.sources[0].id,
                kind: PathScope::Exact,
                path: format!("{n:02}{}", "x".repeat(4000)),
            })
            .collect();
        assert!(f.request().is_err());
        let mut f = Fixture::new()?;
        let source = f.snapshot.sources[0].clone();
        f.snapshot.sources = (1..=18)
            .map(|n| CatalogSource {
                id: SourceRootId::from_identity(Incarnation::from_bytes([n; 16])),
                ..source.clone()
            })
            .collect();
        f.snapshot.memberships[0].scope = f
            .snapshot
            .sources
            .iter()
            .map(|s| ScopeAtom {
                source_root_id: s.id,
                kind: PathScope::Subtree,
                path: String::new(),
            })
            .collect();
        assert!(f.request().is_err());
        Ok(())
    }

    #[test]
    fn complete_returned_draft_preserves_canonical_preview_and_combined_budget() -> Result<()> {
        let f = Fixture::new()?;
        let request = f.request()?;
        let preview = CatalogPreview {
            request_digest: request.identity()?,
            catalog_revision: request.expected_catalog_revision,
            next_revision: request.expected_catalog_revision.next()?,
            operation: request.operation.clone(),
            before: Vec::new(),
            after: vec![CatalogItem::Grant(
                airk_core::grant_provenance::PersistedGrant {
                    grant: grant(&request).clone(),
                    provenance:
                        airk_core::grant_provenance::GrantProvenance::UserReviewedProjectGrant,
                },
            )],
            authority_fence_required: false,
            file_data_changes: false,
            source_policy_impact: None,
        };
        let draft = ReviewedGrantDraft::new(request.clone(), preview.clone())?;
        assert_eq!(draft.preview_digest, preview.identity()?);
        let copied = serde_json::to_value(&draft)?;
        assert_eq!(copied["request"], serde_json::to_value(&request)?);
        assert_eq!(copied["preview"], serde_json::to_value(&preview)?);
        let mut mismatched = preview.clone();
        mismatched.request_digest = f.host.host;
        assert!(ReviewedGrantDraft::new(request, mismatched).is_err());

        let mut larger = f;
        larger.snapshot.memberships[0].scope = (0..6)
            .map(|n| ScopeAtom {
                source_root_id: larger.snapshot.sources[0].id,
                kind: PathScope::Exact,
                path: format!("{n:02}{}", "x".repeat(4000)),
            })
            .collect();
        let request = larger.request()?;
        let preview = CatalogPreview {
            request_digest: request.identity()?,
            operation: request.operation.clone(),
            after: Vec::new(),
            source_policy_impact: None,
            ..preview
        };
        // Each piece fits by itself; duplicating them in the review reply cannot
        // exceed the one bounded native reply envelope.
        encoded_size(&request, MAX_PAGE_BYTES)?;
        encoded_size(&preview, MAX_PAGE_BYTES)?;
        assert!(ReviewedGrantDraft::new(request, preview).is_err());
        Ok(())
    }
}
