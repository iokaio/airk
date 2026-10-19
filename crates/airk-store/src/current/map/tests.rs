// SPDX-License-Identifier: Apache-2.0
use super::*;
use airk_core::identity::Incarnation;
use rusqlite::Connection;
use std::collections::BTreeMap;

fn scope() -> CurrentScope {
    CurrentScope {
        root: "C:/originals/current-fixture".into(),
        collection: "current-fixture".into(),
        incarnation: Incarnation::from_bytes([4; 16]),
    }
}
fn database() -> Result<Connection> {
    let connection = Connection::open_in_memory()?;
    create(&connection)?;
    Ok(connection)
}
fn artifact(number: usize) -> Result<Sha256Digest> {
    key(
        &scope(),
        MapDomain::Artifact,
        format!("file-{number:05}.txt").as_bytes(),
    )
}

#[test]
fn keys_prove_membership_absence_collision_and_complete_bounded_enumeration() -> Result<()> {
    let connection = database()?;
    let scope = scope();
    let mut root = empty_root(&scope)?;
    let mut expected = BTreeMap::new();
    for number in 0..128 {
        let key = artifact(number)?;
        let value = format!("captured-{number:05}").into_bytes();
        root = put(&connection, &scope, root, key, Some(&value))?;
        expected.insert(key, value);
    }
    for (key, value) in &expected {
        assert_eq!(get(&connection, &scope, root, *key)?, Some(value.clone()));
    }
    assert_eq!(get(&connection, &scope, root, artifact(100_001)?)?, None);
    let mut visited = BTreeMap::new();
    visit_all(
        &connection,
        &scope,
        root,
        MapVisitLimits::default(),
        &mut |key, value| {
            visited.insert(key, value.to_vec());
            Ok(())
        },
    )?;
    assert_eq!(visited, expected);
    assert!(
        visit_all(
            &connection,
            &scope,
            root,
            MapVisitLimits {
                entries: 10,
                ..Default::default()
            },
            &mut |_, _| Ok(())
        )
        .is_err()
    );

    let mut prefixes = BTreeMap::new();
    let (first, second) = (0..10_000)
        .find_map(|number| {
            let key = artifact(number + 1000).ok()?;
            prefixes.insert(prefix(key), key).map(|old| (old, key))
        })
        .context("fixture found no 16-bit collision")?;
    root = put(&connection, &scope, root, first, Some(b"first"))?;
    root = put(&connection, &scope, root, second, Some(b"second"))?;
    assert_eq!(
        get(&connection, &scope, root, first)?,
        Some(b"first".to_vec())
    );
    assert_eq!(
        get(&connection, &scope, root, second)?,
        Some(b"second".to_vec())
    );
    root = put(&connection, &scope, root, first, None)?;
    assert_eq!(get(&connection, &scope, root, first)?, None);
    assert_eq!(
        get(&connection, &scope, root, second)?,
        Some(b"second".to_vec())
    );
    root = put(&connection, &scope, root, second, None)?;
    assert_eq!(get(&connection, &scope, root, second)?, None);
    Ok(())
}

#[test]
fn changed_bytes_missing_buckets_and_foreign_scope_fail_closed() -> Result<()> {
    let scope = scope();
    let key = artifact(1)?;
    for mutation in [
        "UPDATE current_map_buckets SET content=zeroblob(length(content)) WHERE prefix=?1",
        "DELETE FROM current_map_buckets WHERE prefix=?1",
        "DELETE FROM current_map_nodes WHERE height=0 AND idx=?1",
    ] {
        let connection = database()?;
        let root = put(
            &connection,
            &scope,
            empty_root(&scope)?,
            key,
            Some(b"original"),
        )?;
        connection.execute(mutation, [i64::from(prefix(key))])?;
        if mutation.starts_with("DELETE FROM current_map_nodes") {
            // A point read reconstructs its own leaf from the bucket. A full
            // traversal must also authenticate the stored branch child.
            assert_eq!(
                get(&connection, &scope, root, key)?,
                Some(b"original".to_vec())
            );
        } else {
            assert!(get(&connection, &scope, root, key).is_err(), "{mutation}");
        }
        assert!(
            visit_all(
                &connection,
                &scope,
                root,
                MapVisitLimits::default(),
                &mut |_, _| Ok(())
            )
            .is_err(),
            "{mutation}"
        );
    }
    let connection = database()?;
    let root = put(
        &connection,
        &scope,
        empty_root(&scope)?,
        key,
        Some(b"original"),
    )?;
    let mut foreign = scope.clone();
    foreign.incarnation = Incarnation::from_bytes([7; 16]);
    assert!(get(&connection, &foreign, root, key).is_err());
    assert!(
        super::key(&scope, MapDomain::Vector, b"same")?
            != super::key(&scope, MapDomain::Artifact, b"same")?
    );
    Ok(())
}

#[test]
fn oversized_or_unordered_inputs_do_not_publish_a_new_root() -> Result<()> {
    let connection = database()?;
    let scope = scope();
    let original = empty_root(&scope)?;
    let key = artifact(1)?;
    assert!(
        put(
            &connection,
            &scope,
            original,
            key,
            Some(&vec![0; MAX_VALUE_BYTES + 1])
        )
        .is_err()
    );
    assert_eq!(get(&connection, &scope, original, key)?, None);
    let root = put(&connection, &scope, original, key, Some(b"valid"))?;
    let mut entries = vec![
        Entry {
            key,
            value: b"first".to_vec(),
        },
        Entry {
            key,
            value: b"second".to_vec(),
        },
    ];
    assert!(encode(prefix(key), &entries).is_err());
    entries.reverse();
    assert!(encode(prefix(key), &entries).is_err());
    assert_eq!(
        get(&connection, &scope, root, key)?,
        Some(b"valid".to_vec())
    );
    Ok(())
}
