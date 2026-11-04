// SPDX-License-Identifier: Apache-2.0
//! Real Windows filesystem/component checks. Catalog fixtures enable isolated
//! metadata authority only; installed runtime/credential qualification is separate.
use super::super::{NativeSetupPins, prepare_index_storage, prepare_with_hook};
use super::*;
use crate::service::{collection::Collection, registry::ServiceRegistry};
use airk_core::{catalog_setup::IndexSetupRequest, service::ProjectRegistration};
fn pins(request: &IndexSetupRequest) -> Result<NativeSetupPins> {
    let data = Path::new(&request.registration.data_root);
    Ok(NativeSetupPins::from_native_review(
        SetupDirectory::open_canonical(data.parent().unwrap())?.file,
        if data.exists() {
            Some(SetupDirectory::open_canonical(data)?.file)
        } else {
            None
        },
        request
            .sources
            .iter()
            .map(|source| {
                Ok(SetupDirectory::open_canonical(Path::new(&source.canonical_path))?.file)
            })
            .collect::<Result<Vec<_>>>()?,
    ))
}
fn no_jobs(registry: &ServiceRegistry) -> Result<()> {
    let jobs: i64 =
        registry
            .connection
            .query_row("SELECT count(*) FROM service_jobs", [], |row| row.get(0))?;
    assert_eq!(jobs, 0);
    Ok(())
}

#[test]
fn actual_windows_staged_setup_records_and_replays_native_readable_metadata() -> Result<()> {
    let (_temporary, mut registry, request) = super::super::super::tests::fixture()?;
    let control = registry.root.clone();
    let source = Path::new(&request.registration.root).join("original.txt");
    std::fs::write(&source, b"original source")?;
    let reserved = registry.reserve_index_setup(&request)?;
    let native = prepare_index_storage(&registry, &control, &reserved, pins(&request)?)?;
    let collection = Collection::existing(
        Path::new(&request.registration.data_root),
        &request.registration.root,
        &request.registration.collection,
    )?;
    let selection = collection.select()?;
    assert!(
        matches!(
            selection
                .store
                .lock()
                .as_ref()
                .err()
                .and_then(|error| error.downcast_ref::<std::fs::TryLockError>()),
            Some(std::fs::TryLockError::WouldBlock)
        ),
        "actual native writer key was not retained"
    );
    assert!(selection.store.persistent_revision()?.is_none());
    let roots =
        crate::roots::RootConfig::load(selection.store.directory(), &request.registration.root)?;
    assert_eq!(
        roots.roots.get("support"),
        Some(&request.sources[1].canonical_path)
    );
    let prepared = native.record(&mut registry)?;
    assert_eq!(
        prepared.stage,
        airk_core::catalog_setup::SetupStage::Prepared
    );
    let replay = prepare_index_storage(&registry, &control, &prepared, pins(&request)?)?
        .record(&mut registry)?;
    assert_eq!(replay, prepared);
    assert_eq!(std::fs::read(&source)?, b"original source");
    no_jobs(&registry)
}
#[test]
fn windows_actual_prepare_data_replacement_after_pin_never_writes_foreign_root() -> Result<()> {
    let (_temporary, mut registry, request) = super::super::super::tests::fixture()?;
    let control = registry.root.clone();
    let data = Path::new(&request.registration.data_root);
    std::fs::create_dir(data)?;
    let captured = pins(&request)?;
    let reserved = registry.reserve_index_setup(&request)?;
    let moved = data.with_file_name("moved-original-data");
    let mut entered = false;
    let result = prepare_with_hook(&registry, &control, &reserved, captured, || {
        entered = true;
        std::fs::rename(data, &moved).unwrap();
        std::fs::create_dir(data).unwrap();
        std::fs::write(data.join("source.txt"), b"foreign source").unwrap();
    });
    assert!(
        entered,
        "fixture never reached actual post-pin/pre-effect boundary"
    );
    assert!(result.is_err());
    assert_eq!(std::fs::read_dir(data)?.count(), 1);
    assert_eq!(std::fs::read(data.join("source.txt"))?, b"foreign source");
    assert!(moved.join(crate::service::data_scope::OWNER).exists());
    no_jobs(&registry)
}
#[test]
fn windows_actual_prepare_parent_guard_prevents_foreign_leaf_redirection() -> Result<()> {
    let (_temporary, mut registry, mut request) = super::super::super::tests::fixture()?;
    let control = registry.root.clone();
    let base = control.parent().unwrap();
    let parent = base.join("selected-parent");
    std::fs::create_dir(&parent)?;
    let original = Path::new(&request.registration.root).join("original.txt");
    std::fs::write(&original, b"original source")?;
    let outside = base.join("other");
    std::fs::write(outside.join("source.txt"), b"outside original")?;
    request.registration = ProjectRegistration::new(
        request.registration.root.clone(),
        parent.join("data").to_str().unwrap().into(),
        request.registration.collection.clone(),
    )?;
    let captured = pins(&request)?;
    let reserved = registry.reserve_index_setup(&request)?;
    let moved = parent.with_file_name("moved-selected-parent");
    let mut entered = false;
    let mut denied = None;
    let result = prepare_with_hook(&registry, &control, &reserved, captured, || {
        entered = true;
        denied = std::fs::rename(&parent, &moved)
            .err()
            .and_then(|error| error.raw_os_error());
    });
    assert!(
        entered,
        "fixture never reached actual post-pin/pre-effect boundary"
    );
    assert!(
        matches!(denied, Some(5 | 32)),
        "actual Windows guard did not prevent parent redirection: {denied:?}"
    );
    assert!(!moved.exists());
    let prepared = result?.record(&mut registry)?;
    assert_eq!(
        prepared.stage,
        airk_core::catalog_setup::SetupStage::Prepared
    );
    assert!(
        parent
            .join("data")
            .join(crate::service::data_scope::OWNER)
            .exists()
    );
    assert_eq!(std::fs::read(&original)?, b"original source");
    assert_eq!(
        std::fs::read(outside.join("source.txt"))?,
        b"outside original"
    );
    assert_eq!(std::fs::read_dir(outside)?.count(), 1);
    no_jobs(&registry)
}
#[test]
fn windows_namespace_replacement_keeps_all_mutations_on_retained_object() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    let base = temporary.path().canonicalize()?;
    let root = SetupDirectory::open_canonical(&base)?;
    let namespace = root.create_dir(OsStr::new("namespace"))?;
    std::fs::rename(base.join("namespace"), base.join("old-namespace"))?;
    std::fs::create_dir(base.join("namespace"))?;
    std::fs::write(base.join("namespace/source.txt"), b"foreign bytes")?;
    namespace.create_file(OsStr::new("metadata.json"), b"owned metadata")?;
    namespace.create_dir(OsStr::new("slot"))?;
    namespace.sync()?;
    let mut captured = namespace.entries()?.collect::<Result<Vec<_>>>()?;
    captured.sort();
    assert_eq!(
        captured,
        vec![OsString::from("metadata.json"), OsString::from("slot")],
        "reopened cursor must enumerate the same retained directory after name replacement"
    );
    assert_eq!(std::fs::read_dir(base.join("namespace"))?.count(), 1);
    assert_eq!(
        std::fs::read(base.join("namespace/source.txt"))?,
        b"foreign bytes"
    );
    assert_eq!(
        std::fs::read(base.join("old-namespace/metadata.json"))?,
        b"owned metadata"
    );
    Ok(())
}
#[test]
fn windows_no_replace_publication_preserves_unknown_destination() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    let base = temporary.path().canonicalize()?;
    let root = SetupDirectory::open_canonical(&base)?;
    let prepared = root.create_file(OsStr::new("prepared.json"), b"native")?;
    root.create_file(OsStr::new("occupied.json"), b"unknown user bytes")?;
    assert!(
        root.publish_no_replace(
            OsStr::new("prepared.json"),
            &prepared,
            &root,
            OsStr::new("occupied.json")
        )
        .is_err()
    );
    assert_eq!(
        std::fs::read(base.join("occupied.json"))?,
        b"unknown user bytes"
    );
    assert_eq!(std::fs::read(base.join("prepared.json"))?, b"native");
    Ok(())
}
#[test]
fn windows_native_handle_capacity_flush_and_inventory_are_actual_and_bounded() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    let base = temporary.path().canonicalize()?;
    let root = SetupDirectory::open_canonical(&base)?;
    for name in ["a.json", "b.json"] {
        root.create_file(OsStr::new(name), b"native")?;
    }
    let mut first = root.entries()?.collect::<Result<Vec<_>>>()?;
    first.sort();
    assert_eq!(
        first,
        vec![OsString::from("a.json"), OsString::from("b.json")]
    );
    assert_eq!(
        root.entries()?.count(),
        2,
        "inventory cursor must be independent"
    );
    root.sync()?;
    let capacity = root.capacity(&base)?;
    assert!(capacity.total_to_caller_bytes > 0);
    assert!(capacity.available_to_caller_bytes <= capacity.free_bytes);
    Ok(())
}
#[test]
fn windows_exact_handle_cleanup_preserves_a_foreign_name_replacement() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    let base = temporary.path().canonicalize()?;
    let root = SetupDirectory::open_canonical(&base)?;
    let owned = root.create_file(OsStr::new("record"), b"native record")?;
    std::fs::rename(base.join("record"), base.join("moved-record"))?;
    std::fs::write(base.join("record"), b"foreign source")?;
    assert!(
        root.unlink_file_if_identity(OsStr::new("record"), &owned)
            .is_err()
    );
    assert_eq!(std::fs::read(base.join("record"))?, b"foreign source");
    assert_eq!(std::fs::read(base.join("moved-record"))?, b"native record");
    root.unlink_file_if_identity(OsStr::new("moved-record"), &owned)?;
    drop(owned);
    assert!(!base.join("moved-record").exists());
    Ok(())
}
#[test]
fn windows_native_regular_metadata_cannot_adopt_a_source_hardlink() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    let base = temporary.path().canonicalize()?;
    let root = SetupDirectory::open_canonical(&base)?;
    std::fs::write(base.join("source.txt"), b"original source")?;
    std::fs::hard_link(base.join("source.txt"), base.join("metadata.json"))?;
    assert!(
        root.open_file(OsStr::new("metadata.json"), SetupFileMode::ReadWrite)
            .is_err()
    );
    assert_eq!(std::fs::read(base.join("source.txt"))?, b"original source");
    assert_eq!(
        std::fs::read(base.join("metadata.json"))?,
        b"original source"
    );
    Ok(())
}

#[test]
fn windows_staged_setup_reuses_owned_shared_root_without_changing_other_collection_or_policy()
-> Result<()> {
    let (_temporary, mut registry, request) = super::super::super::tests::fixture()?;
    let control = registry.root.clone();
    let data = Path::new(&request.registration.data_root);
    std::fs::create_dir(data)?;
    crate::service::data_scope::bind(data, &control, request.key.namespace)?;
    let other = Collection::initialize(
        data,
        &request.registration.root,
        "unrelated",
        airk_core::identity::Incarnation::from_bytes([120; 16]),
    )?;
    let other_store = other.selected_store()?;
    let retained = other_store.directory().join("retained.bin");
    std::fs::write(&retained, b"unrelated retained bytes")?;
    let other_owner = data.join("collections-1.4/unrelated/service-owner.json");
    let before = std::fs::read(&other_owner)?;
    let quota = crate::quota::Quota::open_service(data)?;
    let policy = crate::quota::QuotaPolicy {
        root_bytes: 256 * 1024 * 1024,
        default_collection_bytes: 128 * 1024 * 1024,
        collections: Default::default(),
    };
    quota.configure(policy)?;
    let saved_policy = std::fs::read(data.join(".quota-1.4/policy.json"))?;
    let reserved = registry.reserve_index_setup(&request)?;
    let prepared = prepare_index_storage(&registry, &control, &reserved, pins(&request)?)?
        .record(&mut registry)?;
    assert_eq!(
        prepared.stage,
        airk_core::catalog_setup::SetupStage::Prepared
    );
    assert_eq!(std::fs::read(&other_owner)?, before);
    assert_eq!(std::fs::read(&retained)?, b"unrelated retained bytes");
    assert_eq!(
        std::fs::read(data.join(".quota-1.4/policy.json"))?,
        saved_policy
    );
    assert!(other.selected_store()?.persistent_revision()?.is_none());
    no_jobs(&registry)
}

#[test]
fn windows_directory_publication_rejects_a_swapped_source_before_moving_unknowns() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    let base = temporary.path().canonicalize()?;
    let root = SetupDirectory::open_canonical(&base)?;
    let prepared = root.create_dir(OsStr::new("stage"))?;
    prepared.create_file(OsStr::new("native.json"), b"native metadata")?;
    std::fs::rename(base.join("stage"), base.join("moved-owned-stage"))?;
    let foreign = root.create_dir(OsStr::new("stage"))?;
    foreign.create_file(OsStr::new("source.txt"), b"unknown source")?;
    assert!(
        root.publish_no_replace(
            OsStr::new("stage"),
            &prepared.file,
            &root,
            OsStr::new("final")
        )
        .is_err()
    );
    assert!(!base.join("final").exists());
    assert_eq!(
        std::fs::read(base.join("stage/source.txt"))?,
        b"unknown source"
    );
    assert_eq!(
        std::fs::read(base.join("moved-owned-stage/native.json"))?,
        b"native metadata"
    );
    Ok(())
}
#[test]
fn windows_atomic_metadata_publication_rejects_a_swapped_source_before_moving_unknowns()
-> Result<()> {
    let temporary = tempfile::tempdir()?;
    let base = temporary.path().canonicalize()?;
    let root = SetupDirectory::open_canonical(&base)?;
    let prepared = root.create_file(OsStr::new("scratch"), b"native metadata")?;
    std::fs::rename(base.join("scratch"), base.join("moved-owned-scratch"))?;
    root.create_file(OsStr::new("scratch"), b"unknown source")?;
    assert!(
        root.publish_no_replace(
            OsStr::new("scratch"),
            &prepared,
            &root,
            OsStr::new("final.json")
        )
        .is_err()
    );
    assert!(!base.join("final.json").exists());
    assert_eq!(std::fs::read(base.join("scratch"))?, b"unknown source");
    assert_eq!(
        std::fs::read(base.join("moved-owned-scratch"))?,
        b"native metadata"
    );
    Ok(())
}
#[test]
fn windows_source_swap_after_identity_check_renames_only_the_actual_owned_handle() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    let base = temporary.path().canonicalize()?;
    let root = SetupDirectory::open_canonical(&base)?;
    let prepared = root.create_file(OsStr::new("scratch"), b"native metadata")?;
    let mut entered = false;
    root.publish_with_hook(
        OsStr::new("scratch"),
        &prepared,
        &root,
        OsStr::new("final.json"),
        || {
            entered = true;
            std::fs::rename(base.join("scratch"), base.join("moved-owned-scratch")).unwrap();
            std::fs::write(base.join("scratch"), b"unknown source").unwrap();
        },
    )?;
    assert!(entered);
    assert_eq!(std::fs::read(base.join("final.json"))?, b"native metadata");
    assert_eq!(std::fs::read(base.join("scratch"))?, b"unknown source");
    assert!(!base.join("moved-owned-scratch").exists());
    Ok(())
}

#[test]
fn windows_native_error_context_preserves_not_found_classification() -> Result<()> {
    let error = nt_result(0xc0000034u32 as i32, "NtCreateFile relative child").unwrap_err();
    assert_eq!(
        error.downcast_ref::<std::io::Error>().unwrap().kind(),
        std::io::ErrorKind::NotFound
    );
    assert!(error.to_string().contains("NtCreateFile relative child"));
    assert!(error.to_string().contains("0xc0000034"));
    Ok(())
}

#[test]
fn windows_empty_name_directory_opens_keep_interleaved_inventory_cursors_independent() -> Result<()>
{
    let temporary = tempfile::tempdir()?;
    let base = temporary.path().canonicalize()?;
    let mut expected = Vec::new();
    for index in 0..256 {
        let name = OsString::from(format!("entry-{index:04}-{}", "x".repeat(180)));
        std::fs::write(base.join(&name), b"owned enumeration fixture")?;
        expected.push(name);
    }
    expected.sort();
    let directory = SetupDirectory::open_canonical(&base)?;
    let mut first = directory.entries()?;
    let mut second = directory.entries()?;
    let mut first_names = vec![
        first
            .next()
            .context("first cursor missing actual entry")??,
    ];
    let mut second_names = vec![
        second
            .next()
            .context("second cursor missing actual entry")??,
    ];
    // Names exceed a 64KiB native page, so the first cursor must fetch another
    // page before the second finishes. A duplicated file object would skip rows.
    first_names.extend(first.collect::<Result<Vec<_>>>()?);
    second_names.extend(second.collect::<Result<Vec<_>>>()?);
    first_names.sort();
    second_names.sort();
    assert_eq!(first_names, expected);
    assert_eq!(second_names, expected);
    directory.sync()?;
    Ok(())
}

#[test]
fn windows_owner_root_publish_crash_child() -> Result<()> {
    let Some(control) = std::env::var_os("AIRK_TEST_SETUP_BOUND_ROOT_CONTROL") else {
        return Ok(());
    };
    let key: airk_core::service::RequestKey =
        serde_json::from_str(&std::env::var("AIRK_TEST_SETUP_BOUND_ROOT_KEY")?)?;
    let caller: airk_core::identity::Incarnation =
        std::env::var("AIRK_TEST_SETUP_BOUND_ROOT_CALLER")?.parse()?;
    let registry = ServiceRegistry::open(Path::new(&control), key.namespace)?;
    let reserved = registry
        .index_setup_receipt(caller, &key)?
        .context("child saved setup reservation missing")?;
    prepare_index_storage(
        &registry,
        Path::new(&control),
        &reserved,
        pins(&reserved.request)?,
    )?;
    anyhow::bail!("actual post-owner-publication crash boundary was not reached")
}
#[test]
fn windows_published_owner_root_crash_resumes_exact_saved_reservation_without_empty_adoption()
-> Result<()> {
    let (_temporary, mut registry, request) = super::super::super::tests::fixture()?;
    let control = registry.root.clone();
    let original = Path::new(&request.registration.root).join("original.txt");
    std::fs::write(&original, b"original source")?;
    let reserved = registry.reserve_index_setup(&request)?;
    drop(registry);
    let child=std::process::Command::new(std::env::current_exe()?).args(["--exact","service::registry::catalog::setup::storage::windows::tests::windows_owner_root_publish_crash_child","--nocapture"])
        .env("AIRK_TEST_SETUP_BOUND_ROOT_CONTROL",&control).env("AIRK_TEST_SETUP_BOUND_ROOT_KEY",serde_json::to_string(&request.key)?)
        .env("AIRK_TEST_SETUP_BOUND_ROOT_CALLER",request.caller.to_string()).env("AIRK_TEST_SETUP_BOUND_ROOT_PUBLISH_CRASH","1").output()?;
    assert_eq!(
        child.status.code(),
        Some(77),
        "{}",
        String::from_utf8_lossy(&child.stderr)
    );
    let data = Path::new(&request.registration.data_root);
    let owner = std::fs::read(data.join(crate::service::data_scope::OWNER))?;
    assert_eq!(
        owner,
        crate::service::data_scope::initial_owner_bytes(data, &control, request.key.namespace)?
    );
    assert!(crate::service::data_scope::has_saved_binding(
        data,
        &control,
        request.key.namespace
    )?);
    assert!(
        !data.join("collections-1.4").exists(),
        "the crash must precede collection creation"
    );
    let mut registry = ServiceRegistry::open(&control, request.key.namespace)?;
    assert_eq!(
        registry
            .index_setup_receipt(request.caller, &request.key)?
            .unwrap(),
        reserved
    );
    let prepared = prepare_index_storage(&registry, &control, &reserved, pins(&request)?)?
        .record(&mut registry)?;
    assert_eq!(
        prepared.stage,
        airk_core::catalog_setup::SetupStage::Prepared
    );
    assert_eq!(
        std::fs::read(data.join(crate::service::data_scope::OWNER))?,
        owner
    );
    assert_eq!(std::fs::read(&original)?, b"original source");
    no_jobs(&registry)
}
#[test]
fn windows_initial_root_unknown_entry_is_preserved_and_never_published() -> Result<()> {
    let (_temporary, mut registry, request) = super::super::super::tests::fixture()?;
    let control = registry.root.clone();
    let reserved = registry.reserve_index_setup(&request)?;
    let data = Path::new(&request.registration.data_root);
    let stage = data
        .parent()
        .unwrap()
        .join(format!(".airk-setup-{}.part", reserved.operation_id));
    let mut entered = false;
    let result = prepare_with_hook(&registry, &control, &reserved, pins(&request)?, || {
        entered = true;
        std::fs::write(stage.join("foreign-source.txt"), b"preserve unknown source").unwrap();
    });
    assert!(entered);
    assert!(result.is_err());
    assert!(!data.exists());
    assert_eq!(
        std::fs::read(stage.join("foreign-source.txt"))?,
        b"preserve unknown source"
    );
    assert_eq!(
        registry
            .index_setup_receipt(request.caller, &request.key)?
            .unwrap(),
        reserved
    );
    no_jobs(&registry)
}

#[test]
fn windows_legacy_private_stage_crash_child() -> Result<()> {
    let Some(control) = std::env::var_os("AIRK_TEST_SETUP_LEGACY_STAGE_CONTROL") else {
        return Ok(());
    };
    let key: airk_core::service::RequestKey =
        serde_json::from_str(&std::env::var("AIRK_TEST_SETUP_LEGACY_STAGE_KEY")?)?;
    let caller: airk_core::identity::Incarnation =
        std::env::var("AIRK_TEST_SETUP_LEGACY_STAGE_CALLER")?.parse()?;
    let kind = std::env::var("AIRK_TEST_SETUP_LEGACY_STAGE_KIND")?;
    let registry = ServiceRegistry::open(Path::new(&control), key.namespace)?;
    let reserved = registry
        .index_setup_receipt(caller, &key)?
        .context("legacy child saved request missing")?;
    let metadata = super::super::Metadata::new(Path::new(&control), &reserved)?;
    let request = &reserved.request;
    let data = Path::new(&request.registration.data_root);
    let _project =
        crate::service::ProjectLease::acquire(Path::new(&control), request.registration.binding)?;
    let parent = SetupDirectory::open_canonical(data.parent().unwrap())?;
    let stage_name = OsString::from(format!(".airk-setup-{}.part", reserved.operation_id));
    let root = parent.create_dir(&stage_name)?;
    let claim = super::super::open_or_create(&root, OsStr::new(crate::service::data_scope::LOCK))?;
    claim.try_lock()?;
    let owner_allowance = crate::quota::setup::SetupMetadataAllowance::reserve_at(
        &root,
        data,
        "",
        metadata.owner.len() as u64,
        super::super::allocation(&reserved, b"owner")?,
    )?;
    super::super::write_exact(&root, crate::service::data_scope::OWNER, &metadata.owner)?;
    // Reproduce a dead owner allocation retained across a prior process crash,
    // followed by the earlier order's actual native collection preparation.
    std::mem::forget(owner_allowance);
    let namespace = root.create_dir(OsStr::new("collections-1.4"))?;
    let writers = namespace.create_dir(OsStr::new(".writer-locks"))?;
    let writer =
        super::super::open_or_create(&writers, OsStr::new(&request.registration.collection))?;
    writer.try_lock()?;
    let _collection_allowance = crate::quota::setup::SetupMetadataAllowance::reserve_at(
        &root,
        data,
        &request.registration.collection,
        metadata.collection_bytes()?,
        super::super::allocation(&reserved, b"collection")?,
    )?;
    let staged = root.create_dir(&stage_name)?;
    if kind == "partial" {
        super::super::write_exact(&staged, metadata.files[0].0, &metadata.files[0].1)?;
    } else {
        ensure!(kind == "complete", "unknown legacy fixture kind");
        super::super::write_collection(&staged, &metadata)?;
        super::super::validate_collection(&staged, &metadata, true)?;
        root.publish_no_replace(
            &stage_name,
            &staged.file,
            &namespace,
            OsStr::new(&request.registration.collection),
        )?;
    }
    // No Drop: actual OS process death releases the real locks, leaving native
    // record/lease files and full or partial metadata in the private stage.
    std::process::exit(if kind == "partial" { 78 } else { 79 });
}
fn resume_legacy_private_stage(kind: &str) -> Result<()> {
    let (_temporary, mut registry, request) = super::super::super::tests::fixture()?;
    let control = registry.root.clone();
    let original = Path::new(&request.registration.root).join("original.txt");
    std::fs::write(&original, b"original source")?;
    let reserved = registry.reserve_index_setup(&request)?;
    drop(registry);
    let legacy=std::process::Command::new(std::env::current_exe()?).args(["--exact","service::registry::catalog::setup::storage::windows::tests::windows_legacy_private_stage_crash_child","--nocapture"])
        .env("AIRK_TEST_SETUP_LEGACY_STAGE_CONTROL",&control).env("AIRK_TEST_SETUP_LEGACY_STAGE_KEY",serde_json::to_string(&request.key)?)
        .env("AIRK_TEST_SETUP_LEGACY_STAGE_CALLER",request.caller.to_string()).env("AIRK_TEST_SETUP_LEGACY_STAGE_KIND",kind).output()?;
    assert_eq!(
        legacy.status.code(),
        Some(if kind == "partial" { 78 } else { 79 }),
        "{}",
        String::from_utf8_lossy(&legacy.stderr)
    );
    let data = Path::new(&request.registration.data_root);
    assert!(!data.exists());
    let stage_name = format!(".airk-setup-{}.part", reserved.operation_id);
    let stage = data.parent().unwrap().join(&stage_name);
    let collection_allocation = super::super::allocation(&reserved, b"collection")?;
    let relative_record = Path::new(".quota-1.4")
        .join("reservations")
        .join(format!("r-{collection_allocation}"));
    let before_record = std::fs::read(stage.join(&relative_record))?;
    let before_identity = {
        let file = File::open(stage.join(&relative_record))?;
        super::super::file_identity(&file)?
    };
    let source_owner = if kind == "partial" {
        stage.join(&stage_name).join("service-owner.json")
    } else {
        stage
            .join("collections-1.4")
            .join(&request.registration.collection)
            .join("service-owner.json")
    };
    let original_owner = std::fs::read(source_owner)?;
    let publish=std::process::Command::new(std::env::current_exe()?).args(["--exact","service::registry::catalog::setup::storage::windows::tests::windows_owner_root_publish_crash_child","--nocapture"])
        .env("AIRK_TEST_SETUP_BOUND_ROOT_CONTROL",&control).env("AIRK_TEST_SETUP_BOUND_ROOT_KEY",serde_json::to_string(&request.key)?)
        .env("AIRK_TEST_SETUP_BOUND_ROOT_CALLER",request.caller.to_string()).env("AIRK_TEST_SETUP_BOUND_ROOT_PUBLISH_CRASH","1").output()?;
    assert_eq!(
        publish.status.code(),
        Some(77),
        "{}",
        String::from_utf8_lossy(&publish.stderr)
    );
    assert!(crate::service::data_scope::has_saved_binding(
        data,
        &control,
        request.key.namespace
    )?);
    if kind == "partial" {
        assert_eq!(
            std::fs::read(data.join(&relative_record))?,
            before_record,
            "partial metadata must retain its already-admitted native allocation"
        );
        let after_identity = {
            let file = File::open(data.join(&relative_record))?;
            super::super::file_identity(&file)?
        };
        assert_eq!(after_identity, before_identity);
    } else {
        assert!(
            !data.join(&relative_record).exists(),
            "complete metadata must settle its exact dead allocation"
        );
    }
    let mut registry = ServiceRegistry::open(&control, request.key.namespace)?;
    assert_eq!(
        registry
            .index_setup_receipt(request.caller, &request.key)?
            .unwrap(),
        reserved
    );
    let prepared = prepare_index_storage(&registry, &control, &reserved, pins(&request)?)?
        .record(&mut registry)?;
    assert_eq!(
        prepared.stage,
        airk_core::catalog_setup::SetupStage::Prepared
    );
    let collection = data
        .join("collections-1.4")
        .join(&request.registration.collection);
    assert_eq!(
        std::fs::read(collection.join("service-owner.json"))?,
        original_owner
    );
    assert!(!data.join(&relative_record).exists());
    assert_eq!(std::fs::read(&original)?, b"original source");
    assert!(
        Collection::existing(
            data,
            &request.registration.root,
            &request.registration.collection
        )?
        .selected_store()?
        .persistent_revision()?
        .is_none()
    );
    no_jobs(&registry)
}
#[test]
fn windows_same_saved_partial_legacy_stage_resumes_with_its_original_admission() -> Result<()> {
    resume_legacy_private_stage("partial")
}
#[test]
fn windows_same_saved_complete_legacy_stage_resumes_without_discarding_native_metadata()
-> Result<()> {
    resume_legacy_private_stage("complete")
}
