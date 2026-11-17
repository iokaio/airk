# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import json,hashlib,difflib
B=Path(__file__).parent;W=B/'p10-worktree';O=B/'post-p14-uninitialized-retirement-proposed-r1';O.mkdir(exist_ok=False)
files={}
p='crates/airk-store/src/service_project_removal.rs';a=(W/p).read_text();s=a
anchor='        let absent_collection = if !present && absent_data.is_none() {'
assert s.count(anchor)==1
s=s.replace(anchor,'''        let uninitialized_data = if !present && absent_data.is_none() {
            RetiredUninitializedData::capture(Path::new(&registration.data_root))?
        } else {
            None
        };
        let absent_collection = if !present && absent_data.is_none() && uninitialized_data.is_none() {''')
anchor='        if let Some(absent) = &absent_collection {\n            absent.revalidate()?;\n        }'
assert s.count(anchor)==2
s=s.replace(anchor,anchor+'''
        if let Some(uninitialized) = &uninitialized_data {
            uninitialized.revalidate()?;
        }''')
i=s.index('// Native-only, nonserializable proof.')
s=s[:i]+'''// Metadata-only registration does not initialize or claim its data root. This
// proof authorizes only dropping already retired registry references: no file,
// allowance, ownership receipt or advisory is created, removed or adopted.
struct RetiredUninitializedData {
    path: std::path::PathBuf,
    root: super::catalog::setup::storage::SetupDirectory,
}
impl RetiredUninitializedData {
    fn capture(data: &Path) -> Result<Option<Self>> {
        let root = super::catalog::setup::storage::SetupDirectory::open_canonical(data)?;
        match root.open_entry(std::ffi::OsStr::new(super::super::data_scope::OWNER)) {
            Ok(_) => return Ok(None),
            Err(error) if missing_entry(&error) => (),
            Err(error) => return Err(error),
        }
        let proof = Self { path: data.to_path_buf(), root };
        proof.revalidate()?;
        Ok(Some(proof))
    }
    fn revalidate(&self) -> Result<()> {
        use std::ffi::OsStr;
        ensure!(super::catalog::setup::storage::SetupDirectory::open_canonical(&self.path)?
            .identity()? == self.root.identity()?, "uninitialized retired data root changed");
        // Any native ownership/claim/quota state requires the ordinary complete
        // ownership-aware cleanup. Unrelated user/legacy files are untouched.
        for name in [super::super::data_scope::OWNER, super::super::data_scope::PART,
            super::super::data_scope::LOCK, ".quota-1.4", ".lifecycle-1.4.lock"] {
            match self.root.open_entry(OsStr::new(name)) {
                Err(error) if missing_entry(&error) => (),
                Err(error) => return Err(error),
                Ok(_) => anyhow::bail!("uninitialized retired data root contains native state"),
            }
        }
        match self.root.open_dir(OsStr::new("collections-1.4")) {
            Err(error) if missing_entry(&error) => (),
            Err(error) => return Err(error),
            Ok(directory) => ensure!(directory.entries()?.next().transpose()?.is_none(),
                "unbound retired collection namespace is not empty"),
        }
        Ok(())
    }
}
fn missing_entry(error: &anyhow::Error) -> bool {
    error.downcast_ref::<std::io::Error>()
        .is_some_and(|error| error.kind() == std::io::ErrorKind::NotFound)
}

'''+s[i:]
files[p]=(a,s)
p='crates/airk-store/src/quota/retired.rs';a=(W/p).read_text();s=a.replace('    advisory: File,','    advisory: Option<File>,')
old='''        let advisory = locks.open_file(
            OsStr::new(&registration.collection),
            SetupFileMode::ReadWrite,
        )?;
        ensure!(
            advisory.metadata()?.len() == 0,
            "unknown collection advisory"
        );
        advisory.try_lock()?;'''
new='''        let advisory = match locks.open_file(
            OsStr::new(&registration.collection), SetupFileMode::ReadWrite) {
            Ok(advisory) => {
                ensure!(advisory.metadata()?.len() == 0, "unknown collection advisory");
                advisory.try_lock()?;
                Some(advisory)
            }
            Err(error) if absent(&error) => None,
            Err(error) => return Err(error),
        };'''
assert old in s;s=s.replace(old,new)
anchor='''        // Every target lease and row is held and every inventory validated before'''
s=s.replace(anchor,'''        // An activated sibling may own this shared root before this metadata-only
        // registration ever opened a writer. Missing advisory authorizes NO
        // selected-credit disposal, even if someone removed an old advisory.
        ensure!(proof.advisory.is_some()
            || (terminal_current.is_none() && ordinary.is_empty()
                && grants.as_ref().is_none_or(|(_, rows)| rows.is_empty())),
            "selected retired credit requires its actual collection advisory");
'''+anchor)
s=s.replace('            (&self.locks, self.collection.as_str(), &self.advisory),\n','')
anchor='''        match self.namespace.open_dir(OsStr::new(&self.collection)) {'''
s=s.replace(anchor,'''        match &self.advisory {
            Some(advisory) => ensure!(file_identity(&self.locks.open_file(
                OsStr::new(&self.collection), SetupFileMode::Read)?)? == file_identity(advisory)?,
                "retired collection advisory changed"),
            None => match self.locks.open_entry(OsStr::new(&self.collection)) {
                Err(error) if absent(&error) => (),
                Err(error) => return Err(error),
                Ok(_) => anyhow::bail!("retired collection advisory appeared"),
            },
        }
'''+anchor)
files[p]=(a,s)
p='crates/airk-store/src/quota/retired_tests.rs';a=(W/p).read_text();s=a+'''
#[test]
fn metadata_only_shared_root_retirement_never_disposes_credit_without_advisory() -> Result<()> {
    let f = Fixture::new()?;
    let healthy = f.grants(&f.healthy.collection)?;
    let selected = f.grants(&f.registration.collection)?;
    fs::remove_dir_all(f.directory())?;
    let advisory = f.quota.root.join("collections-1.4/.writer-locks")
        .join(&f.registration.collection);
    fs::remove_file(&advisory)?;
    let lease = f.lease()?;
    // Actual selected native credits still require actual advisory custody.
    assert!(AbsentCollectionSettlement::settle(&f.registration, &lease, &f.control, f.namespace).is_err());
    assert_eq!(f.grants(&f.registration.collection)?, selected);
    assert_eq!(f.grants(&f.healthy.collection)?, healthy);
    drop(lease);
    // A genuinely never-activated sibling has neither advisory nor credits.
    let mut unstarted = f.registration.clone();
    unstarted.collection = "never-activated".into();
    unstarted = ProjectRegistration::new(unstarted.root, unstarted.data_root, unstarted.collection)?;
    let lease = ProjectLease::acquire(&f.control, unstarted.binding)?;
    let proof = AbsentCollectionSettlement::settle(&unstarted, &lease, &f.control, f.namespace)?;
    proof.revalidate()?;
    assert!(!f.quota.root.join("collections-1.4/.writer-locks/never-activated").exists());
    assert_eq!(f.grants(&f.registration.collection)?, selected);
    assert_eq!(f.grants(&f.healthy.collection)?, healthy);
    f.assert_sources()?;
    Ok(())
}
''';files[p]=(a,s)
# Keep assertions external to production: use the existing native removal test module.
p='crates/airk-store/src/service_project_removal.rs';a,s=files[p]
i=s.rindex('\n}')
s=s[:i]+'''
    #[test]
    fn metadata_only_registration_retires_without_claiming_uninitialized_storage() -> Result<()> {
        let owner = tempfile::tempdir()?;
        let base = owner.path().canonicalize()?;
        let data = base.join("data"); let source = base.join("source"); let control = base.join("control");
        for path in [&data, &source, &control] { std::fs::create_dir(path)?; }
        std::fs::write(source.join("source.md"), b"preserved source")?;
        std::fs::write(data.join("unrelated-user-data"), b"preserved user data")?;
        std::fs::create_dir(data.join("collections-1.4"))?;
        let registration = ProjectRegistration::new(source.to_str().unwrap().into(),
            data.to_str().unwrap().into(), "never-started".into())?;
        let mut registry = ServiceRegistry::open(&control,
            airk_core::identity::Incarnation::from_bytes([61; 16]))?;
        registry.register(&registration)?;
        std::fs::rename(&source, base.join("offline-source"))?;
        let epoch = registry.get(registration.binding)?.fencing_generation;
        assert!(registry.retire_missing_source(registration.binding, epoch, 0)?);
        assert!(registry.purge_removed_project_turn(registration.binding)?);
        assert!(registry.project_was_removed(registration.binding)?);
        assert!(registry.retained_project(registration.binding).is_err());
        assert_eq!(std::fs::read(data.join("unrelated-user-data"))?, b"preserved user data");
        assert_eq!(std::fs::read(base.join("offline-source/source.md"))?, b"preserved source");
        let mut names = std::fs::read_dir(&data)?.map(|e| e.map(|e| e.file_name()))
            .collect::<std::io::Result<Vec<_>>>()?; names.sort();
        assert_eq!(names, vec![std::ffi::OsString::from("collections-1.4"),
            std::ffi::OsString::from("unrelated-user-data")]);
        assert!(std::fs::read_dir(data.join("collections-1.4"))?.next().is_none());
        registry.register(&registration)?;
        assert_eq!(registry.get(registration.binding)?.registration, registration);
        Ok(())
    }
    #[test]
    fn uninitialized_retirement_proof_refuses_native_state_and_replaced_root() -> Result<()> {
        let owner = tempfile::tempdir()?; let base = owner.path().canonicalize()?;
        let data = base.join("data"); std::fs::create_dir(&data)?;
        let proof = RetiredUninitializedData::capture(&data)?.context("fresh proof missing")?;
        for name in [super::super::super::data_scope::PART, super::super::super::data_scope::LOCK,
            ".quota-1.4", ".lifecycle-1.4.lock"] {
            std::fs::write(data.join(name), b"unknown native state")?;
            assert!(proof.revalidate().is_err());
            assert!(RetiredUninitializedData::capture(&data).is_err());
            assert_eq!(std::fs::read(data.join(name))?, b"unknown native state");
            std::fs::remove_file(data.join(name))?;
        }
        std::fs::create_dir(data.join("collections-1.4"))?;
        std::fs::write(data.join("collections-1.4/unknown"), b"unknown collection")?;
        assert!(proof.revalidate().is_err());
        std::fs::remove_file(data.join("collections-1.4/unknown"))?;
        std::fs::rename(&data, base.join("old-data"))?;
        std::fs::create_dir(&data)?;
        assert!(proof.revalidate().is_err());
        Ok(())
    }
'''+s[i:]
files[p]=(a,s)
patch='';manifest={'status':'inert_proposal_not_applied_not_tested','files':[],'new_tests':3,'reason':'Native metadata-only registration is valid before worker activation. Logical reference cleanup must not create ownership or dispose selected credits without actual custody.'}
for p,(a,s) in files.items():
 out=O/'source'/p;out.parent.mkdir(parents=True,exist_ok=True);out.write_text(s,newline='\n')
 patch+=''.join(difflib.unified_diff(a.splitlines(True),s.splitlines(True),fromfile='a/'+p,tofile='b/'+p))
 manifest['files'].append({'path':p,'base_sha256':hashlib.sha256((W/p).read_bytes()).hexdigest(),'proposed_sha256':hashlib.sha256(out.read_bytes()).hexdigest()})
(O/'fix.patch').write_text(patch,newline='\n');manifest['patch_sha256']=hashlib.sha256((O/'fix.patch').read_bytes()).hexdigest()
(O/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n');print(manifest)
