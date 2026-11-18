// SPDX-License-Identifier: Apache-2.0
use super::*;
use airk_process::directory::OpenedDirectory;

fn fixture() -> (tempfile::TempDir, AliasContext) {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("private");
    airk_ipc::endpoint::prepare_root(&home).unwrap();
    let program = home.join("program");
    let target = program.join(if cfg!(windows) {
        "bin/airkctl.exe"
    } else {
        "bin/airkctl"
    });
    let context = AliasContext {
        program,
        home,
        target,
        install_id: Incarnation::from_bytes([9; 16]),
        allow_owned: false,
        lookup_directory: std::env::current_dir().unwrap(),
    };
    (temp, context)
}

#[test]
fn alias_names_refuse_paths_reserved_devices_and_shell_syntax() {
    for name in ["rk", "rk123", "my_cli-2"] {
        validate_alias(name).unwrap();
    }
    for name in [
        "", "../rk", "/rk", "r.k", "1rk", "airk", "AIRKCTL", "NuL", "COM1", "lpt9", "a b", "x%",
        "x!", "x\n",
    ] {
        assert!(validate_alias(name).is_err(), "{name:?}");
    }
    assert!(validate_alias(&"a".repeat(33)).is_err());
}

#[test]
fn auto_alias_observes_path_and_destination_and_selects_first_free() {
    let (_temp, context) = fixture();
    let external = context.home.as_path().join("external");
    std::fs::create_dir(&external).unwrap();
    std::fs::write(external.join("rk"), b"foreign command").unwrap();
    let destination = expected(&context.home, &context.target, context.install_id, "rk1")
        .unwrap()
        .launcher_path;
    std::fs::create_dir_all(destination.parent().unwrap()).unwrap();
    std::fs::write(&destination, b"foreign destination").unwrap();
    let search = external.to_str().unwrap();
    let selected = select(
        &context,
        &crate::setup_role::CliAliasSelection::Auto,
        search,
    )
    .unwrap();
    assert_eq!(selected.alias.as_deref(), Some("rk2"));
    assert!(
        select(
            &context,
            &crate::setup_role::CliAliasSelection::Named("rk".into()),
            search
        )
        .is_err()
    );
    assert_eq!(std::fs::read(destination).unwrap(), b"foreign destination");
}

#[test]
fn alias_intent_binds_original_operation_and_refuses_recovery_overrides() {
    let (_temp, context) = fixture();
    let operation = context.home.as_path().join("operation.json");
    let digest = Sha256Digest::from_bytes([3; 32]);
    bind(
        &operation,
        digest,
        &context,
        Some(&crate::setup_role::CliAliasSelection::None),
        None,
        false,
    )
    .unwrap();
    let saved = load_intent(&operation, digest).unwrap().unwrap();
    assert!(saved.alias.is_none());
    bind(&operation, digest, &context, None, None, true).unwrap();
    assert!(
        bind(
            &operation,
            digest,
            &context,
            Some(&crate::setup_role::CliAliasSelection::Auto),
            Some(""),
            true
        )
        .is_err()
    );
    assert!(load_intent(&operation, Sha256Digest::from_bytes([4; 32])).is_err());
}

#[test]
fn legacy_operation_does_not_guess_alias_and_admitted_request_cannot_add_choice() {
    let (_temp, context) = fixture();
    let operation = context.home.as_path().join("operation.json");
    let digest = Sha256Digest::from_bytes([3; 32]);
    bind(&operation, digest, &context, None, None, true).unwrap();
    assert!(load_intent(&operation, digest).unwrap().is_none());
    assert!(
        bind(
            &operation,
            digest,
            &context,
            Some(&crate::setup_role::CliAliasSelection::None),
            None,
            true
        )
        .is_err()
    );
    assert!(
        !context
            .home
            .as_path()
            .join("cli-alias-intent.json")
            .exists()
    );
}

#[test]
fn alias_note_changes_selection_without_leaking_path_and_preserves_modified_note() {
    let (_temp, context) = fixture();
    let root = OpenedDirectory::open(context.home.as_path()).unwrap();
    write_note(
        &root,
        &context.home,
        &context.target,
        context.install_id,
        Some("rk2"),
    )
    .unwrap();
    let text = std::fs::read_to_string(context.home.as_path().join("airkctl-aliases.txt")).unwrap();
    assert!(text.contains("Selected alias: rk2\n"));
    assert!(text.contains(&format!(
        "Canonical executable: {}",
        context.target.display()
    )));
    write_note(
        &root,
        &context.home,
        &context.target,
        context.install_id,
        None,
    )
    .unwrap();
    let text = std::fs::read_to_string(context.home.as_path().join("airkctl-aliases.txt")).unwrap();
    assert!(text.contains("Selected alias: none\n"));
    std::fs::write(
        context.home.as_path().join("airkctl-aliases.txt"),
        b"user text",
    )
    .unwrap();
    assert!(remove_note(&root, &context.home, &context.target, context.install_id).is_err());
    assert_eq!(
        std::fs::read(context.home.as_path().join("airkctl-aliases.txt")).unwrap(),
        b"user text"
    );
}

#[test]
fn alias_note_never_adopts_unowned_identical_text() {
    let (_temp, context) = fixture();
    let root = OpenedDirectory::open(context.home.as_path()).unwrap();
    write_note(
        &root,
        &context.home,
        &context.target,
        context.install_id,
        None,
    )
    .unwrap();
    std::fs::remove_file(context.home.as_path().join("cli-alias-note-owner.json")).unwrap();
    let before = std::fs::read(context.home.as_path().join("airkctl-aliases.txt")).unwrap();
    assert!(
        write_note(
            &root,
            &context.home,
            &context.target,
            context.install_id,
            None
        )
        .is_err()
    );
    assert_eq!(
        std::fs::read(context.home.as_path().join("airkctl-aliases.txt")).unwrap(),
        before
    );
}

#[test]
fn saved_lookup_directory_keeps_relative_and_empty_path_meaning() {
    let (_temp, mut context) = fixture();
    let original = context.home.as_path().join("original");
    let changed = context.home.as_path().join("changed");
    std::fs::create_dir_all(original.join("tools")).unwrap();
    std::fs::create_dir_all(changed.join("tools")).unwrap();
    std::fs::write(original.join("tools/rk"), b"original conflict").unwrap();
    context.lookup_directory = original.clone();
    assert_eq!(probe(&context, "rk", "tools", &[]).unwrap(), "collision");
    let selection = select(
        &context,
        &crate::setup_role::CliAliasSelection::Auto,
        "tools",
    )
    .unwrap();
    assert_eq!(selection.alias.as_deref(), Some("rk1"));
    context.lookup_directory = changed;
    assert_eq!(probe(&context, "rk", "tools", &[]).unwrap(), "available");
    // Replay restores the saved context, independent of the later observer cwd.
    context.lookup_directory = selection.lookup_directory;
    assert_eq!(
        probe(
            &context,
            "rk",
            &selection.search_path,
            &selection.path_extensions
        )
        .unwrap(),
        "collision"
    );
    std::fs::write(original.join("rk"), b"empty entry conflict").unwrap();
    assert_eq!(probe(&context, "rk", "", &[]).unwrap(), "collision");
}

#[test]
fn saved_named_selection_refuses_new_collision_without_reselecting() {
    let (_temp, context) = fixture();
    let external = context.home.as_path().join("external");
    std::fs::create_dir(&external).unwrap();
    let selected = select(
        &context,
        &crate::setup_role::CliAliasSelection::Auto,
        external.to_str().unwrap(),
    )
    .unwrap();
    assert_eq!(selected.alias.as_deref(), Some("rk"));
    std::fs::write(external.join("rk"), b"new foreign command").unwrap();
    assert_eq!(
        probe(
            &context,
            selected.alias.as_deref().unwrap(),
            &selected.search_path,
            &selected.path_extensions
        )
        .unwrap(),
        "collision"
    );
    assert_eq!(selected.alias.as_deref(), Some("rk"));
}

#[cfg(windows)]
#[test]
fn windows_alias_note_and_owner_ads_refuse_update_and_removal() {
    for name in ["airkctl-aliases.txt", "cli-alias-note-owner.json"] {
        let (_temp, context) = fixture();
        let root = OpenedDirectory::open(context.home.as_path()).unwrap();
        write_note(
            &root,
            &context.home,
            &context.target,
            context.install_id,
            Some("rk"),
        )
        .unwrap();
        let note = context.home.as_path().join("airkctl-aliases.txt");
        let owner = context.home.as_path().join("cli-alias-note-owner.json");
        let old_note = std::fs::read(&note).unwrap();
        let old_owner = std::fs::read(&owner).unwrap();
        let stream = context.home.as_path().join(format!("{name}:foreign"));
        std::fs::write(&stream, b"retained stream").unwrap();
        assert!(remove_note(&root, &context.home, &context.target, context.install_id).is_err());
        assert!(
            write_note(
                &root,
                &context.home,
                &context.target,
                context.install_id,
                None
            )
            .is_err()
        );
        assert_eq!(std::fs::read(&note).unwrap(), old_note);
        assert_eq!(std::fs::read(&owner).unwrap(), old_owner);
        assert_eq!(std::fs::read(&stream).unwrap(), b"retained stream");
    }
}
