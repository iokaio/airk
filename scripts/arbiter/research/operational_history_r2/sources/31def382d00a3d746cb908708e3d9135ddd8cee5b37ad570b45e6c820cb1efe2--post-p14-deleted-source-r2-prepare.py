# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import difflib,hashlib,json
root=Path(r'D:/code/Github/airk')
out=root/'artifacts/v190/post-p14-deleted-source-r2-proposed'
out.mkdir(exist_ok=True)
files={}
def edit(rel,fn):
    old=(root/rel).read_text(encoding='utf-8')
    files[rel]=(old,fn(old))
rel='crates/airk-store/src/service_project_removal.rs'
def production(s):
    s=s.replace('use anyhow::{Result, ensure};','use anyhow::{Context, Result, ensure};')
    old='''        if present || policy.is_some() {
'''
    new='''        // Explicit committed retirement, not a source watcher observation, is
        // the authority for logical cleanup. Never recreate a missing data root
        // just to retain a capsule or open its quota ledger.
        let absent_data = if present {
            None
        } else {
            RetiredDataAbsence::capture(Path::new(&registration.data_root))?
        };
        if let Some(snapshot) = &policy {
            let job = self.job(snapshot.job)?;
            ensure!(
                matches!(job.state,
                    airk_core::service::JobState::Succeeded
                    | airk_core::service::JobState::Failed
                    | airk_core::service::JobState::Canceled),
                "retirement must settle the current policy job before cleanup"
            );
        }
        if absent_data.is_none() && (present || policy.is_some()) {
'''
    assert old in s;s=s.replace(old,new,1)
    old='''                let job = self.job(snapshot.job)?;
                ensure!(
                    matches!(
                        job.state,
                        airk_core::service::JobState::Succeeded
                            | airk_core::service::JobState::Failed
                            | airk_core::service::JobState::Canceled
                    ),
                    "retirement must settle the current policy job before cleanup"
                );
'''
    assert old in s;s=s.replace(old,'',1)
    needle='''        // Child metadata is removed before its FK parents. Other projects/plans remain.
'''
    s=s.replace(needle,'''        if let Some(absent) = &absent_data {
            absent.revalidate()?;
        }
'''+needle,1)
    needle='''        bump(&tx)?;
        tx.commit()?;
        Ok(true)
'''
    assert needle in s;s=s.replace(needle,'''        // The held ancestor still names the absent data tree at the native
        // metadata commit boundary. This records no physical disposal claim.
        if let Some(absent) = &absent_data {
            absent.revalidate()?;
        }
'''+needle,1)
    s+='''
// Native-only, nonserializable proof. No source path, quota opener, marker or
// missing directory creation is involved. A present/redirected/unavailable data
// root must use the existing ownership-aware physical cleanup instead.
struct RetiredDataAbsence {
    ancestor_path: std::path::PathBuf,
    ancestor: super::catalog::setup::storage::SetupDirectory,
    missing: std::ffi::OsString,
}
impl RetiredDataAbsence {
    fn capture(data: &Path) -> Result<Option<Self>> {
        match std::fs::symlink_metadata(data) {
            Ok(_) => return Ok(None),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) => return Err(error.into()),
        }
        let mut selected = None;
        for candidate in data.ancestors().skip(1) {
            match std::fs::symlink_metadata(candidate) {
                Ok(metadata) => {
                    ensure!(metadata.is_dir() && !metadata.file_type().is_symlink(),
                        "retired data ancestor is not a plain directory");
                    selected = Some(candidate.to_path_buf());
                    break;
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
                Err(error) => return Err(error.into()),
            }
        }
        let ancestor_path = selected.context("retired data has no existing ancestor")?;
        let ancestor = super::catalog::setup::storage::SetupDirectory::open_canonical(&ancestor_path)?;
        let relative = data.strip_prefix(&ancestor_path)?;
        let missing = match relative.components().next().context("retired data missing child absent")? {
            std::path::Component::Normal(name) => name.to_os_string(),
            _ => anyhow::bail!("invalid retired data absence path"),
        };
        let proof = Self { ancestor_path, ancestor, missing };
        proof.revalidate()?;
        Ok(Some(proof))
    }
    fn revalidate(&self) -> Result<()> {
        ensure!(super::catalog::setup::storage::SetupDirectory::open_canonical(&self.ancestor_path)?
            .identity()? == self.ancestor.identity()?, "retired data ancestor changed");
        match self.ancestor.open_dir(&self.missing) {
            Err(error) if error.downcast_ref::<std::io::Error>()
                .is_some_and(|error| error.kind() == std::io::ErrorKind::NotFound) => Ok(()),
            Err(error) => Err(error),
            Ok(_) => anyhow::bail!("retired data reappeared; physical cleanup requires ownership validation"),
        }
    }
}

#[cfg(test)]
mod missing_data_tests {
    use super::*;
    #[test]
    fn missing_data_proof_preserves_reappeared_tree() -> Result<()> {
        let owner = tempfile::tempdir()?;
        let path = owner.path().canonicalize()?.join("removed").join("data");
        let proof = RetiredDataAbsence::capture(&path)?.context("absence proof missing")?;
        std::fs::create_dir_all(&path)?;
        std::fs::write(path.join("sentinel"), b"preserved")?;
        assert!(proof.revalidate().is_err());
        assert_eq!(std::fs::read(path.join("sentinel"))?, b"preserved");
        assert!(RetiredDataAbsence::capture(&path)?.is_none());
        Ok(())
    }
}
'''
    return s
edit(rel,production)
# Rebase original actual native fixture patch mechanically onto live source.
oldpatch=(root/'artifacts/v190/post-p14-deleted-source-regression.patch').read_text(encoding='utf-8')
start=oldpatch.index('@@ -0,0 +1,243 @@\n')+len('@@ -0,0 +1,243 @@\n')
newtest=''.join(line[1:]+'\n' for line in oldpatch[start:].splitlines() if line.startswith('+'))
files['crates/airk-cli/tests/combined_reads/deleted_source_recovery.rs']=('',newtest)
def fixture(s):
    needle='''    fn with_runtime(dedicated_is_indexed: bool, count: usize, helper: bool) -> Result<Self> {
'''
    assert needle in s
    s=s.replace(needle,needle+'''        Self::with_runtime_layout(dedicated_is_indexed, count, helper, None)
    }
    fn with_runtime_layout(
        dedicated_is_indexed: bool, count: usize, helper: bool,
        removed_last_parent: Option<bool>,
    ) -> Result<Self> {
''',1)
    old='''            let source = base.join(format!("source{n}"));
            let data = base.join(format!("data{n}"));
'''
    new='''            let nested = n + 1 == count && removed_last_parent.is_some();
            let parent = if nested {
                let parent = base.join("removed-parent");
                fs::create_dir(&parent)?;
                parent
            } else { base.clone() };
            let source = parent.join(format!("source{n}"));
            let data = if nested && removed_last_parent == Some(true) {
                parent.join(format!("data{n}"))
            } else { base.join(format!("data{n}")) };
'''
    assert old in s;return s.replace(old,new,1)
edit('crates/airk-cli/tests/combined_reads.rs',fixture)
edit('crates/airk-cli/tests/combined_reads/permanent_trim.rs',lambda s:s.replace('async fn policy_key(', '#[path = "deleted_source_recovery.rs"]\nmod deleted_source_recovery;\n\nasync fn policy_key(',1))
patch=[];manifest=[]
for rel,(old,new) in files.items():
    target=out/'source'/rel;target.parent.mkdir(parents=True,exist_ok=True);target.write_text(new,encoding='utf-8',newline='\n')
    base=out/'base'/rel;base.parent.mkdir(parents=True,exist_ok=True)
    if old:base.write_bytes((root/rel).read_bytes())
    patch.append('diff --git a/'+rel+' b/'+rel+'\n')
    if not old:patch.append('new file mode 100644\n')
    patch.extend(difflib.unified_diff(old.splitlines(True),new.splitlines(True),fromfile='a/'+rel if old else '/dev/null',tofile='b/'+rel))
    manifest.append({'path':rel,'base_sha256':hashlib.sha256((root/rel).read_bytes()).hexdigest() if old else None,'proposed_sha256':hashlib.sha256(target.read_bytes()).hexdigest()})
(out/'missing-data.patch').write_text(''.join(patch),encoding='utf-8',newline='\n')
(out/'base-manifest.json').write_text(json.dumps(manifest,indent=2)+'\n',encoding='utf-8')
print(json.dumps({'files':len(files),'patch_sha256':hashlib.sha256((out/'missing-data.patch').read_bytes()).hexdigest()}))
