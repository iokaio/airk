// SPDX-License-Identifier: Apache-2.0
//! Settle what an interrupted standalone saved-index adoption left behind.
//!
//! Simplification S2 (decision Q1 and the user's follow-up decision): AIRK no
//! longer adopts standalone saved indexes and never resumes an interrupted
//! adoption. Its leftovers are corrected where possible and deleted otherwise,
//! so the ordinary installer flow is not blocked:
//!
//! - the native admission fence armed for the adoption's exact checkpoint
//!   operation is cleared, only while the service is stopped under installer
//!   exclusion; a fence owned by another operation is never cleared;
//! - the service is restarted with the currently selected runtime only when the
//!   adoption had stopped a running, not user-stopped service and starting is
//!   still allowed;
//! - adoption journals and the owner-verified private archive copies are
//!   deleted;
//! - catalog rows, adopted index data, original standalone data and sources,
//!   agent configurations and the cache-retirement journal are kept.
//!
//! The caller holds the installer lock. Every step re-reads current files, so
//! repeating the settlement after a crash at any point is safe; the
//! maintenance journal is removed last.
use super::*;
use airk_core::identity::Incarnation;

const JOURNAL: &str = "legacy-maintenance.json";
const MAX_JOURNAL: u64 = 512 * 1024;
/// Side files of the maintenance journal: immutable state, cycle and
/// restoration snapshots, named `legacy-maintenance-*-{operation}[-n].json`.
const SIDE_PREFIX: &str = "legacy-maintenance-";
const BATCH_PREFIX: &str = "legacy-index-adoption-batch-";
const MAX_BATCH: u64 = 16 * 1024 * 1024;
const PRIVATE_ARCHIVES: &str = "legacy-adoption";
/// Bound on the private archive walk: 32 groups of at most 100,000 captured
/// files each, plus their directories and validation copies.
const MAX_ARCHIVE_ENTRIES: usize = 8_000_000;
const MAX_ARCHIVE_DEPTH: usize = 128;
/// Stable code for catalog rows of an adoption interrupted before Ready.
const INTERRUPTED_ADOPTION_REBUILD: &str = "interrupted_adoption_rebuild_required";
const INTERRUPTED_ADOPTION_GUIDANCE: &str = "A standalone adoption was interrupted before this index was ready. Its catalog rows and index data, if any, are kept as an inert entry: no rollback API survives, and changing persisted catalog state needs the plan's D8 decision. Rebuild this index by setting it up again from its original sources in Indexing, retiring the inert entry if needed, and keep the machine and AIRK running (no shutdown or sleep) until that rebuild completes.";

/// Only the fields settlement needs. Earlier journals carry more; they are
/// ignored here rather than validated against a removed implementation.
#[derive(Deserialize)]
struct Journal {
    schema: u32,
    install: PathBuf,
    control: PathBuf,
    operation: Incarnation,
    checkpoint_operation: Incarnation,
    phase: String,
    original_explicit_user_stop: bool,
    cycle_health: Option<serde_json::Value>,
}

/// What a settlement restored, deleted and left, each with its reason. A
/// non-empty settlement is reported as one JSON line on standard error, so the
/// command's own result shape is unchanged.
#[derive(Default, Serialize)]
pub(super) struct Settlement {
    restored: Vec<serde_json::Value>,
    deleted: Vec<serde_json::Value>,
    left: Vec<serde_json::Value>,
}

impl Settlement {
    pub(super) fn restored(&mut self, path: &Path, reason: &str) {
        self.restored
            .push(serde_json::json!({"path":path,"reason":reason}));
    }
    pub(super) fn deleted(&mut self, path: &Path, reason: &str) {
        self.deleted
            .push(serde_json::json!({"path":path,"reason":reason}));
    }
    pub(super) fn left(&mut self, value: serde_json::Value) {
        self.left.push(value);
    }
    pub(super) fn is_empty(&self) -> bool {
        self.restored.is_empty() && self.deleted.is_empty() && self.left.is_empty()
    }
    pub(super) fn report(&self) {
        if self.is_empty() {
            return;
        }
        let mut value = serde_json::json!({"code":"legacy_adoption_settled","schema":1});
        value["restored"] = serde_json::json!(self.restored);
        value["deleted"] = serde_json::json!(self.deleted);
        value["left"] = serde_json::json!(self.left);
        eprintln!("{value}");
    }
}

fn present(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

fn read(root: &Path, control: &Path) -> Result<Journal> {
    let journal: Journal =
        serde_json::from_slice(&read_bounded(&root.join(JOURNAL), MAX_JOURNAL)?)?;
    ensure!(
        journal.schema == 1 && journal.install == root && journal.control == control,
        "saved-index maintenance journal ownership changed"
    );
    Ok(journal)
}

/// Refuse a manager change or activation only while the journal remains,
/// which settlement leaves only when it could not clear the adoption's own
/// native admission fence.
pub(super) fn require_settled(root: &Path) -> Result<()> {
    ensure!(
        !present(&root.join(JOURNAL))?,
        "legacy_maintenance_pending_before_manager_change: an interrupted standalone adoption still holds its native admission fence or left an unverifiable journal (see the legacy_adoption_settled report); stop the AIRK service, then retry"
    );
    Ok(())
}

/// Settle every leftover of an interrupted standalone adoption. The caller
/// holds the installer lock. Without any leftover this reads only directory
/// metadata and changes nothing.
pub(super) async fn settle_locked(root: &Path) -> Result<Settlement> {
    let root = root.canonicalize()?;
    let mut settlement = Settlement::default();
    let retained = settle_maintenance(&root, &mut settlement).await?;
    remove_journals(&root, retained, &mut settlement)?;
    remove_private_archives(&root, &mut settlement)?;
    settlement.report();
    Ok(settlement)
}

/// Returns whether the maintenance journal had to be kept.
async fn settle_maintenance(root: &Path, settlement: &mut Settlement) -> Result<bool> {
    let path = root.join(JOURNAL);
    if !present(&path)? {
        return Ok(false);
    }
    let control = layout::control(root)?;
    let fence = service_runtime::checkpoint::fence_operation(&control)?;
    let journal = match read(root, &control) {
        Ok(journal) => journal,
        Err(error) => {
            if fence.is_some() || safe_file(&path).is_err() {
                settlement.left(serde_json::json!({"path":path,"reason":format!(
                    "unreadable maintenance journal kept: it is not a regular file, or a native admission fence exists and its owner cannot be proven ({error:#})"
                )}));
                return Ok(true);
            }
            fs::remove_file(&path)?;
            sync_directory(root)?;
            settlement.deleted(
                &path,
                "unreadable maintenance journal removed; no native admission fence remains",
            );
            return Ok(false);
        }
    };
    let ours = fence == Some(journal.checkpoint_operation);
    if let Some(other) = fence.filter(|_| !ours) {
        settlement.left(serde_json::json!({"path":control.join("native-upgrade-checkpoint.json"),
            "operation":other,"reason":"native admission fence belongs to another operation; not cleared"}));
    }
    // The adoption stopped a running, not user-stopped service and did not
    // record its restoration. Restart only that service, never another owner's.
    let restart = journal.phase != "restored"
        && journal.cycle_health.is_some()
        && !journal.original_explicit_user_stop
        && fence.is_none_or(|_| ours);
    let mut stopped = false;
    if ours || restart {
        let manager = crate::service_manager::ServiceManager::for_runtime(root)?;
        match manager
            .require_stopped_for_maintenance()
            .and_then(|()| quiescent(root))
        {
            Ok(exclusion) => {
                if ours {
                    // Every native owner lease is held: no live admission
                    // state can depend on this fence.
                    service_runtime::checkpoint::settle(&control, journal.checkpoint_operation)?;
                    settlement.restored(
                        &control.join("native-upgrade-checkpoint.json"),
                        "cleared the interrupted adoption's own native admission fence under installer exclusion",
                    );
                }
                drop(exclusion);
                stopped = true;
            }
            Err(error) if ours => {
                settlement.left(serde_json::json!({"path":path,"operation":journal.operation,
                    "checkpoint_operation":journal.checkpoint_operation,"reason":format!(
                        "the native admission fence is kept because the service is not stopped ({error:#}); stop the AIRK service, then retry"
                    )}));
                return Ok(true);
            }
            // A running service was already restored or started by its owner.
            Err(_) => {}
        }
    }
    if restart && stopped {
        restart_selected(root, settlement).await;
    }
    safe_file(&path)?;
    fs::remove_file(&path)?;
    sync_directory(root)?;
    settlement.deleted(
        &path,
        &format!(
            "maintenance journal of standalone adoption {} removed; adoption is never resumed",
            journal.operation
        ),
    );
    Ok(false)
}

/// Uses the currently selected runtime, never the journal's, and only while
/// starting is still allowed (the explicit user stop flag remains clear).
async fn restart_selected(root: &Path, settlement: &mut Settlement) {
    let started = async {
        lifecycle::require_start_allowed(root)?;
        let selected = selection_record(root)?;
        let (main, approval) = verify_runtime(root, &selected.active)?;
        let health = activation::start_selected(root, &approval, &main, None).await?;
        anyhow::Ok((selected.active, health.instance))
    }
    .await;
    match started {
        Ok((runtime, instance)) => {
            settlement
                .restored
                .push(serde_json::json!({"service":"restarted",
            "runtime":runtime,"instance":instance,
            "reason":"the adoption had stopped this running service"}))
        }
        Err(error) => settlement.left(serde_json::json!({"service":"stopped","reason":format!(
            "the adoption had stopped this service; restart was not possible ({error:#})"
        )})),
    }
}

/// Deletes batch journals and, unless the maintenance journal was kept, its
/// snapshot side files. Catalog rows of an adoption interrupted before Ready
/// are reported and kept: no rollback API survives, and changing persisted
/// catalog state needs the plan's D8 decision.
fn remove_journals(root: &Path, retained: bool, settlement: &mut Settlement) -> Result<()> {
    let mut removed = false;
    for entry in fs::read_dir(root)? {
        let path = entry?.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let batch = name.starts_with(BATCH_PREFIX);
        if !name.ends_with(".json") || !(batch || (!retained && name.starts_with(SIDE_PREFIX))) {
            continue;
        }
        if let Err(error) = safe_file(&path) {
            settlement.left(serde_json::json!({"path":path,
                "reason":format!("not a regular adoption journal; preserved ({error:#})")}));
            continue;
        }
        if batch {
            report_batch(&path, settlement);
        }
        fs::remove_file(&path)?;
        removed = true;
        settlement.deleted(
            &path,
            if batch {
                "adoption batch journal removed; adoption is never resumed"
            } else {
                "adoption maintenance snapshot removed"
            },
        );
    }
    if removed {
        sync_directory(root)?;
    }
    Ok(())
}

fn report_batch(path: &Path, settlement: &mut Settlement) {
    let Ok(bytes) = read_bounded(path, MAX_BATCH) else {
        return;
    };
    let Ok(journal) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
        return;
    };
    for group in journal["groups"].as_array().into_iter().flatten().take(32) {
        let setup = &group["plan"]["setup"];
        if setup.is_null() {
            continue;
        }
        let entry = if group["result"].is_null() {
            serde_json::json!({"registration":setup["registration"],"request_key":setup["key"],
                "code":INTERRUPTED_ADOPTION_REBUILD,"reason":INTERRUPTED_ADOPTION_GUIDANCE})
        } else {
            serde_json::json!({"registration":setup["registration"],"request_key":setup["key"],
                "reason":"adopted index is kept with its catalog rows and data"})
        };
        settlement.left(entry);
    }
}

/// `<state>/legacy-adoption` holds private copies of original saved indexes.
/// It is deleted only when its owner record names this installation and state
/// and it contains no symbolic link or reparse point; otherwise it is kept.
fn remove_private_archives(root: &Path, settlement: &mut Settlement) -> Result<()> {
    // Adoption required a claimed layout; without one there is nothing here.
    let Ok((state, _)) = layout::retained_parameters(root) else {
        return Ok(());
    };
    let private = state.join(PRIVATE_ARCHIVES);
    if !present(&private)? {
        return Ok(());
    }
    let owner = serde_json::json!({"schema":1,"install":root,"state":state});
    let checked = (|| -> Result<bool> {
        require_plain_directory(&private)?;
        let record = private.join("owner.json");
        if !present(&record)? {
            // Only the last step of an earlier removal leaves this state.
            ensure!(
                fs::read_dir(&private)?.next().is_none(),
                "private adoption directory has no owner record"
            );
            return Ok(false);
        }
        let saved: serde_json::Value = serde_json::from_slice(&read_bounded(&record, 4096)?)?;
        ensure!(saved == owner, "private adoption directory owner differs");
        let mut entries = 0;
        require_no_links(&private, 0, &mut entries)?;
        Ok(true)
    })();
    let owned = match checked {
        Ok(owned) => owned,
        Err(error) => {
            settlement.left(serde_json::json!({"path":private,
                "reason":format!("private adoption directory kept ({error:#})")}));
            return Ok(());
        }
    };
    let removed = (|| -> Result<()> {
        if owned {
            // The owner record goes last, so an interrupted removal stays provable.
            for entry in fs::read_dir(&private)? {
                let path = entry?.path();
                if path.file_name() == Some(std::ffi::OsStr::new("owner.json")) {
                    continue;
                }
                if fs::symlink_metadata(&path)?.is_dir() {
                    fs::remove_dir_all(&path)?;
                } else {
                    fs::remove_file(&path)?;
                }
            }
            sync_directory(&private)?;
            fs::remove_file(private.join("owner.json"))?;
        }
        fs::remove_dir(&private)?;
        sync_directory(&state)
    })();
    match removed {
        Ok(()) => settlement.deleted(
            &private,
            "private archive copies of original saved indexes removed; the originals are untouched",
        ),
        // Private copies never block the installer flow; a later settlement
        // resumes the removal while the owner record remains.
        Err(error) => settlement.left(serde_json::json!({"path":private,
            "reason":format!("private adoption directory removal is incomplete ({error:#})")})),
    }
    Ok(())
}

fn require_plain_directory(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        metadata.is_dir() && !metadata.file_type().is_symlink(),
        "not a plain directory: {}",
        path.display()
    );
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        ensure!(
            metadata.file_attributes() & 0x400 == 0,
            "reparse point: {}",
            path.display()
        );
    }
    Ok(())
}

fn require_no_links(directory: &Path, depth: usize, entries: &mut usize) -> Result<()> {
    ensure!(
        depth < MAX_ARCHIVE_DEPTH,
        "private adoption directory is too deep"
    );
    for entry in fs::read_dir(directory)? {
        let path = entry?.path();
        *entries += 1;
        ensure!(
            *entries <= MAX_ARCHIVE_ENTRIES,
            "private adoption directory has too many entries"
        );
        let metadata = fs::symlink_metadata(&path)?;
        ensure!(
            !metadata.file_type().is_symlink(),
            "symbolic link: {}",
            path.display()
        );
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            ensure!(
                metadata.file_attributes() & 0x400 == 0,
                "reparse point: {}",
                path.display()
            );
        }
        if metadata.is_dir() {
            require_no_links(&path, depth + 1, entries)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
