// SPDX-License-Identifier: Apache-2.0
use super::*;

fn roster(context: &Path) -> Roster {
    Roster {
        items: vec![],
        context: context.into(),
    }
}

#[test]
fn predecessor_orphan_os_registrations_are_reported_without_an_install_root_or_name_authority() {
    let directory = tempfile::tempdir().unwrap();
    let context = directory.path().join("original-receipt");
    let mut report = roster(&context);
    windows_observations(
        &mut report,
        &json!({"tasks":["\\airksvc-11111111111111111111111111111111"],
        "uninstall":["HKCU\\Uninstall\\AIRK-old"],"path":["C:\\AIRK\\standalone\\runtime"],"complete":true}),
    );
    assert_eq!(report.items.len(), 3);
    assert_eq!(
        report
            .items
            .iter()
            .filter(|item| item.restarts_old_service)
            .count(),
        1
    );
    assert!(
        report
            .items
            .iter()
            .all(|item| item.reason.starts_with("unproven_preserved:"))
    );
    assert!(
        report
            .items
            .iter()
            .any(|item| item.item == "\\airksvc-11111111111111111111111111111111")
    );
    assert!(
        !context.exists(),
        "discovery never creates a receipt or install root"
    );
}

#[test]
fn predecessor_orphan_os_custom_executable_locations_are_disclosed_without_executing_them() {
    let mut report = roster(Path::new("receipt"));
    windows_observations(
        &mut report,
        &json!({"tasks":[{"item":"\\airksvc-11111111111111111111111111111111",
        "executables":["C:\\custom\\retired-AIRK\\runtimes\\saved\\airk.exe"]}],"uninstall":[],"path":[],"complete":true}),
    );
    assert_eq!(report.items.len(), 2);
    assert!(report.items.iter().all(|item| item.restarts_old_service));
    assert!(report.items.iter().any(|item| {
        item.item == "C:\\custom\\retired-AIRK\\runtimes\\saved\\airk.exe"
            && item
                .reason
                .contains("original captured runtime/install receipt")
    }));
}

#[test]
fn predecessor_orphan_mac_gui_candidates_preserve_orphans_and_exclude_only_verified_current_labels()
{
    let current = BTreeSet::from(["io.iokaio.airk.service.stable".to_owned()]);
    let mut report = roster(Path::new("receipt"));
    mac_observations(
        &mut report,
        "services = {\n io.iokaio.airk.service.stable = {\n }\n io.iokaio.airk.service.preview = {\n }\n airk-11111111111111111111111111111111 = {\n }\n}",
        &current,
    );
    assert_eq!(report.items.len(), 2);
    assert!(report.items.iter().all(|item| item.restarts_old_service));
    assert!(
        !report
            .items
            .iter()
            .any(|item| item.item == "io.iokaio.airk.service.stable")
    );
}

#[test]
fn predecessor_orphan_unknown_native_observations_never_become_absence() {
    let mut report = roster(Path::new("receipt"));
    windows_observations(&mut report, &json!({"tasks":[],"complete":false}));
    assert!(
        report
            .items
            .iter()
            .any(|item| item.item == "uninstall registration"
                && item.reason.starts_with("discovery_unknown:"))
    );
    assert!(
        report
            .items
            .iter()
            .any(|item| item.restarts_old_service && item.reason.starts_with("discovery_unknown:"))
    );
    assert!(
        report
            .items
            .iter()
            .all(|item| !item.reason.contains("removed"))
    );
}

#[test]
fn predecessor_orphan_bounded_disclosure_preserves_restart_risks_and_explicit_omissions() {
    let context = Path::new("original/setup-receipt.json");
    let mut items = Vec::new();
    for n in 0..airk_core::install_record::MAX_SETUP_LEFTOVERS {
        retain_bounded(
            &mut items,
            Leftover {
                item: format!("cache-{n:04}"),
                reason: "unproven_preserved: cache".into(),
                restarts_old_service: false,
            },
            context,
        );
    }
    retain_bounded(
        &mut items,
        Leftover {
            item: "orphan login task".into(),
            reason: "unproven_preserved: may restart old image".into(),
            restarts_old_service: true,
        },
        context,
    );
    assert_eq!(items.len(), airk_core::install_record::MAX_SETUP_LEFTOVERS);
    assert_eq!(items[0].item, "orphan login task");
    let marker = items.iter().find(|item| item.reason == OMITTED).unwrap();
    assert_eq!(marker.item, context.display().to_string());
    assert!(marker.restarts_old_service);
    assert!(marker.reason.contains("at least one"));
    assert!(marker.reason.contains("incomplete"));
    for n in 0..20 {
        retain_bounded(
            &mut items,
            Leftover {
                item: format!("more-{n}"),
                reason: "unproven_preserved".into(),
                restarts_old_service: false,
            },
            context,
        );
    }
    assert_eq!(items.len(), airk_core::install_record::MAX_SETUP_LEFTOVERS);
    assert_eq!(
        items.iter().filter(|item| item.reason == OMITTED).count(),
        1
    );
    assert!(items.iter().any(|item| item.item == "orphan login task"));
}

#[test]
fn predecessor_orphan_receipts_and_caches_are_preserved_and_enumeration_is_bounded() {
    let directory = tempfile::tempdir().unwrap();
    let anchor = directory.path().join("private");
    airk_ipc::endpoint::prepare_root(&anchor).unwrap();
    #[cfg(windows)]
    {
        // Record the inherited-parent observation separately. TempDir ACLs
        // depend on the host and cannot qualify the bounded-enumeration fixture.
        let mut refused = roster(&anchor.join("refused-context.json"));
        refused.directory(&anchor, "unprotected parent probe", false, |_| true);
        eprintln!(
            "actual inherited-parent directory observation: {:?}",
            refused.items
        );
    }
    // Both the receipt's parent and the directory observation's parent need
    // native protection. A private leaf alone does not qualify its parent.
    let root = anchor.join("receipts");
    airk_ipc::endpoint::prepare_root(&root).unwrap();
    let root = root.canonicalize().unwrap();
    let receipt = root.join("shell-original.json");
    let bytes = b"original opaque ownership receipt\n";
    std::fs::write(&receipt, bytes).unwrap();
    let mut report = roster(&root.join("original-setup.json"));
    report.candidate(&receipt, "original shell receipt", false);
    assert_eq!(std::fs::read(&receipt).unwrap(), bytes);
    assert_eq!(report.items.len(), 1);
    assert!(report.items[0].reason.starts_with("unproven_preserved:"));
    for n in 0..=MAX_ENTRIES {
        std::fs::write(root.join(format!("airk-{n:032x}.json")), b"fixture").unwrap();
    }
    report.directory(&root, "legacy receipt directory", true, |name| {
        name.starts_with("airk-")
    });
    assert!(
        report
            .items
            .iter()
            .any(|item| item.reason.contains("additional candidates are unknown")),
        "actual bounded directory observations: {:?}",
        report.items
    );
    assert_eq!(std::fs::read(receipt).unwrap(), bytes);
}

#[cfg(unix)]
#[test]
fn predecessor_orphan_links_are_refused_without_reading_or_modifying_targets() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().canonicalize().unwrap();
    let target = root.join("source");
    std::fs::write(&target, b"source bytes").unwrap();
    let link = root.join("receipt.json");
    std::os::unix::fs::symlink(&target, &link).unwrap();
    let mut report = roster(&root.join("original-setup.json"));
    report.candidate(&link, "receipt", true);
    assert_eq!(report.items.len(), 1);
    assert!(report.items[0].reason.starts_with("inspection_refused:"));
    assert_eq!(std::fs::read(target).unwrap(), b"source bytes");
    assert!(
        std::fs::symlink_metadata(link)
            .unwrap()
            .file_type()
            .is_symlink()
    );
}

#[test]
fn predecessor_orphan_native_name_selection_excludes_current_split_tasks() {
    for name in [
        "airk-11111111111111111111111111111111",
        "airksvc-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.service",
        "airk-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA.plist",
    ] {
        assert!(old_id(name), "{name}");
    }
    for name in [
        "airk-service-11111111111111111111111111111111",
        "airkctl",
        "airk-unrelated",
        "someone-else.service",
    ] {
        assert!(!old_id(name), "{name}");
    }
}
