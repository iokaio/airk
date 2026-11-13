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
fn choice_off_narrows_within_the_default_scope() {
    let off = open(AgentChoice::Off, &[1], &[]);
    let docs = names(&["docs"]);
    // U109: names narrow the assigned set (checked when resolved).
    assert_eq!(
        requested(&docs, &[], Some(&off)),
        Ok(Requested::Narrowed {
            indexes: &docs,
            sets: &[],
            default_indexes: &[index_id(1)],
            default_sets: &[],
        })
    );
    assert!(matches!(
        requested(&[], &docs, Some(&off)),
        Ok(Requested::Narrowed { .. })
    ));
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
    // An empty assigned set cannot be narrowed either.
    assert_eq!(
        requested(&docs, &[], Some(&unset)),
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
    // A unique case-insensitive name selects too (after an exact miss).
    assert_eq!(resolve_names(&["Docs"], &[]), Ok(vec![index_id(1)]));
    assert_eq!(
        resolve_names(&["documents"], &[]),
        Err("scope_name_unknown")
    );
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

/// U109: with choice off a call may name a non-empty subset of the assigned
/// set (default sets expanded); anything outside it is refused and named.
#[test]
fn choice_off_names_must_stay_inside_the_assigned_set() {
    let catalog = listing();
    // Assigned: index 3 and set 10 (members 1 and 2).
    let off = open(AgentChoice::Off, &[3], &[10]);
    let ask = |indexes: &[&str], sets: &[&str]| {
        let (indexes, sets) = (names(indexes), names(sets));
        let requested = requested(&indexes, &sets, Some(&off)).unwrap();
        (
            resolve(&catalog, requested),
            outside_assigned_set(&catalog, requested).unwrap_or_default(),
        )
    };
    let docs = index_id(1).to_string();
    let web = set_id(10).to_string();
    // A subset is read alone; a named assigned set is inside too.
    assert_eq!(ask(&[&docs], &[]), (Ok(vec![index_id(1)]), vec![]));
    assert_eq!(
        ask(&[], &[&web]),
        (Ok(vec![index_id(1), index_id(2)]), vec![])
    );
    // Retired index 4 of set 10 is not a member read, so set 10 is inside.
    // Set 12 is empty: nothing to read.
    assert_eq!(
        ask(&[], &[&set_id(12).to_string()]).0,
        Err("scoped_read_unavailable")
    );
    // Outside: refused, and the outside indexes are named.
    let mut assigned_one = open(AgentChoice::Off, &[1], &[]);
    assigned_one.defaults_revision = 4;
    let outside = names(&[&index_id(2).to_string()]);
    let narrowed = requested(&outside, &[], Some(&assigned_one)).unwrap();
    assert_eq!(
        resolve(&catalog, narrowed),
        Err("agent_scope_choice_disabled")
    );
    let named = outside_assigned_set(&catalog, narrowed).unwrap();
    assert_eq!(named, vec![index_id(2)]);
    let message = outside_message(&catalog, &named);
    assert!(message.starts_with("code ("), "{message}");
    assert!(
        message.contains("outside this connection's assigned set"),
        "{message}"
    );
    // A set only partly inside is refused.
    let web_names = names(&[&web]);
    let partly = requested(&[], &web_names, Some(&assigned_one)).unwrap();
    assert_eq!(
        resolve(&catalog, partly),
        Err("agent_scope_choice_disabled")
    );
    assert_eq!(
        outside_assigned_set(&catalog, partly).unwrap(),
        vec![index_id(2)]
    );
}

/// Name precedence: an id, then the exact name, then a unique
/// case-insensitive name; several matches at the deciding level refuse.
#[test]
fn names_fall_back_to_a_unique_case_insensitive_match() {
    let mut catalog = listing();
    catalog.indexes.push(index(5, "Readme", false));
    catalog.indexes.push(index(6, "README", false));
    catalog.projects.push(set(13, "Airk"));
    // Exact wins even when a case variant exists.
    assert_eq!(index_by_name(&catalog, "Readme"), Ok(index_id(5)));
    assert_eq!(index_by_name(&catalog, "README"), Ok(index_id(6)));
    // Case-only matches: unique accepted, several refused.
    assert_eq!(index_by_name(&catalog, "DOCS"), Ok(index_id(1)));
    assert_eq!(
        index_by_name(&catalog, "readme"),
        Err("scope_name_ambiguous")
    );
    assert_eq!(set_by_name(&catalog, "airk"), Ok(set_id(13)));
    assert_eq!(set_by_name(&catalog, "WEB"), Err("scope_name_ambiguous"));
    // The id form is never a name.
    assert_eq!(
        index_by_name(&catalog, &index_id(5).to_string().to_uppercase()),
        Err("scope_name_unknown")
    );
    assert_eq!(
        index_by_name(&catalog, &index_id(5).to_string()),
        Ok(index_id(5))
    );
}
