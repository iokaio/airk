// SPDX-License-Identifier: Apache-2.0
use super::*;

fn index(byte: u8) -> IndexId {
    IndexId::from_identity(Incarnation::from_bytes([byte; 16]))
}

fn defaults() -> WorkspaceDefaults {
    WorkspaceDefaults::empty(Incarnation::from_bytes([7; 16]))
}

#[test]
fn the_most_specific_containing_folder_wins() -> Result<()> {
    let mut stored = defaults();
    stored.apply(&["D:\\code".into()], &[index(1)], &[], 10)?;
    stored.apply(&["d:/code/airk/".into()], &[index(2)], &[], 20)?;
    let hit = |path: &str| stored.matching(path).map(|entry| entry.index_ids[0]);
    assert_eq!(hit("D:\\code\\airk\\crates"), Some(index(2)));
    assert_eq!(hit("d:/code/airk"), Some(index(2)));
    assert_eq!(hit("D:\\code\\other"), Some(index(1)));
    assert_eq!(hit("D:\\code"), Some(index(1)));
    // A sibling sharing a prefix is not inside.
    assert_eq!(hit("D:\\codex"), None);
    assert_eq!(hit("E:\\code"), None);
    Ok(())
}

#[test]
fn apply_replaces_case_insensitively_and_clears_with_no_selection() -> Result<()> {
    let mut stored = defaults();
    stored.apply(
        &["C:\\Work\\A".into(), "C:\\Work\\B".into()],
        &[index(1)],
        &[],
        1,
    )?;
    stored.apply(&["c:/work/a".into()], &[index(3)], &[], 2)?;
    assert_eq!(stored.entries.len(), 2);
    assert_eq!(
        stored.matching("C:\\Work\\A").unwrap().index_ids,
        vec![index(3)]
    );
    assert_eq!(stored.matching("C:\\Work\\A").unwrap().applied_unix_ms, 2);
    stored.apply(&["C:\\Work\\B".into()], &[], &[], 3)?;
    assert_eq!(stored.entries.len(), 1);
    assert!(stored.matching("C:\\Work\\B").is_none());
    stored.validate()
}

#[test]
fn paths_must_be_absolute_and_bounded() {
    for bad in ["", "relative/path", "C:", "a\u{0}b", &"/x".repeat(3000)] {
        assert!(validate_path(bad).is_err(), "{bad:?}");
    }
    for good in ["/home/me", "C:\\x", "c:/x", "\\\\server\\share"] {
        assert!(validate_path(good).is_ok(), "{good:?}");
    }
    let mut stored = defaults();
    assert!(stored.apply(&[], &[index(1)], &[], 1).is_err());
    assert!(stored.apply(&["rel".into()], &[index(1)], &[], 1).is_err());
}

#[test]
fn the_file_round_trips_and_refuses_unknown_fields() -> Result<()> {
    let mut stored = defaults();
    stored.apply(&["/srv/repo".into()], &[index(4)], &[], 5)?;
    let bytes = serde_json::to_vec(&stored)?;
    let parsed: WorkspaceDefaults = serde_json::from_slice(&bytes)?;
    assert_eq!(parsed, stored);
    let mut value: serde_json::Value = serde_json::from_slice(&bytes)?;
    value["extra"] = serde_json::json!(1);
    assert!(serde_json::from_value::<WorkspaceDefaults>(value).is_err());
    Ok(())
}

#[test]
fn file_uris_become_local_paths() {
    assert_eq!(
        path_from_file_uri("file:///d%3A/code/airk").as_deref(),
        Some("d:/code/airk")
    );
    assert_eq!(
        path_from_file_uri("file:///D:/code").as_deref(),
        Some("D:/code")
    );
    assert_eq!(path_from_file_uri("file:///C:").as_deref(), Some("C:/"));
    assert_eq!(
        path_from_file_uri("file:///home/me/x%20y").as_deref(),
        Some("/home/me/x y")
    );
    assert_eq!(
        path_from_file_uri("file://localhost/srv").as_deref(),
        Some("/srv")
    );
    assert_eq!(
        path_from_file_uri("file://server/share/x").as_deref(),
        Some("//server/share/x")
    );
    assert_eq!(path_from_file_uri("https://example.com/x"), None);
    assert_eq!(path_from_file_uri("file:///bad%2"), None);
}
