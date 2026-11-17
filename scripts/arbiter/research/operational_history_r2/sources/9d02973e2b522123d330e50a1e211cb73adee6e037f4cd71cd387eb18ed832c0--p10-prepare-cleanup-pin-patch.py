# SPDX-License-Identifier: Apache-2.0
"""Inert review diff for policy cleanup's invocation-owned selection lease."""
import difflib,hashlib,json
from pathlib import Path
base=Path(__file__).resolve().parent; root=base/'p10-worktree'
changes={}
def read(name):
    value=(root/name).read_text(encoding='utf-8'); changes[name]=[value,value]; return value
def replace_once(value,old,new):
    assert value.count(old)==1,old
    return value.replace(old,new)
name='crates/airk-kernel/src/source_policy.rs'; value=read(name)
value=replace_once(value,'        let selected = self.refresh_service_snapshot()?;','        let mut selected = self.refresh_service_snapshot()?;')
value=replace_once(value,'''        drop(writer);
        let report = capsule''','''        drop(writer);
        // The write phase pinned this slot against replacement. Cleanup below
        // acquires its own selection+writer before dropping its selection to
        // upgrade the object gate. Retaining our write-phase pin would prevent
        // that upgrade forever. Other callers' selections remain untouched.
        drop(selected.service_selection.take());
        let report = capsule''')
changes[name][1]=value
name='crates/airk-kernel/src/split_execution.rs'; value=read(name)
value=replace_once(value,'        let application = selected.apply_source_policy(policy, policy_operation)?;', '''        let mut application = selected.apply_source_policy(policy, policy_operation)?;
        if work.job.purpose == SplitJobPurpose::OriginalCleanup {
            // Each native turn deletes at most 4096 owned files. Keep the job
            // finite while permitting real bounded progress; an actual cleanup
            // error stops immediately and remains in the retained report.
            for _ in 0..15 {
                if !application.physical_cleanup_pending || application.cleanup_error.is_some() {
                    break;
                }
                crate::service_execution::checkpoint()?;
                selected.checkpoint_completed_split_work()?;
                application = selected.apply_source_policy(policy, policy_operation)?;
            }
        }''')
value=replace_once(value,'            policy_application: None,','            policy_application: Some(application),')
changes[name][1]=value
name='crates/airk-kernel/tests/source_policy.rs'; value=read(name)
value+='''
#[test]
fn service_policy_cleanup_releases_only_its_own_pin_and_preserves_actual_error_reports() -> Result<()> {
    let owned=owner()?;
    let base=owned.path().canonicalize()?;
    let root=base.join("source");
    let data=base.join("data");
    fs::create_dir_all(&root)?;
    fs::write(root.join("private.md"),b"private source remains unchanged\\n")?;
    fs::write(root.join("public.md"),b"healthy public evidence\\n")?;
    let kernel=Kernel::open_service_collection(&root,Some(data.clone()),"default")?;
    kernel.configure_vectors(false)?;
    kernel.index_service_sources()?;
    let collection=airk_store::service::collection::Collection::existing(&data,&canonical(&root)?,"default")?;
    let external=collection.select()?;
    let objects=external.store.directory().join("objects/sources");
    let orphan=objects.join("0".repeat(64));
    fs::write(&orphan,b"owned abandoned capture")?;
    let policy=policy(&kernel,"private.md",PathScope::Exact)?;
    let operation=Incarnation::from_bytes([103;16]);
    let pending=kernel.apply_source_policy(&policy,operation)?;
    assert!(pending.physical_cleanup_pending);
    assert!(pending.cleanup_error.is_none(),"a genuine external pin defers cleanup without an I/O error");
    assert!(orphan.exists());
    drop(external);
    // An actual malformed owned-object entry must be reported, rather than
    // being hidden forever behind the cleanup call's own selection lease.
    let malformed=objects.join("nonregular-derived-object");
    fs::create_dir(&malformed)?;
    let failed=kernel.apply_source_policy(&policy,operation)?;
    assert!(failed.physical_cleanup_pending);
    assert!(failed.cleanup_error.as_deref().is_some_and(|error|error.contains("nonregular current object")));
    assert_eq!(kernel.source_policy_status()?,Some(failed));
    assert!(malformed.is_dir());
    fs::remove_dir(malformed)?;
    let complete=kernel.apply_source_policy(&policy,operation)?;
    assert!(!complete.physical_cleanup_pending);
    assert!(complete.cleanup_error.is_none());
    assert_eq!(kernel.source_policy_status()?,Some(complete));
    assert!(!orphan.exists());
    assert_eq!(fs::read(root.join("private.md"))?,b"private source remains unchanged\\n");
    assert!(kernel.read_file("public.md",1,1,4096)?.hits[0].text.contains("healthy public evidence"));
    Ok(())
}
'''
changes[name][1]=value
patch=''.join(''.join(difflib.unified_diff(old.splitlines(keepends=True),new.splitlines(keepends=True),fromfile='a/'+name,tofile='b/'+name)) for name,(old,new) in changes.items())
(base/'p10-cleanup-pin-review.patch').write_text(patch,encoding='utf-8')
(base/'p10-cleanup-pin-review-base.json').write_text(json.dumps({name:hashlib.sha256((root/name).read_bytes()).hexdigest() for name in changes},indent=2)+'\n',encoding='utf-8')
print('Inert cleanup patch prepared:',len(changes),'files; no compiler input changed')
