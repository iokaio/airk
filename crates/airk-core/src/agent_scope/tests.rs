// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::{
    index_catalog::{PolicyReference, ScopeAtom},
    service::ProjectRegistration,
};

fn revision(value: u64) -> CatalogRevision {
    CatalogRevision::try_from(value).unwrap()
}
fn index_id(byte: u8) -> IndexId {
    IndexId::from_identity(Incarnation::from_bytes([byte; 16]))
}
fn set_id(byte: u8) -> QueryProjectId {
    QueryProjectId::from_identity(Incarnation::from_bytes([byte; 16]))
}
fn index(byte: u8, name: &str, tombstone: bool) -> IndexDefinition {
    IndexDefinition {
        id: index_id(byte),
        name: name.into(),
        backing: ProjectRegistration::new(
            format!("/src/{byte}"),
            "/data".into(),
            format!("c{byte}"),
        )
        .unwrap(),
        definition_revision: revision(1),
        source_revision: revision(1),
        policy: Some(PolicyReference {
            epoch: revision(1),
            digest: crate::identity::Sha256Digest::from_bytes([byte; 32]),
        }),
        tombstone,
        source_import: SourceImportState::Complete,
        description: None,
    }
}
fn set(byte: u8, name: &str) -> QueryProject {
    QueryProject {
        id: set_id(byte),
        name: name.into(),
        definition_revision: revision(1),
        scope_revision: revision(1),
        defaults_revision: revision(1),
        default_indexes: Vec::new(),
        description: None,
    }
}
fn member(set: u8, index: u8) -> Membership {
    Membership {
        project_id: set_id(set),
        index_id: index_id(index),
        revision: revision(1),
        scope: Vec::<ScopeAtom>::new(),
    }
}
fn source(byte: u8, index: u8, enabled: bool) -> CatalogSource {
    CatalogSource {
        id: crate::index_catalog::SourceRootId::from_identity(Incarnation::from_bytes([byte; 16])),
        index_id: index_id(index),
        canonical_path: format!("/src/{index}/{byte}"),
        owner: true,
        enabled,
        aliases: Vec::new(),
    }
}

/// Indexes 1 "docs", 2 "code", 3 "code" (a duplicate name), 4 "old"
/// (retired); set 10 "web" = {1, 2, 4}, set 11 "web" (duplicate name),
/// set 12 "empty" with no members.
fn listing() -> CatalogListing {
    CatalogListing {
        namespace: Incarnation::from_bytes([9; 16]),
        revision: Some(revision(7)),
        indexes: vec![
            index(1, "docs", false),
            index(2, "code", false),
            index(3, "code", false),
            index(4, "old", true),
        ],
        projects: vec![set(10, "web"), set(11, "web"), set(12, "empty")],
        memberships: vec![member(10, 1), member(10, 2), member(10, 4)],
        sources: vec![
            source(21, 1, true),
            source(22, 1, false),
            source(23, 2, true),
        ],
    }
}
fn open(choice: AgentChoice, indexes: &[u8], sets: &[u8]) -> OpenConnectionState {
    OpenConnectionState {
        defaults_revision: 3,
        default_index_ids: indexes.iter().map(|byte| index_id(*byte)).collect(),
        default_index_set_ids: sets.iter().map(|byte| set_id(*byte)).collect(),
        agent_choice: choice,
    }
}
fn names(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

#[test]
fn choice_off_reads_only_the_default_scope_and_refuses_names() {
    let off = open(AgentChoice::Off, &[1], &[]);
    let docs = names(&["docs"]);
    assert_eq!(
        requested(&docs, &[], Some(&off)),
        Err("agent_scope_choice_disabled")
    );
    assert_eq!(
        requested(&[], &docs, Some(&off)),
        Err("agent_scope_choice_disabled")
    );
    assert_eq!(
        requested(&[], &[], Some(&off)),
        Ok(Requested::Defaults {
            indexes: &[index_id(1)],
            sets: &[]
        })
    );
    let unset = open(AgentChoice::Off, &[], &[]);
    assert_eq!(
        requested(&[], &[], Some(&unset)),
        Err("agent_default_scope_unset")
    );
    assert_eq!(list_allowed(Some(&off)), Err("agent_scope_choice_disabled"));
}

#[test]
fn choice_on_and_the_local_owner_may_name_a_scope() {
    let on = open(AgentChoice::On, &[], &[]);
    let docs = names(&["docs"]);
    assert_eq!(
        requested(&docs, &[], Some(&on)),
        Ok(Requested::Named {
            indexes: &docs,
            sets: &[]
        })
    );
    assert_eq!(
        requested(&[], &[], Some(&on)),
        Err("agent_default_scope_unset")
    );
    // The local owner has no default: the call must name a scope.
    assert!(matches!(
        requested(&docs, &[], None),
        Ok(Requested::Named { .. })
    ));
    assert_eq!(requested(&[], &[], None), Err("agent_default_scope_unset"));
    assert_eq!(list_allowed(Some(&on)), Ok(()));
    assert_eq!(list_allowed(None), Ok(()));
}

#[test]
fn names_resolve_by_id_then_exact_name_and_refuse_ambiguity() {
    let catalog = listing();
    let resolve_names = |indexes: &[&str], sets: &[&str]| {
        let (indexes, sets) = (names(indexes), names(sets));
        resolve(
            &catalog,
            Requested::Named {
                indexes: &indexes,
                sets: &sets,
            },
        )
    };
    assert_eq!(resolve_names(&["docs"], &[]), Ok(vec![index_id(1)]));
    assert_eq!(
        resolve_names(&[&index_id(2).to_string()], &[]),
        Ok(vec![index_id(2)])
    );
    assert_eq!(resolve_names(&["code"], &[]), Err("scope_name_ambiguous"));
    assert_eq!(resolve_names(&["Docs"], &[]), Err("scope_name_unknown"));
    // A retired index is unknown by id and by name.
    assert_eq!(resolve_names(&["old"], &[]), Err("scope_name_unknown"));
    assert_eq!(
        resolve_names(&[&index_id(4).to_string()], &[]),
        Err("scope_name_unknown")
    );
    assert_eq!(resolve_names(&[], &["web"]), Err("scope_name_ambiguous"));
    assert_eq!(resolve_names(&[], &["nope"]), Err("scope_name_unknown"));
    assert_eq!(
        resolve_names(&[], &[&set_id(13).to_string()]),
        Err("scope_name_unknown")
    );
}

#[test]
fn sets_expand_to_live_members_in_a_deduplicated_union() {
    let catalog = listing();
    let web = set_id(10).to_string();
    let (indexes, sets) = (names(&["docs"]), vec![web]);
    // Set 10 = {1, 2, retired 4}: the union with index 1 is {1, 2}.
    assert_eq!(
        resolve(
            &catalog,
            Requested::Named {
                indexes: &indexes,
                sets: &sets
            }
        ),
        Ok(vec![index_id(1), index_id(2)])
    );
    // Defaults expand the same way; an empty set alone reads nothing.
    assert_eq!(
        resolve(
            &catalog,
            Requested::Defaults {
                indexes: &[index_id(3)],
                sets: &[set_id(10)]
            }
        ),
        Ok(vec![index_id(1), index_id(2), index_id(3)])
    );
    assert_eq!(
        resolve(
            &catalog,
            Requested::Defaults {
                indexes: &[],
                sets: &[set_id(12)]
            }
        ),
        Err("scoped_read_unavailable")
    );
    assert_eq!(
        resolve(
            &catalog,
            Requested::Defaults {
                indexes: &[index_id(4)],
                sets: &[]
            }
        ),
        Err("scope_name_unknown")
    );
}

#[test]
fn more_than_thirty_two_indexes_after_expansion_refuse() {
    let mut catalog = listing();
    catalog.indexes = (1..=33)
        .map(|byte| index(byte, &format!("i{byte}"), false))
        .collect();
    catalog.memberships = (1..=33).map(|byte| member(10, byte)).collect();
    let thirty_two: Vec<IndexId> = (1..=32).map(index_id).collect();
    assert_eq!(
        resolve(
            &catalog,
            Requested::Defaults {
                indexes: &thirty_two,
                sets: &[]
            }
        )
        .map(|ids| ids.len()),
        Ok(32)
    );
    assert_eq!(
        resolve(
            &catalog,
            Requested::Defaults {
                indexes: &[],
                sets: &[set_id(10)]
            }
        ),
        Err("agent_scope_too_large")
    );
}

#[test]
fn stored_defaults_must_name_live_entries() {
    let catalog = listing();
    assert_eq!(
        validate_defaults(&catalog, &[index_id(1)], &[set_id(12)]),
        Ok(())
    );
    assert_eq!(
        validate_defaults(&catalog, &[index_id(4)], &[]),
        Err("scope_name_unknown")
    );
    assert_eq!(
        validate_defaults(&catalog, &[], &[set_id(13)]),
        Err("scope_name_unknown")
    );
    assert_eq!(
        validate_defaults(&catalog, &[index_id(1), index_id(1)], &[]),
        Err("agent_scope_too_large")
    );
}

#[test]
fn the_list_names_paths_generated_descriptions_and_members() {
    let mut catalog = listing();
    catalog.indexes[1].description = Some("Application code".into());
    catalog.indexes[2].source_import = SourceImportState::PendingMetadata;
    let body = list_page(&catalog, None, 48 * 1024).unwrap();
    assert_eq!(body["schema"], 1);
    assert_eq!(body["catalog_revision"], 7);
    assert!(body.get("next_cursor").is_none());
    let indexes = body["indexes"].as_array().unwrap();
    // The retired index is not listed.
    assert_eq!(indexes.len(), 3);
    assert_eq!(
        indexes[0],
        json!({"id": index_id(1), "name": "docs",
            "description": "docs: indexes /src/1/21", "description_generated": true,
            "paths": ["/src/1/21"], "readiness": "ready"})
    );
    assert_eq!(indexes[1]["description"], "Application code");
    assert_eq!(indexes[1]["description_generated"], false);
    assert_eq!(indexes[2]["readiness"], "import_pending");
    assert_eq!(indexes[2]["description"], "code: indexes no enabled source");
    let sets = body["index_sets"].as_array().unwrap();
    assert_eq!(sets.len(), 3);
    assert_eq!(
        sets[0],
        json!({"id": set_id(10), "name": "web",
            "description": "web: index set of docs, code", "description_generated": true,
            "index_ids": [index_id(1), index_id(2)]})
    );
    assert_eq!(sets[2]["description"], "empty: empty index set");
}

#[test]
fn pages_stay_within_the_budget_and_cursors_are_revision_bound() {
    let catalog = listing();
    let whole = list_page(&catalog, None, 48 * 1024).unwrap();
    let mut seen = Vec::new();
    let mut after: Option<String> = None;
    for _ in 0..16 {
        let page = list_page(&catalog, after.as_deref(), 700).unwrap();
        let entries = page["indexes"].as_array().unwrap().len()
            + page["index_sets"].as_array().unwrap().len();
        assert!(entries >= 1);
        assert!(entries == 1 || serde_json::to_vec(&page).unwrap().len() <= 700);
        seen.extend(page["indexes"].as_array().unwrap().iter().cloned());
        seen.extend(page["index_sets"].as_array().unwrap().iter().cloned());
        match page.get("next_cursor").and_then(Value::as_str) {
            Some(next) => after = Some(next.to_owned()),
            None => break,
        }
    }
    let mut expected: Vec<Value> = whole["indexes"].as_array().unwrap().clone();
    expected.extend(whole["index_sets"].as_array().unwrap().iter().cloned());
    assert_eq!(seen, expected);
    let mut moved = catalog.clone();
    moved.revision = Some(revision(8));
    assert_eq!(
        list_page(&moved, Some("lix1.7.1"), 48 * 1024),
        Err("list_cursor_expired")
    );
    assert_eq!(
        list_page(&catalog, Some("lix1.7.99"), 48 * 1024),
        Err("list_cursor_expired")
    );
    assert_eq!(
        list_page(&catalog, Some("garbage"), 48 * 1024),
        Err("list_cursor_expired")
    );
}
