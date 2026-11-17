# SPDX-License-Identifier: Apache-2.0
"""One unchanged-script diagnostic run. No retries, warmup or command reordering.

Retain the script-owned TemporaryDirectory and observe actual original CLI
results. The original Fixture.close still shuts down its service on every exit.
"""
import hashlib,importlib.util,json,os,stat,sys,tempfile,time,traceback
from pathlib import Path
run=Path(__file__).resolve().parent;source=run/'source3c6e81c';evidence=run/'evidence3c';assert run==Path.home()/'airk-builds/explorer-notary-1812-ee5ad77-20261107'
binary=source/'artifacts/neo/signed-notarized-3c/runtime/AIRK.app/Contents/MacOS/airk';script=source/'scripts/check-document-tables.py';started=time.monotonic();rows=[];fixtures=[];owned=[];snapshot_paths=[]
spec=importlib.util.spec_from_file_location('unchanged_document_tables',script);module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
original_temp=module.tempfile.TemporaryDirectory;original_run=module.clients.Fixture.run
def dump(path,value):path.write_text(json.dumps(value,indent=2)+'\n')
class RetainedTemporary:
 def __init__(self,*args,**kwargs):
  assert not args and kwargs=={'prefix':'airk-table-smoke-'}
  self.name=tempfile.mkdtemp(prefix=kwargs['prefix']);self.root=Path(self.name).resolve(strict=True);assert self.root.parent==Path(tempfile.gettempdir()).resolve(strict=True) and self.root.name.startswith('airk-table-smoke-');owned.append(self.root)
  dump(evidence/'document-table-fixture-owner.json',{'owner':str(self.root),'control':str(self.root/'control'),'created_by':'this retained TemporaryDirectory wrapper only'})
 def __enter__(self):return self.name
 def __exit__(self,*exc):return False
def retain_temp(*args,**kwargs):
 return RetainedTemporary(*args,**kwargs) if kwargs.get('prefix')=='airk-table-smoke-' else original_temp(*args,**kwargs)
def filesystem_snapshot(fixture,label):
 root=fixture.owner.resolve(strict=True);assert root in owned and fixture.control.resolve(strict=True).is_relative_to(root)
 result={'schema':'airk.document-table-first-failure-filesystem/1','owner':str(root),'control':str(fixture.control),'projects':fixture.projects,'entries':[],'bounded_json_metadata':[],'raw_database_bytes_retained':False};count=0
 directories=[root,fixture.control]
 allowed_metadata={'service-owner.json','active.json','current-owner.json','current-bootstrap.json','current-runtime-bootstrap.json','.policy'}
 for project in fixture.projects.values():
  data=Path(project['data_root']);assert data.resolve(strict=True).is_relative_to(root);directories.extend([data,data/'collections-1.4',data/'collections-1.4'/project['collection']])
 for directory in list(directories):
  incarnations=directory/'incarnations'
  if incarnations.is_dir() and not incarnations.is_symlink():
   directories.append(incarnations)
   for p in list(incarnations.iterdir())[:8]:
    if p.is_dir() and not p.is_symlink():directories.append(p)
 for directory in dict.fromkeys(directories):
  assert directory.resolve(strict=False).is_relative_to(root)
  if not directory.exists():result['entries'].append({'path':str(directory),'exists':False});continue
  assert directory.is_dir() and not directory.is_symlink()
  for path in sorted(directory.iterdir()):
   count+=1;assert count<=256;metadata=path.lstat();result['entries'].append({'path':str(path),'mode':oct(stat.S_IMODE(metadata.st_mode)),'size':metadata.st_size,'regular':stat.S_ISREG(metadata.st_mode),'symlink':stat.S_ISLNK(metadata.st_mode),'mtime_ns':metadata.st_mtime_ns})
   if path.name in allowed_metadata and stat.S_ISREG(metadata.st_mode) and metadata.st_nlink==1 and metadata.st_uid==os.getuid() and metadata.st_size<=32768:
    data=path.read_bytes()
    try:value=json.loads(data)
    except (ValueError,UnicodeError):continue
    result['bounded_json_metadata'].append({'path':str(path),'sha256':hashlib.sha256(data).hexdigest(),'value':value})
 log=root/'service.stderr'
 if log.is_file() and not log.is_symlink():
  with log.open('rb') as stream:stream.seek(max(0,log.stat().st_size-65536));result['service_stderr_tail']=stream.read().decode('utf-8',errors='replace')
 destination=evidence/(label+'.json');dump(destination,result);snapshot_paths.append(str(destination))
def observe_run(self,*args,**kwargs):
 if self not in fixtures:fixtures.append(self)
 check=kwargs.pop('check',True)
 # The exact command runs first. Keep observations in memory to avoid adding
 # filesystem work before the original first settings read.
 result=original_run(self,*args,check=False,**kwargs)
 supplied=kwargs.get('input');supplied=supplied.decode('utf-8',errors='replace') if isinstance(supplied,bytes) else supplied
 row={'argv':[str(self.binary),*map(str,args)],'stdin':supplied,'exit_code':result.returncode,'stdout':result.stdout,'stderr':result.stderr};rows.append(row)
 if result.returncode and check:
  dump(evidence/'document-table-original-command-trace.json',rows)
  try:filesystem_snapshot(self,'document-table-first-failure-snapshot')
  except Exception as error:row['snapshot_error']={'type':type(error).__name__,'message':str(error)}
  raise AssertionError(f'Native client failed ({result.returncode}): {result.stderr[-4096:]} {result.stdout[-4096:]}')
 return result
module.tempfile.TemporaryDirectory=retain_temp;module.clients.Fixture.run=observe_run
report={'schema':'airk.document-table-unchanged-script-observation/1','original_script_sha256':hashlib.sha256(script.read_bytes()).hexdigest(),'binary':str(binary),'no_warmup_reorder_retry':True,'owned_fixture_retained':True,'service_restart_or_apply_replay':False,'snapshots':snapshot_paths}
sys.argv=[str(script),'--binary',str(binary)]
try:module.main();report['status']='passed'
except BaseException as error:report['status']='failed';report['original_exception']={'type':type(error).__name__,'message':str(error),'traceback':traceback.format_exc()}
finally:
 module.tempfile.TemporaryDirectory=original_temp;module.clients.Fixture.run=original_run
 dump(evidence/'document-table-original-command-trace.json',rows)
 for fixture in fixtures:
  try:filesystem_snapshot(fixture,'document-table-after-original-cleanup-snapshot')
  except Exception as error:report.setdefault('snapshot_errors',[]).append({'type':type(error).__name__,'message':str(error)})
 report.update({'elapsed_seconds':round(time.monotonic()-started,3),'owned_fixtures':list(map(str,owned)),'original_fixture_cleanup':[{'service_pid':f.child.pid if f.child else None,'service_exit_code':f.child.poll() if f.child else None,'log_closed':f.log.closed} for f in fixtures],'command_count':len(rows),'target_absent':not (source/'target').exists() and not (run/'target').exists()})
 dump(evidence/'document-table-observer-result.json',report);print(json.dumps(report,indent=2))
raise SystemExit(0 if report['status']=='passed' else 1)
