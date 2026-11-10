// SPDX-License-Identifier: Apache-2.0
//! Exact ordinary-credit disposal for an already retired, wholly absent collection.
//! No opener here creates a directory, advisory, lifecycle, ledger or lease.
use super::*;
use crate::service::ProjectLease;
use crate::store_fs::{SetupDirectory, SetupFileMode, file_identity};
use airk_core::identity::Incarnation;
use airk_core::service::ProjectRegistration;
use std::ffi::{OsStr, OsString};

/// Internal native custody, not serde/Clone or a caller-provided absence flag.
/// The registry keeps this through its exact retired cleanup transaction.
pub(crate) struct AbsentCollectionSettlement<'a> {
    project: &'a ProjectLease,
    binding: airk_core::service::ProjectBinding,
    control_path: PathBuf,
    root_path: PathBuf,
    collection: String,
    root: SetupDirectory,
    namespace: Option<SetupDirectory>,
    locks: Option<SetupDirectory>,
    control: SetupDirectory,
    bound_owner: File,
    bound_owner_bytes: Vec<u8>,
    lifecycle: File,
    advisory: Option<File>,
    ledger: File,
}
impl<'a> AbsentCollectionSettlement<'a> {
    pub(crate) fn settle(
        registration: &ProjectRegistration,
        lease: &'a ProjectLease,
        control: &Path,
        namespace: Incarnation,
    ) -> Result<Self> {
        Self::settle_with_hook(registration, lease, control, namespace, |_| Ok(()))
    }
    fn settle_with_hook(
        registration: &ProjectRegistration,
        lease: &'a ProjectLease,
        control: &Path,
        namespace: Incarnation,
        mut checkpoint: impl FnMut(SettlementPoint) -> Result<()>,
    ) -> Result<Self> {
        ensure!(
            lease.matches_registration(control, registration.binding),
            "foreign retired collection lease"
        );
        registration.validate()?;
        crate::validate_collection(&registration.collection)?;
        let root_path = PathBuf::from(&registration.data_root);
        ensure_current_root(&root_path)?;
        let root = SetupDirectory::open_canonical(&root_path)?;
        let bound_owner = root.open_file(
            OsStr::new(crate::service::data_scope::OWNER),
            SetupFileMode::Read,
        )?;
        let bound_owner_bytes =
            root.read_file(OsStr::new(crate::service::data_scope::OWNER), 16384)?;
        ensure!(
            crate::service::data_scope::has_saved_binding(&root_path, control, namespace)?,
            "absent collection requires actual saved native data-root binding"
        );
        let lifecycle = root.open_file(
            OsStr::new(Namespace::Service.lifecycle()),
            SetupFileMode::ReadWrite,
        )?;
        ensure!(lifecycle.metadata()?.len() == 0, "unknown lifecycle owner");
        lifecycle.try_lock_shared()?;
        ensure_current_root(&root_path)?;
        let collections = match root.open_dir(OsStr::new(Namespace::Service.collections())) {
            Ok(directory) => Some(directory),
            Err(error) if absent(&error) => None,
            Err(error) => return Err(error),
        };
        let locks = match &collections {
            Some(collections) => match collections.open_dir(OsStr::new(".writer-locks")) {
                Ok(directory) => Some(directory),
                Err(error) if absent(&error) => None,
                Err(error) => return Err(error),
            },
            None => None,
        };
        let advisory = match &locks {
            Some(locks) => match locks.open_file(
                OsStr::new(&registration.collection),
                SetupFileMode::ReadWrite,
            ) {
                Ok(advisory) => {
                    ensure!(
                        advisory.metadata()?.len() == 0,
                        "unknown collection advisory"
                    );
                    advisory.try_lock()?;
                    Some(advisory)
                }
                Err(error) if absent(&error) => None,
                Err(error) => return Err(error),
            },
            None => None,
        };
        let ledger_control = root.open_dir(OsStr::new(Namespace::Service.control()))?;
        let ledger =
            ledger_control.open_file(OsStr::new("ledger.lock"), SetupFileMode::ReadWrite)?;
        ensure!(ledger.metadata()?.len() == 0, "unknown quota ledger lock");
        ledger.try_lock()?;
        let proof = Self {
            project: lease,
            binding: registration.binding,
            control_path: control.to_path_buf(),
            root_path,
            collection: registration.collection.clone(),
            root,
            namespace: collections,
            locks,
            control: ledger_control,
            bound_owner,
            bound_owner_bytes,
            lifecycle,
            advisory,
            ledger,
        };
        proof.revalidate()?;
        for (visited, entry) in proof.control.entries()?.enumerate() {
            ensure!(
                visited < MAX_RESERVATIONS * 2 + 8,
                "excessive quota control inventory"
            );
            let name = entry?;
            let text = name.to_str().context("invalid quota control name")?;
            if text.starts_with("policy-") && text.len() <= 128 {
                ensure!(
                    proof
                        .control
                        .open_file(&name, SetupFileMode::Read)?
                        .metadata()?
                        .len()
                        <= POLICY_MAX,
                    "oversized pending quota policy"
                );
            } else {
                ensure!(
                    [
                        "ledger.lock",
                        "policy.json",
                        "reservations",
                        "current-grants",
                        "split-allocations"
                    ]
                    .contains(&text),
                    "unknown quota control ownership"
                );
                if text == "policy.json" {
                    let policy: QuotaPolicy =
                        serde_json::from_slice(&proof.control.read_file(&name, POLICY_MAX)?)?;
                    policy.validate()?;
                }
            }
        }
        // Operation funding refuses; fully acknowledged Current-owned children
        // are ordinary growth credit and have a separate exact disposal plan.
        let terminal_current = funding::RetiredCurrentInventory::capture(
            &proof.control,
            &proof.root,
            &proof.root_path,
            registration,
        )?;
        let reservations = proof.control.open_dir(OsStr::new("reservations"))?;
        let mut ordinary = Vec::new();
        for (visited, entry) in reservations.entries()?.enumerate() {
            ensure!(
                visited < MAX_RESERVATIONS * 2 + 2,
                "excessive reservation inventory"
            );
            let name = entry?;
            let text = name.to_str().context("invalid reservation name")?;
            ensure!(
                text.starts_with("r-")
                    && text.len() <= 128
                    && text
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.'),
                "unknown reservation inventory entry"
            );
            let row = reservations.open_file(&name, SetupFileMode::Read)?;
            if let Some(stem) = text.strip_suffix(".lease") {
                ensure!(!stem.contains('.'), "noncanonical reservation lease name");
                ensure!(
                    row.metadata()?.len() == 0,
                    "unknown reservation lease contents"
                );
                continue;
            }
            ensure!(
                !text.contains('.') && row.metadata()?.len() <= RECORD_MAX,
                "invalid reservation record"
            );
            let mut bytes = Vec::new();
            row.try_clone()?
                .take(RECORD_MAX + 1)
                .read_to_end(&mut bytes)?;
            ensure!(bytes.len() as u64 <= RECORD_MAX, "reservation record grew");
            let record: Record = serde_json::from_slice(&bytes)?;
            ensure!(record.bytes > 0, "invalid reservation credit");
            if !record.collection.is_empty() {
                crate::validate_collection(&record.collection)?;
            }
            if record.collection != proof.collection {
                continue;
            }
            let lease_name = OsString::from(format!("{text}.lease"));
            let held = reservations.open_file(&lease_name, SetupFileMode::ReadWrite)?;
            ensure!(
                held.metadata()?.len() == 0,
                "unknown selected reservation lease"
            );
            held.try_lock()
                .context("retired collection still has a live ordinary reservation")?;
            ordinary.push((name, row, lease_name, held));
        }
        let grants = match proof.control.open_dir(OsStr::new("current-grants")) {
            Ok(directory) => {
                let rows = current::retired_inventory(&directory, &proof.collection)?;
                Some((directory, rows))
            }
            Err(error) if absent(&error) => None,
            Err(error) => return Err(error),
        };
        // An activated sibling may own this shared root before this metadata-only
        // registration ever opened a writer. Missing advisory authorizes NO
        // selected-credit disposal, even if someone removed an old advisory.
        ensure!(
            proof.advisory.is_some()
                || (terminal_current.as_ref().is_none_or(|plan| plan.is_empty())
                    && ordinary.is_empty()
                    && grants.as_ref().is_none_or(|(_, rows)| rows.is_empty())),
            "selected retired credit requires its actual collection advisory"
        );
        // Every target lease and row is held and every inventory validated before
        // the first effect. Complete logical absence proves ALL selected current
        // journals, database payloads and stages disposed; never synthesize Ready.
        proof.revalidate()?;
        checkpoint(SettlementPoint::BeforeUnlink)?;
        proof.revalidate()?;
        if let Some(plan) = &terminal_current {
            plan.apply()?;
        }
        if let Some((directory, rows)) = &grants {
            for (name, row) in rows {
                directory.unlink_file_if_identity(name, row)?;
                checkpoint(SettlementPoint::AfterGrant)?;
            }
            directory.sync()?;
        }
        for (name, row, lease_name, held) in &ordinary {
            reservations.unlink_file_if_identity(name, row)?;
            checkpoint(SettlementPoint::AfterReservationRecord)?;
            reservations.unlink_file_if_identity(lease_name, held)?;
        }
        reservations.sync()?;
        proof.revalidate()?;
        Ok(proof)
    }
    pub(crate) fn revalidate(&self) -> Result<()> {
        ensure!(
            self.project
                .matches_registration(&self.control_path, self.binding),
            "retired project custody changed"
        );
        ensure_current_root(&self.root_path)?;
        ensure!(
            self.root.identity()?.0 == self.control.identity()?.0,
            "retired quota control crosses filesystem"
        );
        ensure!(
            SetupDirectory::open_canonical(&self.root_path)?.identity()? == self.root.identity()?
                && self
                    .root
                    .open_dir(OsStr::new(Namespace::Service.control()))?
                    .identity()?
                    == self.control.identity()?,
            "retired quota directory changed"
        );
        match &self.namespace {
            Some(namespace) => {
                ensure!(
                    self.root
                        .open_dir(OsStr::new(Namespace::Service.collections()))?
                        .identity()?
                        == namespace.identity()?
                        && namespace.identity()?.0 == self.root.identity()?.0,
                    "retired collection namespace changed"
                );
                match &self.locks {
                    Some(locks) => ensure!(
                        namespace
                            .open_dir(OsStr::new(".writer-locks"))?
                            .identity()?
                            == locks.identity()?
                            && locks.identity()?.0 == self.root.identity()?.0,
                        "retired writer-lock namespace changed"
                    ),
                    None => require_absent_entry(namespace, OsStr::new(".writer-locks"))?,
                }
            }
            None => require_absent_entry(&self.root, OsStr::new(Namespace::Service.collections()))?,
        }
        ensure!(
            self.root
                .read_file(OsStr::new(crate::service::data_scope::OWNER), 16384)?
                == self.bound_owner_bytes,
            "retired data-root binding changed"
        );
        for (directory, name, retained) in [
            (
                &self.root,
                crate::service::data_scope::OWNER,
                &self.bound_owner,
            ),
            (&self.root, Namespace::Service.lifecycle(), &self.lifecycle),
            (&self.control, "ledger.lock", &self.ledger),
        ] {
            ensure!(
                file_identity(&directory.open_file(OsStr::new(name), SetupFileMode::Read)?)?
                    == file_identity(retained)?,
                "retired quota owner changed"
            );
        }
        match (&self.locks, &self.advisory) {
            (Some(locks), Some(advisory)) => ensure!(
                file_identity(
                    &locks.open_file(OsStr::new(&self.collection), SetupFileMode::Read)?
                )? == file_identity(advisory)?,
                "retired collection advisory changed"
            ),
            (Some(locks), None) => require_absent_entry(locks, OsStr::new(&self.collection))?,
            (None, None) => (),
            (None, Some(_)) => anyhow::bail!("retired advisory lost its namespace"),
        }
        if let Some(namespace) = &self.namespace {
            require_absent_entry(namespace, OsStr::new(&self.collection))?;
        }
        Ok(())
    }
}
fn require_absent_entry(directory: &SetupDirectory, name: &OsStr) -> Result<()> {
    match directory.open_entry(name) {
        Err(error) if absent(&error) => Ok(()),
        Err(error) => Err(error),
        Ok(_) => anyhow::bail!("retired logical collection or advisory namespace appeared"),
    }
}
fn absent(error: &anyhow::Error) -> bool {
    error
        .downcast_ref::<io::Error>()
        .is_some_and(|error| error.kind() == io::ErrorKind::NotFound)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SettlementPoint {
    BeforeUnlink,
    AfterGrant,
    AfterReservationRecord,
}
#[cfg(test)]
mod tests;
