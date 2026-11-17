# SPDX-License-Identifier: Apache-2.0
# Exact approved505 unlinked incremental cache directories only. No Cargo/ACL work.
from pathlib import Path
import collections,datetime,gzip,hashlib,importlib.util,json,os,shutil,stat,subprocess,sys,time,traceback
E=Path(r'D:\code\Github\airk\artifacts\v190\final-plan\exec-20261116T052942Z');I=E/'incremental-cleanup-inventory-r1';O=Path(__file__).resolve().parent
spec=importlib.util.spec_from_file_location('approved_same_human_metadata',E/'windows-packaging-candidate-r4-plan/candidate_overlay.proposed.py');m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
actor=m.identity();m.human_path(O,E);started=time.monotonic();ROOT=Path(r'D:\code\Github\airk\target\debug\incremental')
assert ROOT.resolve(strict=True)==ROOT and ROOT.parent==Path(r'D:\code\Github\airk\target\debug')
review=I/'no-hardlink-subset-review.json';assert m.sha(review)=='87c305060b13c808b1144556dfef44867bdad5afe97c10feb3dfb73a1b66a01c'
d=json.loads(review.read_bytes());inventory=I/'inventory.json';assert m.sha(inventory)==d['inventory_sha256']=='280d42fe4e5ca75b8645b38c4831a03773034624370faf51f86890ca57bd1abd';inv=json.loads(inventory.read_bytes());manifest=Path(inv['all_entry_metadata']['path']);assert m.sha(manifest)==d['metadata_sha256']
assert d['exact_root']==str(ROOT) and d['direct_cache_directories_with_no_hardlinks']==505 and d['other_direct_cache_directories_blocked_for_requested_no_hardlink_condition']==536
approved=d['no_hardlink_subset_exact_paths'];names={row['name'] for row in approved};assert len(names)==505
expected={};by_group=collections.defaultdict(dict)
with gzip.open(manifest,'rt',encoding='utf-8') as f:
 for line in f:
  row=json.loads(line);expected[row['path']]=row
  if row['path']:by_group[row['path'].split('/',1)[0]][row['path']]=row
root_before=m.security(ROOT);assert root_before==inv['root_security'] and root_before['owner_sid']==m.HUMAN
protected_paths=[Path(r'D:\code\Github\airk\target\debug\deps'),Path(r'D:\code\Github\airk\target\debug\build'),Path(r'D:\code\Github\airk\artifacts\v190\final-plan\candidate-worktree-r3'),Path(r'D:\code\Github\airk\artifacts\v190\final-plan\candidate-worktree-r4'),I]
protected_before={str(path):m.security(path) for path in protected_paths}

def rust_processes():
 command=r"$rows=@(Get-CimInstance Win32_Process|Where-Object {$_.Name -match '^(cargo|rustc|clippy-driver|rustdoc|cargo-nextest|cargo-clippy|rust-analyzer|sccache|cl|clang|clang-cl|cc1|cc1plus|c\+\+|g\+\+|gcc|link|lld-link)(\.exe)?$'}|ForEach-Object {$owner=Invoke-CimMethod -InputObject $_ -MethodName GetOwnerSid;[pscustomobject]@{name=$_.Name;pid=$_.ProcessId;parent_pid=$_.ParentProcessId;path=$_.ExecutablePath;owner_sid=$owner.Sid}});ConvertTo-Json -InputObject $rows -Depth 4 -Compress"
 return json.loads(subprocess.check_output(['pwsh','-NoProfile','-NonInteractive','-Command',command],text=True,encoding='utf-8'))

def inspect(path,record,*,mutable_directory=False,allow_multiple_links=False):
 # lstat first: no reparse/symlink target is ever followed for recursive removal.
 info=path.lstat();assert not path.is_symlink() and not getattr(info,'st_file_attributes',0)&0x400,'reparse/symlink refused: '+str(path)
 kind='directory' if stat.S_ISDIR(info.st_mode) else 'file' if stat.S_ISREG(info.st_mode) else 'special';assert kind==record['kind'] and kind!='special','entry kind changed: '+str(path)
 assert getattr(info,'st_file_attributes',0)==record['attributes'],'attributes changed: '+str(path)
 security=m.security(path);assert security['owner_sid']==m.HUMAN==record['owner_sid'],'owner changed: '+str(path)
 assert security['file_id']==record['file_id'] and hashlib.sha256(security['sddl'].encode()).hexdigest()==record['acl_sha256'],'file identity/ACL changed: '+str(path)
 assert info.st_nlink==record['nlink'],'link count changed: '+str(path)
 if kind=='file':
  assert allow_multiple_links or info.st_nlink==1,'hardlink refused: '+str(path)
  assert info.st_size==record['bytes'] and info.st_mtime_ns==record['mtime_ns'],'file metadata changed: '+str(path)
 elif not mutable_directory:assert info.st_mtime_ns==record['mtime_ns'],'directory metadata changed: '+str(path)
 return security

def prewalk(target,group):
 assert target.parent==ROOT and target.resolve(strict=True)==target and target.name in names,'not an exact approved direct child'
 assert m.security(ROOT)==root_before and ROOT.resolve(strict=True)==ROOT,'incremental root changed'
 assert m.security(target)==group['current_top_directory_security'],'top directory owner/ACL/fileID changed'
 rows={};stack=[target]
 while stack:
  path=stack.pop();rel=path.relative_to(ROOT).as_posix();assert rel in by_group[group['name']],'new/unreviewed entry: '+rel
  record=by_group[group['name']][rel];inspect(path,record);rows[rel]=record
  if record['kind']=='directory':
   with os.scandir(path) as scan:stack.extend(Path(row.path) for row in scan)
 assert set(rows)==set(by_group[group['name']]),'reviewed entries missing or added'
 assert sum(row['kind']=='file' for row in rows.values())==group['files'],'file count changed'
 return rows

files_deleted=0;directories_deleted=0;logical_deleted=0;deleted=[];skipped=[];errors=[];aborted=None
processes_before=rust_processes();assert not processes_before,'active Cargo/Rust/C compiler; no deletion authorized now'
free_before=shutil.disk_usage('D:\\').free;entry_log=O/'deleted-entries.jsonl.gz';batch_log=O/'targets.jsonl';receipt=O/'receipt.json';assert not entry_log.exists() and not batch_log.exists() and not receipt.exists()
with entry_log.open('xb') as raw,gzip.GzipFile(fileobj=raw,mode='wb',mtime=0) as entries,batch_log.open('x',encoding='utf-8',newline='\n') as log:
 def log_event(row):log.write(json.dumps(row)+'\n');log.flush()
 def remove_validated(path,records):
  global files_deleted,directories_deleted,logical_deleted
  rel=path.relative_to(ROOT).as_posix();record=records[rel];inspect(path,record)
  if record['kind']=='file':
   os.unlink(path);files_deleted+=1;logical_deleted+=record['bytes'];entries.write((json.dumps({'operation':'unlink','path':str(path),'file_id':record['file_id'],'logical_bytes':record['bytes']})+'\n').encode());return
  with os.scandir(path) as scan:children=[Path(child.path) for child in scan]
  expected_children={name for name in records if Path(name).parent.as_posix()==rel};assert {child.relative_to(ROOT).as_posix() for child in children}==expected_children,'directory children changed'
  for child in children:remove_validated(child,records)
  inspect(path,record,mutable_directory=True);os.rmdir(path);directories_deleted+=1;entries.write((json.dumps({'operation':'rmdir','path':str(path),'file_id':record['file_id']})+'\n').encode())
 for group in approved:
  target=Path(group['resolved_path']);began=time.monotonic();prior=(files_deleted,directories_deleted,logical_deleted)
  try:records=prewalk(target,group)
  except BaseException as error:
   row={'state':'skipped_before_any_delete','path':str(target),'reason':repr(error)};skipped.append(row);log_event(row);continue
  try:
   remove_validated(target,records);assert not os.path.lexists(target)
   row={'state':'deleted','path':str(target),'files_deleted':files_deleted-prior[0],'directories_deleted':directories_deleted-prior[1],'logical_bytes_deleted':logical_deleted-prior[2],'elapsed_seconds':time.monotonic()-began};deleted.append(row);log_event(row)
  except BaseException as error:
   row={'state':'error_partial_target','path':str(target),'error':repr(error),'files_deleted':files_deleted-prior[0],'directories_deleted':directories_deleted-prior[1],'logical_bytes_deleted':logical_deleted-prior[2]};errors.append(row);log_event(row);aborted='Unexpected in-target deletion failure: preserve partial result and stop';break
 entries.flush();raw.flush();os.fsync(raw.fileno());log.flush();os.fsync(log.fileno())
# Full remaining536-directory metadata proof. No outside aliases/contents traversed.
blocked_names={row['name'] for row in d['blocked_exact_paths']};remaining_errors=[];remaining_count=0
for name in sorted(blocked_names):
 for rel,record in by_group[name].items():
  try:inspect(ROOT/rel,record,allow_multiple_links=True);remaining_count+=1
  except BaseException as error:remaining_errors.append({'path':str(ROOT/rel),'error':repr(error)})
remaining_top={entry.name for entry in os.scandir(ROOT)};deleted_names={Path(row['path']).name for row in deleted};original_top=set(by_group);assert remaining_top==original_top-deleted_names,'unexpected top-level cache entries changed'
processes_after=rust_processes();free_after=shutil.disk_usage('D:\\').free;protected_after={str(path):m.security(path) for path in protected_paths};root_after=m.security(ROOT)
report={'schema':'airk.private.approved-incremental-cache-cleanup-execution/1','utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'actor':actor,'script_sha256':m.sha(Path(__file__)),'argv':[sys.executable,*sys.argv],'approved_review_sha256':m.sha(review),'exact_resolved_root':str(ROOT),'approved_direct_cache_directories':505,'deleted_direct_cache_directories':len(deleted),'deleted_targets':deleted,'skipped_targets':skipped,'errors':errors,'aborted':aborted,'files_unlinked':files_deleted,'directories_removed':directories_deleted,'logical_bytes_deleted':logical_deleted,'logical_GiB_deleted':logical_deleted/(1<<30),'D_free_before_bytes':free_before,'D_free_after_bytes':free_after,'D_observed_free_delta_bytes':free_after-free_before,'D_observed_free_delta_GiB':(free_after-free_before)/(1<<30),'free_delta_is_observed_host_change_not_exclusive_attribution':True,'rust_cargo_C_compilers_before':processes_before,'rust_cargo_C_compilers_after':processes_after,'no_compiler_process_before_after':not processes_before and not processes_after,'root_owner_acl_fileid_preserved':root_before==root_after,'protected_directory_identities_unchanged':protected_before==protected_after,'protected_before':protected_before,'blocked_hardlink_directories_preserved':len(blocked_names),'blocked_remaining_entries_compared_to_original':remaining_count,'blocked_remaining_metadata_errors':remaining_errors,'blocked_remaining_all_metadata_unchanged':not remaining_errors,'target_log':{'path':str(batch_log),'sha256':m.sha(batch_log)},'exact_deleted_entry_log':{'path':str(entry_log),'sha256':m.sha(entry_log),'bytes':entry_log.stat().st_size},'no_foreign_or_hardlinked_or_reparse_deletion':True,'no_Cargo_global_clean_or_build_sign_package_run':True,'no_ACL_owner_attribute_or_credentials_change':True,'no_debug_deps_pinned_NN_receipts_C3_C4_userdocs_deletion':True,'elapsed_seconds':time.monotonic()-started}
report['passed']=len(deleted)==505 and not skipped and not errors and not aborted and not remaining_errors and not processes_after and root_before==root_after and protected_before==protected_after
with receipt.open('x',encoding='utf-8',newline='\n') as stream:json.dump(report,stream,indent=2);stream.write('\n')
print(json.dumps({'passed':report['passed'],'receipt':str(receipt),'receipt_sha256':m.sha(receipt),'deleted_cache_directories':len(deleted),'skipped':len(skipped),'errors':len(errors),'logical_GiB_deleted':report['logical_GiB_deleted'],'actual_D_free_delta_GiB':report['D_observed_free_delta_GiB'],'D_free_after_GiB':free_after/(1<<30),'536_hardlinked_cache_groups_metadata_unchanged':not remaining_errors,'no_compilers_before_after':report['no_compiler_process_before_after'],'elapsed_seconds':report['elapsed_seconds']}))
if not report['passed']:raise SystemExit(1)
