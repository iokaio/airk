// SPDX-License-Identifier: Apache-2.0
//! Actual filesystem/Store adapter regressions. The shared fixture enables an
//! isolated authority-zero metadata catalog; it is not production activation.
use super::*;
use crate::service::collection::Collection;
use std::os::unix::fs::FileTypeExt;

fn pins(request: &airk_core::catalog_setup::IndexSetupRequest) -> Result<NativeSetupPins> {
    let data = Path::new(&request.registration.data_root);
    Ok(NativeSetupPins {
        parent: SetupDirectory::open_canonical(data.parent().unwrap())?.file,
        data: if data.exists() {
            Some(SetupDirectory::open_canonical(data)?.file)
        } else {
            None
        },
        sources: request
            .sources
            .iter()
            .map(|source| {
                Ok(SetupDirectory::open_canonical(Path::new(&source.canonical_path))?.file)
            })
            .collect::<Result<Vec<_>>>()?,
    })
}
fn no_jobs(registry: &ServiceRegistry) -> Result<()> {
    let jobs: i64 =
        registry
            .connection
            .query_row("SELECT count(*) FROM service_jobs", [], |r| r.get(0))?;
    assert_eq!(jobs, 0);
    Ok(())
}

#[test]
fn owner_publish_crash_child() -> Result<()> {
    let Some(control) = std::env::var_os("AIRK_TEST_SETUP_OWNER_CRASH_CONTROL") else {
        return Ok(());
    };
    let key: airk_core::service::RequestKey =
        serde_json::from_str(&std::env::var("AIRK_TEST_SETUP_OWNER_CRASH_KEY")?)?;
    let caller: Incarnation = std::env::var("AIRK_TEST_SETUP_OWNER_CRASH_CALLER")?.parse()?;
    let registry = ServiceRegistry::open(Path::new(&control), key.namespace)?;
    let receipt = registry
        .index_setup_receipt(caller, &key)?
        .context("child reservation missing")?;
    let _prepared = prepare_index_storage(
        &registry,
        Path::new(&control),
        &receipt,
        pins(&receipt.request)?,
    )?;
    anyhow::bail!("native owner publication fault did not execute")
}

#[test]
fn existing_empty_leaf_owner_publish_crash_resumes_the_exact_saved_reservation() -> Result<()> {
    let (_temporary, mut registry, request) = super::super::tests::fixture()?;
    let control = registry.root.clone();
    let data = Path::new(&request.registration.data_root);
    std::fs::create_dir(data)?;
    let source = Path::new(&request.registration.root).join("original.txt");
    std::fs::write(&source, b"original source")?;
    let reserved = registry.reserve_index_setup(&request)?;
    drop(registry);
    let child = std::process::Command::new(std::env::current_exe()?)
        .args([
            "--exact",
            "service::registry::catalog::setup::storage::tests::owner_publish_crash_child",
            "--nocapture",
        ])
        .env("AIRK_TEST_SETUP_OWNER_CRASH_CONTROL", &control)
        .env(
            "AIRK_TEST_SETUP_OWNER_CRASH_KEY",
            serde_json::to_string(&request.key)?,
        )
        .env(
            "AIRK_TEST_SETUP_OWNER_CRASH_CALLER",
            request.caller.to_string(),
        )
        .env("AIRK_TEST_SETUP_OWNER_CRASH", "1")
        .output()?;
    assert_eq!(
        child.status.code(),
        Some(76),
        "{}",
        String::from_utf8_lossy(&child.stderr)
    );
    assert!(!data.join(data_scope::OWNER).exists());
    let expected = data_scope::initial_owner_bytes(data, &control, request.key.namespace)?;
    let scratch = super::super::native_setup_scratch_name(data_scope::OWNER, &expected);
    assert_eq!(std::fs::read(data.join(&scratch))?, expected);
    assert!(data_scope::recoverable_setup_binding(
        data,
        &control,
        request.key.namespace
    )?);
    assert!(!data_scope::has_saved_binding(
        data,
        &control,
        request.key.namespace
    )?);
    let mut registry = ServiceRegistry::open(&control, request.key.namespace)?;
    assert_eq!(
        registry
            .index_setup_receipt(request.caller, &request.key)?
            .unwrap(),
        reserved
    );
    let prepared = prepare_index_storage(&registry, &control, &reserved, pins(&request)?)?
        .record(&mut registry)?;
    assert_eq!(prepared.stage, SetupStage::Prepared);
    assert!(!data.join(scratch).exists());
    assert_eq!(std::fs::read(data.join(data_scope::OWNER))?, expected);
    assert_eq!(std::fs::read(&source)?, b"original source");
    no_jobs(&registry)?;
    Ok(())
}

#[test]
fn actual_anchored_setup_records_native_proof_without_indexing_and_replays_exactly() -> Result<()> {
    let (_temporary, mut registry, request) = super::super::tests::fixture()?;
    let control = registry.root.clone();
    let source = Path::new(&request.registration.root).join("original.txt");
    std::fs::write(&source, b"original saved source")?;
    let reserved = registry.reserve_index_setup(&request)?;
    let native = prepare_index_storage(&registry, &control, &reserved, pins(&request)?)?;
    let existing = Collection::existing(
        Path::new(&request.registration.data_root),
        &request.registration.root,
        &request.registration.collection,
    )?;
    let conflict = existing.selected_store()?.lock();
    let gate = Path::new(&request.registration.data_root)
        .join("collections-1.4")
        .join(&request.registration.collection)
        .join(".current-gates")
        .join(reserved.operation_id.to_string());
    assert!(
        gate.is_file(),
        "actual native selection did not create its gate"
    );
    assert_eq!(std::fs::metadata(&gate)?.len(), 0);
    assert!(
        matches!(
            conflict
                .as_ref()
                .err()
                .and_then(|error| error.downcast_ref::<std::fs::TryLockError>()),
            Some(std::fs::TryLockError::WouldBlock)
        ),
        "setup did not retain the canonical native writer key"
    );
    let prepared = native.record(&mut registry)?;
    assert_eq!(prepared.stage, SetupStage::Prepared);
    assert!(prepared.preparation.is_some());
    no_jobs(&registry)?;
    assert_eq!(std::fs::read(&source)?, b"original saved source");
    let native = prepare_index_storage(&registry, &control, &prepared, pins(&request)?)?;
    assert_eq!(native.record(&mut registry)?, prepared);
    no_jobs(&registry)?;
    let existing = Collection::existing(
        Path::new(&request.registration.data_root),
        &request.registration.root,
        &request.registration.collection,
    )?;
    assert!(existing.selected_store()?.persistent_revision()?.is_none());
    Ok(())
}

#[test]
fn existing_owned_shared_root_keeps_unrelated_collection_and_quota_policy() -> Result<()> {
    let (_temporary, mut registry, request) = super::super::tests::fixture()?;
    let control = registry.root.clone();
    let data = Path::new(&request.registration.data_root);
    std::fs::create_dir(data)?;
    data_scope::bind(data, &control, request.key.namespace)?;
    let unrelated = Collection::initialize(
        data,
        &request.registration.root,
        "unrelated",
        Incarnation::from_bytes([81; 16]),
    )?;
    let selected = unrelated.selected_store()?;
    std::fs::write(
        selected.directory().join("retained.bin"),
        b"unrelated retained bytes",
    )?;
    let quota = crate::quota::Quota::open_service(data)?;
    let policy = quota.policy()?;
    let reserved = registry.reserve_index_setup(&request)?;
    prepare_index_storage(&registry, &control, &reserved, pins(&request)?)?
        .record(&mut registry)?;
    assert_eq!(
        std::fs::read(selected.directory().join("retained.bin"))?,
        b"unrelated retained bytes"
    );
    assert_eq!(quota.policy()?, policy);
    no_jobs(&registry)?;
    Ok(())
}

#[test]
fn data_replacement_after_pin_never_receives_native_metadata_writes() -> Result<()> {
    let (_temporary, mut registry, request) = super::super::tests::fixture()?;
    let control = registry.root.clone();
    let data = PathBuf::from(&request.registration.data_root);
    std::fs::create_dir(&data)?;
    let reserved = registry.reserve_index_setup(&request)?;
    let captured = pins(&request)?;
    let original = data.with_file_name("approved-original-data");
    let result = prepare_with_hook(&registry, &control, &reserved, captured, || {
        std::fs::rename(&data, &original).unwrap();
        std::fs::create_dir(&data).unwrap();
        std::fs::write(
            data.join("original-source.txt"),
            b"unreviewed replacement source",
        )
        .unwrap();
    });
    assert!(result.is_err());
    assert_eq!(
        std::fs::read_dir(&data)?.count(),
        1,
        "native writes escaped the pinned data directory"
    );
    assert_eq!(
        std::fs::read(data.join("original-source.txt"))?,
        b"unreviewed replacement source"
    );
    assert!(
        original.join(data_scope::OWNER).exists(),
        "actual anchored path was not exercised"
    );
    assert_eq!(
        registry
            .index_setup_receipt(request.caller, &request.key)?
            .unwrap()
            .stage,
        SetupStage::Reserved
    );
    no_jobs(&registry)?;
    Ok(())
}

#[test]
fn parent_replacement_after_pin_never_creates_a_leaf_in_the_replacement() -> Result<()> {
    let (_temporary, mut registry, mut request) = super::super::tests::fixture()?;
    let control = registry.root.clone();
    let parent = control.parent().unwrap().join("selected-parent");
    std::fs::create_dir(&parent)?;
    request.registration = airk_core::service::ProjectRegistration::new(
        request.registration.root.clone(),
        parent.join("data").to_str().unwrap().into(),
        request.registration.collection.clone(),
    )?;
    let reserved = registry.reserve_index_setup(&request)?;
    let captured = pins(&request)?;
    let original = parent.with_file_name("approved-original-parent");
    let result = prepare_with_hook(&registry, &control, &reserved, captured, || {
        std::fs::rename(&parent, &original).unwrap();
        std::fs::create_dir(&parent).unwrap();
        std::fs::write(parent.join("source.txt"), b"replacement source").unwrap();
    });
    assert!(result.is_err());
    assert_eq!(std::fs::read_dir(&parent)?.count(), 1);
    assert!(!parent.join("data").exists());
    assert_eq!(
        std::fs::read(parent.join("source.txt"))?,
        b"replacement source"
    );
    assert!(original.join("data").join(data_scope::OWNER).exists());
    no_jobs(&registry)?;
    Ok(())
}

#[test]
fn no_replace_publication_preserves_an_occupied_unknown_target() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    let root = SetupDirectory::open_canonical(&temporary.path().canonicalize()?)?;
    let source = root.create_dir(OsStr::new("prepared"))?;
    source.create_file(OsStr::new("native.json"), b"native")?;
    let target = root.create_dir(OsStr::new("target"))?;
    target.create_file(OsStr::new("user.txt"), b"original")?;
    assert!(
        root.publish_no_replace(
            OsStr::new("prepared"),
            &source.file,
            &root,
            OsStr::new("target")
        )
        .is_err()
    );
    assert_eq!(target.read_file(OsStr::new("user.txt"), 128)?, b"original");
    assert_eq!(source.read_file(OsStr::new("native.json"), 128)?, b"native");
    Ok(())
}

#[test]
fn unknown_fifo_metadata_is_refused_before_reading_or_blocking() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    let path = temporary.path().canonicalize()?;
    let fifo = CString::new(path.join("unknown.json").as_os_str().as_bytes())?;
    // SAFETY: terminated owned fixture path and ordinary FIFO permission mode.
    assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
    let root = SetupDirectory::open_canonical(&path)?;
    let start = std::time::Instant::now();
    assert!(root.read_file(OsStr::new("unknown.json"), 1024).is_err());
    assert!(start.elapsed() < std::time::Duration::from_secs(1));
    assert!(
        std::fs::symlink_metadata(path.join("unknown.json"))?
            .file_type()
            .is_fifo()
    );
    Ok(())
}

#[test]
fn initial_collection_gate_accepts_only_the_exact_empty_native_slot_and_preserves_unknowns()
-> Result<()> {
    let (_temporary, registry, request) = super::super::tests::fixture()?;
    let metadata = Metadata::from_request(&registry.root, &request)?;
    let fixture = tempfile::tempdir()?;
    let path = fixture.path().canonicalize()?;
    let directory = SetupDirectory::open_canonical(&path)?;
    write_collection(&directory, &metadata)?;
    let gates = directory.create_dir(OsStr::new(".current-gates"))?;
    gates.create_file(OsStr::new(&metadata.slot), b"")?;
    validate_collection(&directory, &metadata, true)?;
    let exact = path.join(".current-gates").join(&metadata.slot);
    std::fs::write(&exact, b"unknown gate bytes")?;
    assert!(validate_collection(&directory, &metadata, true).is_err());
    assert_eq!(std::fs::read(&exact)?, b"unknown gate bytes");
    std::fs::write(&exact, b"")?;
    let foreign = path.join(".current-gates").join("foreign-slot");
    std::fs::write(&foreign, b"unknown source bytes")?;
    assert!(validate_collection(&directory, &metadata, true).is_err());
    assert_eq!(std::fs::read(&foreign)?, b"unknown source bytes");
    std::fs::remove_file(&foreign)?;
    std::fs::remove_file(&exact)?;
    let unknown = path.join("unrelated.txt");
    std::fs::write(&unknown, b"preserve unrelated source")?;
    std::os::unix::fs::symlink(&unknown, &exact)?;
    assert!(validate_collection(&directory, &metadata, true).is_err());
    assert_eq!(std::fs::read(&unknown)?, b"preserve unrelated source");
    assert!(std::fs::symlink_metadata(&exact)?.file_type().is_symlink());
    Ok(())
}
