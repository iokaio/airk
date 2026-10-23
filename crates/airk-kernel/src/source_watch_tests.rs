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
        entries: 0,
        failures: BTreeMap::from([(project, vec![None])]),
    };
    while !plan.step()? {}
    assert_eq!(plan.wanted.len(), MAX_DIRECTORIES);
    assert_eq!(plan.failures[&project], vec![Some(WatchFailure::RootLimit)]);
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
