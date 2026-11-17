# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import json,hashlib,difflib
root=Path('D:/code/Github/airk');base=root/'artifacts/v190/p10-worktree';out=root/'artifacts/v190/p11b-owner-barrier-r2-proposed'
# Reuse transformations, never rewrite the preserved v1 artifact or live inputs.
seed=(root/'artifacts/v190/p11b-prepare-owner-barrier.py').read_text(encoding='utf-8')
exec(compile(seed.replace("p11b-owner-barrier-proposed","p11b-owner-barrier-r2-proposed"),'<inert-v1-seed>','exec'))
paths=json.loads((out/'baseline.json').read_text(encoding='utf-8'))
service='crates/airk-store/src/service_collection.rs';journal='crates/airk-store/src/current/journal.rs';database='crates/airk-store/src/current/database.rs';tests='crates/airk-store/src/service_collection_tests.rs'
def read(p):return (out/p).read_text(encoding='utf-8')
def write(p,s):
    dest=out/p;dest.parent.mkdir(parents=True,exist_ok=True);dest.write_text(s,encoding='utf-8',newline='\n')
s=read(service)
start=s.index('    /// Installs the fail-closed predecessor barrier')
end=s.index('    pub fn required_source_policy',start)
s=s[:start]+'''    /// Native reviewed migration barrier; excludes every recoverable slot reader.
    pub fn require_extraction_contract(
        &self, store: &FileStore, writer: &crate::WriterLock, quota: &Quota, contract: u32,
    ) -> Result<()> {
        retention_barrier::install(self, store, writer, quota, contract, || Ok(()))
    }

'''+s[end:]
s=s.replace('#[path = "service_collection_setup.rs"]','#[path = "service_collection_retention.rs"]\nmod retention_barrier;\n\n#[path = "service_collection_setup.rs"]',1)
start=s.index('pub(crate) fn required_extraction_contract_for_store(')
end=s.index('\n#[derive(Clone, Serialize, Deserialize)]',start)
s=s[:start]+'''pub(crate) fn required_extraction_contract_for_store(
    store: &FileStore,
) -> Result<Option<RequiredExtractionContract>> {
    let Ok(managed) = store.managed_path() else { return Ok(None); };
    if managed.namespace != "collections-1.4" || managed.slot.is_none() { return Ok(None); }
    let pin = crate::service::registry::catalog::setup::storage::SetupDirectory::open_canonical(managed.logical)?;
    let owner: Owner = retention_barrier::read_json(&pin, "service-owner.json", MAX_METADATA as u64)?;
    ensure!(owner.version == 140 && Path::new(&owner.data_root) == managed.root && owner.collection == managed.collection,
        "foreign extraction capability owner");
    Ok(owner.required_extraction_contract)
}
pub(crate) fn required_extraction_contract_for_scope(
    store: &FileStore, scope: &crate::current::CurrentScope,
) -> Result<Option<RequiredExtractionContract>> {
    let managed = store.managed_path()?;
    ensure!(managed.namespace == "collections-1.4" && managed.slot.is_some(),
        "retained contract requires native service incarnation");
    let pin = crate::service::registry::catalog::setup::storage::SetupDirectory::open_canonical(managed.logical)?;
    let owner: Owner = retention_barrier::read_json(&pin, "service-owner.json", MAX_METADATA as u64)?;
    ensure!(owner.version == 140 && Path::new(&owner.data_root) == managed.root && owner.collection == managed.collection
        && owner.root == scope.root && owner.collection == scope.collection
        && store.service_incarnation()? == Some(scope.incarnation), "foreign scoped extraction capability owner");
    Ok(owner.required_extraction_contract)
}
'''+s[end:]
write(service,s)
s=read(database);start=s.index('/// Native writer + exclusive current Gate are held');end=s.index('pub(crate) fn create_owner(',start)
s=s[:start]+'''/// Read the actual retained owner HANDLE; caller holds the exact writer/Gate.
pub(super) fn extraction_contract_bytes(
    directory: &Path, file: &mut std::fs::File, root: &str, collection: &str,
    contract: u32,
) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    file.take(65_537).read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= 65_536, "current ownership header exceeds bound");
    let mut owner = validate_owned_owner(directory, parse_owner(&bytes)?)?;
    ensure!(owner.scope.root == root && owner.scope.collection == collection,
        "retained contract current scope differs from stable owner");
    owner.required_extraction_contract = Some(crate::service::collection::RequiredExtractionContract::try_from(contract)?);
    let bytes = serde_json::to_vec(&owner)?;
    ensure!(bytes.len() <= 65_536, "current capability header exceeds bound");
    Ok(bytes)
}

pub(super) fn validate_extraction_pending(directory: &Path, bytes: &[u8]) -> Result<()> {
    let pending = validate_owned_owner(directory, parse_owner(bytes)?)?;
    let current = read_owner(directory)?;
    ensure!(pending.scope == current.scope
        && pending.required_extraction_contract == Some(crate::service::collection::RequiredExtractionContract::try_from(3)?),
        "foreign pending extraction capability owner");
    Ok(())
}

'''+s[end:];write(database,s)
s=read(journal);start=s.index('    pub(crate) fn require_extraction_contract_with(');end=s.index("    pub fn current_write<'a>(",start)
s=s[:start]+'''    pub(crate) fn prepare_extraction_contract(
        &self, owning_store: &FileStore, writer: &WriterLock, quota: &Quota,
        pin: crate::service::registry::catalog::setup::storage::SetupDirectory,
        logical: &crate::service::registry::catalog::setup::storage::SetupDirectory,
        root: &str, collection: &str, contract: u32,
    ) -> Result<PreparedExtractionContract> {
        crate::maintenance::check_writer(owning_store, writer, quota)?;
        quota.require_store_funding(self)?;
        crate::service::collection::validate_split_store_access(self)?;
        let gate = Gate::exclusive_pinned_slot(owning_store, writer, self, logical)?;
        let expected = match pin.open_file(std::ffi::OsStr::new(database::OWNER_FILE),
            crate::service::registry::catalog::setup::storage::SetupFileMode::Read) {
            Ok(file) => Some(file),
            Err(error) if error.downcast_ref::<std::io::Error>().is_some_and(|e|e.kind()==std::io::ErrorKind::NotFound) => None,
            Err(error) => return Err(error),
        };
        let mut prepared = PreparedExtractionContract { pin, _gate: gate, expected, temporary: None,
            _reservation: None, had_current: false };
        if let Some(expected) = prepared.expected.as_mut() {
            let bytes = database::extraction_contract_bytes(self.directory(), expected, root, collection, contract)?;
            match prepared.pin.open_file(std::ffi::OsStr::new(".current-retention-pending"),
                crate::service::registry::catalog::setup::storage::SetupFileMode::Read) {
                Ok(file) => {
                    let old = prepared.pin.read_file(std::ffi::OsStr::new(".current-retention-pending"), 65_536)?;
                    database::validate_extraction_pending(self.directory(), &old)?;
                    prepared.pin.unlink_file_if_identity(std::ffi::OsStr::new(".current-retention-pending"), &file)?;
                },
                Err(error) if error.downcast_ref::<std::io::Error>().is_some_and(|e|e.kind()==std::io::ErrorKind::NotFound) => (),
                Err(error) => return Err(error),
            }
            let reservation = quota.reserve(collection, 2 * 65_536)?;
            reservation.charge(bytes.len() as u64)?;
            prepared.temporary = Some(crate::service::collection::retention_barrier::PreparedMarker::new(
                &prepared.pin, ".current-retention-pending", &bytes)?);
            prepared._reservation = Some(reservation);
            prepared.had_current = true;
        } else {
            require_absent_current(&prepared.pin)?;
            require_absent_native_entries(&prepared.pin,
                &["current-runtime-bootstrap.json", ".current-retention-pending"])?;
        }
        writer.invalidate_census_before_mutation()?;
        Ok(prepared)
    }

    pub(crate) fn validate_retention_pending(&self) -> Result<()> {
        let pin = crate::service::registry::catalog::setup::storage::SetupDirectory::open_canonical(self.directory())?;
        let bytes = pin.read_file(std::ffi::OsStr::new(".current-retention-pending"), 65_536)?;
        database::validate_extraction_pending(self.directory(), &bytes)
    }

'''+s[end:]
s+='''

/// Keeps exact current directory/expected owner and exclusive Gate through publish.
pub(crate) struct PreparedExtractionContract {
    pin: crate::service::registry::catalog::setup::storage::SetupDirectory,
    _gate: Gate,
    expected: Option<std::fs::File>,
    temporary: Option<crate::service::collection::retention_barrier::PreparedMarker>,
    _reservation: Option<crate::quota::Reservation>,
    had_current: bool,
}
impl Drop for PreparedExtractionContract {
    fn drop(&mut self) { drop(self.temporary.take()); }
}
impl PreparedExtractionContract {
    pub(crate) fn had_current(&self) -> bool { self.had_current }
    pub(crate) fn publish(&mut self) -> Result<()> {
        if let Some(temporary) = self.temporary.as_mut() {
            temporary.publish(&self.pin, database::OWNER_FILE,
                self.expected.as_ref().context("current owner expectation missing")?)?;
        } else {
            require_absent_current(&self.pin)?;
            require_absent_native_entries(&self.pin,
                &["current-runtime-bootstrap.json", ".current-retention-pending"])?;
        }
        Ok(())
    }
}
'''
write(journal,s)
# Current owns the prepared lower marker; shared helper remains crate-visible.
s=read(service).replace('mod retention_barrier;','pub(crate) mod retention_barrier;',1);write(service,s)
s=read(service)
anchor='''                [name] if metadata.is_file() => matches!('''
assert s.count(anchor)==1
s=s.replace(anchor,'''                [".current-retention-pending"] if metadata.is_file() => {
                    FileStore::new(directory).validate_retention_pending()?;
                    true
                }
'''+anchor)
write(service,s)
s=read(tests).replace('fn initialize(store: &FileStore, root: &str, slot: Incarnation)', 'pub(super) fn initialize(store: &FileStore, root: &str, slot: Incarnation)',1);write(tests,s)
module='crates/airk-store/src/service_collection_retention.rs'
write(module,'''// SPDX-License-Identifier: Apache-2.0
//! Native bounded predecessor capability publication; no source or index mutation.
use super::*;
use crate::service::registry::catalog::setup::storage::{SetupDirectory, SetupFileMode, file_identity};
use std::ffi::OsStr;

pub(super) fn read_json<T: serde::de::DeserializeOwned + Serialize>(pin: &SetupDirectory, name: &str, limit: u64) -> Result<T> {
    let bytes = pin.read_file(OsStr::new(name), limit)?;
    let value: T = serde_json::from_slice(&bytes)?;
    ensure!(serde_json::to_vec(&value)? == bytes, "noncanonical retained capability metadata");
    Ok(value)
}
/// A paid temporary in the captured directory; Drop removes only its exact object.
pub(crate) struct PreparedMarker { name: String, file: File, pin: SetupDirectory, published: bool }
impl PreparedMarker {
    pub(crate) fn new(pin: &SetupDirectory, name: &str, bytes: &[u8]) -> Result<Self> {
        let captured = pin.try_clone()?;
        let file = match pin.open_file(OsStr::new(name), SetupFileMode::CreateNew) {
            Ok(file) => file,
            Err(error) if error.downcast_ref::<std::io::Error>().is_some_and(|e|e.kind()==std::io::ErrorKind::AlreadyExists) => {
                let existing = pin.open_file(OsStr::new(name), SetupFileMode::Read)?;
                ensure!(pin.read_file(OsStr::new(name), bytes.len() as u64)? == bytes,
                    "foreign or changed pending capability metadata");
                return Ok(Self { name: name.into(), file: existing, pin: captured, published: false });
            },
            Err(error) => return Err(error),
        };
        let mut prepared = Self { name: name.into(), file, pin: captured, published: false };
        prepared.file.write_all(bytes)?; prepared.file.sync_all()?;
        Ok(prepared)
    }
    pub(crate) fn publish(&mut self, pin: &SetupDirectory, target: &str, expected: &File) -> Result<()> {
        ensure!(self.pin.identity()? == pin.identity()?, "foreign retained publication directory");
        pin.replace_file(OsStr::new(&self.name), &self.file, OsStr::new(target), expected)?;
        self.published = true;
        Ok(())
    }
}
impl Drop for PreparedMarker {
    fn drop(&mut self) { if !self.published { let _ = self.pin.unlink_file_if_identity(OsStr::new(&self.name), &self.file); } }
}
pub(super) fn install(
    collection: &Collection, store: &FileStore, writer: &crate::WriterLock, quota: &Quota,
    contract: u32, after_stable: impl FnOnce() -> Result<()>,
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
    let mut bytes = Vec::new(); (&mut expected).take(MAX_METADATA as u64 + 1).read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= MAX_METADATA, "stable capability owner exceeds bound");
    let mut owner: Owner = serde_json::from_slice(&bytes)?;
    ensure!(serde_json::to_vec(&owner)? == bytes && owner_digest(&owner)? == collection.binding
        && owner.root == collection.owner.root && owner.data_root == collection.owner.data_root
        && owner.collection == collection.owner.collection, "stable retained owner changed");
    let manifest: Manifest = read_json(&pin, "active.json", MAX_METADATA as u64)?;
    ensure!(manifest.version == 140 && manifest.owner == collection.binding
        && manifest.retired.len() <= MAX_RETIRED, "foreign retained selection manifest");
    let mut slots = std::collections::BTreeSet::from([manifest.active]);
    slots.extend(manifest.retired.iter().map(|r|r.slot));
    ensure!(slots.len() == manifest.retired.len() + 1, "duplicate or active retired slot");
    match pin.read_file(OsStr::new("replacement.json"), MAX_METADATA as u64) {
        Ok(bytes) => { let intent: Intent = serde_json::from_slice(&bytes)?;
            ensure!(serde_json::to_vec(&intent)? == bytes && intent.version == 140
                && intent.owner == collection.binding, "foreign recoverable retention target"); slots.insert(intent.slot); },
        Err(error) if error.downcast_ref::<std::io::Error>().is_some_and(|e|e.kind()==std::io::ErrorKind::NotFound) => (),
        Err(error) => return Err(error),
    }
    ensure!(slots.len() <= MAX_RETIRED + 2, "retained slot roster exceeds bound");
    let incarnations = pin.open_dir(OsStr::new("incarnations"))?;
    let entries = incarnations.entries()?.take(MAX_RETIRED + 3).collect::<Result<Vec<_>>>()?;
    ensure!(entries.len() <= MAX_RETIRED + 2, "retained slot inventory exceeds bound");
    let mut actual = std::collections::BTreeSet::new();
    for name in entries { let slot: Incarnation = name.to_str().context("non-UTF8 retained slot")?.parse()?;
        ensure!(slots.contains(&slot), "unowned incarnation preserved during retention migration"); actual.insert(slot); }
    ensure!(actual.contains(&manifest.active), "active retained slot missing");
    let active_pin = incarnations.open_dir(OsStr::new(&manifest.active.to_string()))?;
    ensure!(SetupDirectory::open_canonical(store.directory())?.identity()? == active_pin.identity()?,
        "retention migration requires exact active slot");
    // This is an idempotent already-installed capability check, NOT fresh
    // absence or Ready proof. The exact native split owner may resume its own
    // bootstrap; canonical current recovery still validates every existing byte.
    if owner.required_extraction_contract == Some(required) && slots.len() == 1 {
        match active_pin.open_file(OsStr::new("current-owner.json"), SetupFileMode::Read) {
            Err(error) if error.downcast_ref::<std::io::Error>().is_some_and(|e|e.kind()==std::io::ErrorKind::NotFound) => {
                if let Some(permit) = store.split_access.as_ref() {
                    let _gate = Gate::exclusive_pinned_slot(store, writer, store, &pin)?;
                    if permit.retained_policy_already_installed(store, &pin, &active_pin, quota)? {
                        return Ok(());
                    }
                }
            },
            Ok(_) => (),
            Err(error) => return Err(error),
        }
    }
    let mut markers = Vec::new();
    for slot in actual {
        let candidate = collection.context_store(collection.slot(slot))?;
        let candidate_pin = incarnations.open_dir(OsStr::new(&slot.to_string()))?;
        ensure!(SetupDirectory::open_canonical(candidate.directory())?.identity()? == candidate_pin.identity()?,
            "retained slot pathname changed");
        let marker = candidate.prepare_extraction_contract(store, writer, quota, candidate_pin,
            &pin, &owner.root, &owner.collection, contract)?;
        if slot == manifest.active && !marker.had_current() {
            let permit = store.split_access.as_ref().context("fresh retention requires claimed DestinationBuild")?;
            let funding = quota.funding_use().context("fresh retention requires exact paid phase")?;
            permit.validate_funding_use(funding)?;
            ensure!(funding.phase.purpose == airk_core::catalog_split::SplitJobPurpose::DestinationBuild,
                "only exact DestinationBuild can initialize retained policy");
            permit.validate_pinned_store(store, &pin)?;
        }
        markers.push(marker);
    }
    // Every native writer/Gate and exact expected header is held before ANY
    // effect. A crash here leaves only stricter capability metadata, never new
    // policy/profile bytes or an altered publication. Retry seals all slots.
    owner.required_extraction_contract = Some(required);
    match pin.open_file(OsStr::new(".service-metadata-retention-pending"), SetupFileMode::Read) {
        Ok(file) => {
            let prior: Owner = read_json(&pin, ".service-metadata-retention-pending", MAX_METADATA as u64)?;
            ensure!(owner_digest(&prior)? == collection.binding && prior.root == owner.root
                && prior.data_root == owner.data_root && prior.collection == owner.collection
                && prior.required_extraction_contract == Some(required),
                "foreign pending stable extraction capability");
            pin.unlink_file_if_identity(OsStr::new(".service-metadata-retention-pending"), &file)?;
        },
        Err(error) if error.downcast_ref::<std::io::Error>().is_some_and(|e|e.kind()==std::io::ErrorKind::NotFound) => (),
        Err(error) => return Err(error),
    }
    let bytes = serde_json::to_vec(&owner)?;
    let reservation = quota.reserve(&owner.collection, MAX_METADATA as u64 * 2)?;
    reservation.charge(bytes.len() as u64)?;
    let mut prepared = PreparedMarker::new(&pin, ".service-metadata-retention-pending", &bytes)?;
    ensure!(SetupDirectory::open_canonical(&collection.directory)?.identity()? == pin.identity()?, "retained collection rebound");
    ensure!(file_identity(&pin.open_file(OsStr::new("service-owner.json"), SetupFileMode::Read)?)? == file_identity(&expected)?,
        "retained stable owner replaced before commit");
    prepared.publish(&pin, "service-owner.json", &expected)?;
    after_stable()?;
    for marker in &mut markers { marker.publish()?; }
    ensure!(SetupDirectory::open_canonical(&collection.directory)?.identity()? == pin.identity()?,
        "retention collection pathname changed before configuration handoff");
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
    fn all_recoverable_slot_readers_refuse_then_stable_interruption_retries_without_publication_change() -> Result<()> {
        let temporary = tempfile::tempdir()?;
        let base = temporary.path().canonicalize()?;
        let source = base.join("source"); let data = base.join("data");
        fs::create_dir(&source)?; fs::create_dir(&data)?;
        fs::write(source.join("original.rs"), b"pub struct Original;")?;
        let root = source.to_str().context("fixture root UTF8")?;
        let first = Incarnation::from_bytes([101;16]); let second = Incarnation::from_bytes([102;16]);
        let collection = Collection::initialize(&data, root, "fixture", first)?;
        let active = collection.selected_store()?;
        let token = super::super::tests::initialize(&active, root, first)?;
        let replacement = collection.begin_replacement(second, Sha256Digest::from_bytes([103;32]))?;
        let candidate = replacement.store();
        super::super::tests::initialize(&candidate, root, second)?;
        drop(replacement); // Actual durable replacement intent survives ownership loss.
        let candidate_scope = CurrentScope { root: root.into(), collection: "fixture".into(), incarnation: second };
        let read = candidate.current_read(&candidate_scope)?;
        let writer = active.lock()?; let quota = Quota::open_service(&data)?;
        let stable_before = fs::read(collection.directory.join("service-owner.json"))?;
        let active_before = fs::read(active.directory().join("current-owner.json"))?;
        let candidate_before = fs::read(candidate.directory().join("current-owner.json"))?;
        let refused = install(&collection, &active, &writer, &quota, 3, || Ok(())).unwrap_err();
        assert!(refused.downcast_ref::<crate::current::CurrentGateContention>().is_some());
        assert_eq!(fs::read(collection.directory.join("service-owner.json"))?, stable_before);
        assert_eq!(fs::read(active.directory().join("current-owner.json"))?, active_before);
        assert_eq!(fs::read(candidate.directory().join("current-owner.json"))?, candidate_before);
        drop(read);
        let interrupted = install(&collection, &active, &writer, &quota, 3,
            || anyhow::bail!("injected exact stable-before-lower interruption"));
        assert!(interrupted.is_err());
        assert_eq!(observed_contract(&collection.directory.join("service-owner.json"))?, 3);
        assert_eq!(fs::read(active.directory().join("current-owner.json"))?, active_before);
        assert_eq!(fs::read(candidate.directory().join("current-owner.json"))?, candidate_before);
        assert!(!active.directory().join(".current-retention-pending").exists());
        assert!(!candidate.directory().join(".current-retention-pending").exists());
        install(&collection, &active, &writer, &quota, 3, || Ok(()))?;
        assert_eq!(observed_contract(&active.directory().join("current-owner.json"))?, 3);
        assert_eq!(observed_contract(&candidate.directory().join("current-owner.json"))?, 3);
        let active_scope = CurrentScope { root: root.into(), collection: "fixture".into(), incarnation: first };
        assert_eq!(active.current_read(&active_scope)?.token()?, token);
        assert!(required_extraction_contract_for_scope(&active, &candidate_scope).is_err());
        assert_eq!(required_extraction_contract_for_scope(&active, &active_scope)?.map(u32::from), Some(3));
        drop(writer);
        // Real recovery classifies the abandoned slot but cannot replace the
        // protected active publication. Both retained headers stay sealed.
        collection.recover_abandoned_replacement()?;
        assert_eq!(collection.manifest()?.active, first);
        assert!(collection.manifest()?.retired.iter().any(|row|row.slot==second));
        assert_eq!(observed_contract(&candidate.directory().join("current-owner.json"))?, 3);
        assert_eq!(fs::read(source.join("original.rs"))?, b"pub struct Original;");
        Ok(())
    }
    #[test]
    fn unowned_slot_and_malformed_pending_marker_are_preserved_before_capability_effect() -> Result<()> {
        for unknown_slot in [false, true] {
            let temporary = tempfile::tempdir()?; let base = temporary.path().canonicalize()?;
            let source=base.join("source"); let data=base.join("data");
            fs::create_dir(&source)?; fs::create_dir(&data)?;
            let root=source.to_str().unwrap(); let slot=Incarnation::from_bytes([104;16]);
            let collection=Collection::initialize(&data,root,"fixture",slot)?;
            let active=collection.selected_store()?; super::super::tests::initialize(&active,root,slot)?;
            let preserved=if unknown_slot {
                let foreign=collection.slot(Incarnation::from_bytes([105;16])); fs::create_dir(&foreign)?;
                foreign.join("source.txt")
            } else { active.directory().join(".current-retention-pending") };
            fs::write(&preserved,b"unknown bytes must remain")?;
            let before=fs::read(collection.directory.join("service-owner.json"))?;
            let writer=active.lock()?; let quota=Quota::open_service(&data)?;
            assert!(install(&collection,&active,&writer,&quota,3,||Ok(())).is_err());
            assert_eq!(fs::read(collection.directory.join("service-owner.json"))?,before);
            assert_eq!(fs::read(&preserved)?,b"unknown bytes must remain");
        }
        Ok(())
    }
}
''')
paths[module]=None
gate='crates/airk-store/src/current/gate.rs'
gate_source=(base/gate).read_text(encoding='utf-8')
anchor='''    pub(crate) fn exclusive(store: &FileStore, writer: &WriterLock) -> Result<Self> {'''
helper='''    /// One logical collection writer excludes all incarnation writers. Retain
    /// that real advisory HANDLE; do not fabricate per-slot WriterLock paths.
    pub(crate) fn exclusive_pinned_slot(
        owning_store: &FileStore, writer: &WriterLock, target: &FileStore,
        logical: &crate::service::registry::catalog::setup::storage::SetupDirectory,
    ) -> Result<Self> {
        use crate::service::registry::catalog::setup::storage::{SetupDirectory, SetupFileMode, file_identity};
        use std::ffi::OsStr;
        check_writer(owning_store, writer)?;
        let owner = owning_store.managed_path()?;
        let selected = target.managed_path()?;
        ensure!(owner.namespace == "collections-1.4" && owner.logical == selected.logical
            && owner.root == selected.root && owner.collection == selected.collection,
            "foreign collection writer for retained slot");
        let namespace = SetupDirectory::open_canonical(owner.logical.parent().context("native namespace missing")?)?;
        ensure!(namespace.open_dir(OsStr::new(owner.collection))?.identity()? == logical.identity()?,
            "logical retained owner directory changed");
        let advisory = namespace.open_dir(OsStr::new(".writer-locks"))?
            .open_file(OsStr::new(owner.collection), SetupFileMode::ReadWrite)?;
        ensure!(file_identity(&advisory)? == file_identity(&writer._file)?,
            "retained writer advisory object changed");
        let gates = logical.create_dir(OsStr::new(".current-gates"))?;
        let slot = selected.slot.context("retained slot requires native incarnation")?;
        let file = match gates.open_file(OsStr::new(slot), SetupFileMode::ReadWrite) {
            Ok(file) => file,
            Err(error) if error.downcast_ref::<std::io::Error>().is_some_and(|e|e.kind()==std::io::ErrorKind::NotFound) => {
                match gates.open_file(OsStr::new(slot), SetupFileMode::CreateNew) {
                    Ok(file) => file,
                    Err(error) if error.downcast_ref::<std::io::Error>().is_some_and(|e|e.kind()==std::io::ErrorKind::AlreadyExists) =>
                        gates.open_file(OsStr::new(slot), SetupFileMode::ReadWrite)?,
                    Err(error) => return Err(error),
                }
            },
            Err(error) => return Err(error),
        };
        acquired(file.try_lock(), CurrentGateContention::Readers)?;
        Ok(Self { file, directory: target.directory().into(), _lifecycle: None })
    }

'''
assert gate_source.count(anchor)==1
write(gate,gate_source.replace(anchor,helper+anchor));paths[gate]=hashlib.sha256((base/gate).read_bytes()).hexdigest()
policy='crates/airk-store/src/extraction_policy.rs'
policy_source=(base/policy).read_text(encoding='utf-8')
policy_source+='''

/// Opaque existing metadata through the retained native directory. No source
/// capture, producer execution or ingestion interpretation belongs to Store.
pub(crate) fn extraction_policy_bytes_at(
    pin: &crate::service::registry::catalog::setup::storage::SetupDirectory, owner_root: &str,
) -> Result<Option<Vec<u8>>> {
    let bytes = match pin.read_file(std::ffi::OsStr::new("extraction-policy.json"), MAX_BYTES as u64) {
        Ok(bytes) => bytes,
        Err(error) if error.downcast_ref::<std::io::Error>().is_some_and(|e|e.kind()==std::io::ErrorKind::NotFound) => return Ok(None),
        Err(error) => return Err(error),
    };
    let policy: StoredPolicy = serde_json::from_slice(&bytes)?;
    ensure!(policy.version == 1 && policy.owner_root == owner_root, "foreign pinned extraction policy");
    let bytes = serde_json::to_vec(&policy.policy)?;
    ensure!(bytes.len() <= 131_072, "pinned extraction policy exceeds bound");
    Ok(Some(bytes))
}
'''
write(policy,policy_source);paths[policy]=hashlib.sha256((base/policy).read_bytes()).hexdigest()
split='crates/airk-store/src/service_collection_split.rs'
split_source=(base/split).read_text(encoding='utf-8');anchor='impl SplitBuildPermit {'
helper='''
    pub(super) fn retained_policy_already_installed(
        &self, store: &FileStore,
        logical: &crate::service::registry::catalog::setup::storage::SetupDirectory,
        slot: &crate::service::registry::catalog::setup::storage::SetupDirectory,
        quota: &Quota,
    ) -> Result<bool> {
        self.validate_pinned_store(store, logical)?;
        let funding = quota.funding_use().context("installed retained policy requires exact funding")?;
        self.validate_funding_use(funding)?;
        quota.require_store_funding(store)?;
        ensure!(self.purpose == SplitJobPurpose::DestinationBuild,
            "only exact DestinationBuild can observe installed initial retention");
        let owner: Owner = super::retention_barrier::read_json(logical, "service-owner.json", MAX_METADATA as u64)?;
        if owner.required_extraction_contract != Some(super::RequiredExtractionContract::try_from(3)?) { return Ok(false); }
        let pointer = owner.required_split.as_ref().context("installed retained capability has no split owner")?;
        let bytes = logical.read_file(std::ffi::OsStr::new(&filename(pointer.digest)), MAX_SPLIT_STATE as u64)?;
        ensure!(framed_digest(b"airk/stable-split-state/1", &[&bytes]) == pointer.digest,
            "installed retained split capsule changed");
        let capsule: Capsule = serde_json::from_slice(&bytes)?;
        capsule.snapshot.validate()?;
        ensure!(capsule.schema == 1 && serde_json::to_vec(&capsule)? == bytes
            && capsule.snapshot.receipt.revision == self.revision
            && capsule.snapshot.receipt.operation_id == self.operation
            && capsule.snapshot.receipt.request_digest == self.request,
            "installed retained split phase changed");
        let Some(reference) = capsule.snapshot.request.extraction_policy.as_ref() else { return Ok(false); };
        let Some(body) = crate::extraction_policy::extraction_policy_bytes_at(slot, &self.registration.root)?
        else { return Ok(false); };
        reference.validate_body(&body)?;
        Ok(true)
    }
'''
assert split_source.count(anchor)==1
write(split,split_source.replace(anchor,anchor+helper,1));paths[split]=hashlib.sha256((base/split).read_bytes()).hexdigest()
(out/'baseline.json').write_text(json.dumps(paths,indent=2)+'\n',encoding='utf-8')
print(json.dumps({'r2_files':len(paths),'status':'inert unformatted initial r2; review/fault tests pending'}))
