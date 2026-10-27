// SPDX-License-Identifier: Apache-2.0
use super::*;

fn service_fixture() -> Result<Fixture> {
    let fixture = Fixture::empty_namespace("collections-1.4")?;
    let writer = fixture.store.lock()?;
    fixture
        .store
        .current_write(&writer, &fixture.quota, &fixture.scope, CAPACITY)?
        .initialize(fixture.head.clone())?;
    drop(writer);
    Ok(fixture)
}

#[test]
fn service_build_snapshot_outlives_query_deadline_only_under_its_writer() -> Result<()> {
    let fixture = service_fixture()?;
    let (_reservation, sealed) = fixture.sealed()?;
    let writer = fixture.store.lock()?;
    let mut current =
        fixture
            .store
            .current_write(&writer, &fixture.quota, &fixture.scope, CAPACITY)?;
    let pending = current.prepare(
        &fixture.head.token()?,
        fixture.target(&sealed),
        &sealed,
        &[],
        None,
    )?;
    current.mark_applying(&pending)?;
    current.commit(&pending, pending.target.lexical_commit)?;
    drop(current);

    let mut query = fixture.store.current_read(&fixture.scope)?;
    let mut build = fixture.store.current_build_read(&fixture.scope, &writer)?;
    // Simulate a long semantic build without a two-minute test sleep.
    for read in [&mut query, &mut build] {
        std::sync::Arc::get_mut(read)
            .unwrap()
            .elapse_for_test(Duration::from_secs(121));
    }
    assert!(format!("{:#}", query.read(1).unwrap_err()).contains("snapshot expired"));
    assert_eq!(build.read(1)?.bytes, vec![7; 16]);
    assert_eq!(build.token()?, pending.target.token()?);

    let reservation = fixture.quota.reserve(&fixture.scope.collection, 4 << 20)?;
    let overlay = graph::GraphOverlay::new(
        &reservation,
        build.clone(),
        graph::GraphOverlayLimits {
            maximum_bytes: 1 << 20,
            ..Default::default()
        },
    )?;
    let mut record = overlay.read(1)?;
    record.bytes = vec![9; 16];
    overlay.put(1, record)?;
    let delta = overlay.seal()?;
    drop(query);
    let mut target = pending.target.clone();
    target.sequence += 1;
    target.graph_head = delta.descriptor().target.clone();
    let mut current =
        fixture
            .store
            .current_write(&writer, &fixture.quota, &fixture.scope, CAPACITY)?;
    let next = current.prepare(&build.token()?, target, &delta, &[], None)?;
    current.mark_applying(&next)?;
    current.commit(&next, next.target.lexical_commit)?;
    assert_eq!(
        build.read(1)?.bytes,
        vec![7; 16],
        "build keeps the captured revision"
    );
    assert_eq!(
        fixture.store.current_read(&fixture.scope)?.read(1)?.bytes,
        vec![9; 16]
    );
    drop(current);
    drop(writer);
    assert!(format!("{:#}", build.read(1).unwrap_err()).contains("writer has been released"));
    drop(build);

    let foreign = service_fixture()?;
    let foreign_writer = foreign.store.lock()?;
    assert!(
        fixture
            .store
            .current_build_read(&fixture.scope, &foreign_writer)
            .is_err()
    );
    Ok(())
}

#[test]
fn service_large_publication_recovers_above_old_wal_limit_with_pinned_snapshot() -> Result<()> {
    let fixture = service_fixture()?;
    let old = fixture.store.current_read(&fixture.scope)?;
    let (_reservation, sealed) = fixture.sealed()?;
    let mut target = fixture.target(&sealed);
    // More than the former WAL budget in captured values alone. Distinct
    // buckets keep this a publication test rather than a bucket-limit test.
    let mut prefixes = std::collections::BTreeSet::new();
    let mut changes = Vec::new();
    for number in 0u64.. {
        let change = map::MapChange::new(
            &fixture.scope,
            map::MapDomain::Metadata,
            &number.to_be_bytes(),
            Some(vec![23; 96 << 10]),
        )?;
        let bytes = change.key();
        if prefixes.insert([bytes.as_bytes()[0], bytes.as_bytes()[1]]) {
            changes.push(change);
        }
        if changes.len() == 768 {
            break;
        }
    }
    changes.sort_by_key(map::MapChange::key);
    let reader = database::open_reader(&fixture.store.directory().join("current.sqlite3"))?;
    let mut plan = map::MapPlan::new(&reader, &fixture.scope, fixture.head.artifact_root)?;
    for change in &changes {
        plan.put(change.key(), change.value())?;
    }
    target.artifact_root = plan.root()?;
    drop(plan);
    drop(reader);
    let writer = fixture.store.lock()?;
    let mut current =
        fixture
            .store
            .current_write(&writer, &fixture.quota, &fixture.scope, CAPACITY)?;
    current.grow_capacity(512 << 20)?;
    let pending =
        current.prepare_mapped(&fixture.head.token()?, target, &sealed, &[], None, &changes)?;
    current.mark_applying(&pending)?;
    drop(current);
    drop(writer);
    // Reopen an interrupted Applying publication while an old request remains
    // pinned. Recovery must use the enlarged, durable sidecar allowance.
    let writer = fixture.store.lock()?;
    let mut current =
        fixture
            .store
            .current_write(&writer, &fixture.quota, &fixture.scope, 512 << 20)?;
    current.commit(&pending, pending.target.lexical_commit)?;
    let path = fixture.store.directory().join("current.sqlite3");
    assert!(fs::metadata(path.with_file_name("current.sqlite3-wal"))?.len() > 64 << 20);
    assert_eq!(old.token()?, fixture.head.token()?);
    let latest = fixture.store.current_read(&fixture.scope)?;
    assert_eq!(latest.token()?, pending.target.token()?);
    let reader = database::open_reader(&path)?;
    for change in &changes {
        assert_eq!(
            map::get(
                &reader,
                &fixture.scope,
                pending.target.artifact_root,
                change.key()
            )?,
            change.value().map(Vec::from)
        );
    }
    assert!(database::logical_bytes(&database::open_reader(&path)?)? > fs::metadata(&path)?.len());
    wal::validate_files(fixture.store.directory(), 512 << 20)?;
    Ok(())
}

#[test]
fn service_legacy_allowance_remains_readable_then_upgrades_before_recovery() -> Result<()> {
    let fixture = service_fixture()?;
    let capacity = 128 << 20;
    let writer = fixture.store.lock()?;
    fixture
        .store
        .current_write(&writer, &fixture.quota, &fixture.scope, CAPACITY)?
        .grow_capacity(capacity)?;
    drop(writer);
    let (_reservation, sealed) = fixture.sealed()?;
    let writer = fixture.store.lock()?;
    let mut current =
        fixture
            .store
            .current_write(&writer, &fixture.quota, &fixture.scope, capacity)?;
    let pending = current.prepare(
        &fixture.head.token()?,
        fixture.target(&sealed),
        &sealed,
        &[],
        None,
    )?;
    current.mark_applying(&pending)?;
    drop(current);
    let binding = crate::quota::CurrentGrantScope {
        collection: fixture.scope.collection.clone(),
        incarnation: fixture.scope.incarnation,
        transaction: wal::id(&fixture.scope),
    };
    // Simulate the durable grant left by the installed fixed-budget runtime.
    let mut grant = fixture.quota.current_grant(&writer, &binding)?.unwrap();
    grant.resize(&writer, wal::legacy_total(capacity)?)?;
    drop(writer);
    assert_eq!(
        fixture.store.current_read(&fixture.scope)?.token()?,
        fixture.head.token()?
    );
    let writer = fixture.store.lock()?;
    let mut current =
        fixture
            .store
            .current_write(&writer, &fixture.quota, &fixture.scope, capacity)?;
    assert!(
        fixture
            .quota
            .current_grant(&writer, &binding)?
            .unwrap()
            .capacity()
            >= wal::total(capacity)?
    );
    current.commit(&pending, pending.target.lexical_commit)?;
    assert_eq!(
        fixture.store.current_read(&fixture.scope)?.token()?,
        pending.target.token()?
    );
    Ok(())
}

#[test]
#[cfg(any(unix, windows))]
fn service_publication_retains_admitted_sidecar_identities_across_last_close_and_reopen()
-> Result<()> {
    fn identity(path: &std::path::Path) -> Result<(u64, u64)> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let metadata = fs::metadata(path)?;
            assert_eq!(metadata.nlink(), 1);
            Ok((metadata.dev(), metadata.ino()))
        }
        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawHandle;
            use windows_sys::Win32::Storage::FileSystem::{
                BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle,
            };
            let file = fs::File::open(path)?;
            let mut information = std::mem::MaybeUninit::<BY_HANDLE_FILE_INFORMATION>::uninit();
            // SAFETY: exact live read handle and correctly sized writable output.
            ensure!(
                unsafe {
                    GetFileInformationByHandle(file.as_raw_handle(), information.as_mut_ptr())
                } != 0,
                "cannot inspect sidecar identity"
            );
            let info = unsafe { information.assume_init() };
            assert_eq!(info.nNumberOfLinks, 1);
            Ok((
                u64::from(info.dwVolumeSerialNumber),
                (u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow),
            ))
        }
    }
    let fixture = service_fixture()?;
    let path = fixture.store.directory().join("current.sqlite3");
    let wal = fixture.store.directory().join("current.sqlite3-wal");
    let shm = fixture.store.directory().join("current.sqlite3-shm");
    let mut identities = None;
    let mut handles = Vec::new();
    for _ in 0..8 {
        let connection = database::open_writer(&path, CAPACITY, false)?;
        connection.pragma_update(None, "user_version", wal::VERSION)?;
        let current = (identity(&wal)?, identity(&shm)?);
        if let Some(expected) = identities {
            assert_eq!(current, expected);
        } else {
            identities = Some(current);
            handles.push(fs::File::open(&wal)?);
            handles.push(fs::File::open(&shm)?);
        }
        drop(connection);
        assert_eq!((identity(&wal)?, identity(&shm)?), current);
        let reader = database::open_reader(&path)?;
        assert!(
            reader.query_row("SELECT count(*) FROM sqlite_schema", [], |row| row
                .get::<_, i64>(0))?
                > 0
        );
        drop(reader);
        assert_eq!((identity(&wal)?, identity(&shm)?), current);
        wal::validate_files(fixture.store.directory(), CAPACITY)?;
        assert_eq!(
            fixture.store.current_read(&fixture.scope)?.token()?,
            fixture.head.token()?
        );
    }
    drop(handles);
    let alias = fixture.directory.path().join("unapproved-wal-alias");
    fs::hard_link(&wal, &alias)?;
    assert!(
        fixture.store.current_read(&fixture.scope).is_err(),
        "actual hard link was admitted"
    );
    fs::remove_file(alias)?;
    assert_eq!(
        fixture.store.current_read(&fixture.scope)?.token()?,
        fixture.head.token()?
    );
    let writer = fixture.store.lock()?;
    let grants = fixture.quota.current_grants(&writer)?;
    assert_eq!(grants.len(), 1);
    assert_eq!(grants[0].scope().transaction, wal::id(&fixture.scope));
    Ok(())
}

#[test]
fn service_publication_keeps_old_snapshot_and_graph_coherent_through_replay() -> Result<()> {
    let fixture = service_fixture()?;
    let old = fixture.store.current_read(&fixture.scope)?;
    let (_reservation, sealed) = fixture.sealed()?;
    let writer = fixture.store.lock()?;
    let mut current =
        fixture
            .store
            .current_write(&writer, &fixture.quota, &fixture.scope, CAPACITY)?;
    let pending = current.prepare(
        &fixture.head.token()?,
        fixture.target(&sealed),
        &sealed,
        &[],
        None,
    )?;
    assert_eq!(old.token()?, fixture.head.token()?);
    current.mark_applying(&pending)?;
    // Applying is an intent; the SQL head and all evidence still name the base.
    assert_eq!(
        fixture.store.current_read(&fixture.scope)?.token()?,
        old.token()?
    );
    current.commit(&pending, pending.target.lexical_commit)?;
    let latest = fixture.store.current_read(&fixture.scope)?;
    assert_eq!(latest.token()?, pending.target.token()?);
    assert_eq!(latest.read(1)?.bytes, vec![7; 16]);
    assert_eq!(old.counts()?.allocated_count, 0);
    assert_eq!(old.token()?, fixture.head.token()?);
    assert!(latest.require_token(&old.token()?).is_err());
    // A live reader, including an older snapshot, protects external objects.
    assert!(!current.reclaim_exclusive()?);
    drop(old);
    drop(latest);
    assert!(current.reclaim_exclusive()?);
    Ok(())
}

#[test]
fn service_publication_retains_recovery_capacity_and_rejects_missing_or_oversized_sidecars()
-> Result<()> {
    let fixture = service_fixture()?;
    let pending = fixture.prepare(&[])?;
    let writer = fixture.store.lock()?;
    let mut current =
        fixture
            .store
            .current_write(&writer, &fixture.quota, &fixture.scope, CAPACITY)?;
    current.mark_applying(&pending)?;
    drop(current);
    drop(writer);
    // Reopen/replay after an interrupted Applying intent without live source.
    let writer = fixture.store.lock()?;
    let mut current =
        fixture
            .store
            .current_write(&writer, &fixture.quota, &fixture.scope, CAPACITY)?;
    current.mark_applying(&pending)?;
    current.commit(&pending, pending.target.lexical_commit)?;
    current.release_orphan_grants()?;
    let grants = fixture.quota.current_grants(&writer)?;
    assert_eq!(grants.len(), 1);
    assert_eq!(grants[0].scope().transaction, wal::id(&fixture.scope));
    drop(current);
    drop(writer);
    let sidecar = fixture.store.directory().join("current.sqlite3-wal");
    let file = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&sidecar)?;
    file.set_len(wal::limits(CAPACITY)?.0 + 1)?;
    drop(file);
    assert!(fixture.store.current_read(&fixture.scope).is_err());
    let writer = fixture.store.lock()?;
    assert!(
        fixture
            .store
            .current_write(&writer, &fixture.quota, &fixture.scope, CAPACITY)
            .is_err()
    );
    fs::remove_file(sidecar)?;
    // Simulate an invalid missing lifetime allowance. Neither path may recreate
    // it over an existing database and silently authorize an unowned overlay.
    for grant in fixture.quota.current_grants(&writer)? {
        grant.release(&writer)?;
    }
    assert!(fixture.store.current_read(&fixture.scope).is_err());
    assert!(
        fixture
            .store
            .current_write(&writer, &fixture.quota, &fixture.scope, CAPACITY)
            .is_err()
    );
    Ok(())
}

#[test]
fn service_publication_authenticates_wal_shadow_rows_without_main_file_cache() -> Result<()> {
    let mut fixture = Fixture::empty_namespace("collections-1.4")?;
    let manifest = lexical::LexicalManifest::empty()?;
    fixture.head.lexical_commit = manifest.digest()?;
    let writer = fixture.store.lock()?;
    let mut current =
        fixture
            .store
            .current_write(&writer, &fixture.quota, &fixture.scope, CAPACITY)?;
    current.initialize(fixture.head.clone())?;
    current
        .install_initial_lexical_manifest(&fixture.head.token()?, &manifest.canonical_bytes()?)?;
    let old = fixture.store.current_read(&fixture.scope)?;
    old.verify_lexical()?;
    // Keep the initial snapshot pinned so the forged commit remains in WAL.
    let connection = database::open_writer(
        &fixture.store.directory().join("current.sqlite3"),
        CAPACITY,
        false,
    )?;
    connection.execute("UPDATE current_fts_config SET v=999 WHERE k='version'", [])?;
    assert!(
        fixture
            .store
            .current_read(&fixture.scope)?
            .verify_lexical()
            .is_err()
    );
    old.verify_lexical()?;
    Ok(())
}

#[test]
fn service_publication_process_death_preserves_head_and_releases_only_actual_reader_leases()
-> Result<()> {
    for mode in ["applying", "hot-apply"] {
        let fixture = service_fixture()?;
        let pending = fixture.prepare(&[])?;
        let interrupted = child(&fixture, mode)?;
        let read = fixture.store.current_read(&fixture.scope)?;
        assert_eq!(read.token()?, fixture.head.token()?);
        assert_eq!(read.counts()?.allocated_count, 0);
        assert!(read.has_prepared_update()?);
        drop(read);
        drop(interrupted);
        let reader = child(&fixture, "read")?;
        let writer = fixture.store.lock()?;
        let mut current =
            fixture
                .store
                .current_write(&writer, &fixture.quota, &fixture.scope, CAPACITY)?;
        current.mark_applying(&pending)?;
        current.commit(&pending, pending.target.lexical_commit)?;
        assert!(!current.reclaim_exclusive()?);
        drop(reader);
        assert!(current.reclaim_exclusive()?);
        drop(current);
        drop(writer);
        assert_eq!(
            fixture.store.current_read(&fixture.scope)?.token()?,
            pending.target.token()?
        );
    }
    Ok(())
}

#[test]
fn service_publication_bootstrap_process_death_recovers_each_durable_boundary() -> Result<()> {
    for mode in [
        "bootstrap-before-open",
        "bootstrap-before-commit",
        "bootstrap-after-commit",
    ] {
        let fixture = Fixture::empty_namespace("collections-1.4")?;
        drop(child(&fixture, mode)?);
        let writer = fixture.store.lock()?;
        let mut current =
            fixture
                .store
                .current_write(&writer, &fixture.quota, &fixture.scope, CAPACITY)?;
        assert_eq!(current.initialize(fixture.head.clone())?, fixture.head);
        current.release_orphan_grants()?;
        assert_eq!(fixture.quota.current_grants(&writer)?.len(), 1);
        assert_eq!(
            fixture.store.current_read(&fixture.scope)?.token()?,
            fixture.head.token()?
        );
    }
    Ok(())
}
