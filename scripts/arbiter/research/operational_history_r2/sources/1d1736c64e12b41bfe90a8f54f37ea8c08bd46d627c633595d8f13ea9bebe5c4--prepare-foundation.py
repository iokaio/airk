# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import difflib

base = Path(__file__).parent
root = Path.cwd()
changes = {}
for name in ('service_collection.rs', 'service_collection_purge.rs', 'service_registry.rs', 'service_data_scope.rs', 'service_collection_tests.rs'):
    path = 'crates/airk-store/src/' + name
    original = (root / path).read_text(encoding='utf-8')
    changed = original
    if name == 'service_collection.rs':
        changed = changed.replace('#[path = "service_collection_purge.rs"]\nmod purge;', '#[path = "service_collection_purge.rs"]\nmod purge;\n#[path = "service_collection_integrity.rs"]\npub mod integrity;')
        changed = changed.replace('let owned = cleanup::owned_scratch(&path, self.binding)', 'let owned = integrity::owned_scratch(&path, self.binding)\n                || cleanup::owned_scratch(&path, self.binding)')
    elif name == 'service_collection_purge.rs':
        changed = changed.replace('"service-owner.json"\n', '"service-owner.json"\n                        | "integrity-observation.json"\n')
    elif name == 'service_data_scope.rs':
        changed = changed.replace('impl BoundDataRoot {', '''impl BoundDataRoot {
    pub(crate) fn require_identity(&self, data: &Path, control: &Path, namespace: Incarnation) -> Result<()> {
        self.check()?;
        ensure!(self.0.data == data && self.0.control == control && self.0.namespace == namespace,
            "foreign integrity data root");
        Ok(())
    }
''', 1)
    elif name == 'service_collection_tests.rs':
        changed += base.joinpath('foundation-tests.rs').read_text(encoding='utf-8')
        changed += base.joinpath('lifecycle-tests.rs').read_text(encoding='utf-8')
        changed = changed.replace('    let token = initialize(&store, source_text, slot)?;\n', '''    let token = initialize(&store, source_text, slot)?;
    // F10 receipt follows physical relocation without promoting its audit basis.
    let mut integrity_lease = crate::service::ProjectLease::acquire(&control, registration.binding)?;
    registry.admit_worker(&mut integrity_lease)?;
    let integrity_bound = data_scope::bind(&data, &control, namespace)?;
    let integrity_scope = store.current_owned_scope()?.context("initial scope missing")?;
    let integrity_read = store.current_read(&integrity_scope)?;
    collection.integrity_owner(&registry, &integrity_lease, &integrity_bound)?
        .record_damage(&integrity_read, token.digest, 700)?;
    let integrity_before = fs::read(collection.directory.join("integrity-observation.json"))?;
    drop(integrity_read);
    drop(integrity_lease);
''', 1)
        changed = changed.replace('    let collection = Collection::initialize(&destination, source_text, "fixture", slot)?;\n', '''    let collection = Collection::initialize(&destination, source_text, "fixture", slot)?;
    assert_eq!(fs::read(collection.directory.join("integrity-observation.json"))?, integrity_before);
    let mut resumed_registry = ServiceRegistry::open_saved(&control)?;
    let mut resumed_lease = crate::service::ProjectLease::acquire(&control, migrated.binding)?;
    resumed_registry.admit_worker(&mut resumed_lease)?;
    let resumed_bound = data_scope::bind(&destination, &control, namespace)?;
    let resumed_store = collection.selected_store()?;
    let resumed_scope = resumed_store.current_owned_scope()?.context("relocated scope missing")?;
    let resumed_read = resumed_store.current_read(&resumed_scope)?;
    let resumed_state = collection.integrity_owner(&resumed_registry, &resumed_lease, &resumed_bound)?.anchor(&resumed_read)?;
    assert_eq!(resumed_state.known_damage.len(), 1);
    assert_eq!(resumed_state.known_damage[0].component, token.digest);
    drop(resumed_read);
    drop(resumed_lease);
''', 1)
    else:
        changed += '''
impl ServiceRegistry {
    /// Live service ownership only; this does not certify platform privacy or payload bytes.
    pub(crate) fn integrity_registration(&self, lease: &ProjectLease,
        bound: &super::data_scope::BoundDataRoot) -> Result<(ProjectRegistration, Incarnation, u64)> {
        ensure!(!lease.derived, "cleanup worker cannot publish integrity state");
        let epoch = jobs::worker_epoch(&self.connection, &self.root, lease)?;
        ensure!(self.is_registered(lease.binding)?, "integrity worker registration removed");
        let registration = self.get(lease.binding)?.registration;
        let namespace = self.incarnation()?;
        bound.require_identity(Path::new(&registration.data_root), &self.root, namespace)?;
        Ok((registration, namespace, epoch))
    }
}
'''
    changes[path] = (original, changed)
module = 'crates/airk-store/src/service_collection_integrity.rs'
for name in ('enrichment.rs','progress.rs','indexing.rs','update.rs','publication.rs','metadata.rs'):
    path = 'crates/airk-kernel/src/current/' + name
    old = (root / path).read_text(encoding='utf-8')
    new = old
    if name in ('enrichment.rs','progress.rs'):
        new = new.replace('    pub fn resource_change(\n        &self,\n        store: &FileStore,',
            '    pub fn resource_change(\n        &self,\n        store: &FileStore,\n        writer: &airk_store::WriterLock,')
        new = new.replace('store.retain_current_resource(', 'store.retain_current_resource_owned(writer, ')
        new = new.replace('self.store.retain_current_resource_owned(writer, &bytes, &reservation)',
            'self.store.retain_current_resource_owned(&writer, &bytes, &reservation)')
        new = new.replace('.resource_change(&self.store, &reservation)', '.resource_change(&self.store, &writer, &reservation)')
    elif name == 'indexing.rs':
        new = new.replace('.resource_change(&self.kernel.store,', '.resource_change(&self.kernel.store, self.writer,')
        new = new.replace('.resource_change(&self.store,', '.resource_change(&self.store, &writer,')
    elif name == 'update.rs':
        new = new.replace('.resource_change(self.store, self.reservation)', '.resource_change(self.store, self.writer, self.reservation)')
        new = new.replace('.retain_current_resource(&manifest.canonical_bytes()?, self.reservation)',
            '.retain_current_resource_owned(self.writer, &manifest.canonical_bytes()?, self.reservation)')
        new = new.replace('let metadata = super::metadata::prepare(\n            self.store,',
            'let metadata = super::metadata::prepare(\n            self.store,\n            self.writer,')
    elif name == 'publication.rs':
        new = new.replace('.retain_current_resource(&analysis.canonical_bytes()?, reservation)',
            '.retain_current_resource_owned(writer, &analysis.canonical_bytes()?, reservation)')
    else:
        new = new.replace('pub(super) fn prepare(\n    store: &FileStore,',
            'pub(super) fn prepare(\n    store: &FileStore,\n    writer: &airk_store::WriterLock,')
        new = new.replace('store.retain_current_metadata_encoded(\n            path,',
            'store.retain_current_metadata_encoded_owned(\n            writer,\n            path,')
    if name == 'indexing.rs':
        new = new.replace('self.store.retain_payload_encoded(\n', 'self.store.retain_payload_encoded_owned(\n                        &writer,\n')
    changes[path] = (old, new)
for name in ('compression.rs','analysis.rs','lib.rs','limits.rs','roots.rs'):
    path = 'crates/airk-kernel/src/' + name
    old = (root / path).read_text(encoding='utf-8')
    new = old.replace('self.store.retain_payload_encoded(\n', 'self.store.retain_payload_encoded_owned(\n                &writer,\n')
    new = new.replace('self.store.retain_payload_with_budget(\n', 'self.store.retain_payload_with_budget_owned(\n                &writer,\n')
    new = new.replace('.retain_artifact_with_budget(artifact, None, Some(&reservation))',
        '.retain_artifact_with_budget_owned(&writer, artifact, None, Some(&reservation))')
    if name == 'limits.rs':
        new = new.replace('        let mut temporary = tempfile::NamedTempFile::new_in(self.store.directory())?;',
            '        self.store.invalidate_owned_state_before_mutation(&_lock)?;\n        let mut temporary = tempfile::NamedTempFile::new_in(self.store.directory())?;')
    if name == 'roots.rs':
        new = new.replace('        config.save(self.store.directory())?;',
            '        self.store.invalidate_owned_state_before_mutation(&lock)?;\n        config.save(self.store.directory())?;')
    changes[path] = (old, new)
for path in ('crates/airk-store/src/lib.rs', 'crates/airk-store/src/current/journal.rs',
             'crates/airk-store/src/current/mod.rs', 'crates/airk-store/src/current/resources.rs',
             'crates/airk-store/src/current/metadata.rs', 'crates/airk-store/src/blobs.rs',
             'crates/airk-store/src/history_policy.rs', 'crates/airk-store/src/maintenance.rs',
             'crates/airk-store/src/vector_policy.rs', 'crates/airk-store/src/extraction_policy.rs'):
    old = (root / path).read_text(encoding='utf-8')
    new = old
    if path.endswith('/lib.rs'):
        new = new.replace('    pub fn lock(&self) -> Result<WriterLock> {', '''    /// Native direct-settings adapter: validate the actual writer before its first effect.
    pub fn invalidate_owned_state_before_mutation(&self, writer: &WriterLock) -> Result<()> {
        crate::current::gate::check_writer(self, writer)?;
        writer.invalidate_census_before_mutation()
    }
    pub fn lock(&self) -> Result<WriterLock> {''', 1)
        new = new.replace('pub struct WriterLock {', 'pub struct WriterLock {\n    census_mutations: std::sync::atomic::AtomicU64,', 1)
        new = new.replace('        let guard = WriterLock {', '        let guard = WriterLock {\n            census_mutations: std::sync::atomic::AtomicU64::new(0),', 1)
        new = new.replace('impl WriterLock {', '''impl WriterLock {
    pub(crate) fn census_mutation_epoch(&self) -> u64 {
        self.census_mutations.load(std::sync::atomic::Ordering::Acquire)
    }
    pub(crate) fn invalidate_census_before_mutation(&self) -> Result<()> {
        self.census_mutations.fetch_update(std::sync::atomic::Ordering::AcqRel,
            std::sync::atomic::Ordering::Acquire, |epoch| epoch.checked_add(1))
            .map_err(|_| anyhow::anyhow!("census mutation generation exhausted"))?;
        Ok(())
    }
''', 1)
    elif path.endswith('/journal.rs'):
        new = new.replace('        crate::maintenance::check_writer(self, writer, quota)?;',
            '        crate::maintenance::check_writer(self, writer, quota)?;\n        writer.invalidate_census_before_mutation()?;', 1)
    elif path.endswith('/mod.rs'):
        new = new.replace('        gate::check_writer(self, writer)?;',
            '        gate::check_writer(self, writer)?;\n        writer.invalidate_census_before_mutation()?;')
    elif path.endswith('/resources.rs'):
        new = new.replace('impl FileStore {', '''impl FileStore {
    /// Writer-owned producer seam; legacy callers remain unqualified for census promotion.
    pub fn retain_current_resource_owned(&self, writer: &crate::WriterLock,
        bytes: &[u8], reservation: &Reservation) -> Result<ResourceReference> {
        super::gate::check_writer(self, writer)?;
        writer.invalidate_census_before_mutation()?;
        self.retain_current_resource(bytes, reservation)
    }
''', 1)
    elif path.endswith('/metadata.rs'):
        new = new.replace('impl FileStore {', '''impl FileStore {
    pub fn retain_current_metadata_encoded_owned(&self, writer: &crate::WriterLock,
        path: &str, file: &InventoryFile, reservation: &Reservation,
        mode: airk_core::compression::CompressionMode) -> Result<FileMetadataRef> {
        super::gate::check_writer(self, writer)?;
        writer.invalidate_census_before_mutation()?;
        self.retain_current_metadata_encoded(path, file, reservation, mode)
    }
''', 1)
    elif path.endswith('/history_policy.rs') or path.endswith('/maintenance.rs'):
        new = new.replace('check_writer(self, writer, quota)?;',
            'check_writer(self, writer, quota)?;\n        writer.invalidate_census_before_mutation()?;')
    elif path.endswith('/vector_policy.rs') or path.endswith('/extraction_policy.rs'):
        anchor = '        let reservation = quota.reserve('
        new = new.replace(anchor, '        writer.invalidate_census_before_mutation()?;\n' + anchor)
    else:
        new = new.replace('impl FileStore {', '''impl FileStore {
    pub fn retain_artifact_with_budget_owned(&self, writer: &crate::WriterLock,
        artifact: &Artifact, source: Option<&[u8]>, reservation: Option<&crate::quota::Reservation>) -> Result<Artifact> {
        crate::current::gate::check_writer(self, writer)?;
        writer.invalidate_census_before_mutation()?;
        self.retain_artifact_with_budget(artifact, source, reservation)
    }
    #[allow(clippy::too_many_arguments)]
    pub fn retain_payload_with_budget_owned(&self, writer: &crate::WriterLock,
        artifact: &Artifact, payload: &airk_core::extraction::ExtractionPayload,
        profile: &airk_core::extraction::ExtractionProfile, source: Option<&[u8]>,
        reservation: Option<&crate::quota::Reservation>) -> Result<Artifact> {
        self.retain_payload_encoded_owned(writer, artifact, payload, profile, source, reservation,
            airk_core::compression::CompressionMode::Off)
    }
    #[allow(clippy::too_many_arguments)]
    pub fn retain_payload_encoded_owned(&self, writer: &crate::WriterLock,
        artifact: &Artifact, payload: &airk_core::extraction::ExtractionPayload,
        profile: &airk_core::extraction::ExtractionProfile, source: Option<&[u8]>,
        reservation: Option<&crate::quota::Reservation>, mode: airk_core::compression::CompressionMode) -> Result<Artifact> {
        crate::current::gate::check_writer(self, writer)?;
        writer.invalidate_census_before_mutation()?;
        self.retain_payload_encoded(artifact, payload, profile, source, reservation, mode)
    }
''', 1)
    changes[path] = (old, new)
changes[module] = ('', base.joinpath('integrity.rs').read_text(encoding='utf-8'))
changes['crates/airk-store/src/service_collection_census.rs'] = ('', base.joinpath('census.rs').read_text(encoding='utf-8'))
private_base = root / 'artifacts/optimizations/f10-draft/private-storage-source'
windows_base = root / 'artifacts/optimizations/f10-draft/census-windows-source'
for name in ('private_storage.rs','private_storage_windows.rs'):
    path = 'crates/airk-store/src/' + name
    changes[path] = ('', (private_base / path).read_text(encoding='utf-8'))
path = 'crates/airk-store/src/service_census_windows.rs'
changes[path] = ('', (windows_base / path).read_text(encoding='utf-8'))
path = 'crates/airk-store/Cargo.toml'
changes[path] = ((root/path).read_text(encoding='utf-8'), (private_base/path).read_text(encoding='utf-8'))
path = 'crates/airk-store/src/lib.rs'
old, new = changes[path]
new += '\n#[allow(dead_code)] // Unwired foundation APIs; no read-cutover claim.\nmod private_storage;\n#[cfg(windows)]\n#[allow(dead_code)] // Native walker qualification is a separately scheduled gate.\nmod service_census_windows;\n'
changes[path] = (old, new)
patch = []
for path, (old, new) in changes.items():
    target = base / 'foundation-source' / path
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text(new, encoding='utf-8', newline='\n')
    patch.extend(difflib.unified_diff(old.splitlines(True), new.splitlines(True),
        fromfile='a/' + path if old else '/dev/null', tofile='b/' + path))
base.joinpath('foundation.patch').write_text(''.join(patch), encoding='utf-8', newline='\n')
