// SPDX-License-Identifier: Apache-2.0
use super::*;

#[test]
fn clean_break_cleanup_empty_saved_registry_can_enter_collections_phase() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    let control = temporary.path().join("control");
    local_ipc::prepare_root(&control)?;
    let namespace = Incarnation::from_bytes([246; 16]);
    let registry = ServiceRegistry::open(&control, namespace)?;
    assert!(registry.bindings()?.is_empty());
    let service = ServiceLease::acquire(&control)?;
    // This is the same owning adapter that the actual Collections phase calls
    // before advancing to private control disposal. An authentic empty registry
    // has no collection or data-root deletion targets and must not stall here.
    let _owner = data_scope::CleanBreakNamespaceOwner::acquire(&service, namespace, &[])?;
    assert!(registry.bindings()?.is_empty());
    assert!(!temporary.path().join("data").exists());
    Ok(())
}

#[test]
fn clean_break_cleanup_accepts_canonical_storage_writer_and_quota_layout() -> Result<()> {
    use airk_core::graph_storage::{GraphHead, GraphScope};
    use airk_store::current::{CurrentHead, CurrentScope};
    use airk_store::quota::{CurrentGrantScope, Quota};
    let (owner, root, old_control, namespace, registrations) =
        activation::rebuild::tests::storage_cleanup_fixture(
            |control, namespace, registrations| {
                let registration = &registrations[0];
                let data = Path::new(&registration.data_root);
                #[cfg(windows)]
                supported_inherited_storage_acl(data)?;
                // Storage deliberately permits a caller's existing readable root;
                // it must not inherit the stricter IPC/credential privacy contract.
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    fs::set_permissions(data, fs::Permissions::from_mode(0o755))?;
                }
                data_scope::check(data, control, namespace)?;
                let mut registry = ServiceRegistry::open(control, namespace)?;
                let mut project = ProjectLease::acquire(control, registration.binding)?;
                registry.admit_worker(&mut project)?;
                let sources = airk_core::catalog_source_change::SourceConfigurationSnapshot {
                    primary_root: registration.root.clone(),
                    aliases: Default::default(),
                };
                sources.validate()?;
                registry.record_watch_roots(
                    &project,
                    &sources.aliases.values().cloned().collect::<Vec<_>>(),
                )?;
                assert_eq!(
                    registry.retained_source_roots(registration.binding)?,
                    vec![sources.primary_root],
                );
                let incarnation = Incarnation::from_bytes([12; 16]);
                let collection = Collection::initialize(
                    data,
                    &registration.root,
                    &registration.collection,
                    incarnation,
                )?;
                let store = collection.selected_store()?;
                let writer = store.lock()?;
                let quota = Quota::for_store(&store)?;
                // A native current allowance intentionally survives handle loss.
                // Disposal must reconcile it after this exact collection is absent.
                let grant = quota.reserve_current(
                    &writer,
                    CurrentGrantScope {
                        collection: registration.collection.clone(),
                        incarnation,
                        transaction: airk_core::identity::Sha256Digest::from_bytes([241; 32]),
                    },
                    4096,
                )?;
                let scope = CurrentScope {
                    root: registration.root.clone(),
                    collection: registration.collection.clone(),
                    incarnation,
                };
                let graph_scope = GraphScope {
                    root: scope.root.clone(),
                    collection: scope.collection.clone(),
                    incarnation,
                    graph_instance: Incarnation::from_bytes([242; 16]),
                    specification: airk_core::identity::Sha256Digest::from_bytes([243; 32]),
                    record_bytes: 16,
                };
                let head = CurrentHead {
                    version: 2,
                    scope: scope.clone(),
                    sequence: 0,
                    artifact_root: airk_store::current::map::empty_root(&scope)?,
                    lexical_sequence: 0,
                    lexical_commit: airk_core::identity::Sha256Digest::from_bytes([244; 32]),
                    graph_head: GraphHead::empty(graph_scope)?,
                };
                store
                    .current_write(&writer, &quota, &scope, 8 * 1024 * 1024)?
                    .initialize(head)?;
                assert!(store.directory().join("current.sqlite3").is_file());
                assert!(!store.directory().join("index.sqlite3").exists());
                drop(grant);
                drop(writer);
                let usage_scope = airk_core::usage::UsageScope {
                    collection: registration.collection.clone(),
                    root_binding: Sha256::digest(registration.root.as_bytes()).into(),
                };
                let usage_directory = data.join("collections-1.4").join(&registration.collection);
                assert!(
                    airk_store::usage::UsageStore::open(
                        &usage_directory,
                        &usage_scope,
                        &Default::default(),
                    )?
                    .is_some()
                );
                Ok(())
            },
        )?;
    let registration = &registrations[0];
    let data = Path::new(&registration.data_root);
    assert!(data.join("collections-1.4/.writer-locks").is_dir());
    assert!(data.join(".quota-1.4/current-grants").is_dir());
    assert!(data.join("usage-quota.lock").is_file());
    let source = Path::new(&registration.root).join("original.txt");
    let source_before = fs::read(&source)?;
    let foreign = owner.path().join("foreign-sentinel");
    fs::write(&foreign, b"unrelated original bytes")?;
    let plan = capture(&root, &old_control, &registrations)?;
    assert_eq!(plan.registrations.len(), 1);
    assert_eq!(plan.data_roots, vec![data.to_path_buf()]);
    let service = ServiceLease::acquire(&old_control)?;
    let _source = SourceWatchLease::acquire(&old_control)?;
    let _embedding = EmbeddingLease::acquire(&old_control)?;
    let project = ProjectLease::acquire(&old_control, registration.binding)?;
    let mut removed = false;
    for _ in 0..64 {
        if Collection::purge_for_clean_break_turn(registration, &project)? {
            removed = true;
            break;
        }
    }
    assert!(removed, "bounded native collection disposal did not finish");
    drop(project);
    drop(_source);
    drop(_embedding);
    let mut storage_removed = false;
    for _ in 0..64 {
        if retire_data_root(data, &service, namespace, &registrations)? {
            storage_removed = true;
            break;
        }
    }
    assert!(
        storage_removed,
        "bounded native storage disposal did not finish"
    );
    drop(service);
    assert!(!data.exists());
    assert_eq!(fs::read(source)?, source_before);
    assert_eq!(fs::read(foreign)?, b"unrelated original bytes");
    Ok(())
}

#[cfg(windows)]
fn supported_inherited_storage_acl(data: &Path) -> Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::{
        Foundation::LocalFree,
        Security::{
            Authorization::{
                ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
                SE_FILE_OBJECT, SetNamedSecurityInfoW,
            },
            DACL_SECURITY_INFORMATION, GetSecurityDescriptorDacl,
            PROTECTED_DACL_SECURITY_INFORMATION,
        },
    };
    let original = fs::read(data.join("service-data-1.4.owner.json"))?;
    let descriptor: Vec<u16> = format!(
        "O:{}D:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)(A;OICI;FA;;;OW)",
        local_ipc::current_user()?
    )
    .encode_utf16()
    .chain(Some(0))
    .collect();
    let mut allocated = std::ptr::null_mut();
    // SAFETY: the terminated descriptor and writable output pointer live through conversion.
    ensure!(
        unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                descriptor.as_ptr(),
                SDDL_REVISION_1,
                &mut allocated,
                std::ptr::null_mut(),
            )
        } != 0,
        "fixture storage descriptor conversion failed"
    );
    struct Descriptor(*mut std::ffi::c_void);
    impl Drop for Descriptor {
        fn drop(&mut self) {
            // SAFETY: conversion allocated this security descriptor with LocalAlloc.
            unsafe {
                LocalFree(self.0);
            }
        }
    }
    let held = Descriptor(allocated);
    let (mut present, mut defaulted, mut acl) = (0, 0, std::ptr::null_mut());
    // SAFETY: the converted descriptor remains owned by held; output pointers are writable.
    ensure!(
        unsafe { GetSecurityDescriptorDacl(held.0, &mut present, &mut acl, &mut defaulted) } != 0
            && present != 0
            && !acl.is_null(),
        "fixture storage ACL is unavailable"
    );
    let mut path: Vec<u16> = data.as_os_str().encode_wide().chain(Some(0)).collect();
    // SAFETY: this exact new temporary data root and descriptor remain live. Only
    // its DACL changes; control roots, source roots and failed fixtures are excluded.
    let status = unsafe {
        SetNamedSecurityInfoW(
            path.as_mut_ptr(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            acl,
            std::ptr::null_mut(),
        )
    };
    ensure!(status == 0, "fixture storage ACL setup failed: {status}");
    assert!(
        local_ipc::require_existing_root(data).is_err(),
        "storage fixture accidentally uses IPC-private ACL"
    );
    assert_eq!(
        fs::read(data.join("service-data-1.4.owner.json"))?,
        original
    );
    Ok(())
}

#[test]
fn clean_break_private_descendant_policy_accepts_owned_readable_native_directories() {
    for mode in [0o040700, 0o040755] {
        assert!(private_descendant_allowed(71, 71, mode, false, true));
    }
    assert!(!private_descendant_allowed(71, 72, 0o040755, false, true));
    assert!(!private_descendant_allowed(71, 71, 0o040755, true, true));
    assert!(!private_descendant_allowed(71, 71, 0o040755, false, false));
    assert!(!private_descendant_allowed(71, 71, 0o120777, false, true));
    assert!(!private_descendant_allowed(71, 71, 0o100644, false, true));
}

#[cfg(unix)]
#[test]
fn clean_break_private_turn_accepts_native_readable_children_below_private_root() -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let owner = tempfile::tempdir()?;
    let private = owner.path().join("control");
    local_ipc::prepare_root(&private)?;
    let child = private.join("models/native-client");
    fs::create_dir_all(&child)?;
    fs::set_permissions(private.join("models"), fs::Permissions::from_mode(0o755))?;
    fs::set_permissions(&child, fs::Permissions::from_mode(0o755))?;
    fs::write(child.join("owned-derived-metadata"), b"derived")?;
    assert!(private_turn(&private, &BTreeSet::new())?);
    assert!(fs::read_dir(&private)?.next().is_none());
    let sentinel = private.join("root-privacy-sentinel");
    fs::write(&sentinel, b"private-root proof is required")?;
    fs::set_permissions(&private, fs::Permissions::from_mode(0o755))?;
    assert!(
        private_turn(&private, &BTreeSet::new()).is_err(),
        "root privacy must remain mandatory"
    );
    assert_eq!(fs::read(sentinel)?, b"private-root proof is required");
    Ok(())
}

fn cleanup_scope_fixture(root: &Path, old_control: PathBuf) -> Result<Plan> {
    Ok(Plan {
        schema: 1,
        install: root.canonicalize()?,
        successor_control: crate::install_layout::saved_control(root)?,
        old_control,
        namespace: Incarnation::from_bytes([91; 16]),
        registrations: Vec::new(),
        source_roots: Vec::new(),
        data_roots: Vec::new(),
        retired_data_roots: Vec::new(),
        state_files: BTreeMap::new(),
        logs: None,
        launchers: None,
        runtimes: None,
        endpoints_retired: true,
        phase: Phase::State,
    })
}

#[test]
fn clean_break_cleanup_ordinary_root_cannot_hide_a_runtime_target() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let root = owner.path().join("install");
    local_ipc::prepare_root(&root)?;
    layout::initialize(&root, Some(&root), RuntimeChannel::Stable)?;
    let canonical = root.canonicalize()?;
    let mut plan = cleanup_scope_fixture(&root, canonical.join("old-state/control"))?;
    plan.data_roots.push(canonical.join("runtimes/retained"));
    assert!(
        validate_scope(&root, &plan)
            .unwrap_err()
            .to_string()
            .contains("clean_break_cleanup_active_scope")
    );
    assert!(root.join("runtime-layout.json").is_file());
    Ok(())
}

#[test]
fn clean_break_cleanup_state_equal_to_install_keeps_install_for_ordinary_caller() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let root = owner.path().join("install");
    local_ipc::prepare_root(&root)?;
    let successor = owner.path().join("successor");
    layout::initialize(&root, Some(&successor), RuntimeChannel::Stable)?;
    let canonical = root.canonicalize()?;
    let plan = cleanup_scope_fixture(&root, canonical.join("control"))?;
    save(&root, &plan)?;
    let original_layout = fs::read(root.join("runtime-layout.json"))?;
    let result = cleanup(&root, &plan.old_control, &[])?;
    assert_eq!(result["complete"], true);
    assert_eq!(fs::read(root.join("runtime-layout.json"))?, original_layout);
    assert!(root.is_dir());
    Ok(())
}

#[test]
fn clean_break_private_cleanup_is_bounded_resumable_and_keeps_proof() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let root = owner.path().join("control");
    local_ipc::prepare_root(&root)?;
    let proof = root.join("service-1.4.owner.json");
    fs::write(&proof, b"retained namespace proof")?;
    for index in 0..130 {
        fs::write(root.join(format!("owned-{index}")), b"derived")?;
    }
    let protected = BTreeSet::from([proof.clone()]);
    assert!(!private_turn(&root, &protected)?);
    assert_eq!(fs::read(&proof)?, b"retained namespace proof");
    assert_eq!(fs::read_dir(&root)?.count(), 67);
    assert!(!private_turn(&root, &protected)?);
    assert!(private_turn(&root, &protected)?);
    assert_eq!(fs::read_dir(&root)?.count(), 1);
    assert_eq!(fs::read(&proof)?, b"retained namespace proof");
    assert!(private_turn(&root, &BTreeSet::new())?);
    assert!(fs::read_dir(&root)?.next().is_none());
    Ok(())
}

#[test]
fn clean_break_cleanup_checks_every_source_against_each_target() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let source = owner.path().join("source");
    let data = owner.path().join("data");
    fs::create_dir(&source)?;
    fs::create_dir(&data)?;
    let source = source.canonicalize()?;
    let data = data.canonicalize()?;
    assert!(source_separation(&data, std::slice::from_ref(&source)).is_ok());
    assert!(source_separation(&source, std::slice::from_ref(&source)).is_err());
    assert!(
        source_separation(&owner.path().canonicalize()?, std::slice::from_ref(&source)).is_err()
    );
    assert!(source_separation(&source.join("nested"), &[source]).is_err());
    Ok(())
}

#[test]
fn clean_break_cleanup_directory_census_is_bounded_and_discloses_remainder() -> Result<()> {
    use std::cell::Cell;
    let observed = Cell::new(0usize);
    let entries = (0..100_000).map(|index| {
        observed.set(observed.get() + 1);
        Ok(PathBuf::from(format!("owned-{index}")))
    });
    let (mut stack, mut queued) = (Vec::new(), 1usize);
    assert!(!enqueue_entries(&mut stack, entries, &mut queued)?);
    assert_eq!(queued, 4096);
    assert_eq!(stack.len(), 4095);
    assert_eq!(
        observed.get(),
        4096,
        "one lookahead must not enumerate the remaining directory"
    );
    Ok(())
}

#[test]
fn clean_break_cleanup_never_reports_a_stalled_directory_scan_as_continuing() -> Result<()> {
    assert!(scan_outcome(true, false).is_err());
    assert!(!scan_outcome(true, true)?);
    assert!(scan_outcome(false, false)?);
    assert!(scan_outcome(false, true)?);
    Ok(())
}

#[test]
fn clean_break_cleanup_receipts_remain_bound_to_each_predecessor_control() {
    let first = Path::new("installation/first-state/control");
    let second = Path::new("installation/second-state/control");
    assert_eq!(journal_name(first), journal_name(first));
    assert_ne!(journal_name(first), journal_name(second));
    assert!(!journal_name(first).contains("first-state"));
}

#[cfg(unix)]
#[test]
fn clean_break_private_cleanup_never_follows_source_links() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let source = owner.path().join("source");
    let private = owner.path().join("private");
    fs::create_dir(&source)?;
    fs::create_dir(&private)?;
    fs::write(source.join("original"), b"source")?;
    std::os::unix::fs::symlink(&source, private.join("link"))?;
    assert!(private_turn(&private, &BTreeSet::new()).is_err());
    assert_eq!(fs::read(source.join("original"))?, b"source");
    Ok(())
}
