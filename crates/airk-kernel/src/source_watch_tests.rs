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
