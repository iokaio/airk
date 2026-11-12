// SPDX-License-Identifier: Apache-2.0
//! The catalog check of an in-memory scope grant (U77; contracts §2.5 W4 and
//! §4.6 A2): a window's grant and an admin session's grant are the same kind
//! of grant, so both are checked here. The check selects existing indexes
//! only; it never creates or changes an index, a project, a membership or a
//! catalog grant, and it carries no `GrantId`.
//!
//! The decision (`decide`) is pure over injected catalog observations; the
//! registry reader (`check`) only gathers them. Every refusal is the existing
//! `scoped_read_unavailable` code [V].
use airk_core::{
    catalog_management::{CatalogEntity, CatalogItem},
    identity::{Incarnation, Sha256Digest, framed_digest},
    index_catalog::{CatalogRevision, MAX_SELECTION, unique_bounded},
    service::WindowScopeSelection,
};
use anyhow::Result;
use std::{
    path::Path,
    time::{Duration, Instant},
};

/// The one refusal code of a grant check.
pub(super) const SCOPE_UNAVAILABLE: &str = "scoped_read_unavailable";

/// How long the check waits for the registry, as combined reads wait.
const REGISTRY_WAIT: Duration = Duration::from_secs(2);

/// The catalog clock a grant is checked against.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct CatalogClock {
    /// Activated, a supported authority and policy contract, and no pending
    /// catalog fence.
    pub(super) ready: bool,
    pub(super) namespace: Incarnation,
    pub(super) revision: CatalogRevision,
}

/// Decide a grant over catalog observations: `clock` is the current catalog
/// state (`None` without a catalog) and `detail` reads one entity at that
/// revision.
///
/// Rules: one to `MAX_SELECTION` distinct indexes and at most
/// `MAX_SELECTION` distinct projects; a ready catalog in the selection's
/// namespace at exactly the selection's revision (an older revision is
/// stale); every index present and not retired; every project present; with
/// projects selected, every index a member of one of them. The scope identity
/// covers the namespace, the revision, each index with its backing binding
/// and definition revision, and the projects, all in sorted order.
pub(super) fn decide(
    selection: &WindowScopeSelection,
    clock: Option<CatalogClock>,
    mut detail: impl FnMut(&CatalogEntity) -> Result<Option<CatalogItem>>,
) -> Result<(Sha256Digest, CatalogRevision), &'static str> {
    let refused = |_| SCOPE_UNAVAILABLE;
    if selection.index_ids.is_empty() {
        return Err(SCOPE_UNAVAILABLE);
    }
    unique_bounded(&selection.index_ids, MAX_SELECTION).map_err(refused)?;
    unique_bounded(&selection.project_ids, MAX_SELECTION).map_err(refused)?;
    let clock = clock.ok_or(SCOPE_UNAVAILABLE)?;
    if !clock.ready
        || clock.namespace != selection.namespace
        || clock.revision != selection.expected_catalog_revision
    {
        return Err(SCOPE_UNAVAILABLE);
    }
    let mut indexes = Vec::with_capacity(selection.index_ids.len());
    for id in &selection.index_ids {
        let Some(CatalogItem::Index(index)) =
            detail(&CatalogEntity::Index { id: *id }).map_err(refused)?
        else {
            return Err(SCOPE_UNAVAILABLE);
        };
        if index.id != *id || index.tombstone || index.validate().is_err() {
            return Err(SCOPE_UNAVAILABLE);
        }
        indexes.push((index.id, index.backing.binding, index.definition_revision));
    }
    for id in &selection.project_ids {
        match detail(&CatalogEntity::Project { id: *id }).map_err(refused)? {
            Some(CatalogItem::Project(project)) if project.id == *id => {}
            _ => return Err(SCOPE_UNAVAILABLE),
        }
    }
    if !selection.project_ids.is_empty() {
        for index_id in &selection.index_ids {
            let mut member = false;
            for project_id in &selection.project_ids {
                let entity = CatalogEntity::Membership {
                    project_id: *project_id,
                    index_id: *index_id,
                };
                if let Some(CatalogItem::Membership(found)) = detail(&entity).map_err(refused)?
                    && found.project_id == *project_id
                    && found.index_id == *index_id
                {
                    member = true;
                    break;
                }
            }
            if !member {
                return Err(SCOPE_UNAVAILABLE);
            }
        }
    }
    indexes.sort();
    let mut projects = selection.project_ids.clone();
    projects.sort();
    let encoded = (
        serde_json::to_vec(&(clock.namespace, clock.revision)),
        serde_json::to_vec(&indexes),
        serde_json::to_vec(&projects),
    );
    let (Ok(clock_bytes), Ok(index_bytes), Ok(project_bytes)) = encoded else {
        return Err(SCOPE_UNAVAILABLE);
    };
    let identity = framed_digest(
        b"airk/session-scope-grant/1",
        &[&clock_bytes, &index_bytes, &project_bytes],
    );
    Ok((identity, clock.revision))
}

/// Check `selection` against the catalog of the service at `control`.
/// Blocking: call it outside every lock (and off the async workers).
pub(super) fn check(
    control: &Path,
    selection: &WindowScopeSelection,
) -> Result<(Sha256Digest, CatalogRevision), String> {
    let observe = || -> Result<Result<(Sha256Digest, CatalogRevision), &'static str>> {
        let mut registry = airk_store::service::registry::ServiceRegistry::open_saved_before(
            control,
            Some(Instant::now() + REGISTRY_WAIT),
        )?;
        let pending = registry.catalog_pending_fence()?.is_some();
        let clock = registry.catalog_state()?.map(|state| CatalogClock {
            ready: state.activated
                && state.authority_contract == 1
                && state.policy_contract
                    <= airk_store::service::registry::catalog::SUPPORTED_POLICY_CONTRACT
                && !pending,
            namespace: state.namespace,
            revision: state.revision,
        });
        let revision = clock.map(|clock| clock.revision);
        Ok(decide(selection, clock, |entity| {
            Ok(registry.catalog_detail(entity, revision)?.item)
        }))
    };
    match observe() {
        Ok(decided) => decided.map_err(str::to_owned),
        Err(_) => Err(SCOPE_UNAVAILABLE.to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use airk_core::{
        index_catalog::{
            IndexDefinition, IndexId, Membership, QueryProject, QueryProjectId, SourceImportState,
        },
        service::ProjectRegistration,
    };
    use std::collections::BTreeMap;

    fn revision(value: u64) -> CatalogRevision {
        CatalogRevision::try_from(value).unwrap()
    }
    fn index_id(byte: u8) -> IndexId {
        IndexId::from_identity(Incarnation::from_bytes([byte; 16]))
    }
    fn project_id(byte: u8) -> QueryProjectId {
        QueryProjectId::from_identity(Incarnation::from_bytes([byte; 16]))
    }
    fn namespace() -> Incarnation {
        Incarnation::from_bytes([0x0a; 16])
    }
    fn clock() -> Option<CatalogClock> {
        Some(CatalogClock {
            ready: true,
            namespace: namespace(),
            revision: revision(12),
        })
    }
    fn selection(indexes: &[u8], projects: &[u8]) -> WindowScopeSelection {
        WindowScopeSelection {
            namespace: namespace(),
            expected_catalog_revision: revision(12),
            index_ids: indexes.iter().map(|byte| index_id(*byte)).collect(),
            project_ids: projects.iter().map(|byte| project_id(*byte)).collect(),
        }
    }

    /// A catalog of indexes 1 and 3 (3 retired), project 2 with index 1 as a
    /// member, and project 4 with no members.
    fn catalog() -> BTreeMap<String, CatalogItem> {
        let index = |byte: u8, tombstone: bool| IndexDefinition {
            id: index_id(byte),
            name: format!("Index {byte}"),
            backing: ProjectRegistration::new(
                format!("/fixture/root{byte}"),
                format!("/fixture/data{byte}"),
                "fixture".into(),
            )
            .unwrap(),
            definition_revision: revision(3),
            source_revision: revision(3),
            policy: None,
            tombstone,
            source_import: SourceImportState::Complete,
        };
        let project = |byte: u8| QueryProject {
            id: project_id(byte),
            name: format!("Project {byte}"),
            definition_revision: revision(2),
            scope_revision: revision(2),
            defaults_revision: revision(2),
            default_indexes: Vec::new(),
        };
        let mut items = BTreeMap::new();
        for (entity, item) in [
            (
                CatalogEntity::Index { id: index_id(1) },
                CatalogItem::Index(index(1, false)),
            ),
            (
                CatalogEntity::Index { id: index_id(3) },
                CatalogItem::Index(index(3, true)),
            ),
            (
                CatalogEntity::Project { id: project_id(2) },
                CatalogItem::Project(project(2)),
            ),
            (
                CatalogEntity::Project { id: project_id(4) },
                CatalogItem::Project(project(4)),
            ),
            (
                CatalogEntity::Membership {
                    project_id: project_id(2),
                    index_id: index_id(1),
                },
                CatalogItem::Membership(Membership {
                    project_id: project_id(2),
                    index_id: index_id(1),
                    revision: revision(2),
                    scope: Vec::new(),
                }),
            ),
        ] {
            items.insert(serde_json::to_string(&entity).unwrap(), item);
        }
        items
    }

    fn decide_in(
        selection: &WindowScopeSelection,
        clock: Option<CatalogClock>,
    ) -> Result<(Sha256Digest, CatalogRevision), &'static str> {
        let items = catalog();
        decide(selection, clock, |entity| {
            Ok(items.get(&serde_json::to_string(entity).unwrap()).cloned())
        })
    }

    #[test]
    fn a_member_index_of_the_current_catalog_is_granted() {
        let (identity, granted) = decide_in(&selection(&[1], &[2]), clock()).unwrap();
        assert_eq!(granted, revision(12));
        // Indexes alone, without projects, are an existing-index selection.
        let (alone, _) = decide_in(&selection(&[1], &[]), clock()).unwrap();
        assert_ne!(identity, alone);
        // The same selection at the same revision has the same identity.
        assert_eq!(
            decide_in(&selection(&[1], &[2]), clock()).unwrap().0,
            identity
        );
    }

    #[test]
    fn unknown_retired_foreign_stale_and_non_member_selections_are_refused() {
        let refused = Err(SCOPE_UNAVAILABLE);
        // Unknown and retired indexes, an unknown project, a project the
        // index is not a member of.
        assert_eq!(decide_in(&selection(&[9], &[]), clock()), refused);
        assert_eq!(decide_in(&selection(&[3], &[]), clock()), refused);
        assert_eq!(decide_in(&selection(&[1], &[8]), clock()), refused);
        assert_eq!(decide_in(&selection(&[1], &[4]), clock()), refused);
        // Empty, duplicated or oversized selections.
        assert_eq!(decide_in(&selection(&[], &[2]), clock()), refused);
        assert_eq!(decide_in(&selection(&[1, 1], &[2]), clock()), refused);
        let many: Vec<u8> = (0..=MAX_SELECTION as u8).collect();
        assert_eq!(decide_in(&selection(&many, &[]), clock()), refused);
        // No catalog, a pending fence, another namespace, a stale or a
        // future revision.
        assert_eq!(decide_in(&selection(&[1], &[2]), None), refused);
        for clock in [
            CatalogClock {
                ready: false,
                ..clock().unwrap()
            },
            CatalogClock {
                namespace: Incarnation::from_bytes([0x0b; 16]),
                ..clock().unwrap()
            },
            CatalogClock {
                revision: revision(13),
                ..clock().unwrap()
            },
            CatalogClock {
                revision: revision(11),
                ..clock().unwrap()
            },
        ] {
            assert_eq!(decide_in(&selection(&[1], &[2]), Some(clock)), refused);
        }
        // A registry read failure refuses; it never grants.
        assert_eq!(
            decide(&selection(&[1], &[]), clock(), |_| anyhow::bail!("busy")),
            refused
        );
    }
}
