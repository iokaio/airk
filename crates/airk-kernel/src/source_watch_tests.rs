// SPDX-License-Identifier: Apache-2.0
use super::*;

fn binding(source: &Path, data: &Path, collection: &str) -> Result<Binding> {
    Ok(Binding {
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

fn directory_plan(root: &Path, data: &Path) -> Result<(ProjectBinding, Plan)> {
    let project = binding(root, data, "ignore-scope")?.registration.binding;
    let mut plan = Plan {
        tasks: VecDeque::from([RootTask {
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
    fs::write(base.join(".gitignore"), "source/parent-ignored/\n")?;
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
    fs::write(base.join(".gitignore"), "")?;
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
