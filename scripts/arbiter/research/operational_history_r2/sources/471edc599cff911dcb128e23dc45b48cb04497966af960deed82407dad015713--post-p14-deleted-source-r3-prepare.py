# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import shutil
root=Path(r'D:/code/Github/airk');old=root/'artifacts/v190/post-p14-deleted-source-r2-proposed';out=root/'artifacts/v190/post-p14-deleted-source-r3-proposed'
shutil.copytree(old,out)
rel='crates/airk-store/src/quota.rs';p=out/'base'/rel;p.parent.mkdir(parents=True,exist_ok=True);p.write_bytes((root/rel).read_bytes());s=(root/rel).read_text(encoding='utf-8');s=s.replace('mod current;','mod current;\nmod retired;\npub(crate) use retired::AbsentCollectionSettlement;',1);p=out/'source'/rel;p.parent.mkdir(parents=True,exist_ok=True);p.write_text(s,encoding='utf-8',newline='\n')
rel='crates/airk-store/src/quota/current.rs';p=out/'base'/rel;p.parent.mkdir(parents=True,exist_ok=True);p.write_bytes((root/rel).read_bytes());s=(root/rel).read_text(encoding='utf-8')
s+='''
// Caller retains the actual ledger lock and positive complete-collection
// absence proof. Inventory is read-only; no recovery of another collection.
pub(super) fn retired_inventory(
    directory: &crate::service::registry::catalog::setup::storage::SetupDirectory,
    collection: &str,
) -> Result<Vec<(std::ffi::OsString, File)>> {
    use crate::service::registry::catalog::setup::storage::SetupFileMode;
    let mut selected = Vec::new();
    for (visited, entry) in directory.entries()?.enumerate() {
        ensure!(visited < MAX_RESERVATIONS * 2 + 2, "excessive current quota grant state");
        let name = entry?;
        let text = name.to_str().context("invalid current quota grant name")?;
        let file = directory.open_file(&name, SetupFileMode::Read)?;
        ensure!(file.metadata()?.len() <= RECORD_MAX, "oversized current quota grant");
        if text.starts_with("grant-") && text.len() <= 128 {
            // Unpublished atomic-write scratch cannot own a transaction.
            // Its collection is not authenticated; preserve it without deletion.
            continue;
        }
        let mut bytes = Vec::new();
        file.try_clone()?.take(RECORD_MAX + 1).read_to_end(&mut bytes)?;
        let record = decode_record(Some(&name), &bytes)?;
        if record.scope.collection == collection { selected.push((name, file)); }
    }
    Ok(selected)
}
'''
p=out/'source'/rel;p.parent.mkdir(parents=True,exist_ok=True);p.write_text(s,encoding='utf-8',newline='\n')
rel='crates/airk-store/src/quota/retired.rs';p=out/'source'/rel;p.parent.mkdir(parents=True,exist_ok=True)
p.write_text('''// SPDX-License-Identifier: Apache-2.0
//! Exact ordinary-credit disposal for an already retired, wholly absent collection.
//! No opener here creates a directory, advisory, lifecycle, ledger or lease.
use super::*;
use crate::service::registry::catalog::setup::storage::{SetupDirectory, SetupFileMode, file_identity};
use crate::service::ProjectLease;
use airk_core::identity::Incarnation;
use airk_core::service::ProjectRegistration;
use std::ffi::{OsStr, OsString};

/// Internal native custody, not serde/Clone or a caller-provided absence flag.
/// The registry keeps this through its exact retired cleanup transaction.
pub(crate) struct AbsentCollectionSettlement {
    root_path: PathBuf,
    collection: String,
    root: SetupDirectory,
    namespace: SetupDirectory,
    locks: SetupDirectory,
    control: SetupDirectory,
    lifecycle: File,
    advisory: File,
    ledger: File,
}
impl AbsentCollectionSettlement {
    pub(crate) fn settle(
        registration: &ProjectRegistration,
        lease: &ProjectLease,
        control: &Path,
        namespace: Incarnation,
    ) -> Result<Self> {
        Self::settle_with_hook(registration, lease, control, namespace, || Ok(()))
    }
    fn settle_with_hook(
        registration: &ProjectRegistration,
        lease: &ProjectLease,
        control: &Path,
        namespace: Incarnation,
        before_unlink: impl FnOnce() -> Result<()>,
    ) -> Result<Self> {
        ensure!(lease.identity().0 == registration.binding && lease.root == control,
            "foreign retired collection lease");
        crate::validate_collection(&registration.collection)?;
        let root_path = PathBuf::from(&registration.data_root);
        ensure_current_root(&root_path)?;
        let root = SetupDirectory::open_canonical(&root_path)?;
        crate::service::data_scope::check(&root_path, control, namespace)?;
        let lifecycle = root.open_file(OsStr::new(Namespace::Service.lifecycle()), SetupFileMode::ReadWrite)?;
        ensure!(lifecycle.metadata()?.len() == 0, "unknown lifecycle owner");
        lifecycle.try_lock_shared()?;
        ensure_current_root(&root_path)?;
        let collections = root.open_dir(OsStr::new(Namespace::Service.collections()))?;
        let locks = collections.open_dir(OsStr::new(".writer-locks"))?;
        let advisory = locks.open_file(OsStr::new(&registration.collection), SetupFileMode::ReadWrite)?;
        ensure!(advisory.metadata()?.len() == 0, "unknown collection advisory");
        advisory.try_lock()?;
        let ledger_control = root.open_dir(OsStr::new(Namespace::Service.control()))?;
        let ledger = ledger_control.open_file(OsStr::new("ledger.lock"), SetupFileMode::ReadWrite)?;
        ensure!(ledger.metadata()?.len() == 0, "unknown quota ledger lock");
        ledger.try_lock()?;
        let proof = Self { root_path, collection: registration.collection.clone(), root,
            namespace: collections, locks, control: ledger_control, lifecycle, advisory, ledger };
        proof.revalidate()?;
        // Funding has separate operation/participant custody and native terminal
        // authorization. Generic retirement cannot adopt or release any of it.
        ensure!(!funding::pinned_records(&proof.control, &proof.root, &proof.root_path)?
            .iter().any(|row| row.record.collection == proof.collection),
            "funded allocation requires exact split recovery, not generic retirement");
        let reservations = proof.control.open_dir(OsStr::new("reservations"))?;
        let mut ordinary = Vec::new();
        for (visited, entry) in reservations.entries()?.enumerate() {
            ensure!(visited < MAX_RESERVATIONS * 2 + 2, "excessive reservation inventory");
            let name = entry?;
            let text = name.to_str().context("invalid reservation name")?;
            ensure!(text.starts_with("r-") && text.len() <= 128
                && text.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.'),
                "unknown reservation inventory entry");
            let row = reservations.open_file(&name, SetupFileMode::Read)?;
            if text.ends_with(".lease") {
                ensure!(row.metadata()?.len() == 0, "unknown reservation lease contents");
                continue;
            }
            ensure!(!text.contains('.') && row.metadata()?.len() <= RECORD_MAX,
                "invalid reservation record");
            let mut bytes = Vec::new();
            row.try_clone()?.take(RECORD_MAX + 1).read_to_end(&mut bytes)?;
            ensure!(bytes.len() as u64 <= RECORD_MAX, "reservation record grew");
            let record: Record = serde_json::from_slice(&bytes)?;
            ensure!(record.bytes > 0, "invalid reservation credit");
            if !record.collection.is_empty() { crate::validate_collection(&record.collection)?; }
            if record.collection != proof.collection { continue; }
            let lease_name = OsString::from(format!("{text}.lease"));
            let held = reservations.open_file(&lease_name, SetupFileMode::ReadWrite)?;
            ensure!(held.metadata()?.len() == 0, "unknown selected reservation lease");
            held.try_lock().context("retired collection still has a live ordinary reservation")?;
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
        // Every target lease and row is held and every inventory validated before
        // the first effect. Complete logical absence proves ALL selected current
        // journals, database payloads and stages disposed; never synthesize Ready.
        proof.revalidate()?;
        before_unlink()?;
        proof.revalidate()?;
        if let Some((directory, rows)) = &grants {
            for (name, row) in rows { directory.unlink_file_if_identity(name, row)?; }
            directory.sync()?;
        }
        for (name, row, lease_name, held) in &ordinary {
            reservations.unlink_file_if_identity(name, row)?;
            reservations.unlink_file_if_identity(lease_name, held)?;
        }
        reservations.sync()?;
        proof.revalidate()?;
        Ok(proof)
    }
    pub(crate) fn revalidate(&self) -> Result<()> {
        ensure_current_root(&self.root_path)?;
        ensure!(SetupDirectory::open_canonical(&self.root_path)?.identity()? == self.root.identity()?
            && self.root.open_dir(OsStr::new(Namespace::Service.collections()))?.identity()? == self.namespace.identity()?
            && self.root.open_dir(OsStr::new(Namespace::Service.control()))?.identity()? == self.control.identity()?
            && self.namespace.open_dir(OsStr::new(".writer-locks"))?.identity()? == self.locks.identity()?,
            "retired quota directory changed");
        for (directory, name, retained) in [
            (&self.root, Namespace::Service.lifecycle(), &self.lifecycle),
            (&self.locks, self.collection.as_str(), &self.advisory),
            (&self.control, "ledger.lock", &self.ledger),
        ] {
            ensure!(file_identity(&directory.open_file(OsStr::new(name), SetupFileMode::Read)?)?
                == file_identity(retained)?, "retired quota owner changed");
        }
        match self.namespace.open_dir(OsStr::new(&self.collection)) {
            Err(error) if absent(&error) => Ok(()),
            Err(error) => Err(error),
            Ok(_) => anyhow::bail!("retired logical collection reappeared; no credit disposal"),
        }
    }
}
fn absent(error: &anyhow::Error) -> bool {
    error.downcast_ref::<io::Error>().is_some_and(|error| error.kind() == io::ErrorKind::NotFound)
}
''',encoding='utf-8',newline='\n')
# Integrate only into exact retired cleanup and retain native proof through metadata CAS.
p=out/'source/crates/airk-store/src/service_project_removal.rs';s=p.read_text(encoding='utf-8');needle='''        if absent_data.is_none() && (present || policy.is_some()) {
''';assert needle in s
s=s.replace(needle,'''        let absent_collection = if !present && absent_data.is_none() {
            Some(crate::quota::AbsentCollectionSettlement::settle(
                &registration, &lease, &self.root, self.incarnation()?,
            )?)
        } else { None };
        if present {
''',1)
needle='''        if let Some(absent) = &absent_data {
            absent.revalidate()?;
        }
''';assert s.count(needle)==2
s=s.replace(needle,needle+'''        if let Some(absent) = &absent_collection {
            absent.revalidate()?;
        }
''');p.write_text(s,encoding='utf-8',newline='\n')
print('prepared r3 source overlay only')
