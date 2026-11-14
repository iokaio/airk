// SPDX-License-Identifier: Apache-2.0
//! Separate opt-in state under the actual service lease. No index format changes.
use super::ServiceLease;
use crate::store_fs::{SetupDirectory, SetupFileMode};
use airk_core::{
    arbiter::Configuration,
    identity::{Incarnation, Sha256Digest},
};
use anyhow::{Context, Result, ensure};
use std::{
    ffi::OsStr,
    fs::File,
    io::{Read, Write},
    sync::{Arc, Mutex},
};

const DIRECTORY: &str = "arbiter-v2";
const CONFIGURATION: &str = "configuration.json";

/// The native service adapter must establish its existing owner-private control
/// boundary before construction, exactly as for ServiceModelFiles. No client
/// path, enrollment name or serialized service-role flag supplies that authority.
pub struct ConfigurationStore {
    root: SetupDirectory,
    installation: Sha256Digest,
    catalog: Incarnation,
    writer: Mutex<()>,
    _ownership: Arc<ServiceLease>,
}

fn missing(error: &anyhow::Error) -> bool {
    error
        .downcast_ref::<std::io::Error>()
        .is_some_and(|error| error.kind() == std::io::ErrorKind::NotFound)
}

impl ConfigurationStore {
    pub fn open(
        ownership: Arc<ServiceLease>,
        installation: Sha256Digest,
        catalog: Incarnation,
    ) -> Result<Self> {
        let root = SetupDirectory::open_canonical(ownership.root())?;
        Ok(Self {
            root,
            installation,
            catalog,
            writer: Mutex::new(()),
            _ownership: ownership,
        })
    }

    fn read(&self, directory: &SetupDirectory) -> Result<(Configuration, Option<File>)> {
        let file = match directory.open_file(OsStr::new(CONFIGURATION), SetupFileMode::Read) {
            Ok(file) => file,
            Err(error) if missing(&error) => {
                return Ok((
                    Configuration::disabled(self.installation, self.catalog),
                    None,
                ));
            }
            Err(error) => return Err(error),
        };
        ensure!(
            file.metadata()?.len() <= airk_core::arbiter::MAX_CONFIGURATION_BYTES as u64,
            "Arbiter configuration byte limit"
        );
        let mut bytes = Vec::new();
        (&file)
            .take(airk_core::arbiter::MAX_CONFIGURATION_BYTES as u64 + 1)
            .read_to_end(&mut bytes)?;
        let value = Configuration::decode(&bytes, self.installation, self.catalog)?;
        Ok((value, Some(file)))
    }

    /// Absence is disabled and performs no writes, directory creation or downloads.
    pub fn load(&self) -> Result<Configuration> {
        match self.root.open_dir(OsStr::new("arbiter-v1")) {
            Ok(_) => anyhow::bail!("Arbiter prototype configuration requires explicit migration"),
            Err(error) if missing(&error) => {}
            Err(error) => return Err(error),
        }
        match self.root.open_dir(OsStr::new(DIRECTORY)) {
            Ok(directory) => self.read(&directory).map(|(value, _)| value),
            Err(error) if missing(&error) => {
                Ok(Configuration::disabled(self.installation, self.catalog))
            }
            Err(error) => Err(error),
        }
    }

    /// Read-only final-delivery cut. The same writer and existing OS lock as
    /// apply_checked fence configuration publication through the synchronous
    /// effect. Admission never waits; caller cancellation/deadline is checked
    /// before and after bounded metadata I/O. Never await or acquire lifecycle
    /// locks inside effect. The owning native gate is acquired AFTER Store.
    pub fn with_current<T>(
        &self,
        live: impl Fn() -> Result<()>,
        effect: impl FnOnce(&Configuration) -> Result<T>,
    ) -> Result<T> {
        live()?;
        let _writer = self
            .writer
            .try_lock()
            .map_err(|_| anyhow::anyhow!("Arbiter delivery configuration busy"))?;
        live()?;
        match self.root.open_dir(OsStr::new("arbiter-v1")) {
            Ok(_) => anyhow::bail!("Arbiter prototype configuration requires explicit migration"),
            Err(error) if missing(&error) => {}
            Err(error) => return Err(error),
        }
        let directory = self.root.open_dir(OsStr::new(DIRECTORY))?;
        let lock =
            directory.open_file(OsStr::new("configuration.lock"), SetupFileMode::ReadWrite)?;
        ensure!(
            lock.metadata()?.len() == 0,
            "invalid Arbiter state ownership file"
        );
        lock.try_lock_shared()
            .context("Arbiter delivery configuration writer active")?;
        live()?;
        let (current, _) = self.read(&directory)?;
        live()?;
        effect(&current)
    }

    /// The caller validates management authority and index/catalog membership
    /// before this CAS. Store rechecks exact immutable installation/catalog IDs.
    pub fn apply(
        &self,
        expected_revision: u64,
        candidate: &Configuration,
    ) -> Result<Configuration> {
        self.apply_checked(expected_revision, candidate, || Ok(()))
    }
    /// The native management adapter supplies a pure liveness check. It runs
    /// after writer admission immediately before the first persisted effect.
    pub fn apply_checked(
        &self,
        expected_revision: u64,
        candidate: &Configuration,
        check: impl Fn() -> Result<()>,
    ) -> Result<Configuration> {
        candidate.validate()?;
        ensure!(
            candidate.installation == self.installation && candidate.catalog == self.catalog,
            "foreign Arbiter configuration"
        );
        ensure!(
            expected_revision.checked_add(1) == Some(candidate.revision),
            "Arbiter configuration revision must advance once"
        );
        let bytes = candidate.canonical_bytes()?;
        let _writer = self
            .writer
            .lock()
            .map_err(|_| anyhow::anyhow!("Arbiter state writer poisoned"))?;
        check()?;
        let directory = self.root.create_dir(OsStr::new(DIRECTORY))?;
        let lock_name = OsStr::new("configuration.lock");
        let lock = match directory.open_file(lock_name, SetupFileMode::CreateNew) {
            Ok(file) => file,
            Err(error)
                if error
                    .downcast_ref::<std::io::Error>()
                    .is_some_and(|error| error.kind() == std::io::ErrorKind::AlreadyExists) =>
            {
                directory.open_file(lock_name, SetupFileMode::ReadWrite)?
            }
            Err(error) => return Err(error),
        };
        ensure!(
            lock.metadata()?.len() == 0,
            "invalid Arbiter state ownership file"
        );
        lock.try_lock()
            .context("another Arbiter state writer owns this revision")?;
        let (current, prior) = self.read(&directory)?;
        ensure!(
            current.revision == expected_revision,
            "Arbiter configuration revision changed"
        );
        check()?;
        let stage_name = format!("configuration-{}.part", candidate.revision);
        let stage_name = OsStr::new(&stage_name);
        let mut stage = match directory.open_file(stage_name, SetupFileMode::CreateNew) {
            Ok(mut file) => {
                file.write_all(&bytes)?;
                file.sync_all()?;
                file
            }
            Err(error)
                if error
                    .downcast_ref::<std::io::Error>()
                    .is_some_and(|error| error.kind() == std::io::ErrorKind::AlreadyExists) =>
            {
                let file = directory.open_file(stage_name, SetupFileMode::Read)?;
                ensure!(
                    file.metadata()?.len() == bytes.len() as u64,
                    "conflicting Arbiter state staging file"
                );
                let mut retained = Vec::new();
                (&file)
                    .take(bytes.len() as u64 + 1)
                    .read_to_end(&mut retained)?;
                ensure!(retained == bytes, "conflicting Arbiter state staging bytes");
                file
            }
            Err(error) => return Err(error),
        };
        stage.flush()?;
        if let Some(prior) = prior {
            directory.replace_file(stage_name, &stage, OsStr::new(CONFIGURATION), &prior)?;
        } else {
            directory.publish_no_replace(
                stage_name,
                &stage,
                &directory,
                OsStr::new(CONFIGURATION),
            )?;
        }
        directory.sync()?;
        let (published, _) = self.read(&directory)?;
        ensure!(
            published == *candidate,
            "Arbiter published configuration changed"
        );
        Ok(published)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use airk_core::{
        arbiter::{Enrollment, Task},
        identity::Sha256Digest,
        index_catalog::IndexId,
    };
    use std::collections::BTreeSet;

    fn fixture() -> Result<(tempfile::TempDir, ConfigurationStore)> {
        let owner = tempfile::tempdir()?;
        let lease = Arc::new(ServiceLease::acquire(owner.path())?);
        let store = ConfigurationStore::open(
            lease,
            Sha256Digest::from_bytes([1; 32]),
            Incarnation::from_bytes([2; 16]),
        )?;
        Ok((owner, store))
    }
    fn enabled(store: &ConfigurationStore) -> Result<Configuration> {
        let mut value = store.load()?;
        value.revision += 1;
        value.enabled = true;
        value.selected_profile = Some(Sha256Digest::from_bytes([3; 32]));
        value.indexes = vec![Enrollment {
            index_id: IndexId::from_identity(Incarnation::from_bytes([4; 16])),
            tasks: BTreeSet::from([Task::EvidenceRelevance]),
        }];
        Ok(value)
    }

    #[test]
    fn disabled_load_does_not_create_optional_state() -> Result<()> {
        let (owner, store) = fixture()?;
        assert!(!store.load()?.enabled);
        assert!(!owner.path().join(DIRECTORY).exists());
        Ok(())
    }
    #[test]
    fn explicit_enrollment_cas_survives_reopen_and_stale_write_refuses() -> Result<()> {
        let (_owner, store) = fixture()?;
        let selected = enabled(&store)?;
        assert_eq!(store.apply(1, &selected)?, selected);
        assert_eq!(store.load()?, selected);
        assert!(store.apply(1, &selected).is_err());
        assert_eq!(store.load()?, selected);
        let other = ConfigurationStore::open(
            store._ownership.clone(),
            store.installation,
            Incarnation::from_bytes([9; 16]),
        )?;
        assert!(other.load().is_err());
        Ok(())
    }
    #[test]
    fn interrupted_conflicting_staging_is_preserved_without_publishing() -> Result<()> {
        let (owner, store) = fixture()?;
        std::fs::create_dir(owner.path().join(DIRECTORY))?;
        let stage = owner.path().join(DIRECTORY).join("configuration-2.part");
        std::fs::write(&stage, b"conflicting interrupted state")?;
        assert!(store.apply(1, &enabled(&store)?).is_err());
        assert_eq!(std::fs::read(stage)?, b"conflicting interrupted state");
        assert!(!store.load()?.enabled);
        Ok(())
    }
    #[cfg(unix)]
    #[test]
    fn redirected_state_directory_is_never_followed() -> Result<()> {
        let (owner, store) = fixture()?;
        let outside = tempfile::tempdir()?;
        std::os::unix::fs::symlink(outside.path(), owner.path().join(DIRECTORY))?;
        assert!(store.load().is_err());
        assert!(
            store
                .apply(
                    1,
                    &Configuration {
                        revision: 2,
                        ..Configuration::disabled(store.installation, store.catalog)
                    }
                )
                .is_err()
        );
        assert!(std::fs::read_dir(outside.path())?.next().is_none());
        Ok(())
    }
    #[test]
    fn management_check_after_writer_wait_refuses_without_creating_optional_state() -> Result<()> {
        use std::sync::{
            Barrier,
            atomic::{AtomicBool, Ordering},
        };
        let (owner, store) = fixture()?;
        let store = Arc::new(store);
        let candidate = enabled(&store)?;
        let held = store.writer.lock().unwrap();
        let entered = Arc::new(Barrier::new(2));
        let live = Arc::new(AtomicBool::new(true));
        let child_store = store.clone();
        let child_entered = entered.clone();
        let child_live = live.clone();
        let task = std::thread::spawn(move || {
            child_entered.wait();
            child_store.apply_checked(1, &candidate, || {
                ensure!(
                    child_live.load(Ordering::Acquire),
                    "management request expired"
                );
                Ok(())
            })
        });
        entered.wait();
        live.store(false, Ordering::Release);
        drop(held);
        assert!(task.join().unwrap().is_err());
        assert!(!owner.path().join(DIRECTORY).exists());
        assert!(!store.load()?.enabled);
        Ok(())
    }
    #[test]
    fn final_delivery_policy_admission_refuses_busy_writer_without_waiting_or_effect() -> Result<()>
    {
        let (_root, store) = fixture()?;
        let current = enabled(&store)?;
        store.apply(1, &current)?;
        let _held = store.writer.lock().unwrap();
        let mut effect = false;
        assert!(
            store
                .with_current(
                    || Ok(()),
                    |_| {
                        effect = true;
                        Ok(())
                    }
                )
                .is_err()
        );
        assert!(!effect);
        Ok(())
    }
    #[test]
    fn final_delivery_shared_file_lock_fences_another_configuration_adapter() -> Result<()> {
        let (_root, store) = fixture()?;
        let current = enabled(&store)?;
        store.apply(1, &current)?;
        let other =
            ConfigurationStore::open(store._ownership.clone(), store.installation, store.catalog)?;
        let mut disabled = current.clone();
        disabled.revision += 1;
        disabled.enabled = false;
        store.with_current(||Ok(()),|observed| {
            assert_eq!(*observed,current);
            assert!(other.apply(2,&disabled).is_err(),"another adapter cannot publish while actual delivery poll owns shared policy lock");
            Ok(())
        })?;
        assert_eq!(other.apply(2, &disabled)?, disabled);
        assert_eq!(store.load()?, disabled);
        Ok(())
    }
}
