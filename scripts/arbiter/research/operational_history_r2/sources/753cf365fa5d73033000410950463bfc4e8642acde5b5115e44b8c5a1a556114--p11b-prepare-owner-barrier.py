# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import difflib, hashlib, json

root=Path('D:/code/Github/airk')
base=root/'artifacts/v190/p10-worktree'
out=root/'artifacts/v190/p11b-owner-barrier-proposed'
out.mkdir(exist_ok=True)
paths=['crates/airk-store/src/service_collection.rs','crates/airk-store/src/service_collection_setup.rs','crates/airk-store/src/current/database.rs','crates/airk-store/src/current/journal.rs','crates/airk-store/src/service_collection_tests.rs']
original={p:(base/p).read_text(encoding='utf-8') for p in paths}
sources=dict(original)
def replace(path,old,new):
    assert sources[path].count(old)==1,(path,old[:100],sources[path].count(old))
    sources[path]=sources[path].replace(old,new)
service=paths[0]; setup=paths[1]; database=paths[2]; journal=paths[3]
replace(service,'#[derive(Clone, Serialize, Deserialize)]\n#[serde(deny_unknown_fields)]\nstruct Owner {','''/// Required capabilities use an exact numeric wire value. Unsupported values
/// refuse even metadata-only reopening; absence preserves historical bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "u32", into = "u32")]
pub(crate) struct RequiredExtractionContract(u32);
impl TryFrom<u32> for RequiredExtractionContract {
    type Error = anyhow::Error;
    fn try_from(value: u32) -> Result<Self> {
        ensure!(value == 3, "unsupported required extraction contract");
        Ok(Self(value))
    }
}
impl From<RequiredExtractionContract> for u32 {
    fn from(value: RequiredExtractionContract) -> Self { value.0 }
}

/// Native stable owner capability, reread when constructing every new slot owner.
pub(crate) fn required_extraction_contract_for_store(
    store: &FileStore,
) -> Result<Option<RequiredExtractionContract>> {
    let Ok(managed) = store.managed_path() else { return Ok(None); };
    if managed.namespace != "collections-1.4" || managed.slot.is_none() {
        return Ok(None);
    }
    let owner: Owner = json(&managed.logical.join("service-owner.json"))?;
    ensure!(Path::new(&owner.data_root) == managed.root
        && owner.collection == managed.collection,
        "foreign extraction capability owner");
    Ok(owner.required_extraction_contract)
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Owner {''')
replace(service,'    required_split: Option<split::SplitOwnerPointer>,\n}', '''    required_split: Option<split::SplitOwnerPointer>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    required_extraction_contract: Option<RequiredExtractionContract>,
}''')
replace(service,'    identity.required_split = None;','    identity.required_split = None;\n    identity.required_extraction_contract = None;')
replace(setup,'        required_split: None,','        required_split: None,\n        required_extraction_contract: None,')
anchor='    pub fn required_source_policy(&self) -> Option<airk_core::index_catalog::PolicyReference> {'
method='''    /// Installs the fail-closed predecessor barrier only for an actual selected
    /// managed current publication. The native reviewed Configure owner must call
    /// this before saving policy4 or producing contract3 bytes. The caller must
    /// release its own request/selection leases first; foreign readers defer it.
    pub fn require_extraction_contract(
        &self,
        store: &FileStore,
        writer: &crate::WriterLock,
        quota: &Quota,
        contract: u32,
    ) -> Result<()> {
        let required = RequiredExtractionContract::try_from(contract)?;
        crate::maintenance::check_writer(store, writer, quota)?;
        quota.require_store_funding(store)?;
        let _manifest = manifest_lock(&self.directory)?;
        let manifest = self.manifest()?;
        ensure!(store.directory().canonicalize()? == self.slot(manifest.active).canonicalize()?,
            "extraction migration requires exact active native slot");
        let mut owner: Owner = json(&self.directory.join("service-owner.json"))?;
        ensure!(owner_digest(&owner)? == self.binding
            && owner.root == self.owner.root && owner.data_root == self.owner.data_root
            && owner.collection == self.owner.collection,
            "extraction migration stable owner changed");
        store.current_scope(&owner.root, &owner.collection)?
            .context("extraction migration requires an existing current publication")?;
        // The lower seam validates the current owner and excludes existing old
        // CurrentRead handles before either durable marker can be changed.
        store.require_extraction_contract_with(writer, quota, contract, || {
            if owner.required_extraction_contract == Some(required) { return Ok(()); }
            owner.required_extraction_contract = Some(required);
            let prepared = prepare_with_quota(self, &owner, quota)?;
            crate::durability::persist_synced(prepared.temporary,
                &self.directory.join("service-owner.json"))
        })
    }

'''
replace(service,anchor,method+anchor)
replace(database,'    query_projection_version: Option<u32>,\n}', '''    query_projection_version: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    required_extraction_contract: Option<crate::service::collection::RequiredExtractionContract>,
}''')
replace(database,'        query_projection_version: Some(airk_core::storage_format::NATIVE_QUERY_PROJECTION_VERSION),','''        query_projection_version: Some(airk_core::storage_format::NATIVE_QUERY_PROJECTION_VERSION),
        required_extraction_contract:
            crate::service::collection::required_extraction_contract_for_store(
                &crate::FileStore::new(directory))?,''')
anchor='pub(crate) fn create_owner(directory: &Path, scope: &CurrentScope, capacity: u64) -> Result<()> {'
helper='''/// Native writer + exclusive current Gate are held by the owning adapter.
/// Parsing the existing header before publishing the stable marker proves an
/// actual recognized current owner; no absent-current activation is synthesized.
pub(super) fn require_extraction_contract(
    directory: &Path,
    quota: &crate::quota::Quota,
    contract: u32,
    publish_stable: impl FnOnce() -> Result<()>,
) -> Result<()> {
    let required = crate::service::collection::RequiredExtractionContract::try_from(contract)?;
    let mut owner = read_owner(directory)?;
    let reservation = quota.reserve(&owner.scope.collection, 2 * 65_536)?;
    owner.required_extraction_contract = Some(required);
    let bytes = serde_json::to_vec(&owner)?;
    ensure!(bytes.len() <= 65_536, "current capability header exceeds bound");
    reservation.charge(bytes.len() as u64)?;
    let mut temporary = tempfile::NamedTempFile::new_in(directory)?;
    temporary.write_all(&bytes)?;
    temporary.as_file().sync_all()?;
    // Stable-first fences every subsequently constructed current owner, even if
    // this process crashes before replacing this selected current header.
    publish_stable()?;
    crate::durability::persist_synced(temporary, &directory.join(OWNER_FILE))
}

'''
replace(database,anchor,helper+anchor)
anchor="    pub fn current_write<'a>("
helper='''    pub(crate) fn require_extraction_contract_with(
        &self,
        writer: &WriterLock,
        quota: &Quota,
        contract: u32,
        publish_stable: impl FnOnce() -> Result<()>,
    ) -> Result<()> {
        crate::maintenance::check_writer(self, writer, quota)?;
        crate::service::collection::validate_split_store_access(self)?;
        quota.require_store_funding(self)?;
        let directory = super::gate::check_writer(self, writer)?;
        let _gate = Gate::exclusive(self, writer)?;
        writer.invalidate_census_before_mutation()?;
        database::require_extraction_contract(&directory, quota, contract, publish_stable)
    }

'''
replace(journal,anchor,helper+anchor)
sources[paths[4]]+='''

/// Model the actual predecessor's current owner parser, not a fake artifact/profile.
/// Its cached FileStore still rereads this header after acquiring a shared Gate.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
struct PreRetentionCurrentOwner {
    version: u32,
    scope: CurrentScope,
    capacity: u64,
    #[serde(default)]
    query_projection_version: Option<u32>,
}

#[test]
fn retention_barrier_excludes_actual_readers_and_preserves_publication_and_source() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    let base = temporary.path().canonicalize()?;
    let data = base.join("data");
    let source = base.join("source");
    fs::create_dir(&data)?;
    fs::create_dir(&source)?;
    fs::write(source.join("original.rs"), b"fn original() {}")?;
    let root = source.to_str().context("non-UTF-8 fixture source")?;
    let slot = Incarnation::from_bytes([62; 16]);
    let collection = Collection::initialize(&data, root, "fixture", slot)?;
    let store = collection.selected_store()?;
    let token = initialize(&store, root, slot)?;
    let scope = store.current_scope(root, "fixture")?.context("missing current")?;
    let quota = Quota::open_service(&data)?;
    let prior_owner = fs::read(collection.directory.join("service-owner.json"))?;
    let prior_current = fs::read(store.directory().join("current-owner.json"))?;
    serde_json::from_slice::<PreRetentionCurrentOwner>(&prior_current)?;
    let read = store.current_read(&scope)?;
    let writer = store.lock()?;
    let error = collection.require_extraction_contract(&store, &writer, &quota, 3)
        .expect_err("active actual current reader must defer capability activation");
    assert!(error.downcast_ref::<crate::current::CurrentGateContention>().is_some());
    assert_eq!(fs::read(collection.directory.join("service-owner.json"))?, prior_owner);
    assert_eq!(fs::read(store.directory().join("current-owner.json"))?, prior_current);
    drop(read);
    collection.require_extraction_contract(&store, &writer, &quota, 3)?;
    collection.require_extraction_contract(&store, &writer, &quota, 3)?;
    assert!(collection.require_extraction_contract(&store, &writer, &quota, 4).is_err());
    drop(writer);
    let current = fs::read(store.directory().join("current-owner.json"))?;
    assert!(serde_json::from_slice::<PreRetentionCurrentOwner>(&current).is_err());
    let stable: serde_json::Value = serde_json::from_slice(
        &fs::read(collection.directory.join("service-owner.json"))?)?;
    assert_eq!(stable["required_extraction_contract"], 3);
    assert_eq!(serde_json::from_slice::<serde_json::Value>(&current)?["required_extraction_contract"], 3);
    let reopened = Collection::existing(&data, root, "fixture")?;
    assert_eq!(reopened.binding, collection.binding);
    assert_eq!(store.current_read(&scope)?.token()?, token);
    assert_eq!(fs::read(source.join("original.rs"))?, b"fn original() {}");
    // A new replacement slot created through an already cached Collection must
    // inherit the current stable barrier rather than the cached owner clone.
    let next_slot = Incarnation::from_bytes([63; 16]);
    let next = FileStore::new(collection.slot(next_slot));
    initialize(&next, root, next_slot)?;
    let next_owner: serde_json::Value = serde_json::from_slice(
        &fs::read(next.directory().join("current-owner.json"))?)?;
    assert_eq!(next_owner["required_extraction_contract"], 3);
    let writer = next.lock()?;
    assert!(collection.require_extraction_contract(&next, &writer, &quota, 3).is_err());
    Ok(())
}
'''
for path,source in sources.items():
    dest=out/path; dest.parent.mkdir(parents=True,exist_ok=True); dest.write_text(source,encoding='utf-8',newline='\n')
patch=''.join(''.join(difflib.unified_diff(original[p].splitlines(True),sources[p].splitlines(True),fromfile='a/'+p,tofile='b/'+p)) for p in paths)
(out/'owner-barrier.patch').write_text(patch,encoding='utf-8',newline='\n')
(out/'baseline.json').write_text(json.dumps({p:hashlib.sha256((base/p).read_bytes()).hexdigest() for p in paths},indent=2)+'\n',encoding='utf-8')
print(json.dumps({'out':str(out),'files':len(paths),'patch_sha256':hashlib.sha256(patch.encode()).hexdigest()}))
