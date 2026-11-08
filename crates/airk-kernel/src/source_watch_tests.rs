// SPDX-License-Identifier: Apache-2.0
use super::*;

fn binding(source: &Path, data: &Path, collection: &str) -> Result<Binding> {
    Ok(Binding {
        source_policy: None,
        registration: ProjectRegistration::new(
            source.to_str().unwrap().into(),
            data.to_str().unwrap().into(),
            collection.into(),
        )?,
        epoch: 1,
        configuration_observed: true,
        additional_roots: vec![],
    })
}

#[test]
fn checkpoint_covers_all_bound_scopes_and_closes_native_notifications_without_debounce()
-> Result<()> {
    let owner = tempfile::tempdir()?;
    let base = owner.path().canonicalize()?;
    let first = base.join("first");
    let second = base.join("second");
    std::fs::create_dir(&first)?;
    std::fs::create_dir(&second)?;
    let mut watcher = SourceWatcher::new();
    let a = binding(&first, &base.join("data"), "a")?;
    let b = binding(&second, &base.join("data"), "b")?;
    let mut expected = vec![a.registration.binding, b.registration.binding];
    expected.sort();
    watcher.install(a)?;
    watcher.install(b)?;
    let notices = watcher.checkpoint_notices()?;
    assert_eq!(
        notices
            .iter()
            .map(|notice| notice.project)
            .collect::<Vec<_>>(),
        expected
    );
    assert!(
        notices
            .iter()
            .all(|notice| notice.epoch == 1 && notice.sequence > 0)
    );
    assert!(watcher.watcher.is_none());
    assert!(watcher.entries.is_empty());
    assert!(watcher.checkpoint_notices()?.is_empty());
    Ok(())
}

fn directory_plan(root: &Path, data: &Path) -> Result<(ProjectBinding, Plan)> {
    directory_plan_with_policy(root, data, None)
}

fn directory_plan_with_policy(
    root: &Path,
    data: &Path,
    policy: Option<airk_core::source_policy::SourcePolicy>,
) -> Result<(ProjectBinding, Plan)> {
    let project = binding(root, data, "ignore-scope")?.registration.binding;
    let mut plan = Plan {
        tasks: VecDeque::from([RootTask {
            policy,
            project,
            index: 0,
            root: root.into(),
            data: data.into(),
        }]),
        walk: None,
        wanted: BTreeSet::new(),
        path_bytes: 0,
        usage: BTreeMap::new(),
        failures: BTreeMap::from([(project, vec![None])]),
        publishing: false,
        published_after: None,
    };
    while !plan.step()? {}
    Ok((project, plan))
}

#[test]
fn unobserved_configuration_never_admits_directory_planning_or_source_notices() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let base = owner.path().canonicalize()?;
    let source = base.join("unavailable-source");
    let data = base.join("data");
    let mut unknown = binding(&source, &data, "unobserved")?;
    let project = unknown.registration.binding;
    unknown.configuration_observed = false;
    unknown
        .additional_roots
        .push(base.join("unavailable-alias").to_str().unwrap().into());
    let mut watcher = SourceWatcher::new();
    assert!(!watcher.install(unknown)?);
    assert!(!watcher.entries.contains_key(&project));
    assert!(watcher.root_index.is_empty());
    assert_eq!(watcher.weight, 0);
    for _ in 0..3 {
        watcher.poll()?;
        assert!(watcher.plan.as_ref().is_none_or(|plan| {
            plan.tasks.is_empty()
                && plan.walk.is_none()
                && plan.wanted.is_empty()
                && plan.usage.is_empty()
        }));
        assert!(watcher.watched.is_empty());
        assert!(watcher.notices(32)?.is_empty());
    }
    let observation = watcher.observations(32).pop().unwrap();
    assert_eq!(observation.project, project);
    assert_eq!(observation.native_roots, 0);
    assert_eq!(observation.failure, Some(WatchFailure::MetadataUnavailable));
    assert!(observation.traversal.is_none());
    assert!(!source.exists() && !data.exists());
    Ok(())
}

#[test]
fn authority_loss_revokes_old_watches_without_stranding_healthy_or_requalified_bindings()
-> Result<()> {
    let owner = tempfile::tempdir()?;
    let base = owner.path().canonicalize()?;
    let source = base.join("source");
    let healthy = base.join("healthy");
    let data = base.join("data");
    fs::create_dir(&source)?;
    fs::create_dir(&healthy)?;
    let marker = source.join(".gitignore");
    fs::write(&marker, b"# unchanged source control\n")?;
    for fenced_first in [false, true] {
        let mut watcher = SourceWatcher::new();
        ensure!(watcher.watcher.is_some(), "native watcher unavailable");
        let original = binding(&source, &data, "lost-authority")?;
        let project = original.registration.binding;
        let peer = binding(&healthy, &data, "healthy")?;
        let peer_id = peer.registration.binding;
        assert!(watcher.install(original)?);
        assert!(watcher.install(peer)?);
        for path in [&source, &healthy] {
            watcher
                .watcher
                .as_mut()
                .unwrap()
                .watch(path, RecursiveMode::NonRecursive)?;
            watcher.watched.insert(path.clone());
        }
        watcher.sender.try_send(vec![marker.clone()])?;
        if fenced_first {
            watcher.fence_policy(project);
            assert!(watcher.watcher.is_none());
        }
        let mut unknown = binding(&source, &data, "lost-authority")?;
        unknown.configuration_observed = false;
        assert!(!watcher.install(unknown)?);
        assert!(watcher.watcher.is_none());
        assert!(watcher.watched.is_empty() && watcher.plan.is_none());
        assert!(watcher.receiver.try_recv().is_err());
        assert!(!watcher.entries.contains_key(&project));
        assert!(watcher.entries.contains_key(&peer_id));
        assert_eq!(
            watcher.observations[&project].failure,
            Some(WatchFailure::MetadataUnavailable)
        );
        assert_eq!(watcher.observations[&peer_id].native_roots, 0);
        // Delayed events from the withdrawn scope cannot admit source work.
        watcher.sender.try_send(vec![marker.clone()])?;
        let until = Instant::now() + Duration::from_secs(5);
        while !watcher.watched.contains(&healthy) && Instant::now() < until {
            watcher.poll()?;
        }
        assert!(watcher.watched.contains(&healthy));
        assert!(!watcher.watched.contains(&source));
        assert!(
            watcher
                .notices(32)?
                .iter()
                .all(|notice| notice.project == peer_id)
        );
        // Repeated failed qualification must not keep dropping the peer backend.
        let mut unknown = binding(&source, &data, "lost-authority")?;
        unknown.configuration_observed = false;
        assert!(!watcher.install(unknown)?);
        watcher.fence_policy(project);
        assert!(watcher.watcher.is_some() && watcher.watched.contains(&healthy));
        // A successful same-epoch legacy empty-policy observation remains valid.
        assert!(watcher.install(binding(&source, &data, "lost-authority")?)?);
        let until = Instant::now() + Duration::from_secs(5);
        while !watcher.watched.contains(&source) && Instant::now() < until {
            watcher.poll()?;
        }
        assert!(watcher.watched.contains(&source) && watcher.watched.contains(&healthy));
        assert_eq!(watcher.entries[&project].binding.epoch, 1);
    }
    assert_eq!(fs::read(&marker)?, b"# unchanged source control\n");
    Ok(())
}

#[cfg(any(windows, target_os = "macos"))]
#[test]
fn permanent_watch_policy_prunes_denies_and_reports_unknown_names_and_ignore_controls() -> Result<()>
{
    use airk_core::{
        identity::{Incarnation, Sha256Digest},
        index_catalog::{CatalogRevision, DenyId, IndexId, PathScope, SourceRootId},
        source_policy::{DenyReason, DenyRule, POLICY_SCHEMA, SourcePolicy, SourceRoot},
    };
    let owned = tempfile::tempdir()?;
    let base = owned.path().canonicalize()?;
    let root = base.join("source");
    let data = base.join("data");
    fs::create_dir_all(root.join("private/nested"))?;
    fs::create_dir_all(root.join("public/nested"))?;
    fs::create_dir_all(&data)?;
    let source = SourceRootId::from_identity(Incarnation::from_bytes([1; 16]));
    let mut policy = SourcePolicy {
        schema: POLICY_SCHEMA.into(),
        index_id: IndexId::from_identity(Incarnation::from_bytes([1; 16])),
        owner_binding: Sha256Digest::from_bytes([1; 32]),
        namespace: "collections-1.4".into(),
        collection: "watch-fixture".into(),
        epoch: CatalogRevision::INITIAL,
        roots: vec![SourceRoot {
            id: source,
            canonical_path: root.to_str().unwrap().into(),
            aliases: vec![],
            owner: true,
        }],
        positive_includes: None,
        denies: vec![DenyRule {
            source_root_id: source,
            kind: PathScope::Subtree,
            path: "private".into(),
            rule_id: DenyId::from_identity(Incarnation::from_bytes([1; 16])),
            reason: DenyReason::Trim,
            created_epoch: CatalogRevision::INITIAL,
            inherited_from: None,
        }],
    };
    let (project, plan) = directory_plan_with_policy(&root, &data, Some(policy.clone()))?;
    assert_eq!(plan.failures[&project], vec![None]);
    assert!(plan.wanted.contains(&root.join("public/nested")));
    assert!(!plan.wanted.contains(&root.join("private")));
    fs::create_dir(root.join("pr\u{ed}vate"))?;
    let (project, plan) = directory_plan_with_policy(&root, &data, Some(policy.clone()))?;
    assert_eq!(
        plan.failures[&project],
        vec![Some(WatchFailure::MetadataUnavailable)]
    );
    policy.denies[0].path = ".airkignore".into();
    policy.denies[0].kind = PathScope::Exact;
    fs::write(root.join(".airkignore"), "# preserved control input\n")?;
    let (project, plan) = directory_plan_with_policy(&root, &data, Some(policy))?;
    assert_eq!(
        plan.failures[&project],
        vec![Some(WatchFailure::MetadataUnavailable)]
    );
    assert_eq!(
        fs::read_to_string(root.join(".airkignore"))?,
        "# preserved control input\n"
    );
    Ok(())
}

#[test]
fn directory_plans_share_indexing_ignores_negations_and_reload_changed_rules() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let base = owner.path().canonicalize()?;
    let root = base.join("source");
    let data = base.join("data");
    fs::create_dir(&root)?;
    fs::create_dir(&data)?;
    let paths = [
        "artifacts/private",
        "nested/drop",
        "nested/keep",
        "ignored",
        "custom",
        "src/cache",
        "src/live",
        "parent-ignored",
        "local-excluded",
    ];
    for path in paths {
        fs::create_dir_all(root.join(path))?;
        fs::write(
            root.join(path).join("marker.txt"),
            b"Original fictional source.\n",
        )?;
    }
    fs::write(
        root.join(".gitignore"),
        "artifacts/\nnested/*\n!nested/keep/\n",
    )?;
    fs::write(root.join(".ignore"), "ignored/\n")?;
    fs::write(root.join(".airkignore"), "custom/\n")?;
    fs::write(root.join("src/.gitignore"), "cache/\n")?;
    // Protected test roots can live beneath this repository's artifacts/ rule.
    // Neutralize only this fixture branch; scenario rules remain authoritative.
    let baseline = "!source/artifacts/\n!source/artifacts/**\n";
    fs::write(
        base.join(".gitignore"),
        format!("{baseline}source/parent-ignored/\n"),
    )?;
    fs::create_dir_all(root.join(".git/info"))?;
    fs::write(root.join(".git/info/exclude"), "local-excluded/\n")?;
    let (project, plan) = directory_plan(&root, &data)?;
    assert_eq!(plan.failures[&project], vec![None]);
    assert!(
        plan.wanted.contains(&root),
        "the parent must observe ignore-file edits"
    );
    for path in paths {
        let indexed =
            airk_ingest::read_selected_source(&root, &data, &format!("{path}/marker.txt")).is_ok();
        assert_eq!(
            plan.wanted.contains(&root.join(path)),
            indexed,
            "watch/index scope differs at {path}"
        );
    }
    assert!(!plan.wanted.contains(&root.join("artifacts")));
    assert!(plan.wanted.contains(&root.join("nested/keep")));
    assert!(!plan.wanted.contains(&root.join("nested/drop")));

    fs::write(root.join(".gitignore"), "")?;
    fs::write(root.join("src/.gitignore"), "")?;
    fs::write(base.join(".gitignore"), baseline)?;
    fs::write(root.join(".git/info/exclude"), "")?;
    let (_, refreshed) = directory_plan(&root, &data)?;
    assert!(refreshed.wanted.contains(&root.join("artifacts/private")));
    assert!(refreshed.wanted.contains(&root.join("nested/drop")));
    assert!(refreshed.wanted.contains(&root.join("src/cache")));
    assert!(refreshed.wanted.contains(&root.join("parent-ignored")));
    assert!(refreshed.wanted.contains(&root.join("local-excluded")));
    assert!(!refreshed.wanted.contains(&root.join("ignored")));
    assert!(!refreshed.wanted.contains(&root.join("custom")));
    assert_eq!(
        fs::read(root.join("src/live/marker.txt"))?,
        b"Original fictional source.\n"
    );
    Ok(())
}

#[test]
fn removing_an_ignored_watch_scope_keeps_its_parent_and_requests_source_reconciliation()
-> Result<()> {
    let owner = tempfile::tempdir()?;
    let base = owner.path().canonicalize()?;
    let root = base.join("source");
    let data = base.join("data");
    fs::create_dir_all(root.join("generated"))?;
    fs::create_dir(&data)?;
    let (project, initial) = directory_plan(&root, &data)?;
    let mut watcher = SourceWatcher::new();
    assert!(watcher.watcher.is_some());
    watcher.install(binding(&root, &data, "ignore-scope")?)?;
    watcher.watched = initial.wanted.clone();
    watcher.force_rebind = false;
    watcher.entries.get_mut(&project).unwrap().dirty = None;
    assert!(watcher.publish_plan(initial)?.is_none());
    assert!(watcher.entries[&project].dirty.is_none());

    fs::write(base.join(".gitignore"), "source/generated/\n")?;
    let (_, changed) = directory_plan(&root, &data)?;
    assert!(watcher.publish_plan(changed)?.is_some());
    assert!(watcher.watched.contains(&root));
    assert!(!watcher.watched.contains(&root.join("generated")));
    assert!(watcher.entries[&project].dirty.is_some());
    Ok(())
}

#[test]
fn ignore_configuration_errors_and_foreign_matcher_paths_remain_explicit() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let base = owner.path().canonicalize()?;
    let root = base.join("source");
    let data = base.join("data");
    fs::create_dir_all(root.join("included"))?;
    fs::create_dir(&data)?;
    fs::write(root.join(".gitignore"), "[z-a]\n")?;
    let (project, plan) = directory_plan(&root, &data)?;
    assert_eq!(
        plan.failures[&project],
        vec![Some(WatchFailure::MetadataUnavailable)]
    );
    let mut ignores = airk_ingest::source_scope::DirectoryIgnore::new(&root);
    assert!(ignores.ignored(&base.join("foreign")).is_err());
    assert!(ignores.ignored(&root.join("../foreign")).is_err());
    Ok(())
}

#[cfg(windows)]
#[test]
fn ignored_unreadable_directory_is_pruned_but_an_included_denial_remains_a_failure() -> Result<()> {
    use std::{
        os::windows::process::CommandExt,
        process::{Command, Stdio},
    };
    struct Restore {
        path: PathBuf,
        tool: PathBuf,
    }
    impl Drop for Restore {
        fn drop(&mut self) {
            let _ = Command::new(&self.tool)
                .arg(&self.path)
                .args(["/remove:d", "*S-1-1-0"])
                .creation_flags(0x08000000)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
    }
    let owner = tempfile::tempdir()?;
    let base = owner.path().canonicalize()?;
    let root = base.join("source");
    let data = base.join("data");
    let locked = root.join("artifacts/locked");
    fs::create_dir_all(&locked)?;
    fs::create_dir(&data)?;
    // Retain a neutral owned ancestor for this branch even when the test root
    // itself is beneath the repository's artifacts/ ignore rule.
    fs::write(
        base.join(".gitignore"),
        "!source/artifacts/\n!source/artifacts/**\n",
    )?;
    let restore = Restore {
        path: locked.clone(),
        tool: PathBuf::from(std::env::var_os("SystemRoot").unwrap()).join("System32/icacls.exe"),
    };
    ensure!(
        Command::new(&restore.tool)
            .arg(&locked)
            .args(["/deny", "*S-1-1-0:(RD)"])
            .creation_flags(0x08000000)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()?
            .success(),
        "fixture could not deny listing its owned directory"
    );
    assert_eq!(
        fs::read_dir(&locked).err().unwrap().kind(),
        std::io::ErrorKind::PermissionDenied
    );
    fs::write(root.join(".gitignore"), "artifacts/\n")?;
    let (project, ignored) = directory_plan(&root, &data)?;
    assert_eq!(ignored.failures[&project], vec![None]);
    assert_eq!(ignored.wanted, BTreeSet::from([root.clone()]));
    assert!(!ignored.usage[&project].traversal.unavailable());

    fs::write(root.join(".gitignore"), "")?;
    let (_, included) = directory_plan(&root, &data)?;
    assert_eq!(
        included.failures[&project],
        vec![Some(WatchFailure::RootUnavailable)]
    );
    let unreadable = &included.usage[&project].traversal.unreadable;
    assert_eq!(unreadable.len(), 1);
    assert_eq!(unreadable[0].path, locked.to_string_lossy());
    assert_eq!(unreadable[0].reason, "permission_denied");
    drop(restore);
    assert!(fs::read_dir(&locked).is_ok());
    Ok(())
}

#[test]
fn traversal_reports_a_disappeared_subdirectory_and_clears_on_the_next_successful_plan()
-> Result<()> {
    let owner = tempfile::tempdir()?;
    let root = owner.path().canonicalize()?;
    let child = root.join("temporarily-missing");
    let data = root.join("derived");
    let project = binding(&root, &data, "diagnostics")?.registration.binding;
    let mut plan = Plan {
        tasks: VecDeque::new(),
        walk: Some(Walk {
            ignores: airk_ingest::source_scope::DirectoryIgnore::new(&root),
            task: RootTask {
                policy: None,
                project,
                index: 0,
                root: root.clone(),
                data: data.clone(),
            },
            pending: vec![(child.clone(), 1)],
            queued_bytes: child.as_os_str().len(),
            cursor: None,
        }),
        wanted: BTreeSet::new(),
        path_bytes: 0,
        usage: BTreeMap::new(),
        failures: BTreeMap::from([(project, vec![None])]),
        publishing: false,
        published_after: None,
    };
    // A directory discovered by the parent disappears before its queued visit.
    while !plan.step()? {}
    assert_eq!(
        plan.failures[&project],
        vec![Some(WatchFailure::RootUnavailable)]
    );
    let report = &plan.usage[&project].traversal;
    report.validate()?;
    assert_eq!(report.unreadable[0].path, child.to_string_lossy());
    assert_eq!(report.unreadable[0].reason, "not_found");
    assert_eq!(
        directory_error(&std::io::Error::from(std::io::ErrorKind::PermissionDenied)),
        "permission_denied"
    );
    fs::create_dir(&child)?;
    plan.usage.clear();
    plan.failures.insert(project, vec![None]);
    plan.tasks.push_back(RootTask {
        policy: None,
        project,
        index: 0,
        root,
        data,
    });
    while !plan.step()? {}
    assert_eq!(plan.failures[&project], vec![None]);
    assert!(!plan.usage[&project].traversal.unavailable());
    assert!(plan.wanted.contains(&child));
    Ok(())
}

#[test]
fn notice_rotation_and_acknowledgements_preserve_later_dirty_work() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let mut watcher = SourceWatcher::new();
    let now = Instant::now();
    let before = now - Duration::from_secs(31);
    for index in 0..40 {
        let binding = binding(owner.path(), owner.path(), &format!("project-{index}"))?;
        watcher.entries.insert(
            binding.registration.binding,
            Entry {
                binding,
                weight: 0,
                dirty: Some(Dirty {
                    first: before,
                    last: before,
                }),
                sequence: 0,
                inflight: None,
                periodic: now,
            },
        );
    }
    let mut seen = BTreeSet::new();
    for _ in 0..3 {
        for notice in watcher.notices(16)? {
            assert!(seen.insert(notice.project));
            // A newer saved event arrives before the old notice is acknowledged.
            SourceWatcher::dirty(watcher.entries.get_mut(&notice.project).unwrap(), now);
            watcher.acknowledge(notice.project, notice.epoch + 1, notice.sequence);
            assert!(watcher.entries[&notice.project].inflight.is_some());
            watcher.acknowledge(notice.project, notice.epoch, notice.sequence);
            assert!(watcher.entries[&notice.project].dirty.is_some());
        }
    }
    assert_eq!(
        seen.len(),
        40,
        "an early project page starved later changes"
    );
    assert_eq!(watcher.pending_count(), 40);
    assert!(
        watcher.notices(16)?.is_empty(),
        "coalescing quiet time was bypassed"
    );
    Ok(())
}

#[test]
fn directory_budget_degrades_one_scope_without_adding_unbounded_native_registrations() -> Result<()>
{
    let owner = tempfile::tempdir()?;
    let source = owner.path().join("source");
    fs::create_dir(&source)?;
    let data = owner.path().join("data");
    fs::create_dir(&data)?;
    let registration = binding(&source, &data, "fixture")?.registration;
    let project = registration.binding;
    let mut plan = Plan {
        tasks: VecDeque::from([RootTask {
            policy: None,
            project,
            index: 0,
            root: source,
            data,
        }]),
        walk: None,
        wanted: (0..MAX_DIRECTORIES)
            .map(|index| PathBuf::from(format!("admitted-{index}")))
            .collect(),
        path_bytes: MAX_PATH_BYTES,
        usage: BTreeMap::new(),
        failures: BTreeMap::from([(project, vec![None])]),
        publishing: false,
        published_after: None,
    };
    while !plan.step()? {}
    assert_eq!(plan.wanted.len(), MAX_DIRECTORIES);
    assert_eq!(
        plan.failures[&project],
        vec![Some(WatchFailure::GlobalDirectoryLimit)]
    );
    Ok(())
}

#[test]
fn four_project_plans_exceed_old_directory_limit_and_keep_scope_budgets_independent() -> Result<()>
{
    let owner = tempfile::tempdir()?;
    let base = owner.path().canonicalize()?;
    let data = base.join("data");
    fs::create_dir(&data)?;
    let mut plan = Plan {
        tasks: VecDeque::new(),
        walk: None,
        wanted: BTreeSet::new(),
        path_bytes: 0,
        usage: BTreeMap::new(),
        failures: BTreeMap::new(),
        publishing: false,
        published_after: None,
    };
    for n in 0..4 {
        let root = base.join(format!("project-{n}"));
        fs::create_dir(&root)?;
        for directory in 0..600 {
            fs::create_dir(root.join(format!("dir-{directory}")))?;
        }
        // Existing policy exclusions remain pruned before descending.
        fs::create_dir_all(root.join("node_modules/ignored/nested"))?;
        let project = binding(&root, &data, &format!("project-{n}"))?
            .registration
            .binding;
        plan.tasks.push_back(RootTask {
            policy: None,
            project,
            index: 0,
            root,
            data: data.clone(),
        });
        plan.failures.insert(project, vec![None]);
    }
    let mut turns = 0;
    while !plan.step()? {
        turns += 1;
    }
    assert!(
        turns > 1,
        "large traversal must yield between bounded turns"
    );
    assert_eq!(plan.wanted.len(), 4 * 601);
    assert!(plan.failures.values().all(|failures| failures == &[None]));
    let project = *plan.usage.keys().next().unwrap();
    let healthy = *plan.usage.keys().last().unwrap();
    // Exhaust one project's traversal budget; the next project still proceeds.
    plan.usage.get_mut(&project).unwrap().entries = MAX_SCAN_ENTRIES;
    for id in [project, healthy] {
        let root = base.join(format!("extra-{id}"));
        fs::create_dir(&root)?;
        plan.tasks.push_back(RootTask {
            policy: None,
            project: id,
            index: 0,
            root,
            data: data.clone(),
        });
    }
    while !plan.step()? {}
    assert_eq!(
        plan.failures[&project],
        vec![Some(WatchFailure::ScanEntryLimit)]
    );
    assert_eq!(plan.failures[&healthy], vec![None]);
    Ok(())
}

#[test]
fn native_nested_events_are_acknowledged_without_changing_sources_and_missing_roots_are_independent()
-> Result<()> {
    let owner = tempfile::tempdir()?;
    let base = owner.path().canonicalize()?;
    let source = base.join("source");
    fs::create_dir_all(source.join("nested"))?;
    let data = base.join("data");
    fs::create_dir(&data)?;
    let file = source.join("nested/original.txt");
    fs::write(&file, b"Original source fixture.\n")?;
    let healthy = binding(&source, &data, "healthy")?;
    let project = healthy.registration.binding;
    let missing = binding(&base.join("missing"), &data, "missing")?;
    let missing_project = missing.registration.binding;
    let mut watcher = SourceWatcher::new();
    watcher.install(healthy)?;
    watcher.install(missing)?;
    let until = Instant::now() + Duration::from_secs(15);
    let mut ready = false;
    let mut missing_reported = false;
    let mut baseline = None;
    while Instant::now() < until {
        watcher.poll()?;
        for observation in watcher.observations(32) {
            if observation.project == project {
                ready =
                    observation.backend == WatchBackend::Native && observation.native_roots == 1;
            }
            if observation.project == missing_project {
                missing_reported = observation.failure == Some(WatchFailure::RootUnavailable);
            }
        }
        for notice in watcher.notices(32)? {
            watcher.acknowledge(notice.project, notice.epoch, notice.sequence);
            if notice.project == project {
                baseline = Some(notice.sequence);
            }
        }
        if ready && missing_reported && baseline.is_some() && watcher.pending_count() == 0 {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    ensure!(
        ready && missing_reported && baseline.is_some(),
        "native watcher did not establish independent healthy/missing scopes"
    );
    let changed = b"Changed by the fixture, not the watcher.\n";
    fs::write(&file, changed)?;
    let until = Instant::now() + Duration::from_secs(10);
    let mut observed = false;
    while Instant::now() < until {
        watcher.poll()?;
        for notice in watcher.notices(32)? {
            if notice.project == project && notice.sequence > baseline.unwrap() {
                observed = true;
            }
            watcher.acknowledge(notice.project, notice.epoch, notice.sequence);
        }
        if observed {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    ensure!(
        observed,
        "nested saved-file change did not produce a new source notice"
    );
    assert_eq!(fs::read(&file)?, changed);
    Ok(())
}
