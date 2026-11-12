// SPDX-License-Identifier: Apache-2.0
use super::*;

#[test]
fn the_installation_state_folder_reads_the_record_log_location() {
    let base = std::env::temp_dir().join("airkctl-u89-fixture");
    let (state, logs) = (base.join("state"), base.join("logs"));
    assert_eq!(
        ring_parent(&state, Some((&state, &logs))),
        Some(logs.clone())
    );
    // Any other control root, or no installation, keeps the rings beside it.
    for control in [base.join("control"), state.join("control"), base.clone()] {
        assert_eq!(ring_parent(&control, Some((&state, &logs))), None);
    }
    assert_eq!(ring_parent(&state, None), None);
}

#[test]
fn locate_normalizes_the_record_and_the_control_root_alike() {
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
    let state = Path::new(&locations.state);
    let logs = locate(state, Some(&locations)).unwrap();
    assert!(logs.ends_with("logs"), "{logs:?}");
    // An explicit development control folder is unchanged.
    let explicit = root.join("dev-control");
    assert_eq!(locate(&explicit, Some(&locations)).unwrap(), explicit);
    assert_eq!(locate(state, None).unwrap(), state);
}
