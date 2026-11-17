# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import hashlib,json,difflib
B=Path(__file__).parent; W=B/'p10-worktree'; O=B/'post-p14-native-wal-fixture-proposed-r1'; O.mkdir(exist_ok=False)
p='crates/airk-cli/tests/combined_reads/deleted_source_recovery.rs'
old=(W/p).read_text(); s=old
a=s.index('        !selected.is_empty()',s.index('pub(crate) async fn retire_completed_destination'))
b=s.index('        "real native terminal WAL children missing',a)
s=s[:a]+'''        selected.len() == 1
            && original.is_empty()
            && before.children == selected
            && selected.iter().all(|child| child.current_owned
                && matches!(child.kind, FundingChildKind::Current { .. })),
'''+s[b:]
anchor='    let logical = Path::new(&destination.data_root)'
i=s.index(anchor,s.index('pub(crate) async fn retire_completed_destination'))
s=s[:i]+'''    // The original already owns a standalone native WAL allowance. The split
    // reuses it; only the newly created destination consumes funded Current.
    let original_registration = &complete.request.original_registration;
    let ordinary_inventory = || -> Result<Vec<(PathBuf, Vec<u8>)>> {
        let mut rows = Vec::new();
        for entry in fs::read_dir(Path::new(&destination.data_root).join(".quota-1.4/current-grants"))? {
            let path = entry?.path();
            let metadata = fs::symlink_metadata(&path)?;
            ensure!(metadata.is_file() && !metadata.file_type().is_symlink()
                && metadata.len() <= 65536, "invalid ordinary Current fixture record");
            let bytes = fs::read(&path)?;
            let record: Value = serde_json::from_slice(&bytes)?;
            if record["scope"]["collection"] == original_registration.collection {
                rows.push((path, bytes));
            }
        }
        rows.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(rows)
    };
    let ordinary_before = ordinary_inventory()?;
    ensure!(!ordinary_before.is_empty(), "original native standalone WAL credit missing");
    #[cfg(windows)]
    let ordinary_scope: airk_store::quota::CurrentGrantScope = serde_json::from_value(
        serde_json::from_slice::<Value>(&ordinary_before[0].1)?["scope"].clone())?;
    let parent_path = Path::new(&destination.data_root)
        .join(".quota-1.4/split-allocations").join(format!("f-{operation}.json"));
    let scratch = parent_path.parent().context("funding parent directory missing")?
        .join(format!("funding-retire-{}", before.identity()?));
    ensure!(!scratch.try_exists()?, "native fixture unexpectedly has retirement scratch");
'''+s[i:]
s=s.replace('OS share denial interrupts the REAL native metadata replace,','OS share denial interrupts the REAL native final-parent unlink,')
a=s.index('        let scratch = Path::new(&destination.data_root)',s.index('if let Some(parent) = interrupted_parent'))
b=s.index('        stop_deletion_service(f, service).await?;',a)
s=s[:a]+'''        // The destination is the sole funded child, so native cleanup unlinks
        // its parent directly. No replacement scratch belongs to this branch.
'''+s[b:]
s=s.replace('observing scratch before killing a worker cannot prove rename failed.','stopping a worker alone cannot prove the anchored unlink was denied.')
s=s.replace('no-DELETE HANDLE failed to deny actual native metadata replacement','no-DELETE HANDLE failed to deny actual native final-parent unlink')
s=s.replace('NtSetInformationFile setup publication: NTSTATUS','NtCreateFile relative child: NTSTATUS')
s=s.replace('unexpected native replacement OS error','unexpected native unlink OS error')
s=s.replace('denial was not the exact anchored metadata replacement','denial was not the exact anchored final-parent unlink open')
s=s.replace('&& scratch.is_file(),','&& !scratch.try_exists()?,')
s=s.replace('or lost scratch','or created replacement scratch')
s=s.replace('interrupted.children == before.children && scratch.is_file(),','interrupted == before && !scratch.try_exists()?,')
s=s.replace('        let original_registration = &complete.request.original_registration;\n','')
a=s.index('        let mut grants = quota.current_grants(&writer)?;',s.index('if let Some(parent) = interrupted_parent'))
b=s.index('        *service = Some(f.start().await?);',a)
s=s[:a]+'''        let mut grants = quota.current_grants(&writer)?;
        let grant = grants.iter_mut().find(|grant| grant.scope() == &ordinary_scope)
            .context("exact healthy original standalone Current grant not found")?;
        let capacity = grant.capacity();
        ensure!(capacity > 1, "real Current fixture lacks resize headroom");
        grant.resize(&writer, capacity - 1)?;
        ensure!(quota.current_grant(&writer, &ordinary_scope)?.context("resized grant missing")?
            .capacity() == capacity - 1 && ordinary_inventory()? != ordinary_before,
            "ordinary Current resize did not persist real progress");
        grant.resize(&writer, capacity)?;
        drop(grants);
        drop(writer);
        drop(store);
        drop(collection);
        ensure!(quota.observe_funding(operation)? == interrupted
                && ordinary_inventory()? == ordinary_before && !scratch.try_exists()?,
            "ordinary B progress changed A funding lineage/revision or failed exact restoration");
'''+s[b:]
a=s.index('    let after = quota.observe_funding(operation)?;',s.index('pub(crate) async fn retire_completed_destination'))
b=s.index('    let saved = ServiceRegistry::open_saved',a)
s=s[:a]+'''    ensure!(matches!(fs::symlink_metadata(&parent_path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound)
        && !scratch.try_exists()? && ordinary_inventory()? == ordinary_before,
        "final funded parent was not removed or original ordinary Current credit changed");
'''+s[b:]
files={p:(old,s)}
p='crates/airk-cli/tests/combined_reads/split.rs'; old=(W/p).read_text()
oldname='post_p14_interrupted_retirement_reuses_funding_scratch_after_real_healthy_current_revision'
newname='post_p14_interrupted_final_funding_retirement_preserves_ordinary_current_progress'
assert old.count(oldname)==1
files[p]=(old,old.replace(oldname,newname))
patch=''; manifest={'status':'frozen_proposal_not_applied_not_tested','base_native':'aef56a8252cde5016dde1a69221d5bcc700f77581e0cbe6a904eaac9f882a72a','files':[],'coverage':'Actual native sole funded destination retirement; Windows typed anchored unlink denial and standalone original Current progress with unchanged funding parent. Existing persistence unit alone models cross-parent-revision scratch behavior.'}
for p,(a,b) in files.items():
    out=O/'files'/p;out.parent.mkdir(parents=True,exist_ok=True);out.write_text(b,newline='\n')
    patch+=''.join(difflib.unified_diff(a.splitlines(True),b.splitlines(True),fromfile='a/'+p,tofile='b/'+p))
    manifest['files'].append({'path':p,'base_sha256':hashlib.sha256((W/p).read_bytes()).hexdigest(),'proposed_sha256':hashlib.sha256(out.read_bytes()).hexdigest()})
(O/'fix.patch').write_text(patch,newline='\n');manifest['patch_sha256']=hashlib.sha256((O/'fix.patch').read_bytes()).hexdigest()
(O/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
print(O);print(manifest)
