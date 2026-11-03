// SPDX-License-Identifier: Apache-2.0
use super::*;

#[test]
fn expired_observation_does_no_marker_capture_or_child_enumeration() -> Result<()> {
    let owner = tempfile::tempdir()?;
    fs::write(owner.path().join(".git"), "gitdir: not-read\n")?;
    let observation = observe_repository_at(
        owner.path(),
        true,
        &BroadRootLocations::default(),
        Instant::now() - Duration::from_secs(1),
    )?;
    assert!(observation.deadline_exhausted);
    assert!(!observation.reply_truncated);
    assert_eq!(observation.marker_checks, 0);
    assert_eq!(observation.git_marker_reads, 0);
    assert_eq!(observation.child_entries_observed, 0);
    Ok(())
}

#[test]
fn reply_budget_discloses_omitted_markers_without_expanding_the_folder_scan() -> Result<()> {
    let owner = tempfile::tempdir()?;
    for number in 0..20 {
        let child = owner.path().join(format!("repo-{number}"));
        fs::create_dir(&child)?;
        fs::write(
            child.join(".git"),
            format!("gitdir: {}\n", "x".repeat(3000)),
        )?;
    }
    let observation =
        observe_repository_with_locations(owner.path(), true, &BroadRootLocations::default())?;
    assert!(observation.reply_truncated);
    assert!(!observation.children_truncated);
    assert!(!observation.deadline_exhausted);
    assert!(observation.child_directories_inspected < 20);
    assert!(observation.git_marker_reads > observation.repository_markers.len());
    assert!(encoded_size(&observation, MAX_REPOSITORY_BYTES).is_ok());
    Ok(())
}

#[test]
fn marker_observations_are_shallow_and_report_gitdir_without_following_it() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let root = owner.path().join("root");
    let outside = owner.path().join("outside");
    fs::create_dir_all(root.join("child/deeper/.git"))?;
    fs::create_dir_all(root.join(".hg"))?;
    fs::create_dir_all(&outside)?;
    let original = "gitdir: ../outside\n";
    fs::write(root.join(".git"), original)?;
    fs::write(outside.join("sentinel"), b"never followed or changed")?;
    fs::create_dir_all(root.join("child/.jj"))?;
    let root_only =
        observe_repository_with_locations(&root, false, &BroadRootLocations::default())?;
    assert_eq!(root_only.child_entries_observed, 0);
    assert_eq!(root_only.marker_checks, 4);
    let git = root_only
        .repository_markers
        .iter()
        .find(|marker| marker.marker == ".git")
        .unwrap();
    assert_eq!(git.gitdir_target.as_deref(), Some("../outside"));
    assert_eq!(git.target_state, GitTargetState::Captured);
    let children = observe_repository_with_locations(&root, true, &BroadRootLocations::default())?;
    assert!(
        children
            .repository_markers
            .iter()
            .any(|marker| marker.relative_root == "child" && marker.marker == ".jj")
    );
    assert!(
        !children
            .repository_markers
            .iter()
            .any(|marker| marker.relative_root.contains("deeper"))
    );
    assert_eq!(children.git_marker_reads, 1);
    assert_eq!(children.git_marker_bytes_captured, original.len());
    assert_eq!(children.git_marker_read_budget_bytes, 4097);
    assert_eq!(fs::read(root.join(".git"))?, original.as_bytes());
    assert_eq!(
        fs::read(outside.join("sentinel"))?,
        b"never followed or changed"
    );
    assert_eq!(fs::read_dir(&outside)?.count(), 1);
    Ok(())
}

#[test]
fn directory_sampling_and_oversized_marker_targets_have_explicit_bounds() -> Result<()> {
    let owner = tempfile::tempdir()?;
    fs::write(owner.path().join(".git"), vec![b'x'; 9000])?;
    for number in 0..200 {
        fs::create_dir(owner.path().join(format!("child-{number:03}")))?;
    }
    let observation =
        observe_repository_with_locations(owner.path(), true, &BroadRootLocations::default())?;
    assert!(observation.children_truncated);
    assert_eq!(observation.child_entries_observed, 129);
    assert!(observation.child_directories_inspected <= 128);
    assert!(observation.marker_checks <= 4 * 129);
    assert_eq!(observation.git_marker_bytes_captured, 4097);
    let git = observation
        .repository_markers
        .iter()
        .find(|marker| marker.marker == ".git")
        .unwrap();
    assert_eq!(git.target_state, GitTargetState::Truncated);
    assert!(git.gitdir_target.is_none());
    assert!(encoded_size(&observation, MAX_REPOSITORY_BYTES).is_ok());
    Ok(())
}

#[test]
fn broad_root_hints_are_native_metadata_without_source_or_candidate_scans() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let home = owner.path().join("unopened-home");
    let synced = owner.path().join("unopened-synced");
    let locations = BroadRootLocations {
        home: Some(home.clone()),
        temporary: Some(owner.path().join("unopened-temp")),
        synced: vec![synced.clone()],
    };
    assert_eq!(
        broad_root_reasons(&home, &locations)?,
        vec![BroadRootKind::Home]
    );
    assert_eq!(
        broad_root_reasons(&home.join("Documents"), &locations)?,
        vec![BroadRootKind::Documents]
    );
    assert_eq!(
        broad_root_reasons(&synced, &locations)?,
        vec![BroadRootKind::Synced]
    );
    assert!(broad_root_reasons(&home.join("Documents-other"), &locations)?.is_empty());
    assert!(!home.exists());
    assert!(!synced.exists());
    #[cfg(windows)]
    assert_eq!(
        broad_root_reasons(
            &PathBuf::from(format!(r"\\?\{}", home.display())),
            &locations
        )?,
        vec![BroadRootKind::Home]
    );
    Ok(())
}

#[cfg(unix)]
#[test]
fn marker_and_child_aliases_are_reported_or_skipped_without_following() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let root = owner.path().join("root");
    let outside = owner.path().join("outside");
    fs::create_dir(&root)?;
    fs::create_dir(&outside)?;
    fs::write(outside.join(".git"), "gitdir: secret-target\n")?;
    std::os::unix::fs::symlink(outside.join(".git"), root.join(".git"))?;
    std::os::unix::fs::symlink(&outside, root.join("child"))?;
    let observation =
        observe_repository_with_locations(&root, true, &BroadRootLocations::default())?;
    assert_eq!(observation.child_aliases_skipped, 1);
    assert_eq!(observation.git_marker_reads, 0);
    assert_eq!(
        observation.repository_markers[0].entry_kind,
        MarkerEntryKind::Alias
    );
    assert!(observation.repository_markers[0].gitdir_target.is_none());
    Ok(())
}
