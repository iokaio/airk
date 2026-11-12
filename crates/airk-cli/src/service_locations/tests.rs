// SPDX-License-Identifier: Apache-2.0
use super::*;

fn installation(base: &Path) -> Installed {
    Installed {
        state: base.join("state"),
        logs: base.join("logs"),
    }
}

#[test]
fn the_installed_state_folder_takes_the_record_log_location() {
    let base = std::env::temp_dir().join("airk-u89-fixture");
    let adopted = installation(&base);
    assert_eq!(
        logs_for(&base.join("state"), Some(&adopted)),
        Some(base.join("logs"))
    );
}

#[test]
fn other_control_roots_keep_the_pre_split_selection() {
    let base = std::env::temp_dir().join("airk-u89-fixture");
    let adopted = installation(&base);
    // An explicit development or test control folder, a sibling, a child of
    // the state folder and no adoption at all (`service-foreground`).
    for control in [
        base.join("control"),
        base.join("state-2"),
        base.join("state").join("control"),
        base.clone(),
    ] {
        assert_eq!(logs_for(&control, Some(&adopted)), None, "{control:?}");
    }
    assert_eq!(logs_for(&base.join("state"), None), None);
}

#[test]
fn a_control_root_belongs_only_to_the_installation_with_that_state_folder() {
    let base = std::env::temp_dir().join("airk-u89-fixture");
    assert_eq!(
        belongs(&base.join("state"), installation(&base)),
        Some(installation(&base))
    );
    assert_eq!(belongs(&base.join("elsewhere"), installation(&base)), None);
}

#[test]
fn record_locations_are_normalized_like_the_control_root() {
    let owner = tempfile::tempdir().unwrap();
    let root = owner.path().canonicalize().unwrap();
    let text = |path: &Path| path.to_str().unwrap().to_owned();
    let locations = Locations {
        program: text(&root.join("program")),
        data: text(&root.join("data")),
        state: text(&root.join("state")),
        cache: text(&root.join("cache")),
        logs: text(&root.join("logs")),
    };
    let normalized = installed(&locations).unwrap();
    // The control root as `airkctl service start` passes it (the record's
    // text) normalizes to the same state folder.
    let control = normal(Path::new(&locations.state)).unwrap();
    assert_eq!(
        logs_for(&control, Some(&normalized)),
        Some(normal(&root.join("logs")).unwrap())
    );
}
