# SPDX-License-Identifier: Apache-2.0
"""Separate failed-test cleanup authority; never invent an installed-editor Stop pass.

Default: read-only plan. --execute may be used only after primary review.
No account enumeration, account-directory removal, service restart or force.
"""
import argparse,hashlib,json,os,re,shutil,stat,subprocess,sys
from pathlib import Path
args=argparse.ArgumentParser();args.add_argument('--execute',action='store_true');options=args.parse_args()
run=Path(__file__).resolve().parent;source=run/'source';evidence=run/'evidence'
assert run==Path.home()/'airk-builds/explorer-notary-1812-ee5ad77-20261107'
def record_failure(kind,value,trace):
 if options.execute:
  try:
   state=globals().get('authority',{'original_editor_qualification':'failed; unchanged'}).copy();state['cleanup_execution_error']={'type':kind.__name__,'message':str(value)}
   (evidence/'failed-ui-independent-cleanup-error.json').write_text(json.dumps(state,indent=2)+'\n')
  except Exception:pass
 sys.__excepthook__(kind,value,trace)
sys.excepthook=record_failure
fixture=source/'artifacts/editor-qualification/native-installed-WYOvHW/same';install=fixture/'native-service';control=install/'control'
binary=source/'artifacts/neo/signed-notarized-r2/runtime/AIRK.app/Contents/MacOS/airk';uid=os.getuid();assert uid==501
def digest(data):return hashlib.sha256(data).hexdigest()
def directory(path):
 s=path.lstat();assert stat.S_ISDIR(s.st_mode) and not stat.S_ISLNK(s.st_mode) and s.st_uid==uid and path.resolve(strict=True)==path;return s
def read_file(path,limit=32768):
 before=path.lstat();assert stat.S_ISREG(before.st_mode) and not stat.S_ISLNK(before.st_mode) and before.st_nlink==1 and before.st_uid==uid and before.st_size<=limit
 fd=os.open(path,os.O_RDONLY|os.O_NOFOLLOW)
 try:
  opened=os.fstat(fd);assert (opened.st_dev,opened.st_ino,opened.st_size)==(before.st_dev,before.st_ino,before.st_size)
  with os.fdopen(fd,'rb',closefd=False) as stream:data=stream.read(limit+1)
  assert len(data)==before.st_size and len(data)<=limit
 finally:os.close(fd)
 return data,before
def identity(s):return (s.st_dev,s.st_ino,s.st_size,s.st_mtime_ns,s.st_ctime_ns,s.st_uid,s.st_nlink)
def child_of(root,path):return path!=root and path.is_relative_to(root)
directory(fixture);directory(install);directory(control)
settled=json.loads((evidence/'native-ui-failure-settlement.json').read_text());assert settled['original_scenario_status']=='failed; unchanged' and settled['service_health_unavailable_after']
checks={row['name']:row for row in settled['checks']};assert checks['canonical-shutdown-exact-owned-control']['exit_code']==0 and json.loads(checks['canonical-shutdown-exact-owned-control']['stdout'])['status']=='stopping';assert checks['post-cleanup-health']['exit_code']!=0
health=json.loads(checks['health-exact-owned-control']['stdout'])['observation'];runtime=settled['verified_runtime_sha256'];namespace=settled['verified_namespace']
assert health['runtime_sha256']==runtime and health['catalog_clock']['namespace']==namespace and health['version']=='1.8.12'
profiles=[];code_pids=[]
for role,name in [('a','airk-native-LFeqgn'),('b','airk-native-tIxYuk')]:
 marker=json.loads((fixture/f'profile-{role}-owned.json').read_text());profile=Path(marker['path']);assert marker=={'path':str(profile),'label':role,'private_user_data_dir':True} and profile==Path('/private/tmp')/name
 s=directory(profile);assert stat.S_IMODE(s.st_mode)==0o700 and re.fullmatch('airk-native-[A-Za-z0-9]+',profile.name)
 closed=json.loads((fixture/f'{role}-editor.log.forced-cleanup.json').read_text());assert closed['closed'] is True and closed['result']=={'code':0,'signal':None};code_pids.append(closed['pid']);profiles.append(profile)
def os_exit_proof():
 tracked=[health['process_id'],*code_pids];p=subprocess.run(['/bin/ps','-p',','.join(map(str,tracked)),'-o','pid=,comm='],capture_output=True,text=True,timeout=15);assert p.returncode in [0,1] and not p.stdout.strip(),'Tracked process PID is still present or reused'
 text=subprocess.check_output(['/bin/ps','-axo','pid=,command='],text=True,timeout=15)
 needles=[str(fixture),*[str(p) for p in profiles],*[str(Path('/tmp')/p.name) for p in profiles]]
 matching=[line.split(None,1)[0] for line in text.splitlines() if any(n in line for n in needles)];assert not matching,'An exact owned fixture/profile process remains'
 return {'tracked_pids_absent':tracked,'exact_fixture_profile_process_matches':matching}
process_proof=os_exit_proof()
observed=subprocess.run([str(binary),'runtime','inspect','--install-dir',str(install)],capture_output=True,text=True,timeout=30);assert observed.returncode==0
inspection=json.loads(observed.stdout);assert inspection['approval']['manifest_sha256']==runtime and inspection['registry_namespace']==namespace and Path(inspection['control_dir'])==control and inspection['payload_verified'] is True
down=subprocess.run([str(binary),'service','health','--runtime-dir',str(control)],capture_output=True,text=True,timeout=30);assert down.returncode!=0
baseline=json.loads((fixture/'uninstall-receipt-baseline.json').read_text());plan=baseline['plan'];assert plan['baseline']=='all_exact_targets_absent' and Path(plan['fixture'])==fixture and Path(plan['installDir'])==install
launcher=Path.home()/'Library/Application Support/AIRK/launcher';receipt_root=launcher/'extension-uninstall';assert Path(plan['launcher'])==launcher and Path(plan['receiptRoot'])==receipt_root
assert plan['targets']==[{'root':root,'name':digest(json.dumps([str(install),root],separators=(',',':')).encode())+'.json'} for root in ['',str(fixture/'workspace-a'),str(fixture/'workspace-b')]]
def owned_storage(value):
 storage=Path(value);assert storage.is_absolute()
 matches=[p for p in profiles if storage==p/'User/globalStorage/iokaio.airk'];assert len(matches)==1
 current=matches[0]
 for part in ['User','globalStorage','iokaio.airk']:current=current/part;directory(current)
 return str(storage)
receipts=[];absent=[]
for target in plan['targets']:
 filename=receipt_root/target['name']
 try:directory(launcher);directory(receipt_root);data,before=read_file(filename)
 except FileNotFoundError:absent.append(target['name']);continue
 value=json.loads(data);assert value['schema']==1 and set(value)<= {'schema','installDir','storage','spec'} and value['installDir']==str(install)
 spec=value.get('spec');assert (spec.get('root') if spec else '')==target['root'];storage=owned_storage(value['storage'])
 if spec:
  assert spec['service']['installDir']==str(install) and child_of(fixture,Path(spec['root'])) and child_of(fixture,Path(spec['dataDir'])) and child_of(install,Path(spec['command'])) and re.fullmatch('[a-f0-9]{64}',spec['service']['project']) and re.fullmatch('[A-Za-z0-9_-][A-Za-z0-9._-]{0,127}',spec['collection'])
 receipts.append({'name':target['name'],'path':str(filename),'sha256':digest(data),'bytes':len(data),'storage':storage,'data':data,'stat':before})
public_receipts=[{k:v for k,v in row.items() if k not in ['data','stat']} for row in receipts]
authority={'schema':'airk.failed-native-editor-independent-cleanup/1','cleanup_authority':'verified_canonical_shutdown_and_os_exit; NOT original UI Stop','original_editor_qualification':'failed; unchanged','original_ui_cleanup':'unqualified; unchanged','runtime_sha256':runtime,'namespace':namespace,'service_instance':health['instance'],'service_pid':health['process_id'],'owned_control':str(control),'process_proof':process_proof,'profiles':list(map(str,profiles)),'exact_receipts':public_receipts,'absent_receipts':absent,'account_directory_enumerated':False,'service_restart_or_force':False,'execute_requested':options.execute,'target_absent':not (source/'target').exists() and not (run/'target').exists()}
if not options.execute:
 (evidence/'failed-ui-cleanup-plan.json').write_text(json.dumps(authority,indent=2)+'\n');print(json.dumps(authority,indent=2));raise SystemExit(0)
# Every target is fully validated before the first unlink. Keep backups outside profiles.
backup=evidence/'failed-ui-owned-cleanup';backup.mkdir(mode=0o700,exist_ok=False);log_records=[]
for profile in profiles:
 origin=profile/'logs';total=0;files=0
 if origin.exists():
  directory(origin)
  for parent,dirs,names in os.walk(origin,followlinks=False):
   parent=Path(parent);assert len(parent.relative_to(origin).parts)<=12 and not any((parent/d).is_symlink() for d in dirs)
   for name in names:
    p=parent/name;data,_=read_file(p,32*1024**2);total+=len(data);files+=1;assert total<=32*1024**2 and files<=256
    destination=backup/'profile-logs'/profile.name/p.relative_to(origin);destination.parent.mkdir(parents=True,exist_ok=True);destination.write_bytes(data);assert digest(destination.read_bytes())==digest(data)
    log_records.append({'profile':str(profile),'relative_log':p.relative_to(origin).as_posix(),'sha256':digest(data),'bytes':len(data)})
for row in receipts:
 destination=backup/'receipts'/row['name'];destination.parent.mkdir(exist_ok=True);fd=os.open(destination,os.O_WRONLY|os.O_CREAT|os.O_EXCL,0o600)
 with os.fdopen(fd,'wb') as stream:stream.write(row['data'])
 assert digest(destination.read_bytes())==row['sha256']
authority['retained_profile_logs']=log_records;(backup/'cleanup-authority.json').write_text(json.dumps(authority,indent=2)+'\n')
os_exit_proof()
authority['removed_receipts']=[]
for row in receipts:
 filename=Path(row['path']);directory(launcher);directory(receipt_root);current,s=read_file(filename);assert identity(s)==identity(row['stat']) and current==row['data'];filename.unlink()
 authority['removed_receipts'].append(row['name']);(backup/'cleanup-progress.json').write_text(json.dumps(authority,indent=2)+'\n')
for target in plan['targets']:assert not os.path.lexists(receipt_root/target['name'])
authority['receipt_cleanup_verified']=True;authority['removed_profiles']=[];assert shutil.rmtree.avoids_symlink_attacks
for profile in profiles:
 os_exit_proof();s=directory(profile);assert stat.S_IMODE(s.st_mode)==0o700 and profile.parent==Path('/private/tmp');shutil.rmtree(profile);assert not os.path.lexists(profile);authority['removed_profiles'].append(str(profile))
 (backup/'cleanup-progress.json').write_text(json.dumps(authority,indent=2)+'\n')
authority['status']='owned_cleanup_complete; original editor scenario remains failed';(evidence/'failed-ui-independent-cleanup-result.json').write_text(json.dumps(authority,indent=2)+'\n');print(json.dumps(authority,indent=2))
