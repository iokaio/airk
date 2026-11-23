// SPDX-License-Identifier: Apache-2.0
//! Config5 uses the SAME authenticated CAS path/lock, with retained exact config3 and
//! config4 bytes.
use super::*;
use airk_core::arbiter::{
    Profile,
    device::{Configuration4Transition, DeviceCatalog, DeviceCatalogBinding, DeviceConfiguration},
};
use sha2::{Digest, Sha256};
#[derive(Clone, Debug)]
pub enum StoredConfiguration {
    Legacy(Configuration),
    Device(DeviceConfiguration),
}
#[derive(Clone, Debug)]
pub struct ConfigurationBindingObservation {
    pub binding: DeviceCatalogBinding,
    pub revision: u64,
    pub configuration_sha256: Sha256Digest,
    /// The document is configuration/4, written before Large was retired.
    pub configuration4: bool,
}
/// Native-only transition data, not a serialized permission. The owner supplies
/// authenticated compiled snapshots and the live authorization predicate.
pub struct CatalogRebindRequest<'a, 'old, 'new> {
    pub expected_revision: u64,
    pub expected_configuration: Sha256Digest,
    pub expected_old: &'a DeviceCatalogBinding,
    pub old_catalog: &'a DeviceCatalog,
    pub new_catalog: &'a DeviceCatalog,
    pub resolve_old: &'a dyn Fn(Sha256Digest) -> Option<&'old Profile>,
    pub resolve_new: &'a dyn Fn(Sha256Digest) -> Option<&'new Profile>,
    pub authorize: &'a dyn Fn(&DeviceCatalogBinding, &DeviceCatalogBinding) -> Result<()>,
}
pub struct DeviceConfigurationStore<'a> {
    owner: &'a ConfigurationStore,
    binding: DeviceCatalogBinding,
}
impl ConfigurationStore {
    pub fn device(&self, binding: DeviceCatalogBinding) -> DeviceConfigurationStore<'_> {
        DeviceConfigurationStore {
            owner: self,
            binding,
        }
    }
}
fn digest(bytes: &[u8]) -> Sha256Digest {
    Sha256Digest::from_bytes(Sha256::digest(bytes).into())
}
fn bounded(file: &File) -> Result<Vec<u8>> {
    ensure!(
        file.metadata()?.len() <= airk_core::arbiter::MAX_CONFIGURATION_BYTES as u64,
        "device configuration byte limit"
    );
    let mut bytes = Vec::new();
    file.take(airk_core::arbiter::MAX_CONFIGURATION_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= airk_core::arbiter::MAX_CONFIGURATION_BYTES,
        "device configuration grew"
    );
    Ok(bytes)
}
impl DeviceConfigurationStore<'_> {
    fn disabled(&self) -> DeviceConfiguration {
        DeviceConfiguration::disabled(
            self.owner.installation,
            self.owner.catalog,
            self.binding.clone(),
        )
    }
    fn read(
        &self,
        directory: &SetupDirectory,
    ) -> Result<(StoredConfiguration, Option<File>, Vec<u8>)> {
        let file = match directory.open_file(OsStr::new(CONFIGURATION), SetupFileMode::Read) {
            Ok(file) => file,
            Err(error) if missing(&error) => {
                ensure!(
                    !self.owner.has_legacy_state()?,
                    "retained v1/v2 requires explicit disabled initialization"
                );
                return Ok((
                    StoredConfiguration::Device(self.disabled()),
                    None,
                    Vec::new(),
                ));
            }
            Err(error) => return Err(error),
        };
        let bytes = bounded(&file)?;
        #[derive(serde::Deserialize)]
        struct Header {
            schema: String,
        }
        let header: Header = serde_json::from_slice(&bytes)?;
        // Direct typed parse from ORIGINAL bytes preserves duplicate/unknown-key refusal.
        let value = match header.schema.as_str() {
            airk_core::arbiter::CONFIGURATION_SCHEMA => StoredConfiguration::Legacy(
                Configuration::decode(&bytes, self.owner.installation, self.owner.catalog)?,
            ),
            airk_core::arbiter::device::DEVICE_CONFIGURATION_SCHEMA => {
                StoredConfiguration::Device(DeviceConfiguration::decode(
                    &bytes,
                    self.owner.installation,
                    self.owner.catalog,
                    &self.binding,
                )?)
            }
            airk_core::arbiter::device::DEVICE_CONFIGURATION_V4_SCHEMA => anyhow::bail!(
                "configuration/4 requires the compiled catalog transition to configuration/5; original bytes preserved"
            ),
            _ => anyhow::bail!("unknown Arbiter configuration schema; original bytes preserved"),
        };
        Ok((value, Some(file), bytes))
    }
    /// Diagnostic metadata ONLY. It chooses no catalog and returns no active
    /// configuration/permission. Native owner must find this binding in its
    /// authenticated compiled transition inventory before calling rebind_checked.
    pub fn inspect_device_binding(&self) -> Result<Option<ConfigurationBindingObservation>> {
        let directory = match self.owner.root.open_dir(OsStr::new(DIRECTORY)) {
            Ok(directory) => directory,
            Err(error) if missing(&error) => return Ok(None),
            Err(error) => return Err(error),
        };
        let file = match directory.open_file(OsStr::new(CONFIGURATION), SetupFileMode::Read) {
            Ok(file) => file,
            Err(error) if missing(&error) => return Ok(None),
            Err(error) => return Err(error),
        };
        let raw = bounded(&file)?;
        #[derive(serde::Deserialize)]
        struct Header {
            schema: String,
        }
        let header: Header = serde_json::from_slice(&raw)?;
        if header.schema == airk_core::arbiter::CONFIGURATION_SCHEMA {
            return Ok(None);
        }
        if header.schema == airk_core::arbiter::device::DEVICE_CONFIGURATION_V4_SCHEMA {
            let (binding, revision) = airk_core::arbiter::device::configuration4_binding(
                &raw,
                self.owner.installation,
                self.owner.catalog,
            )?;
            return Ok(Some(ConfigurationBindingObservation {
                binding,
                revision,
                configuration_sha256: digest(&raw),
                configuration4: true,
            }));
        }
        let value: DeviceConfiguration = serde_json::from_slice(&raw)?;
        value.validate()?;
        ensure!(
            value.installation == self.owner.installation
                && value.catalog == self.owner.catalog
                && value.canonical_bytes()? == raw,
            "diagnostic binding observation is foreign/noncanonical"
        );
        Ok(Some(ConfigurationBindingObservation {
            binding: value.device_catalog,
            revision: value.revision,
            configuration_sha256: digest(&raw),
            configuration4: false,
        }))
    }
    /// Positive absence plus retained v1/v2 only. This is a non-writable
    /// initialization observation, never a substitute for corrupt/current3 data.
    pub fn initialization_status(
        &self,
        check: impl Fn() -> Result<()>,
    ) -> Result<Option<airk_core::arbiter::InitializationStatus>> {
        check()?;
        let _writer = self
            .owner
            .writer
            .try_lock()
            .map_err(|_| anyhow::anyhow!("initialization observation busy"))?;
        check()?;
        match self.owner.root.open_dir(OsStr::new(DIRECTORY)) {
            Ok(directory) => {
                match directory.open_file(OsStr::new(CONFIGURATION), SetupFileMode::Read) {
                    Ok(_) => {
                        check()?;
                        return Ok(None);
                    }
                    Err(error) if missing(&error) => {}
                    Err(error) => return Err(error),
                }
            }
            Err(error) if missing(&error) => {}
            Err(error) => return Err(error),
        }
        if !self.owner.has_legacy_state()? {
            check()?;
            return Ok(None);
        }
        let actual_legacy_digest = self.owner.legacy_configuration_digest()?;
        check()?;
        Ok(Some(airk_core::arbiter::InitializationStatus {actual_legacy_digest,
            reason:"Retained v1/v2 state requires explicit new disabled configuration; prior consent is not adopted".into()}))
    }
    /// Read-only inspection, including an unmigrated exact config3 CPU document.
    pub fn load(&self) -> Result<StoredConfiguration> {
        match self.owner.root.open_dir(OsStr::new(DIRECTORY)) {
            Ok(directory) => Ok(self.read(&directory)?.0),
            Err(error) if missing(&error) => {
                ensure!(
                    !self.owner.has_legacy_state()?,
                    "retained v1/v2 requires explicit disabled initialization"
                );
                Ok(StoredConfiguration::Device(self.disabled()))
            }
            Err(error) => Err(error),
        }
    }
    fn lock(directory: &SetupDirectory) -> Result<File> {
        let lock = match directory
            .open_file(OsStr::new("configuration.lock"), SetupFileMode::CreateNew)
        {
            Ok(file) => file,
            Err(error)
                if error
                    .downcast_ref::<std::io::Error>()
                    .is_some_and(|error| error.kind() == std::io::ErrorKind::AlreadyExists) =>
            {
                directory.open_file(OsStr::new("configuration.lock"), SetupFileMode::ReadWrite)?
            }
            Err(error) => return Err(error),
        };
        ensure!(
            lock.metadata()?.len() == 0,
            "invalid Arbiter configuration lock"
        );
        lock.try_lock()
            .context("Arbiter configuration writer active")?;
        Ok(lock)
    }
    fn sync_retained(file: &File) -> Result<()> {
        file.sync_all()
            .context("retained configuration file fsync failed")
    }
    fn retain(
        directory: &SetupDirectory,
        name: &OsStr,
        bytes: &[u8],
        sync: &impl Fn(&File) -> Result<()>,
    ) -> Result<File> {
        let file = match directory.open_file(name, SetupFileMode::CreateNew) {
            Ok(mut file) => {
                file.write_all(bytes)?;
                file
            }
            Err(error)
                if error
                    .downcast_ref::<std::io::Error>()
                    .is_some_and(|error| error.kind() == std::io::ErrorKind::AlreadyExists) =>
            {
                // ReadWrite is needed for FlushFileBuffers on Windows; exact bytes
                // and owner-private/no-follow custody still precede the common fsync.
                let file = directory.open_file(name, SetupFileMode::ReadWrite)?;
                ensure!(
                    bounded(&file)? == bytes,
                    "conflicting retained Arbiter bytes preserved"
                );
                file
            }
            Err(error) => return Err(error),
        };
        sync(&file)?;
        Ok(file)
    }
    fn publish(
        &self,
        directory: &SetupDirectory,
        value: &DeviceConfiguration,
        prior: Option<&File>,
        check: &impl Fn() -> Result<()>,
    ) -> Result<DeviceConfiguration> {
        self.publish_synced(directory, value, prior, check, &Self::sync_retained)
    }
    fn publish_synced(
        &self,
        directory: &SetupDirectory,
        value: &DeviceConfiguration,
        prior: Option<&File>,
        check: &impl Fn() -> Result<()>,
        retention_sync: &impl Fn(&File) -> Result<()>,
    ) -> Result<DeviceConfiguration> {
        let bytes = value.canonical_bytes()?;
        check()?;
        let name = format!("device-configuration-{}.part", value.revision);
        let stage = Self::retain(directory, OsStr::new(&name), &bytes, retention_sync)?;
        check()?;
        if let Some(prior) = prior {
            directory.replace_file(OsStr::new(&name), &stage, OsStr::new(CONFIGURATION), prior)?;
        } else {
            directory.publish_no_replace(
                OsStr::new(&name),
                &stage,
                directory,
                OsStr::new(CONFIGURATION),
            )?;
        }
        directory.sync()?;
        let (StoredConfiguration::Device(observed), _, _) = self.read(directory)? else {
            anyhow::bail!("device publication remained legacy");
        };
        ensure!(
            observed == *value,
            "device configuration changed during publication"
        );
        Ok(observed)
    }
    /// Automatic native upgrade calls this after authenticated descriptor lookup.
    /// No user interaction is required, but original liveness/source checks remain.
    pub fn load_or_migrate<'p>(
        &self,
        resolve: impl Fn(Sha256Digest) -> Option<&'p Profile>,
        check: impl Fn() -> Result<()>,
    ) -> Result<DeviceConfiguration> {
        check()?;
        match self.load()? {
            StoredConfiguration::Device(value) => {
                check()?;
                Ok(value)
            }
            StoredConfiguration::Legacy(old) => {
                let bytes = old.canonical_bytes()?;
                self.migrate_checked(old.revision, digest(&bytes), resolve, check)
            }
        }
    }
    pub fn migrate_checked<'p>(
        &self,
        expected_revision: u64,
        expected_configuration: Sha256Digest,
        resolve: impl Fn(Sha256Digest) -> Option<&'p Profile>,
        check: impl Fn() -> Result<()>,
    ) -> Result<DeviceConfiguration> {
        self.migrate_synced(
            expected_revision,
            expected_configuration,
            resolve,
            check,
            &Self::sync_retained,
        )
    }
    fn migrate_synced<'p>(
        &self,
        expected_revision: u64,
        expected_configuration: Sha256Digest,
        resolve: impl Fn(Sha256Digest) -> Option<&'p Profile>,
        check: impl Fn() -> Result<()>,
        retention_sync: &impl Fn(&File) -> Result<()>,
    ) -> Result<DeviceConfiguration> {
        let _writer = self
            .owner
            .writer
            .lock()
            .map_err(|_| anyhow::anyhow!("Arbiter writer poisoned"))?;
        check()?;
        let directory = self.owner.root.open_dir(OsStr::new(DIRECTORY))?;
        let _lock = Self::lock(&directory)?;
        check()?;
        let (document, prior, raw) = self.read(&directory)?;
        let archive_name = format!("configuration-v3-{expected_configuration}.retained");
        let old = match document {
            StoredConfiguration::Legacy(old) => {
                ensure!(
                    old.revision == expected_revision && digest(&raw) == expected_configuration,
                    "legacy configuration changed before migration"
                );
                old
            }
            StoredConfiguration::Device(current) => {
                let archive =
                    directory.open_file(OsStr::new(&archive_name), SetupFileMode::Read)?;
                let bytes = bounded(&archive)?;
                ensure!(
                    digest(&bytes) == expected_configuration,
                    "retained migration bytes differ"
                );
                let old =
                    Configuration::decode(&bytes, self.owner.installation, self.owner.catalog)?;
                ensure!(
                    old.revision == expected_revision,
                    "retained migration revision differs"
                );
                let expected =
                    DeviceConfiguration::migrate_legacy(&old, self.binding.clone(), resolve)?;
                ensure!(
                    current == expected,
                    "current config4 already advanced; migration refuses overwrite"
                );
                check()?;
                return Ok(current);
            }
        };
        let value = DeviceConfiguration::migrate_legacy(&old, self.binding.clone(), resolve)?;
        check()?;
        // Create-new exact3 archive is synced BEFORE atomic replacement. Existing
        // archive must reopen through the same no-follow owner-private guard.
        let _archive = Self::retain(&directory, OsStr::new(&archive_name), &raw, retention_sync)?;
        directory.sync()?;
        check()?;
        self.publish_synced(&directory, &value, prior.as_ref(), &check, retention_sync)
    }
    /// Native automatic upgrade of a configuration/4 document, written before
    /// Large was retired, to configuration/5 under the compiled transition
    /// (`DeviceConfiguration::migrate_v4`). The exact /4 bytes are retained
    /// beside it before the atomic replacement. A document that is already
    /// configuration/5 is returned as it is.
    pub fn migrate_v4_checked(
        &self,
        transition: &Configuration4Transition,
        check: impl Fn() -> Result<()>,
    ) -> Result<DeviceConfiguration> {
        ensure!(
            transition.new == self.binding,
            "configuration/4 transition targets another catalog binding"
        );
        let _writer = self
            .owner
            .writer
            .lock()
            .map_err(|_| anyhow::anyhow!("Arbiter writer poisoned"))?;
        check()?;
        let directory = self.owner.root.open_dir(OsStr::new(DIRECTORY))?;
        let _lock = Self::lock(&directory)?;
        check()?;
        let file = directory.open_file(OsStr::new(CONFIGURATION), SetupFileMode::Read)?;
        let raw = bounded(&file)?;
        #[derive(serde::Deserialize)]
        struct Header {
            schema: String,
        }
        let header: Header = serde_json::from_slice(&raw)?;
        if header.schema != airk_core::arbiter::device::DEVICE_CONFIGURATION_V4_SCHEMA {
            let (StoredConfiguration::Device(current), _, _) = self.read(&directory)? else {
                anyhow::bail!("configuration3 requires native migration");
            };
            check()?;
            return Ok(current);
        }
        let value = DeviceConfiguration::migrate_v4(
            &raw,
            self.owner.installation,
            self.owner.catalog,
            transition,
        )?;
        check()?;
        let archive_name = format!("configuration-v4-{}.retained", digest(&raw));
        let _archive = Self::retain(
            &directory,
            OsStr::new(&archive_name),
            &raw,
            &Self::sync_retained,
        )?;
        directory.sync()?;
        check()?;
        self.publish(&directory, &value, Some(&file), &check)
    }
    /// Native automatic-upgrade only. Old/new catalog objects and descriptor
    /// resolvers must come from the authenticated compiled compatibility owner;
    /// no public ManagementRequest accepts those snapshots or this predicate.
    pub fn rebind_checked(
        &self,
        request: CatalogRebindRequest<'_, '_, '_>,
        check: impl Fn() -> Result<()>,
    ) -> Result<DeviceConfiguration> {
        let CatalogRebindRequest {
            expected_revision,
            expected_configuration,
            expected_old,
            old_catalog,
            new_catalog,
            resolve_old,
            resolve_new,
            authorize,
        } = request;
        let _writer = self
            .owner
            .writer
            .lock()
            .map_err(|_| anyhow::anyhow!("Arbiter writer poisoned"))?;
        check()?;
        authorize(expected_old, &self.binding)?;
        let directory = self.owner.root.open_dir(OsStr::new(DIRECTORY))?;
        let _lock = Self::lock(&directory)?;
        check()?;
        let source = self.owner.device(expected_old.clone());
        let (document, prior, raw) = source.read(&directory)?;
        let StoredConfiguration::Device(current) = document else {
            anyhow::bail!("catalog rebind requires canonical configuration4");
        };
        ensure!(
            prior.is_some()
                && current.revision == expected_revision
                && digest(&raw) == expected_configuration,
            "configuration changed/missing before catalog rebind"
        );
        let value = current.rebind_catalog(
            expected_old,
            self.binding.clone(),
            old_catalog,
            new_catalog,
            resolve_old,
            resolve_new,
        )?;
        check()?;
        authorize(expected_old, &self.binding)?;
        let archive_name = format!("configuration-v4-{}.retained", expected_configuration);
        let _archive = Self::retain(
            &directory,
            OsStr::new(&archive_name),
            &raw,
            &Self::sync_retained,
        )?;
        directory.sync()?;
        check()?;
        authorize(expected_old, &self.binding)?;
        self.publish(&directory, &value, prior.as_ref(), &|| {
            check()?;
            authorize(expected_old, &self.binding)
        })
    }
    pub fn initialize_checked(
        &self,
        expected_legacy_configuration: Option<Sha256Digest>,
        check: impl Fn() -> Result<()>,
    ) -> Result<DeviceConfiguration> {
        let _writer = self
            .owner
            .writer
            .lock()
            .map_err(|_| anyhow::anyhow!("Arbiter writer poisoned"))?;
        check()?;
        ensure!(
            self.owner.legacy_configuration_digest()? == expected_legacy_configuration,
            "legacy initialization digest differs"
        );
        let directory = self.owner.root.create_dir(OsStr::new(DIRECTORY))?;
        let _lock = Self::lock(&directory)?;
        match directory.open_file(OsStr::new(CONFIGURATION), SetupFileMode::Read) {
            Ok(_) => {
                let (document, _, _) = self.read(&directory)?;
                return match document {
                    StoredConfiguration::Device(value) => {
                        check()?;
                        Ok(value)
                    }
                    StoredConfiguration::Legacy(_) => anyhow::bail!(
                        "valid config3 requires native migration, not disabled initialization"
                    ),
                };
            }
            Err(error) if missing(&error) => {}
            Err(error) => return Err(error),
        }
        check()?;
        ensure!(
            self.owner.legacy_configuration_digest()? == expected_legacy_configuration,
            "legacy changed during initialization"
        );
        self.publish(&directory, &self.disabled(), None, &check)
    }
    pub fn apply_checked(
        &self,
        expected_revision: u64,
        candidate: &DeviceConfiguration,
        check: impl Fn() -> Result<()>,
    ) -> Result<DeviceConfiguration> {
        candidate.validate()?;
        ensure!(
            candidate.installation == self.owner.installation
                && candidate.catalog == self.owner.catalog
                && candidate.device_catalog == self.binding
                && expected_revision.checked_add(1) == Some(candidate.revision),
            "foreign/nonmonotonic device configuration"
        );
        let _writer = self
            .owner
            .writer
            .lock()
            .map_err(|_| anyhow::anyhow!("Arbiter writer poisoned"))?;
        check()?;
        let directory = self.owner.root.create_dir(OsStr::new(DIRECTORY))?;
        let _lock = Self::lock(&directory)?;
        let (document, prior, _) = self.read(&directory)?;
        let StoredConfiguration::Device(current) = document else {
            anyhow::bail!("config3 requires native migration before device mutation");
        };
        ensure!(
            current.revision == expected_revision,
            "device configuration revision changed"
        );
        check()?;
        self.publish(&directory, candidate, prior.as_ref(), &check)
    }
    pub fn with_current<T>(
        &self,
        live: impl Fn() -> Result<()>,
        effect: impl FnOnce(&DeviceConfiguration) -> Result<T>,
    ) -> Result<T> {
        live()?;
        let _writer = self
            .owner
            .writer
            .try_lock()
            .map_err(|_| anyhow::anyhow!("device delivery configuration busy"))?;
        live()?;
        let directory = self.owner.root.open_dir(OsStr::new(DIRECTORY))?;
        let lock =
            directory.open_file(OsStr::new("configuration.lock"), SetupFileMode::ReadWrite)?;
        ensure!(
            lock.metadata()?.len() == 0,
            "invalid device configuration lock"
        );
        lock.try_lock_shared()
            .context("device delivery writer active")?;
        live()?;
        let (document, _, _) = self.read(&directory)?;
        let StoredConfiguration::Device(current) = document else {
            anyhow::bail!("device delivery requires migrated configuration");
        };
        live()?;
        effect(&current)
    }
}
#[cfg(test)]
mod tests;
