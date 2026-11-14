// SPDX-License-Identifier: Apache-2.0
use super::super::writer_test_support::Editable;
use super::*;
use serde_json::json;

#[test]
fn config_retained_eligibility_actual_git_and_literal_package_controls() {
    let fixture = Editable::new();
    Eligibility::review(fixture.path()).unwrap();
    assert!(
        !fixture.path().join(".airk").exists(),
        "admission creates no archive or receipt"
    );
    fs::write(fixture.path().join(".gitignore"), b"").unwrap();
    assert!(Eligibility::review(fixture.path()).is_err());
}

#[test]
fn config_retained_eligibility_tracked_but_ignored_archive_is_refused() {
    let fixture = Editable::new();
    let retained = fixture.path().join(".airk/agent-edits/placeholder");
    fs::create_dir_all(retained.parent().unwrap()).unwrap();
    fs::write(&retained, b"synthetic tracked fixture").unwrap();
    fixture.git(&["add", "--force", "--", ".airk/agent-edits/placeholder"]);
    assert!(
        Eligibility::review(fixture.path()).is_err(),
        "ignore does not protect already tracked files"
    );
}

#[test]
fn config_retained_eligibility_rechecks_changed_ignore_and_package_controls() {
    let fixture = Editable::new();
    let admitted = Eligibility::review(fixture.path()).unwrap();
    fs::write(fixture.path().join(".gitignore"), b"").unwrap();
    assert!(admitted.revalidate().is_err());
    fs::write(fixture.path().join(".gitignore"), b".airk/\n").unwrap();
    fs::write(
        fixture.path().join("package.json"),
        br#"{"name":"changed","packageManager":"npm@11.12.1","files":["payload"]}"#,
    )
    .unwrap();
    assert!(
        admitted.revalidate().is_err(),
        "changed even if still disjoint requires fresh admission"
    );
}

#[test]
fn config_retained_eligibility_independent_package_overlap_and_automatic_entries() {
    let fixture = Editable::new();
    let root = fixture.path();
    let archive = root.join(".airk/agent-edits");
    for mut value in [
        json!({"files":["payload"]}),
        json!({"files":[],"main":"index.js"}),
        json!({"files":["payload"],"bin":{"fixture":"payload/main.js"}}),
    ] {
        value["packageManager"] = json!("npm@11.12.1");
        npm_excludes(root, &archive, &serde_json::to_vec(&value).unwrap()).unwrap();
    }
    for mut value in [
        json!({}),
        json!({"files":["*"]}),
        json!({"files":["."]}),
        json!({"files":[".airk"]}),
        json!({"files":[".airk/agent-edits/v1"]}),
        json!({"files":["payload"],"main":".airk/agent-edits/v1/prepared"}),
        json!({"files":["payload"],"bin":{"fixture":".airk/agent-edits/v1/before"}}),
        json!({"files":["payload"],"directories":{"bin":".airk"}}),
        json!({"files":["payload"],"browser":".airk/agent-edits/v1/prepared"}),
        json!({"files":["payload"],"browser":{"server.js":"client.js"}}),
        json!({"files":["payload"],"bundleDependencies":[]}),
        json!({"files":["payload"],"workspaces":[]}),
        json!({"files":["payload"],"scripts":{"prepare":"copy private data"}}),
    ] {
        value["packageManager"] = json!("npm@11.12.1");
        assert!(
            npm_excludes(root, &archive, &serde_json::to_vec(&value).unwrap()).is_err(),
            "{value}"
        );
    }
    for version in [Value::Null, json!("npm@11.12.2"), json!("pnpm@11.12.1")] {
        let value = json!({"files":["payload"],"packageManager":version});
        assert!(npm_excludes(root, &archive, &serde_json::to_vec(&value).unwrap()).is_err());
    }
}

#[test]
fn config_retained_eligibility_unknown_packaging_context_refuses_without_source_reads() {
    let fixture = Editable::new();
    fs::write(
        fixture.path().join("Cargo.toml"),
        b"[package]\nname='fixture'\nversion='0.0.0'\n",
    )
    .unwrap();
    assert!(Eligibility::review(fixture.path()).is_err());
    assert!(!fixture.path().join(".airk").exists());
}

#[test]
fn config_retained_eligibility_never_executes_git_from_the_source_worktree() {
    let fixture = Editable::new();
    let actual = git_program(fixture.path()).unwrap();
    let shadow = fixture.path().join("shadow");
    fs::create_dir(&shadow).unwrap();
    fs::write(
        shadow.join(if cfg!(windows) { "git.exe" } else { "git" }),
        b"untrusted fixture; never execute",
    )
    .unwrap();
    let resolved = resolve_git_path(
        fixture.path(),
        vec![shadow, actual.parent().unwrap().into()].into_iter(),
    )
    .unwrap();
    assert_eq!(resolved, actual);
}
