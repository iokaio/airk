// SPDX-License-Identifier: Apache-2.0
//! Inactive-project cleanup stays inside proven collection ownership and keeps settings.
use super::*;
use crate::service::{ProjectLease, registry::ServiceRegistry};
use airk_core::service_cleanup::{
    DerivedCleanupPreview, DerivedCleanupReport, MAX_CLEANUP_BYTES, MAX_CLEANUP_FILES,
};

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CleanupScope {
    version: u32,
    owner: Sha256Digest,
    manifest: Manifest,
    replacement: Option<Intent>,
    settings: Sha256Digest,
    namespace: Incarnation,
    request_epoch: u64,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CleanupIntent {
    version: u32,
    owner: Sha256Digest,
    scope: CleanupScope,
    preview: DerivedCleanupPreview,
    replacement: Incarnation,
    targets: Vec<CleanupTarget>,
    complete: bool,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CleanupTarget {
    slot: Incarnation,
    files: u64,
    bytes: u64,
}

pub(super) fn owned_scratch(path: &Path, owner: Sha256Digest) -> bool {
    json::<CleanupIntent>(path).is_ok_and(|intent| {
        intent.version == 140
            && intent.owner == owner
            && intent.scope.owner == owner
            && intent.preview.owner == owner
            && digest(&intent.scope).is_ok_and(|guard| guard == intent.preview.guard)
    })
}

impl Collection {
    fn cleanup_owner(&self, lease: &ProjectLease) -> Result<()> {
        let registry = ServiceRegistry::open_saved(&lease.root)?;
        let registration = registry.validate_inactive_worker(lease)?;
        ensure!(
            registration.root == self.owner.root
                && registration.data_root == self.owner.data_root
                && registration.collection == self.owner.collection,
            "derived cleanup belongs to another collection"
        );
        crate::managed_collection::validate_ancestors(&self.directory)?;
        ensure!(
            plain(&self.directory)?.is_dir(),
            "derived cleanup owner directory is unavailable"
        );
        Ok(())
    }

    fn cleanup_scope(&self, lease: &ProjectLease) -> Result<CleanupScope> {
        let manifest = self.manifest()?;
        let replacement = if self.directory.join("replacement.json").try_exists()? {
            let intent: Intent = json(&self.directory.join("replacement.json"))?;
            ensure!(
                intent.version == 140 && intent.owner == self.binding,
                "foreign replacement during cleanup preview"
            );
            Some(intent)
        } else {
            None
        };
        let source = FileStore::new(self.slot(manifest.active));
        let settings = settings_digest(&source, &self.owner.root)?;
        let (namespace, request_epoch) =
            ServiceRegistry::open_saved(&lease.root)?.saved_request_epoch()?;
        Ok(CleanupScope {
            version: 1,
            owner: self.binding,
            manifest,
            replacement,
            settings,
            namespace,
            request_epoch,
        })
    }

    /// The guard binds the exact owned incarnation set and saved configuration.
    /// Counts are bounded observations, not a promise that source files are current.
    pub fn derived_cleanup_preview(&self, lease: &ProjectLease) -> Result<DerivedCleanupPreview> {
        self.cleanup_owner(lease)?;
        let _replacement = acquire_file(&self.directory, "replacement.lock")?;
        let _manifest = manifest_lock(&self.directory)?;
        if let Some(intent) = self.cleanup_intent()?
            && !intent.complete
        {
            let mut preview = intent.preview;
            preview.resuming = true;
            return Ok(preview);
        }
        let scope = self.cleanup_scope(lease)?;
        let saved = settings(
            &FileStore::new(self.slot(scope.manifest.active)),
            &self.owner.root,
        )?;
        let mut targets = std::collections::BTreeSet::from([scope.manifest.active]);
        targets.extend(scope.manifest.retired.iter().map(|retired| retired.slot));
        if let Some(replacement) = &scope.replacement {
            targets.insert(replacement.slot);
        }
        ensure!(
            targets.len() <= 10,
            "derived cleanup incarnation budget exceeded"
        );
        let mut files = 0u64;
        let mut bytes = 0u64;
        let mut complete = true;
        'inventory: for slot in &targets {
            let directory = self.slot(*slot);
            if !directory.try_exists()? {
                continue;
            }
            let (mut entries, _) = retirement_inventory(&directory)?;
            entries.sort();
            for entry in entries {
                let length = plain(&entry)?.len();
                if files == MAX_CLEANUP_FILES
                    || bytes
                        .checked_add(length)
                        .is_none_or(|total| total > MAX_CLEANUP_BYTES)
                {
                    complete = false;
                    break 'inventory;
                }
                files += 1;
                bytes += length;
            }
        }
        let preview = DerivedCleanupPreview {
            version: 1,
            owner: self.binding,
            guard: digest(&scope)?,
            active: scope.manifest.active,
            known_incarnations: targets.len().try_into()?,
            owned_files: files,
            owned_bytes: bytes,
            inventory_complete: complete,
            preserved_settings_files: saved.len().try_into()?,
            preserved_settings_bytes: saved.iter().map(|(_, bytes)| bytes.len() as u64).sum(),
            resuming: false,
        };
        preview.validate()?;
        Ok(preview)
    }

    fn cleanup_intent(&self) -> Result<Option<CleanupIntent>> {
        let path = self.directory.join("derived-cleanup.json");
        if !path.try_exists()? {
            return Ok(None);
        }
        let intent: CleanupIntent = json(&path)?;
        intent.preview.validate()?;
        ensure!(
            intent.version == 140
                && intent.owner == self.binding
                && intent.scope.owner == self.binding
                && intent.preview.owner == self.binding
                && digest(&intent.scope)? == intent.preview.guard,
            "foreign cleanup receipt"
        );
        let mut expected = std::collections::BTreeSet::from([intent.scope.manifest.active]);
        expected.extend(intent.scope.manifest.retired.iter().map(|entry| entry.slot));
        expected.extend(intent.scope.replacement.iter().map(|entry| entry.slot));
        ensure!(
            expected.len() <= 10
                && intent.targets.len() == expected.len()
                && intent
                    .targets
                    .iter()
                    .map(|entry| entry.slot)
                    .collect::<std::collections::BTreeSet<_>>()
                    == expected
                && !expected.contains(&intent.replacement),
            "invalid cleanup target set"
        );
        let files = intent
            .targets
            .iter()
            .try_fold(0u64, |total, entry| total.checked_add(entry.files))
            .context("cleanup file count overflow")?;
        let bytes = intent
            .targets
            .iter()
            .try_fold(0u64, |total, entry| total.checked_add(entry.bytes))
            .context("cleanup byte count overflow")?;
        ensure!(
            files <= MAX_CLEANUP_FILES * 10 && bytes <= MAX_CLEANUP_BYTES,
            "cleanup inventory exceeds budget"
        );
        Ok(Some(intent))
    }

    /// One resumable turn. The inactive project lease remains held through all I/O.
    /// Only known owned files are removed; unknown entries and aliases fail closed.
    pub fn delete_derived_turn(
        &self,
        lease: &ProjectLease,
        job: Sha256Digest,
        guard: Sha256Digest,
    ) -> Result<DerivedCleanupReport> {
        self.cleanup_owner(lease)?;
        let _replacement = acquire_file(&self.directory, "replacement.lock")?;
        let previous = self.cleanup_intent()?;
        ensure!(
            previous
                .as_ref()
                .is_none_or(|intent| intent.complete || intent.preview.guard == guard),
            "an interrupted cleanup must resume its original approval"
        );
        let mut intent = if let Some(intent) =
            previous.filter(|intent| intent.preview.guard == guard)
        {
            intent
        } else {
            let _manifest = manifest_lock(&self.directory)?;
            let scope = self.cleanup_scope(lease)?;
            ensure!(
                digest(&scope)? == guard,
                "cleanup preview changed; inspect again before approving"
            );
            let mut slots = std::collections::BTreeSet::from([scope.manifest.active]);
            slots.extend(scope.manifest.retired.iter().map(|entry| entry.slot));
            slots.extend(scope.replacement.iter().map(|entry| entry.slot));
            let mut targets = Vec::new();
            let mut total_bytes = 0u64;
            for slot in slots {
                let directory = self.slot(slot);
                let (files, _) = if directory.try_exists()? {
                    retirement_inventory(&directory)?
                } else {
                    (Vec::new(), Vec::new())
                };
                let bytes = files.iter().try_fold(0u64, |total, path| -> Result<u64> {
                    total
                        .checked_add(plain(path)?.len())
                        .context("cleanup byte count overflow")
                })?;
                total_bytes = total_bytes
                    .checked_add(bytes)
                    .context("cleanup byte count overflow")?;
                ensure!(
                    total_bytes <= MAX_CLEANUP_BYTES,
                    "cleanup byte budget exceeded"
                );
                targets.push(CleanupTarget {
                    slot,
                    files: files.len().try_into()?,
                    bytes,
                });
            }
            let saved = settings(
                &FileStore::new(self.slot(scope.manifest.active)),
                &self.owner.root,
            )?;
            let preview = DerivedCleanupPreview {
                version: 1,
                owner: self.binding,
                guard,
                active: scope.manifest.active,
                known_incarnations: targets.len().try_into()?,
                owned_files: targets
                    .iter()
                    .map(|target| target.files)
                    .sum::<u64>()
                    .min(MAX_CLEANUP_FILES),
                owned_bytes: total_bytes,
                inventory_complete: targets.iter().map(|target| target.files).sum::<u64>()
                    <= MAX_CLEANUP_FILES,
                preserved_settings_files: saved.len().try_into()?,
                preserved_settings_bytes: saved.iter().map(|(_, bytes)| bytes.len() as u64).sum(),
                resuming: false,
            };
            preview.validate()?;
            let replacement: Incarnation = framed_digest(
                b"service-derived-cleanup-slot/1.4",
                &[guard.to_string().as_bytes(), job.to_string().as_bytes()],
            )
            .to_string()[..32]
                .parse()?;
            ensure!(
                !self.slot(replacement).try_exists()?
                    && targets.iter().all(|target| target.slot != replacement),
                "cleanup replacement already exists"
            );
            let intent = CleanupIntent {
                version: 140,
                owner: self.binding,
                scope,
                preview,
                replacement,
                targets,
                complete: false,
            };
            // Authorization/fencing precedes even the durable cleanup intent.
            // If intent persistence fails, this approved scope can be retried.
            ServiceRegistry::open(&lease.root, intent.scope.namespace)?
                .begin_derived_cleanup(lease, job, guard)?;
            write(self, "derived-cleanup.json", &intent)?;
            intent
        };
        let mut registry = ServiceRegistry::open(&lease.root, intent.scope.namespace)?;
        registry.begin_derived_cleanup(lease, job, guard)?;
        if !intent.complete {
            let manifest = {
                let _lock = manifest_lock(&self.directory)?;
                self.manifest()?
            };
            ensure!(
                manifest.active == intent.scope.manifest.active
                    || manifest.active == intent.replacement,
                "cleanup active incarnation changed"
            );
            ensure!(
                manifest.retired.iter().all(|entry| intent
                    .targets
                    .iter()
                    .any(|target| target.slot == entry.slot)),
                "cleanup retirement scope changed"
            );
            if manifest.active == intent.scope.manifest.active {
                if !manifest.retired.is_empty() {
                    self.retire_locked()?;
                    return self.cleanup_report(&intent);
                }
                if self.directory.join("replacement.json").try_exists()? {
                    let replacement: Intent = json(&self.directory.join("replacement.json"))?;
                    ensure!(
                        intent
                            .scope
                            .replacement
                            .as_ref()
                            .is_some_and(
                                |expected| digest(expected).ok() == digest(&replacement).ok()
                            ),
                        "cleanup replacement scope changed"
                    );
                    self.recover_abandoned_replacement_locked()?;
                    return self.cleanup_report(&intent);
                }
                let source = FileStore::new(self.slot(manifest.active));
                ensure!(
                    settings_digest(&source, &self.owner.root)? == intent.scope.settings,
                    "cleanup saved settings changed"
                );
                let destination = self.slot(intent.replacement);
                if !destination.try_exists()? {
                    fs::create_dir(&destination)?;
                    crate::maintenance::sync_parent(&destination)?;
                }
                let (files, directories) = retirement_inventory(&destination)?;
                ensure!(
                    directories.len() == 1
                        && files.iter().all(|path| path
                            .file_name()
                            .and_then(|name| name.to_str())
                            .is_some_and(|name| name == "writer.lock" || SETTINGS.contains(&name))),
                    "unexpected cleanup settings backup entry"
                );
                let saved = settings(&source, &self.owner.root)?;
                let quota = Quota::open_service(Path::new(&self.owner.data_root))?;
                let reservation = quota.reserve(&self.owner.collection, 1 << 20)?;
                for (name, bytes) in &saved {
                    let path = destination.join(name);
                    if path.try_exists()? {
                        ensure!(plain(&path)?.is_file(), "invalid cleanup settings backup");
                        if fs::read(&path)? == *bytes {
                            continue;
                        }
                        // Only the intent-owned backup is replaced after a torn copy.
                        fs::remove_file(&path)?;
                    }
                    let mut file = reservation.create_file(path)?;
                    file.write_all(bytes)?;
                    file.sync_all()?;
                }
                ensure!(
                    settings_digest(&FileStore::new(&destination), &self.owner.root)?
                        == intent.scope.settings,
                    "cleanup settings backup incomplete"
                );
                let _writer = source.lock()?;
                let incarnation = source
                    .current_scope(&self.owner.root, &self.owner.collection)?
                    .map(|scope| scope.incarnation);
                let _gate = Gate::exclusive(&source, &_writer)?;
                let _lock = manifest_lock(&self.directory)?;
                ensure!(
                    digest(&self.manifest()?)? == digest(&manifest)?,
                    "cleanup selection changed"
                );
                let mut next = manifest;
                next.epoch = next
                    .epoch
                    .checked_add(1)
                    .context("cleanup selection epoch exhausted")?;
                next.retired.push(Retired {
                    slot: next.active,
                    incarnation,
                });
                next.active = intent.replacement;
                next.last_rebuild = None;
                write(self, "active.json", &next)?;
            } else if !manifest.retired.is_empty() {
                self.retire_locked()?;
            }
            let manifest = {
                let _lock = manifest_lock(&self.directory)?;
                self.manifest()?
            };
            if manifest.active == intent.replacement && manifest.retired.is_empty() {
                for target in &intent.targets {
                    ensure!(
                        !self.slot(target.slot).try_exists()?,
                        "cleanup target remains without a retirement receipt"
                    );
                }
                intent.complete = true;
                write(self, "derived-cleanup.json", &intent)?;
            }
        }
        self.cleanup_report(&intent)
    }

    fn cleanup_report(&self, intent: &CleanupIntent) -> Result<DerivedCleanupReport> {
        let mut removed = Vec::new();
        for target in &intent.targets {
            if !self.slot(target.slot).try_exists()? {
                removed.push(target);
            }
        }
        let report = DerivedCleanupReport {
            version: 1,
            owner: self.binding,
            guard: intent.preview.guard,
            complete: intent.complete,
            pending_incarnations: if intent.complete {
                0
            } else {
                (intent.targets.len() - removed.len()).max(1).try_into()?
            },
            removed_known_files: removed.iter().map(|target| target.files).sum(),
            removed_known_bytes: removed.iter().map(|target| target.bytes).sum(),
            preserved_settings_files: intent.preview.preserved_settings_files,
            preserved_settings_bytes: intent.preview.preserved_settings_bytes,
        };
        report.validate()?;
        Ok(report)
    }
}
