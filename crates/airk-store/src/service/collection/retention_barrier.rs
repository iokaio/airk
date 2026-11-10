// SPDX-License-Identifier: Apache-2.0
//! Native bounded predecessor capability publication; no source or index mutation.
use super::*;
use crate::store_fs::{SetupDirectory, SetupFileMode, file_identity};
use std::ffi::OsStr;

pub(super) fn read_json<T: serde::de::DeserializeOwned + Serialize>(
    pin: &SetupDirectory,
    name: &str,
    limit: u64,
) -> Result<T> {
    let bytes = pin.read_file(OsStr::new(name), limit)?;
    let value: T = serde_json::from_slice(&bytes)?;
    ensure!(
        serde_json::to_vec(&value)? == bytes,
        "noncanonical retained capability metadata"
    );
    Ok(value)
}
/// A paid temporary inside exact reservation staging; partial crash bytes stay owned.
pub(crate) struct PreparedMarker {
    name: String,
    file: File,
    pin: SetupDirectory,
    published: bool,
}
impl PreparedMarker {
    pub(crate) fn new(reservation: &crate::quota::Reservation, bytes: &[u8]) -> Result<Self> {
        Self::prepare(reservation, bytes, |_| Ok(()))
    }
    fn prepare(
        reservation: &crate::quota::Reservation,
        bytes: &[u8],
        after_open: impl FnOnce(&mut File) -> Result<()>,
    ) -> Result<Self> {
        let stage = reservation.staging_directory()?;
        let pin = SetupDirectory::open_canonical(&stage)?;
        let name = "retention-owner.json".to_string();
        let file = pin.open_file(OsStr::new(&name), SetupFileMode::CreateNew)?;
        let mut prepared = Self {
            name,
            file,
            pin,
            published: false,
        };
        after_open(&mut prepared.file)?;
        prepared.file.write_all(bytes)?;
        prepared.file.sync_all()?;
        Ok(prepared)
    }
    pub(crate) fn publish(
        &mut self,
        destination: &SetupDirectory,
        target: &str,
        expected: &File,
    ) -> Result<()> {
        self.pin.replace_file_to(
            OsStr::new(&self.name),
            &self.file,
            destination,
            OsStr::new(target),
            expected,
        )?;
        self.published = true;
        Ok(())
    }
}
impl Drop for PreparedMarker {
    fn drop(&mut self) {
        if !self.published {
            let _ = self
                .pin
                .unlink_file_if_identity(OsStr::new(&self.name), &self.file);
        }
    }
}
fn required_owner_bytes(owner: &Owner, required: RequiredExtractionContract) -> Result<Vec<u8>> {
    let mut next = owner.clone();
    next.required_extraction_contract = Some(required);
    let bytes = serde_json::to_vec(&next)?;
    ensure!(
        bytes.len() <= MAX_METADATA,
        "required extraction stable owner exceeds metadata bound"
    );
    Ok(bytes)
}

/// Borrowed native ownership inputs, not serializable authority or a bypass.
pub(crate) struct RetentionOwnerContext<'a> {
    pub owning_store: &'a FileStore,
    pub writer: &'a crate::WriterLock,
    pub quota: &'a Quota,
    pub logical: &'a SetupDirectory,
    pub root: &'a str,
    pub collection: &'a str,
    pub contract: u32,
}

pub(super) fn install(
    collection: &Collection,
    store: &FileStore,
    writer: &crate::WriterLock,
    quota: &Quota,
    contract: u32,
    after_stable: impl FnOnce() -> Result<()>,
) -> Result<()> {
    let required = RequiredExtractionContract::try_from(contract)?;
    crate::maintenance::check_writer(store, writer, quota)?;
    quota.require_store_funding(store)?;
    // Same ordering as native replacement: no new/recovered target may appear
    // between this bounded roster and final lower-owner publication.
    let _replacement = acquire_file(&collection.directory, "replacement.lock")?;
    let _manifest = manifest_lock(&collection.directory)?;
    let pin = SetupDirectory::open_canonical(&collection.directory)?;
    let mut expected = pin.open_file(OsStr::new("service-owner.json"), SetupFileMode::Read)?;
    let mut bytes = Vec::new();
    (&mut expected)
        .take(MAX_METADATA as u64 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= MAX_METADATA,
        "stable capability owner exceeds bound"
    );
    let owner: Owner = serde_json::from_slice(&bytes)?;
    ensure!(
        serde_json::to_vec(&owner)? == bytes
            && owner_digest(&owner)? == collection.binding
            && owner.root == collection.owner.root
            && owner.data_root == collection.owner.data_root
            && owner.collection == collection.owner.collection,
        "stable retained owner changed"
    );
    let stable_bytes = required_owner_bytes(&owner, required)?;
    let manifest: Manifest = read_json(&pin, "active.json", MAX_METADATA as u64)?;
    ensure!(
        manifest.version == 140
            && manifest.owner == collection.binding
            && manifest.retired.len() <= MAX_RETIRED,
        "foreign retained selection manifest"
    );
    let mut slots = std::collections::BTreeSet::from([manifest.active]);
    slots.extend(manifest.retired.iter().map(|r| r.slot));
    ensure!(
        slots.len() == manifest.retired.len() + 1,
        "duplicate or active retired slot"
    );
    match pin.read_file(OsStr::new("replacement.json"), MAX_METADATA as u64) {
        Ok(bytes) => {
            let intent: Intent = serde_json::from_slice(&bytes)?;
            ensure!(
                serde_json::to_vec(&intent)? == bytes
                    && intent.version == 140
                    && intent.owner == collection.binding,
                "foreign recoverable retention target"
            );
            slots.insert(intent.slot);
        }
        Err(error)
            if error
                .downcast_ref::<std::io::Error>()
                .is_some_and(|e| e.kind() == std::io::ErrorKind::NotFound) => {}
        Err(error) => return Err(error),
    }
    ensure!(
        slots.len() <= MAX_RETIRED + 2,
        "retained slot roster exceeds bound"
    );
    let incarnations = pin.open_dir(OsStr::new("incarnations"))?;
    let entries = incarnations
        .entries()?
        .take(MAX_RETIRED + 3)
        .collect::<Result<Vec<_>>>()?;
    ensure!(
        entries.len() <= MAX_RETIRED + 2,
        "retained slot inventory exceeds bound"
    );
    let mut actual = std::collections::BTreeSet::new();
    for name in entries {
        let slot: Incarnation = name.to_str().context("non-UTF8 retained slot")?.parse()?;
        ensure!(
            slots.contains(&slot),
            "unowned incarnation preserved during retention migration"
        );
        actual.insert(slot);
    }
    ensure!(
        actual.contains(&manifest.active),
        "active retained slot missing"
    );
    let active_pin = incarnations.open_dir(OsStr::new(&manifest.active.to_string()))?;
    ensure!(
        SetupDirectory::open_canonical(store.directory())?.identity()? == active_pin.identity()?,
        "retention migration requires exact active slot"
    );
    // This is an idempotent already-installed capability check, NOT fresh
    // absence or Ready proof. The exact native split owner may resume its own
    // bootstrap; canonical current recovery still validates every existing byte.
    if owner.required_extraction_contract == Some(required) && slots.len() == 1 {
        match active_pin.open_file(OsStr::new("current-owner.json"), SetupFileMode::Read) {
            Err(error)
                if error
                    .downcast_ref::<std::io::Error>()
                    .is_some_and(|e| e.kind() == std::io::ErrorKind::NotFound) =>
            {
                if let Some(permit) = store.split_access.as_ref() {
                    let _gate = Gate::exclusive_pinned_slot(store, writer, store, &pin)?;
                    if permit.retained_policy_already_installed(store, &pin, &active_pin, quota)? {
                        return Ok(());
                    }
                }
            }
            Ok(_) => (),
            Err(error) => return Err(error),
        }
    }
    let context = RetentionOwnerContext {
        owning_store: store,
        writer,
        quota,
        logical: &pin,
        root: &owner.root,
        collection: &owner.collection,
        contract,
    };
    let mut markers = Vec::new();
    for slot in actual {
        let candidate = collection.context_store(collection.slot(slot))?;
        let candidate_pin = incarnations.open_dir(OsStr::new(&slot.to_string()))?;
        ensure!(
            SetupDirectory::open_canonical(candidate.directory())?.identity()?
                == candidate_pin.identity()?,
            "retained slot pathname changed"
        );
        let marker = candidate.prepare_extraction_contract(&context, candidate_pin)?;
        if slot == manifest.active && !marker.had_current() {
            let permit = store
                .split_access
                .as_ref()
                .context("fresh retention requires claimed DestinationBuild")?;
            let funding = quota
                .funding_use()
                .context("fresh retention requires exact paid phase")?;
            permit.validate_funding_use(funding)?;
            ensure!(
                funding.phase.purpose
                    == airk_core::catalog_split::SplitJobPurpose::DestinationBuild,
                "only exact DestinationBuild can initialize retained policy"
            );
            permit.validate_pinned_store(store, &pin)?;
        }
        markers.push(marker);
    }
    // Every native writer/Gate and exact expected header is held before ANY
    // effect. A crash here leaves only stricter capability metadata, never new
    // policy/profile bytes or an altered publication. Retry seals all slots.
    let bytes = stable_bytes;
    let reservation = quota.reserve(&owner.collection, MAX_METADATA as u64 * 2)?;
    reservation.charge(bytes.len() as u64)?;
    let mut prepared = PreparedMarker::new(&reservation, &bytes)?;
    ensure!(
        SetupDirectory::open_canonical(&collection.directory)?.identity()? == pin.identity()?,
        "retained collection rebound"
    );
    ensure!(
        file_identity(&pin.open_file(OsStr::new("service-owner.json"), SetupFileMode::Read)?)?
            == file_identity(&expected)?,
        "retained stable owner replaced before commit"
    );
    prepared.publish(&pin, "service-owner.json", &expected)?;
    after_stable()?;
    for marker in &mut markers {
        marker.publish()?;
    }
    ensure!(
        SetupDirectory::open_canonical(&collection.directory)?.identity()? == pin.identity()?,
        "retention collection pathname changed before configuration handoff"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::current::CurrentScope;
    fn observed_contract(path: &Path) -> Result<serde_json::Value> {
        let value: serde_json::Value = serde_json::from_slice(&fs::read(path)?)?;
        Ok(value["required_extraction_contract"].clone())
    }
    #[test]
    fn all_recoverable_slot_readers_refuse_then_stable_interruption_retries_without_publication_change()
    -> Result<()> {
        let temporary = tempfile::tempdir()?;
        let base = temporary.path().canonicalize()?;
        let source = base.join("source");
        let data = base.join("data");
        fs::create_dir(&source)?;
        fs::create_dir(&data)?;
        fs::write(source.join("original.rs"), b"pub struct Original;")?;
        let root = source.to_str().context("fixture root UTF8")?;
        let first = Incarnation::from_bytes([101; 16]);
        let second = Incarnation::from_bytes([102; 16]);
        let collection = Collection::initialize(&data, root, "fixture", first)?;
        let active = collection.selected_store()?;
        let token = super::super::tests::initialize(&active, root, first)?;
        let replacement =
            collection.begin_replacement(second, Sha256Digest::from_bytes([103; 32]))?;
        let candidate = replacement.store();
        super::super::tests::initialize(&candidate, root, second)?;
        drop(replacement); // Actual durable replacement intent survives ownership loss.
        let candidate_scope = CurrentScope {
            root: root.into(),
            collection: "fixture".into(),
            incarnation: second,
        };
        let read = candidate.current_read(&candidate_scope)?;
        let writer = active.lock()?;
        let quota = Quota::open_service(&data)?;
        let reserved_before = quota.status()?.reserved_bytes;
        let stable_before = fs::read(collection.directory.join("service-owner.json"))?;
        let active_before = fs::read(active.directory().join("current-owner.json"))?;
        let candidate_before = fs::read(candidate.directory().join("current-owner.json"))?;
        let refused = install(&collection, &active, &writer, &quota, 3, || Ok(())).unwrap_err();
        assert!(
            refused
                .downcast_ref::<crate::current::CurrentGateContention>()
                .is_some()
        );
        assert_eq!(
            fs::read(collection.directory.join("service-owner.json"))?,
            stable_before
        );
        assert_eq!(
            fs::read(active.directory().join("current-owner.json"))?,
            active_before
        );
        assert_eq!(
            fs::read(candidate.directory().join("current-owner.json"))?,
            candidate_before
        );
        drop(read);
        let interrupted = install(&collection, &active, &writer, &quota, 3, || {
            anyhow::bail!("injected exact stable-before-lower interruption")
        });
        assert!(interrupted.is_err());
        assert_eq!(
            observed_contract(&collection.directory.join("service-owner.json"))?,
            3
        );
        assert_eq!(
            fs::read(active.directory().join("current-owner.json"))?,
            active_before
        );
        assert_eq!(
            fs::read(candidate.directory().join("current-owner.json"))?,
            candidate_before
        );
        assert_eq!(
            quota.status()?.reserved_bytes,
            reserved_before,
            "closed marker staging leaked reservation credit"
        );
        install(&collection, &active, &writer, &quota, 3, || Ok(()))?;
        assert_eq!(
            observed_contract(&active.directory().join("current-owner.json"))?,
            3
        );
        assert_eq!(
            observed_contract(&candidate.directory().join("current-owner.json"))?,
            3
        );
        let active_scope = CurrentScope {
            root: root.into(),
            collection: "fixture".into(),
            incarnation: first,
        };
        assert_eq!(active.current_read(&active_scope)?.token()?, token);
        assert!(required_extraction_contract_for_scope(&active, &candidate_scope).is_err());
        assert_eq!(
            required_extraction_contract_for_scope(&active, &active_scope)?.map(u32::from),
            Some(3)
        );
        drop(writer);
        // Real recovery classifies the abandoned slot but cannot replace the
        // protected active publication. Both retained headers stay sealed.
        collection.recover_abandoned_replacement()?;
        assert_eq!(collection.manifest()?.active, first);
        assert!(
            collection
                .manifest()?
                .retired
                .iter()
                .any(|row| row.slot == second)
        );
        assert_eq!(
            observed_contract(&candidate.directory().join("current-owner.json"))?,
            3
        );
        assert_eq!(
            fs::read(source.join("original.rs"))?,
            b"pub struct Original;"
        );
        Ok(())
    }
    #[test]
    fn required_owner_encoding_refuses_growth_past_exact_metadata_bound() -> Result<()> {
        let temporary = tempfile::tempdir()?;
        let base = temporary.path().canonicalize()?;
        let mut owner =
            super::super::setup::initial_owner(&base, base.to_str().unwrap(), "fixture")?;
        let count = serde_json::to_vec(&owner)?.len();
        owner.root.push_str(&"x".repeat(MAX_METADATA - count));
        let original = serde_json::to_vec(&owner)?;
        assert_eq!(original.len(), MAX_METADATA);
        assert!(required_owner_bytes(&owner, RequiredExtractionContract::try_from(3)?).is_err());
        assert_eq!(serde_json::to_vec(&owner)?, original);
        assert!(owner.required_extraction_contract.is_none());
        Ok(())
    }
    #[test]
    fn unowned_slot_is_preserved_before_capability_effect() -> Result<()> {
        let temporary = tempfile::tempdir()?;
        let base = temporary.path().canonicalize()?;
        let source = base.join("source");
        let data = base.join("data");
        fs::create_dir(&source)?;
        fs::create_dir(&data)?;
        let root = source.to_str().unwrap();
        let slot = Incarnation::from_bytes([104; 16]);
        let collection = Collection::initialize(&data, root, "fixture", slot)?;
        let active = collection.selected_store()?;
        super::super::tests::initialize(&active, root, slot)?;
        let foreign = collection.slot(Incarnation::from_bytes([105; 16]));
        fs::create_dir(&foreign)?;
        let preserved = foreign.join("source.txt");
        fs::write(&preserved, b"unknown bytes must remain")?;
        let before = fs::read(collection.directory.join("service-owner.json"))?;
        let writer = active.lock()?;
        let quota = Quota::open_service(&data)?;
        assert!(install(&collection, &active, &writer, &quota, 3, || Ok(())).is_err());
        assert_eq!(
            fs::read(collection.directory.join("service-owner.json"))?,
            before
        );
        assert_eq!(fs::read(&preserved)?, b"unknown bytes must remain");
        Ok(())
    }
    #[test]
    #[ignore = "Subprocess crash entry; exercised by interrupted_partial_marker_stage_recovers_before_retry"]
    fn abrupt_partial_marker_stage_child() -> Result<()> {
        let base = PathBuf::from(
            std::env::var_os("AIRK_RETENTION_CRASH_BASE")
                .context("owned crash fixture required")?,
        )
        .canonicalize()?;
        let data = base.join("data");
        let source = base.join("source");
        let root = source.to_str().unwrap();
        let collection = Collection::existing(&data, root, "fixture")?;
        let store = collection.selected_store()?;
        let _writer = store.lock()?;
        let quota = Quota::open_service(&data)?;
        let reservation = quota.reserve("fixture", 131072)?;
        reservation.charge(4096)?;
        let stage = reservation.staging_directory()?;
        fs::write(base.join("owned-stage.txt"), stage.to_str().unwrap())?;
        let original = fs::read(store.directory().join("current-owner.json"))?;
        let _ = PreparedMarker::prepare(&reservation, &original, |file| {
            file.write_all(b"{")?;
            file.sync_all()?;
            std::process::exit(79);
        })?;
        anyhow::bail!("crash fixture unexpectedly returned")
    }
    #[test]
    fn interrupted_partial_marker_stage_recovers_before_retry() -> Result<()> {
        use std::process::{Command, Stdio};
        use std::time::{Duration, Instant};
        let temporary = tempfile::tempdir()?;
        let base = temporary.path().canonicalize()?;
        let data = base.join("data");
        let source = base.join("source");
        fs::create_dir(&data)?;
        fs::create_dir(&source)?;
        fs::write(source.join("original.rs"), b"original source")?;
        let root = source.to_str().unwrap();
        let slot = Incarnation::from_bytes([106; 16]);
        let collection = Collection::initialize(&data, root, "fixture", slot)?;
        let store = collection.selected_store()?;
        let token = super::super::tests::initialize(&store, root, slot)?;
        let stdout = File::create_new(base.join("child.stdout"))?;
        let stderr = File::create_new(base.join("child.stderr"))?;
        // The exact child executes only the marker/Quota fixture and spawns no
        // descendants. Existing Store child guard kills/reaps on every failure.
        let mut child=super::super::tests::ChildGuard(Command::new(std::env::current_exe()?)
            .args(["--exact","service::collection::retention_barrier::tests::abrupt_partial_marker_stage_child","--ignored","--test-threads=1","--nocapture"])
            .env("AIRK_RETENTION_CRASH_BASE",&base).stdin(Stdio::null()).stdout(stdout).stderr(stderr).spawn()?);
        let until = Instant::now() + Duration::from_secs(30);
        let status = loop {
            if let Some(status) = child.0.try_wait()? {
                break status;
            }
            ensure!(
                Instant::now() < until,
                "bounded marker crash child timed out"
            );
            std::thread::sleep(Duration::from_millis(10));
        };
        ensure!(
            status.code() == Some(79),
            "marker child did not reach exact crash boundary: {status}"
        );
        let stage = PathBuf::from(fs::read_to_string(base.join("owned-stage.txt"))?);
        ensure!(
            stage.starts_with(&collection.directory),
            "foreign fixture stage"
        );
        assert_eq!(fs::read(stage.join("retention-owner.json"))?, b"{");
        assert!(observed_contract(&collection.directory.join("service-owner.json"))?.is_null());
        // No Drop ran in the child. Actual ordinary reservation recovery owns
        // this partial file; no slot/root unknown-file allowlist is introduced.
        let quota = Quota::open_service(&data)?;
        let writer = store.lock()?;
        install(&collection, &store, &writer, &quota, 3, || Ok(()))?;
        assert!(
            !stage.exists(),
            "dead owned partial stage must be reconciled before fresh reservation"
        );
        let scope = CurrentScope {
            root: root.into(),
            collection: "fixture".into(),
            incarnation: slot,
        };
        assert_eq!(store.current_read(&scope)?.token()?, token);
        assert_eq!(fs::read(source.join("original.rs"))?, b"original source");
        Ok(())
    }
}
