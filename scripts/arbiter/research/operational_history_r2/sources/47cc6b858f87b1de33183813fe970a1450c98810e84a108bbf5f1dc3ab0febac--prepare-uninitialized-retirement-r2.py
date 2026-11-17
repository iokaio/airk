# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import json,hashlib,difflib,shutil
B=Path(__file__).parent;W=B/'p10-worktree';A=B/'post-p14-uninitialized-retirement-proposed-r1';O=B/'post-p14-uninitialized-retirement-proposed-r2';shutil.copytree(A,O)
p='crates/airk-store/src/quota/retired.rs';f=O/'source'/p;s=f.read_text()
s=s.replace('    namespace: SetupDirectory,\n    locks: SetupDirectory,','    namespace: Option<SetupDirectory>,\n    locks: Option<SetupDirectory>,')
a=s.index('        let collections = root.open_dir(');b=s.index('        let ledger_control =',a)
s=s[:a]+'''        let collections = match root.open_dir(OsStr::new(Namespace::Service.collections())) {
            Ok(directory) => Some(directory),
            Err(error) if absent(&error) => None,
            Err(error) => return Err(error),
        };
        let locks = match &collections {
            Some(collections) => match collections.open_dir(OsStr::new(".writer-locks")) {
                Ok(directory) => Some(directory),
                Err(error) if absent(&error) => None,
                Err(error) => return Err(error),
            },
            None => None,
        };
        let advisory = match &locks {
            Some(locks) => match locks.open_file(
                OsStr::new(&registration.collection), SetupFileMode::ReadWrite) {
                Ok(advisory) => {
                    ensure!(advisory.metadata()?.len() == 0, "unknown collection advisory");
                    advisory.try_lock()?;
                    Some(advisory)
                }
                Err(error) if absent(&error) => None,
                Err(error) => return Err(error),
            },
            None => None,
        };
'''+s[b:]
s=s.replace('terminal_current.is_none() && ordinary.is_empty()',
    'terminal_current.as_ref().is_none_or(|plan| plan.is_empty()) && ordinary.is_empty()')
a=s.index('        ensure!(\n            self.root.identity()?.0 == self.namespace.identity()?.0');b=s.index('        ensure!(\n            self.root\n                .read_file',a)
s=s[:a]+'''        ensure!(self.root.identity()?.0 == self.control.identity()?.0,
            "retired quota control crosses filesystem");
        ensure!(SetupDirectory::open_canonical(&self.root_path)?.identity()? == self.root.identity()?
            && self.root.open_dir(OsStr::new(Namespace::Service.control()))?.identity()?
                == self.control.identity()?, "retired quota directory changed");
        match &self.namespace {
            Some(namespace) => {
                ensure!(self.root.open_dir(OsStr::new(Namespace::Service.collections()))?.identity()?
                    == namespace.identity()? && namespace.identity()?.0 == self.root.identity()?.0,
                    "retired collection namespace changed");
                match &self.locks {
                    Some(locks) => ensure!(namespace.open_dir(OsStr::new(".writer-locks"))?.identity()?
                        == locks.identity()? && locks.identity()?.0 == self.root.identity()?.0,
                        "retired writer-lock namespace changed"),
                    None => require_absent_entry(namespace, OsStr::new(".writer-locks"))?,
                }
            }
            None => require_absent_entry(&self.root, OsStr::new(Namespace::Service.collections()))?,
        }
'''+s[b:]
a=s.index('        match &self.advisory {');b=s.index('\n    }\n}\nfn absent',a)
s=s[:a]+'''        match (&self.locks, &self.advisory) {
            (Some(locks), Some(advisory)) => ensure!(file_identity(&locks.open_file(
                OsStr::new(&self.collection), SetupFileMode::Read)?)? == file_identity(advisory)?,
                "retired collection advisory changed"),
            (Some(locks), None) => require_absent_entry(locks, OsStr::new(&self.collection))?,
            (None, None) => (),
            (None, Some(_)) => anyhow::bail!("retired advisory lost its namespace"),
        }
        if let Some(namespace) = &self.namespace {
            require_absent_entry(namespace, OsStr::new(&self.collection))?;
        }
        Ok(())'''+s[b:]
s=s.replace('fn absent(error: &anyhow::Error) -> bool {','''fn require_absent_entry(directory: &SetupDirectory, name: &OsStr) -> Result<()> {
    match directory.open_entry(name) {
        Err(error) if absent(&error) => Ok(()),
        Err(error) => Err(error),
        Ok(_) => anyhow::bail!("retired logical collection or advisory namespace appeared"),
    }
}
fn absent(error: &anyhow::Error) -> bool {''')
f.write_text(s,newline='\n')
p='crates/airk-store/src/quota/funding_retired.rs';a=(W/p).read_text();s=a.replace('impl RetiredCurrentInventory {','''impl RetiredCurrentInventory {
    /// Capture still validates all funding records. Only selected effects decide
    /// whether metadata-only retirement would need actual writer custody.
    pub(in crate::quota) fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
''',1);f=O/'source'/p;f.write_text(s,newline='\n')
p='crates/airk-store/src/service_watch_tests.rs';a=(W/p).read_text();s=a
old='''    let mut registry = ServiceRegistry::open(owner.path(), nonce(1))?;
    let registration =
        ProjectRegistration::new("/original".into(), "/derived".into(), "missing".into())?;
    let other = ProjectRegistration::new("/other".into(), "/other-data".into(), "other".into())?;'''
new='''    let base = owner.path().canonicalize()?;
    for name in ["control", "source", "data", "other-source", "other-data"] {
        std::fs::create_dir(base.join(name))?;
    }
    let control = base.join("control");
    let mut registry = ServiceRegistry::open(&control, nonce(1))?;
    let registration = ProjectRegistration::new(base.join("source").to_str().unwrap().into(),
        base.join("data").to_str().unwrap().into(), "missing".into())?;
    let other = ProjectRegistration::new(base.join("other-source").to_str().unwrap().into(),
        base.join("other-data").to_str().unwrap().into(), "other".into())?;'''
assert old in s;s=s.replace(old,new,1)
start=s.index('fn retire_missing_source_cancels_work_');end=s.index('\n#[test]',start)
part=s[start:end].replace('ProjectLease::acquire(owner.path(), project)?','ProjectLease::acquire(&control, project)?').replace('ServiceRegistry::open(owner.path(), nonce(1))?','ServiceRegistry::open(&control, nonce(1))?')
part=part.replace('    // A configuration change invalidates the prior observation.','    std::fs::remove_dir(base.join("source"))?;\n    // A configuration change invalidates the prior observation.')
s=s[:start]+part+s[end:];f=O/'source'/p;f.write_text(s,newline='\n')
p='crates/airk-store/src/quota/retired_tests.rs';f=O/'source'/p;s=f.read_text()
anchor='''    // A genuinely never-activated sibling has neither advisory nor credits.'''
s=s.replace(anchor,'''    // Populate an actual B-only live operation through the production quota API.
    // Its presence must be fully validated and preserved, not mistaken for A effects.
    let first = airk_core::index_catalog::IndexId::from_identity(Incarnation::from_bytes([11;16]));
    let second = airk_core::index_catalog::IndexId::from_identity(Incarnation::from_bytes([12;16]));
    let third = ProjectRegistration::new(f.control.parent().unwrap().join("third-source")
        .to_str().unwrap().into(), f.registration.data_root.clone(), "funded-c".into())?;
    let funding = f.quota.reserve_funding(&funding::FundingRequest {
        operation_id: Incarnation::from_bytes([13;16]), request_digest: Sha256Digest::from_bytes([14;32]),
        namespace: f.namespace, original_index: first, original: f.healthy.clone(),
        destination_index: second, destination: third,
        budgets: vec![funding::FundingBudget{index_id:first,bytes:4096},
            funding::FundingBudget{index_id:second,bytes:4096}],
        quota_digests: BTreeMap::from([(f.registration.data_root.clone(),f.quota.funding_policy_digest()?)]),
    })?;
'''+anchor)
s=s.replace('    f.assert_sources()?;\n    Ok(())\n}\n', '    f.assert_sources()?;\n    Ok(())\n}\n')
anchor='''    assert_eq!(f.grants(&f.healthy.collection)?, healthy);
    f.assert_sources()?;
    Ok(())
}
'''
pos=s.rfind(anchor);assert pos>=0;s=s[:pos]+s[pos:].replace(anchor,'''    assert_eq!(f.grants(&f.healthy.collection)?, healthy);
    drop(proof);
    assert_eq!(f.quota.observe_funding(funding.operation_id)?, funding);
    f.assert_sources()?;
    Ok(())
}
''',1)
f.write_text(s,newline='\n')
paths=[r['path'] for r in json.loads((A/'manifest.json').read_text())['files']]+['crates/airk-store/src/quota/funding_retired.rs','crates/airk-store/src/service_watch_tests.rs']
m={'status':'inert_proposal_not_applied_not_tested','files':[],'new_tests':3,'r2_delta':'Selected-effects-empty predicate after full funding validation, B-only actual funding regression, existing Windows watch fixture now uses actual canonical private roots. Optional missing namespaces remain read-only and require zero selected effects; never synthesize ownership/advisory.'};patch=''
for p in paths:
 a=(W/p).read_text();f=O/'source'/p;s=f.read_text();patch+=''.join(difflib.unified_diff(a.splitlines(True),s.splitlines(True),fromfile='a/'+p,tofile='b/'+p));m['files'].append({'path':p,'base_sha256':hashlib.sha256((W/p).read_bytes()).hexdigest(),'proposed_sha256':hashlib.sha256(f.read_bytes()).hexdigest()})
(O/'fix.patch').write_text(patch,newline='\n');m['patch_sha256']=hashlib.sha256((O/'fix.patch').read_bytes()).hexdigest();(O/'manifest.json').write_text(json.dumps(m,indent=2)+'\n');print(m['patch_sha256'])
