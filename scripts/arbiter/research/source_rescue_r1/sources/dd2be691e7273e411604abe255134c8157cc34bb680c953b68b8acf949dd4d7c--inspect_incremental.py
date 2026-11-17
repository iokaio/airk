# SPDX-License-Identifier: Apache-2.0
# Private read-only exact target/debug/incremental metadata inventory. No deletion.
from pathlib import Path
import collections,ctypes,datetime,gzip,hashlib,importlib.util,json,os,shutil,stat,subprocess,sys,time
from ctypes import wintypes
E=Path(r'D:\code\Github\airk\artifacts\v190\final-plan\exec-20261116T052942Z')
P=E/'windows-packaging-candidate-r4-plan';OUT=Path(__file__).resolve().parent
spec=importlib.util.spec_from_file_location('existing_human_metadata_reader',P/'candidate_overlay.proposed.py');m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
actor=m.identity();m.human_path(OUT,E);started=time.monotonic()
ROOT=Path(r'D:\code\Github\airk\target\debug\incremental');assert ROOT.resolve(strict=True)==ROOT and ROOT.parent==Path(r'D:\code\Github\airk\target\debug')
root_security=m.security(ROOT);assert root_security['owner_sid']==m.HUMAN
kernel=ctypes.WinDLL('kernel32',use_last_error=True);kernel.GetCompressedFileSizeW.argtypes=[wintypes.LPCWSTR,ctypes.POINTER(wintypes.DWORD)];kernel.GetCompressedFileSizeW.restype=wintypes.DWORD

def process_snapshot():
 script=r"$rows=@(Get-CimInstance Win32_Process|Where-Object {$_.Name -match '^(cargo|rustc|clippy-driver|rustdoc|cargo-nextest|cargo-clippy|rust-analyzer|sccache)(\.exe)?$'}|ForEach-Object {$o=Invoke-CimMethod -InputObject $_ -MethodName GetOwnerSid;[pscustomobject]@{name=$_.Name;pid=$_.ProcessId;parent_pid=$_.ParentProcessId;created=[string]$_.CreationDate;path=$_.ExecutablePath;owner_sid=$o.Sid}});ConvertTo-Json -InputObject $rows -Depth 4 -Compress"
 return json.loads(subprocess.check_output(['pwsh','-NoProfile','-NonInteractive','-Command',script],text=True,encoding='utf-8'))

def allocated(path):
 high=wintypes.DWORD();ctypes.set_last_error(0);low=kernel.GetCompressedFileSizeW(str(path),ctypes.byref(high));error=ctypes.get_last_error()
 if low==0xffffffff and error:raise ctypes.WinError(error)
 return (high.value<<32)|low

before_processes=process_snapshot();before_free={drive:shutil.disk_usage(drive).free for drive in ('C:\\','D:\\')};groups={};suffixes=collections.Counter();names=collections.Counter();owner_counts=collections.Counter();acl_table={};foreign=[];hardlinks=[];reparse=[];special=[];errors=[];fileids={};duplicate_fileids=[];dirs=0;files=0;logical=0;physical=0;protected_names=[]
manifest=OUT/'full-entry-metadata.jsonl.gz'
with manifest.open('xb') as rawout,gzip.GzipFile(fileobj=rawout,mode='wb',mtime=0) as packed:
 stack=[ROOT]
 while stack:
  path=stack.pop();rel=path.relative_to(ROOT).as_posix();rel='' if rel=='.' else rel
  try:
   info=path.lstat();security=m.security(path);owner_counts[security['owner_sid']]+=1;acl_id=hashlib.sha256(security['sddl'].encode()).hexdigest();acl_table[acl_id]=security['sddl'];attrs=getattr(info,'st_file_attributes',0);is_reparse=bool(attrs&0x400) or path.is_symlink();kind='directory' if stat.S_ISDIR(info.st_mode) else 'file' if stat.S_ISREG(info.st_mode) else 'special';entry={'path':rel,'kind':kind,'bytes':info.st_size if kind=='file' else 0,'allocated_bytes':0,'nlink':info.st_nlink,'attributes':attrs,'owner_sid':security['owner_sid'],'acl_sha256':acl_id,'file_id':security['file_id'],'mtime_ns':info.st_mtime_ns}
   if security['owner_sid']!=m.HUMAN:foreign.append(rel)
   if is_reparse:reparse.append(rel)
   group=rel.split('/',1)[0] if rel else '<root>';g=groups.setdefault(group,{'resolved_path':str(ROOT/group) if group!='<root>' else str(ROOT),'files':0,'directories':0,'logical_bytes':0,'allocated_bytes':0,'foreign':0,'hardlinks':0,'reparse':0,'special':0})
   if security['owner_sid']!=m.HUMAN:g['foreign']+=1
   if is_reparse:g['reparse']+=1
   if kind=='file':
    files+=1;g['files']+=1;logical+=info.st_size;g['logical_bytes']+=info.st_size;physical_size=allocated(path);physical+=physical_size;g['allocated_bytes']+=physical_size;entry['allocated_bytes']=physical_size;suffixes[path.suffix.lower()]+=1;names[path.name]+=1
    if info.st_nlink!=1:hardlinks.append(rel);g['hardlinks']+=1
    key=tuple(security['file_id'])
    if key in fileids:duplicate_fileids.append([fileids[key],rel])
    else:fileids[key]=rel
    if path.suffix.lower() in ('.exe','.dll','.onnx','.safetensors','.vsix','.zip','.log','.json','.md','.txt','.csv'):protected_names.append(rel)
   elif kind=='directory':
    dirs+=1;g['directories']+=1
    if not is_reparse:
     with os.scandir(path) as scan:stack.extend(Path(row.path) for row in scan)
   else:special.append(rel);g['special']+=1
   packed.write((json.dumps(entry,separators=(',',':'))+'\n').encode())
  except BaseException as error:errors.append({'path':rel,'error':repr(error)})
after_processes=process_snapshot();root_after=m.security(ROOT);after_free={drive:shutil.disk_usage(drive).free for drive in ('C:\\','D:\\')};assert root_after==root_security
report={'schema':'airk.private.incremental-cache-readonly-cleanup-inventory/1','utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'actor':actor,'requested_exact_root':str(ROOT),'resolved_exact_root':str(ROOT.resolve(strict=True)),'root_security':root_security,'root_security_unchanged':True,'only_inventory_root_examined':True,'directories':dirs,'files':files,'logical_bytes':logical,'logical_GiB':logical/(1<<30),'allocated_bytes':physical,'allocated_GiB':physical/(1<<30),'owner_entry_counts':dict(owner_counts),'foreign_entries':foreign,'non_single_link_files':hardlinks,'duplicate_file_ids':duplicate_fileids,'reparse_entries':reparse,'special_entries':special,'unreadable_or_metadata_errors':errors,'protected_extension_names':protected_names,'suffix_counts':dict(suffixes),'common_filenames':names.most_common(24),'all_entry_metadata':{'path':str(manifest),'sha256':m.sha(manifest),'bytes':manifest.stat().st_size,'acl_descriptors_by_sha256':acl_table},'top_level_caches':sorted([dict(name=name,**value) for name,value in groups.items() if name!='<root>'],key=lambda value:value['allocated_bytes'],reverse=True),'rust_processes_before':before_processes,'rust_processes_after':after_processes,'active_Rust_processes_absent_at_both_observations':not before_processes and not after_processes,'Root_reported_current_CARGO_INCREMENTAL_policy':'0','inventory_process_CARGO_INCREMENTAL':os.environ.get('CARGO_INCREMENTAL'),'drive_free_before_bytes':before_free,'drive_free_after_bytes':after_free,'all_entries_human_owned_regular_unlinked_unredirected':not foreign and not hardlinks and not duplicate_fileids and not reparse and not special and not errors,'cache_content_bytes_not_read_or_changed':True,'delete_performed':False,'disposable_candidate_assessment':'Only compilation caches below this exact incremental root; Rust can rebuild them. Root must review exact resolved paths and recheck active Rust before deletion. No file is deleted or automatically declared unneeded by this inventory.','excluded_and_untouched':['target/debug/deps','target/debug/*.exe','target/debug/build','target/.rustc_info.json','all retained receipts/artifacts','all pinned NN/runtime images','all Root/C4 public source and candidate outputs'],'elapsed_seconds':time.monotonic()-started}
with (OUT/'inventory.json').open('x',encoding='utf-8',newline='\n') as stream:json.dump(report,stream,indent=2);stream.write('\n')
print(json.dumps({'inventory':str(OUT/'inventory.json'),'sha256':m.sha(OUT/'inventory.json'),'files':files,'directories':dirs,'logical_GiB':report['logical_GiB'],'allocated_GiB':report['allocated_GiB'],'all_human_regular_unlinked_unredirected':report['all_entries_human_owned_regular_unlinked_unredirected'],'no_active_Rust_before_after':report['active_Rust_processes_absent_at_both_observations'],'protected_name_count':len(protected_names),'errors':len(errors),'top_level_caches':len(report['top_level_caches']),'delete_performed':False,'elapsed_seconds':report['elapsed_seconds']}))
