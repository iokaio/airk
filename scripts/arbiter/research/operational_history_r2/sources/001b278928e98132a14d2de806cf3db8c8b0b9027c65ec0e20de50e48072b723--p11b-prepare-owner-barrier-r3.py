# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import json,hashlib
root=Path('D:/code/Github/airk');base=root/'artifacts/v190/p10-worktree'
prior=root/'artifacts/v190/p11b-owner-barrier-r2-proposed';out=root/'artifacts/v190/p11b-owner-barrier-r3-proposed';out.mkdir(exist_ok=True)
baseline=json.loads((prior/'baseline.json').read_text(encoding='utf-8'))
def read(p):return (out/p).read_text(encoding='utf-8')
def write(p,s):
    f=out/p;f.parent.mkdir(parents=True,exist_ok=True);f.write_text(s,encoding='utf-8',newline='\n')
for p in baseline:write(p,(prior/p).read_text(encoding='utf-8'))
service='crates/airk-store/src/service_collection.rs';database='crates/airk-store/src/current/database.rs';journal='crates/airk-store/src/current/journal.rs';module='crates/airk-store/src/service_collection_retention.rs';tests='crates/airk-store/src/service_collection_tests.rs'
s=read(service);a=s.index('                [".current-retention-pending"]');b=s.index('                [name] if metadata.is_file()',a);s=s[:a]+s[b:];write(service,s)
s=read(database);a=s.index('pub(super) fn validate_extraction_pending(');b=s.index('pub(crate) fn create_owner(',a);s=s[:a]+s[b:];write(database,s)
s=read(journal);a=s.index('            match prepared.pin.open_file(',s.index('pub(crate) fn prepare_extraction_contract'));b=s.index('            let reservation = quota.reserve(',a);s=s[:a]+s[b:]
s=s.replace('''                    &prepared.pin,
                    ".current-retention-pending",''','''                    &reservation,''')
s=s.replace('''                    ".current-retention-pending",
''','')
a=s.index('    pub(crate) fn validate_retention_pending(');b=s.index("    pub fn current_write<'a>(",a);s=s[:a]+s[b:];write(journal,s)
s=read(module);a=s.index('impl PreparedMarker {');b=s.index('impl Drop for PreparedMarker',a)
s=s[:a]+'''impl PreparedMarker {
    pub(crate) fn new(reservation: &crate::quota::Reservation, bytes: &[u8]) -> Result<Self> {
        Self::prepare(reservation, bytes, |_| Ok(()))
    }
    fn prepare(reservation: &crate::quota::Reservation, bytes: &[u8],
        after_open: impl FnOnce(&mut File) -> Result<()>) -> Result<Self> {
        let stage = reservation.staging_directory()?;
        let pin = SetupDirectory::open_canonical(&stage)?;
        let name = "retention-owner.json".to_string();
        let file = pin.open_file(OsStr::new(&name), SetupFileMode::CreateNew)?;
        let mut prepared = Self { name, file, pin, published: false };
        after_open(&mut prepared.file)?;
        prepared.file.write_all(bytes)?;
        prepared.file.sync_all()?;
        Ok(prepared)
    }
    pub(crate) fn publish(&mut self, destination: &SetupDirectory, target: &str, expected: &File) -> Result<()> {
        self.pin.replace_file_to(OsStr::new(&self.name), &self.file, destination, OsStr::new(target), expected)?;
        self.published = true;
        Ok(())
    }
}
'''+s[b:]
a=s.index('    match pin.open_file(\n        OsStr::new(".service-metadata-retention-pending")');b=s.index('    let bytes = serde_json::to_vec(&owner)?;',a);s=s[:a]+s[b:]
s=s.replace('PreparedMarker::new(&pin, ".service-metadata-retention-pending", &bytes)?','PreparedMarker::new(&reservation, &bytes)?')
s=s.replace('    let mut owner: Owner = serde_json::from_slice(&bytes)?;', '    let owner: Owner = serde_json::from_slice(&bytes)?;',1)
anchor='    let manifest: Manifest = read_json(&pin, "active.json", MAX_METADATA as u64)?;'
assert s.count(anchor)==1
s=s.replace(anchor,'    let stable_bytes = required_owner_bytes(&owner, required)?;\n'+anchor,1)
s=s.replace('    owner.required_extraction_contract = Some(required);\n    let bytes = serde_json::to_vec(&owner)?;','    let bytes = stable_bytes;',1)
anchor='pub(super) fn install('
helper='''fn required_owner_bytes(owner: &Owner, required: RequiredExtractionContract) -> Result<Vec<u8>> {
    let mut next=owner.clone(); next.required_extraction_contract=Some(required);
    let bytes=serde_json::to_vec(&next)?;
    ensure!(bytes.len()<=MAX_METADATA,"required extraction stable owner exceeds metadata bound");
    Ok(bytes)
}

'''
assert s.count(anchor)==1;s=s.replace(anchor,helper+anchor,1)
s=s.replace('        let quota = Quota::open_service(&data)?;\n        let stable_before',
    '        let quota = Quota::open_service(&data)?;\n        let reserved_before = quota.status()?.reserved_bytes;\n        let stable_before',1)
a=s.index('        assert!(\n            !active\n');b=s.index('        install(&collection, &active, &writer, &quota, 3, || Ok(()))?;',a)
s=s[:a]+'        assert_eq!(quota.status()?.reserved_bytes, reserved_before, "closed marker staging leaked reservation credit");\n'+s[b:]
# The unknown fixed lower leaf no longer belongs to this protocol. Unknown slot
# preservation remains a real negative; staging crash coverage is added below.
a=s.index('    #[test]\n    fn unowned_slot_and_malformed_pending_marker');s=s[:a]+'''    #[test]
    fn required_owner_encoding_refuses_growth_past_exact_metadata_bound() -> Result<()> {
        let temporary=tempfile::tempdir()?;let base=temporary.path().canonicalize()?;
        let mut owner=super::super::setup::initial_owner(&base,base.to_str().unwrap(),"fixture")?;
        let count=serde_json::to_vec(&owner)?.len();
        owner.root.push_str(&"x".repeat(MAX_METADATA-count));
        let original=serde_json::to_vec(&owner)?;
        assert_eq!(original.len(),MAX_METADATA);
        assert!(required_owner_bytes(&owner,RequiredExtractionContract::try_from(3)?).is_err());
        assert_eq!(serde_json::to_vec(&owner)?,original);
        assert!(owner.required_extraction_contract.is_none());
        Ok(())
    }
    #[test]
    fn unowned_slot_is_preserved_before_capability_effect() -> Result<()> {
        let temporary=tempfile::tempdir()?; let base=temporary.path().canonicalize()?;
        let source=base.join("source"); let data=base.join("data");
        fs::create_dir(&source)?; fs::create_dir(&data)?;
        let root=source.to_str().unwrap(); let slot=Incarnation::from_bytes([104;16]);
        let collection=Collection::initialize(&data,root,"fixture",slot)?;
        let active=collection.selected_store()?; super::super::tests::initialize(&active,root,slot)?;
        let foreign=collection.slot(Incarnation::from_bytes([105;16])); fs::create_dir(&foreign)?;
        let preserved=foreign.join("source.txt"); fs::write(&preserved,b"unknown bytes must remain")?;
        let before=fs::read(collection.directory.join("service-owner.json"))?;
        let writer=active.lock()?; let quota=Quota::open_service(&data)?;
        assert!(install(&collection,&active,&writer,&quota,3,||Ok(())).is_err());
        assert_eq!(fs::read(collection.directory.join("service-owner.json"))?,before);
        assert_eq!(fs::read(&preserved)?,b"unknown bytes must remain");
        Ok(())
    }
    #[test]
    #[ignore = "Subprocess crash entry; exercised by interrupted_partial_marker_stage_recovers_before_retry"]
    fn abrupt_partial_marker_stage_child() -> Result<()> {
        let base=PathBuf::from(std::env::var_os("AIRK_RETENTION_CRASH_BASE").context("owned crash fixture required")?).canonicalize()?;
        let data=base.join("data"); let source=base.join("source");
        let root=source.to_str().unwrap(); let collection=Collection::existing(&data,root,"fixture")?;
        let store=collection.selected_store()?; let _writer=store.lock()?;
        let quota=Quota::open_service(&data)?; let reservation=quota.reserve("fixture",131072)?;
        reservation.charge(4096)?;
        let stage=reservation.staging_directory()?;
        fs::write(base.join("owned-stage.txt"),stage.to_str().unwrap())?;
        let original=fs::read(store.directory().join("current-owner.json"))?;
        let _ = PreparedMarker::prepare(&reservation,&original,|file| {
            file.write_all(b"{")?; file.sync_all()?;
            std::process::exit(79);
        })?;
        anyhow::bail!("crash fixture unexpectedly returned")
    }
    #[test]
    fn interrupted_partial_marker_stage_recovers_before_retry() -> Result<()> {
        use std::process::{Command,Stdio};
        use std::time::{Duration,Instant};
        let temporary=tempfile::tempdir()?;let base=temporary.path().canonicalize()?;
        let data=base.join("data");let source=base.join("source");
        fs::create_dir(&data)?;fs::create_dir(&source)?;fs::write(source.join("original.rs"),b"original source")?;
        let root=source.to_str().unwrap();let slot=Incarnation::from_bytes([106;16]);
        let collection=Collection::initialize(&data,root,"fixture",slot)?;let store=collection.selected_store()?;
        let token=super::super::tests::initialize(&store,root,slot)?;
        let stdout=File::create_new(base.join("child.stdout"))?;let stderr=File::create_new(base.join("child.stderr"))?;
        // The exact child executes only the marker/Quota fixture and spawns no
        // descendants. Existing Store child guard kills/reaps on every failure.
        let mut child=super::super::tests::ChildGuard(Command::new(std::env::current_exe()?)
            .args(["--exact","service::collection::retention_barrier::tests::abrupt_partial_marker_stage_child","--ignored","--test-threads=1","--nocapture"])
            .env("AIRK_RETENTION_CRASH_BASE",&base).stdin(Stdio::null()).stdout(stdout).stderr(stderr).spawn()?);
        let until=Instant::now()+Duration::from_secs(30);
        let status=loop {if let Some(status)=child.0.try_wait()? {break status;}
            ensure!(Instant::now()<until,"bounded marker crash child timed out");std::thread::sleep(Duration::from_millis(10));};
        ensure!(status.code()==Some(79),"marker child did not reach exact crash boundary: {status}");
        let stage=PathBuf::from(fs::read_to_string(base.join("owned-stage.txt"))?);
        ensure!(stage.starts_with(&collection.directory),"foreign fixture stage");
        assert_eq!(fs::read(stage.join("retention-owner.json"))?,b"{");
        assert!(observed_contract(&collection.directory.join("service-owner.json"))?.is_null());
        // No Drop ran in the child. Actual ordinary reservation recovery owns
        // this partial file; no slot/root unknown-file allowlist is introduced.
        let quota=Quota::open_service(&data)?;let writer=store.lock()?;
        install(&collection,&store,&writer,&quota,3,||Ok(()))?;
        assert!(!stage.exists(),"dead owned partial stage must be reconciled before fresh reservation");
        let scope=CurrentScope{root:root.into(),collection:"fixture".into(),incarnation:slot};
        assert_eq!(store.current_read(&scope)?.token()?,token);
        assert_eq!(fs::read(source.join("original.rs"))?,b"original source");
        Ok(())
    }
}
'''
s=s.replace('/// A paid temporary in the captured directory; Drop removes only its exact object.','/// A paid temporary inside exact reservation staging; partial crash bytes stay owned.')
write(module,s)
s=read(tests).replace('struct ChildGuard(Child);','pub(super) struct ChildGuard(pub(super) Child);',1);write(tests,s)
# Cross-directory publication keeps both parents retained and the existing
# expected-target HANDLE check; same-directory callers preserve their wrapper.
storage='crates/airk-store/src/service_catalog_setup_storage.rs'
s=(base/storage).read_text(encoding='utf-8');a=s.index('    pub(crate) fn replace_file(');b=s.index('    pub(crate) fn sync(',a)
s=s[:a]+'''    pub(crate) fn replace_file(
        &self,name:&OsStr,owned:&File,target:&OsStr,expected:&File,
    )->Result<()> { self.replace_file_to(name,owned,self,target,expected) }
    /// Expected-object check under owning exclusion plus descriptor-relative
    /// rename. Unix does not supply atomic compare-and-replace-by-inode.
    pub(crate) fn replace_file_to(
        &self,name:&OsStr,owned:&File,destination:&Self,target:&OsStr,expected:&File,
    )->Result<()> {
        let source=self.open_file(name,SetupFileMode::Read)?;
        let current=destination.open_file(target,SetupFileMode::Read)?;
        ensure!(file_identity(&source)?==file_identity(owned)?
            && file_identity(&current)?==file_identity(expected)?,"native metadata replacement identity changed");
        let name=component(name)?;let target=component(target)?;
        // SAFETY: two retained directories and validated single-component names.
        ensure!(unsafe {libc::renameat(self.as_raw_fd(),name.as_ptr(),destination.as_raw_fd(),target.as_ptr())}==0,
            std::io::Error::last_os_error());
        destination.sync()?;self.sync()
    }
'''+s[b:];write(storage,s);baseline[storage]=hashlib.sha256((base/storage).read_bytes()).hexdigest()
windows='crates/airk-store/src/service_catalog_setup_storage_windows.rs'
s=(base/windows).read_text(encoding='utf-8');a=s.index('    pub(crate) fn replace_file(');b=s.index('    fn publish_named(',a)
s=s[:a]+'''    pub(crate) fn replace_file(
        &self,name:&OsStr,owned:&File,target:&OsStr,expected:&File,
    )->Result<()> { self.replace_file_to(name,owned,self,target,expected) }
    pub(crate) fn replace_file_to(
        &self,name:&OsStr,owned:&File,destination:&Self,target:&OsStr,expected:&File,
    )->Result<()> {
        let source=self.open_file(name,SetupFileMode::Read)?;
        let current=destination.open_file(target,SetupFileMode::Read)?;
        ensure!(native_file_identity(&source)?==native_file_identity(owned)?
            && native_file_identity(&current)?==native_file_identity(expected)?,"native metadata replacement identity changed");
        self.publish_named(name,owned,destination,target,true,||{})
    }
'''+s[b:];write(windows,s);baseline[windows]=hashlib.sha256((base/windows).read_bytes()).hexdigest()
(out/'baseline.json').write_text(json.dumps(baseline,indent=2)+'\n',encoding='utf-8')
print(json.dumps({'files':len(baseline),'status':'inert staged marker recovery r3; parse/finalize pending'}))
