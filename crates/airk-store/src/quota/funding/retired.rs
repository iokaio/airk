// SPDX-License-Identifier: Apache-2.0
//! Disposal of fully acknowledged allocations already handed to ordinary Current.
//! Active/unacknowledged operation funding never gains retirement authority here.
use super::*;
use crate::store_fs::{SetupDirectory, SetupFileMode};
use std::ffi::{OsStr, OsString};

/// Whole-root reset validates every participant, including zero-credit pending
/// work, against the complete original registry. No state conversion or recovery
/// is implied. The reset owner separately excludes all native holders and proves
/// complete logical collection absence before it disposes this exact inventory.
pub(in crate::quota) fn clean_break_inventory(
    control: &SetupDirectory,
    root: &SetupDirectory,
    logical_root: &Path,
    registrations: &[ProjectRegistration],
    namespace: Incarnation,
) -> Result<super::super::clean_break::CleanBreakInventory> {
    let directory = control.open_dir(OsStr::new(DIRECTORY))?;
    ensure!(
        directory.identity()?.0 == root.identity()?.0,
        "funding cleanup crosses filesystem"
    );
    let mut result = super::super::clean_break::CleanBreakInventory::default();
    let mut leases_seen = std::collections::BTreeSet::new();
    let mut reservation_children = std::collections::BTreeSet::new();
    let mut open_reservation_children = std::collections::BTreeSet::new();
    result.directory(&[".quota-1.4", DIRECTORY]);
    for (count, name) in directory.entries()?.enumerate() {
        ensure!(
            count < MAX_OPERATIONS * 2 + 1,
            "funding cleanup inventory exceeds bound"
        );
        let name = name?;
        let text = name.to_str().context("invalid funding name")?;
        if text == "leases" {
            let leases = directory.open_dir(&name)?;
            ensure!(
                leases.identity()?.0 == root.identity()?.0,
                "funding leases cross filesystem"
            );
            result.directory(&[".quota-1.4", DIRECTORY, text]);
            for (count, name) in leases.entries()?.enumerate() {
                ensure!(count < MAX_RESERVATIONS * 2, "funding cleanup lease limit");
                let name = name?;
                validate_lease_name(&name)?;
                let lease: Sha256Digest = name
                    .to_str()
                    .context("funding lease UTF-8")?
                    .strip_prefix("l-")
                    .and_then(|name| name.strip_suffix(".lease"))
                    .context("invalid funding lease identity")?
                    .parse()?;
                leases_seen.insert(lease);
                ensure!(
                    leases
                        .open_file(&name, SetupFileMode::Read)?
                        .metadata()?
                        .len()
                        == 0,
                    "unknown funding lease contents"
                );
                result.file(
                    &[
                        ".quota-1.4",
                        DIRECTORY,
                        text,
                        name.to_str().context("funding lease UTF-8")?,
                    ],
                    0,
                    true,
                );
            }
            continue;
        }
        let bytes = directory.read_file(&name, MAX_RECORD_BYTES)?;
        let record: FundingReceipt = serde_json::from_slice(&bytes)?;
        record.validate()?;
        ensure!(
            text == record.filename()
                && record.request.namespace == namespace
                && record.data_root == logical_root
                && record.root_identity == pinned_identity(root, logical_root, namespace)?,
            "foreign clean-break funding identity"
        );
        ensure!(
            registrations.contains(&record.request.original)
                && registrations.contains(&record.request.destination),
            "foreign clean-break funding request participant"
        );
        for part in &record.participants {
            let registration = record
                .request
                .registration(part.index_id)
                .context("missing funding participant")?;
            ensure!(
                registrations.contains(registration),
                "foreign clean-break funding participant"
            );
        }
        for child in &record.children {
            if matches!(child.kind, FundingChildKind::Reservation { .. }) {
                ensure!(
                    reservation_children.insert(child.id),
                    "duplicate funding reservation authority"
                );
                if !child.closed {
                    open_reservation_children.insert(child.id);
                }
                if let FundingChildKind::Reservation { staging_name } = &child.kind {
                    let registration = record
                        .request
                        .registration(child.index_id)
                        .context("funded staging registration missing")?;
                    ensure!(
                        Path::new(&registration.data_root) == logical_root,
                        "funded staging crosses data root"
                    );
                    let quota = Quota {
                        root: logical_root.to_path_buf(),
                        namespace: Namespace::Service,
                        funding: None,
                    };
                    let stage = staging_path(
                        &quota,
                        &registration.collection,
                        &logical_root
                            .join(".quota-1.4/reservations")
                            .join(staging_name),
                    )?;
                    result
                        .staging
                        .entry(registration.binding)
                        .or_default()
                        .insert(stage);
                }
            }
        }
        result.file(&[".quota-1.4", DIRECTORY, text], MAX_RECORD_BYTES, false);
    }
    ensure!(
        leases_seen.is_subset(&reservation_children),
        "orphan or foreign funding lease is preserved"
    );
    ensure!(
        open_reservation_children.is_subset(&leases_seen),
        "open funding reservation is missing native lease custody"
    );
    Ok(result)
}

pub(super) fn participant(
    record: &FundingReceipt,
    registration: &ProjectRegistration,
) -> Result<Option<IndexId>> {
    let Some(part) = record
        .participants
        .iter()
        .find(|part| part.collection == registration.collection)
    else {
        return Ok(None);
    };
    ensure!(
        record.request.registration(part.index_id) == Some(registration)
            && record.complete
            && record.release_acknowledged
            && part.remaining_growth == 0,
        "retirement requires exact fully acknowledged ordinary Current ownership"
    );
    ensure!(
        record
            .children
            .iter()
            .filter(|child| child.index_id == part.index_id)
            .all(|child| child.current_owned
                && !child.closed
                && matches!(child.kind, FundingChildKind::Current { .. })),
        "retirement cannot adopt operation-owned or ordinary reservation children"
    );
    Ok(Some(part.index_id))
}
struct Row {
    name: OsString,
    owned: File,
    scratch: OsString,
    next: Option<Vec<u8>>,
}
/// Caller has exact native retired participant, positive whole-logical absence,
/// actual global writer exclusion and the existing lifecycle/ledger handles.
pub(in crate::quota) struct RetiredCurrentInventory {
    directory: SetupDirectory,
    root: PathBuf,
    rows: Vec<Row>,
}
impl RetiredCurrentInventory {
    /// Capture still validates all funding records. Only selected effects decide
    /// whether metadata-only retirement would need actual writer custody.
    pub(in crate::quota) fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    pub(in crate::quota) fn capture(
        control: &SetupDirectory,
        root: &SetupDirectory,
        logical_root: &Path,
        registration: &ProjectRegistration,
    ) -> Result<Option<Self>> {
        // Validate ALL known funding metadata before any ordinary/funded effect.
        // Zero-credit live or unacknowledged participants still refuse.
        pinned_records_checked(control, root, logical_root, Some(registration))?;
        let directory = match control.open_dir(OsStr::new(DIRECTORY)) {
            Ok(directory) => directory,
            Err(error) if absent(&error) => return Ok(None),
            Err(error) => return Err(error),
        };
        let mut rows = Vec::new();
        for (visited, entry) in directory.entries()?.enumerate() {
            ensure!(
                visited < MAX_OPERATIONS * 2 + 1,
                "excessive retired Current funding inventory"
            );
            let name = entry?;
            let text = name.to_str().context("funding name UTF-8")?;
            if text == "leases" || text.starts_with("funding-") {
                continue;
            }
            let owned = directory.open_file(&name, SetupFileMode::Read)?;
            let mut record: FundingReceipt =
                serde_json::from_slice(&directory.read_file(&name, MAX_RECORD_BYTES)?)?;
            record.validate()?;
            ensure!(
                text == record.filename()
                    && record.data_root == logical_root
                    && record.root_identity
                        == pinned_identity(root, logical_root, record.request.namespace)?,
                "retired Current funding identity changed"
            );
            let Some(index) = participant(&record, registration)? else {
                continue;
            };
            if !record.children.iter().any(|child| child.index_id == index) {
                continue;
            }
            let scratch = OsString::from(format!("funding-retire-{}", record.identity()?));
            record.children.retain(|child| child.index_id != index);
            advance(&mut record)?;
            record.validate()?;
            let next = if record.children.is_empty() {
                None
            } else {
                Some(serde_json::to_vec(&record)?)
            };
            if let Some(bytes) = &next {
                ensure!(
                    bytes.len() as u64 <= MAX_RECORD_BYTES,
                    "retired Current metadata exceeds bound"
                );
            }
            // Validate the exact bounded plain scratch before ANY grant effects,
            // including final-parent disposal. No prefix-wide deletion is allowed.
            match directory.read_file(&scratch, MAX_RECORD_BYTES) {
                Ok(_) => (),
                Err(error) if absent(&error) => (),
                Err(error) => return Err(error),
            }
            rows.push(Row {
                name,
                owned,
                scratch,
                next,
            });
        }
        Ok(Some(Self {
            directory,
            root: logical_root.to_path_buf(),
            rows,
        }))
    }
    pub(in crate::quota) fn apply(&self) -> Result<()> {
        self.apply_with_hook(|_| Ok(()))
    }
    fn apply_with_hook(
        &self,
        mut checkpoint: impl FnMut(PersistPoint) -> Result<()>,
    ) -> Result<()> {
        for row in &self.rows {
            if let Some(bytes) = &row.next {
                // Same bounded ledger metadata capacity rule as ordinary
                // release_current; no source, model or new operation allocation.
                crate::capacity::require_growth(&self.root, MAX_RECORD_BYTES)?;
                let mut pending = match self
                    .directory
                    .open_file(&row.scratch, SetupFileMode::ReadWrite)
                {
                    Ok(file) => file,
                    Err(error) if absent(&error) => self
                        .directory
                        .open_file(&row.scratch, SetupFileMode::CreateNew)?,
                    Err(error) => return Err(error),
                };
                pending.set_len(0)?;
                let middle = bytes.len() / 2;
                pending.write_all(&bytes[..middle])?;
                checkpoint(PersistPoint::PendingPrefix)?;
                pending.write_all(&bytes[middle..])?;
                pending.sync_all()?;
                checkpoint(PersistPoint::PendingSynced)?;
                self.directory
                    .replace_file(&row.scratch, &pending, &row.name, &row.owned)?;
            } else {
                // Forget the operation only after its one exact immutable scratch
                // is gone and that unlink is durable. An interrupted write must
                // never outlive the parent authority needed to reclaim it.
                match self.directory.open_file(&row.scratch, SetupFileMode::Read) {
                    Ok(pending) => {
                        ensure!(
                            pending.metadata()?.len() <= MAX_RECORD_BYTES,
                            "oversized retired Current scratch"
                        );
                        self.directory
                            .unlink_file_if_identity(&row.scratch, &pending)?;
                        self.directory.sync()?;
                    }
                    Err(error) if absent(&error) => (),
                    Err(error) => return Err(error),
                }
                // Native catalog retains operation/release history, exactly as
                // ordinary release_current forgetting its final acknowledged child.
                checkpoint(PersistPoint::BeforeParentUnlink)?;
                self.directory
                    .unlink_file_if_identity(&row.name, &row.owned)?;
            }
        }
        self.directory.sync()
    }
}
fn absent(error: &anyhow::Error) -> bool {
    error
        .downcast_ref::<io::Error>()
        .is_some_and(|error| error.kind() == io::ErrorKind::NotFound)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PersistPoint {
    PendingPrefix,
    PendingSynced,
    BeforeParentUnlink,
}

#[cfg(test)]
mod persistence_tests {
    use super::*;
    // Persistence-only custody test. It deliberately does not manufacture a
    // terminal FundingReceipt or assert native release authority: the separate
    // real shared-root provider-cutover retirement fixture covers admission.
    fn inventory(path: &Path, next: Option<&[u8]>) -> Result<RetiredCurrentInventory> {
        let directory = SetupDirectory::open_canonical(path)?;
        let name = OsString::from("f-owned.json");
        let owned = directory.open_file(&name, SetupFileMode::Read)?;
        Ok(RetiredCurrentInventory {
            directory,
            root: path.to_path_buf(),
            rows: vec![Row {
                name,
                owned,
                scratch: OsString::from("funding-retire-immutable-allocation"),
                next: next.map(Vec::from),
            }],
        })
    }
    #[test]
    fn interrupted_custody_reuses_one_scratch_across_payload_revisions_and_final_forget()
    -> Result<()> {
        let temp = tempfile::tempdir()?;
        let root = temp.path().canonicalize()?;
        fs::write(root.join("f-owned.json"), b"original-authority")?;
        fs::write(root.join("unrelated-owned-row"), b"preserved")?;
        let first = inventory(&root, Some(b"revision-one-after-A"))?;
        assert!(
            first
                .apply_with_hook(|point| {
                    if point == PersistPoint::PendingPrefix {
                        anyhow::bail!("injected interrupted write")
                    }
                    Ok(())
                })
                .is_err()
        );
        drop(first);
        assert_eq!(fs::read(root.join("f-owned.json"))?, b"original-authority");
        assert!(root.join("funding-retire-immutable-allocation").exists());
        // Model the intervening authority revision at this persistence boundary;
        // this is not a claim of a real native B Current resize in this unit.
        fs::write(root.join("f-owned.json"), b"revision-two-B-progress")?;
        let second = inventory(&root, Some(b"revision-three-after-A"))?;
        second.apply()?;
        drop(second);
        assert_eq!(
            fs::read(root.join("f-owned.json"))?,
            b"revision-three-after-A"
        );
        assert!(!root.join("funding-retire-immutable-allocation").exists());
        // Crash before replacing the final metadata leaves scratch plus parent.
        let partial = inventory(&root, Some(b"partial-final-metadata"))?;
        assert!(
            partial
                .apply_with_hook(|point| {
                    if point == PersistPoint::PendingPrefix {
                        anyhow::bail!("injected final interruption")
                    }
                    Ok(())
                })
                .is_err()
        );
        drop(partial);
        let last = inventory(&root, None)?;
        assert!(
            last.apply_with_hook(|point| {
                if point == PersistPoint::BeforeParentUnlink {
                    anyhow::bail!("injected after scratch unlink")
                }
                Ok(())
            })
            .is_err()
        );
        drop(last);
        assert!(!root.join("funding-retire-immutable-allocation").exists());
        assert!(root.join("f-owned.json").exists());
        let retry = inventory(&root, None)?;
        retry.apply()?;
        drop(retry);
        assert!(!root.join("f-owned.json").exists());
        assert_eq!(fs::read(root.join("unrelated-owned-row"))?, b"preserved");
        Ok(())
    }
}
