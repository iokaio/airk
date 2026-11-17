# SPDX-License-Identifier: Apache-2.0
import base64,hashlib,importlib.util,json,os,platform,re,resource,shutil,signal,subprocess,sys,tarfile,time
from pathlib import Path
sys.dont_write_bytecode=True
run=Path(__file__).resolve().parent
assert run.parent==Path.home()/'airk-builds' and run.name.startswith('explorer-1812-')
assert platform.system()=='Darwin' and platform.machine()=='arm64'
os.umask(0o077)
source=run/'source'; target=run/'target'; evidence=run/'evidence';evidence.mkdir(mode=0o700)
record=json.loads((run/'snapshot.json').read_text()); steps=[]; failures=[]; owned_group=None; code=1
def sha(p):
 with p.open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
def save(name,data):
 (run/name).write_text(json.dumps(data,indent=2)+'\n')
def verify():
 for n,h in record['files'].items():assert (source/n).is_file() and sha(source/n)==h,n
 return {'files':len(record['files']),'unchanged':True}
tools=Path.home()/'airk-builds/timing-1.6.4.PutU1P/tools'; inputs=Path.home()/'airk-builds/cargo-clean-69d305e3933f/inputs'
nodebin=tools/'node-v24.15.0-darwin-arm64/bin'; python=str(tools/'python3.13')
environment=dict(os.environ,PATH=str(nodebin)+':'+str(tools)+':'+str(Path.home()/'.cargo/bin')+':'+os.environ.get('PATH',''),
 CARGO_BUILD_JOBS='1',CARGO_NET_OFFLINE='true',CARGO_INCREMENTAL='0',MACOSX_DEPLOYMENT_TARGET='13.3',PYTHONUTF8='1',PYTHONDONTWRITEBYTECODE='1',CARGO_TARGET_DIR=str(target),
 AIRK_BUNDLED_MODEL_DIR=str(inputs/'local-vector-model'),AIRK_BUNDLED_RUNTIME_DIR=str(inputs/'local-onnx-runtime'),AIRK_TEST_MODEL_DIR=str(inputs/'local-vector-model/minilm-l6-v2-s8-v1'),AIRK_PRIVATE_CENSUS_BASE=str(run/'private-census'))
for key in ['AIRK_TEST_BINARY','AIRK_CATALOG_CANDIDATE_BINARY','AIRK_EDITOR_RUN','AIRK_EDITOR_SCENARIOS']:environment.pop(key,None)
def step(name,command,required=True,release=False,timeout=3600):
 global owned_group
 (run/'phase.txt').write_text(name); start=time.monotonic(); env=environment.copy()
 if release:env.pop('CARGO_INCREMENTAL',None)
 with (evidence/(name+'.log')).open('x') as output:
  child=subprocess.Popen(command,cwd=source,env=env,stdout=output,stderr=subprocess.STDOUT,start_new_session=True);owned_group=child.pid
  timed_out=False
  try:ret=child.wait(timeout=timeout)
  except subprocess.TimeoutExpired:
   timed_out=True;os.killpg(child.pid,signal.SIGTERM)
   try:ret=child.wait(timeout=15)
   except subprocess.TimeoutExpired:os.killpg(child.pid,signal.SIGKILL);ret=child.wait(timeout=10)
  owned_group=None
 text=(evidence/(name+'.log')).read_text(errors='replace');timings=[]
 for value in re.findall(r'Timing report saved to (.+\.html)',text):
  path=Path(value.strip());destination=evidence/path.name
  if path.is_file():shutil.copy2(path,destination);timings.append(str(destination))
 row={'name':name,'command':command,'exit_code':ret,'timed_out':timed_out,'required':required,'elapsed_seconds':round(time.monotonic()-start,3),'target_dir':str(target),'incremental':env.get('CARGO_INCREMENTAL','unset; release profile default'),'timings':timings}
 steps.append(row);save('steps.json',steps);print(json.dumps(row),flush=True)
 if ret and required:raise RuntimeError('Required stage failed: '+name)
 return ret
try:
 assert not source.exists() and not target.exists();source.mkdir(mode=0o700)
 assert sha(run/'source.tar.gz')==record['archive_sha256']
 with tarfile.open(run/'source.tar.gz') as archive:
  members=archive.getmembers();assert {m.name for m in members}==set(record['files'])
  assert all(m.isfile() and not Path(m.name).is_absolute() and (source/m.name).resolve().is_relative_to(source) for m in members)
  archive.extractall(source,filter='data')
 before=verify()
 subprocess.run(['git','init','--quiet'],cwd=source,check=True)
 empty_ignore=run/'empty.ignore';empty_ignore.write_bytes(b'')
 subprocess.run(['git','config','core.excludesFile',str(empty_ignore)],cwd=source,check=True)
 for oid,row in json.loads((run/'git-metadata.json').read_text()).items():
  raw=base64.b64decode(row['base64']);assert hashlib.sha1(row['type'].encode()+b' '+str(len(raw)).encode()+b'\0'+raw).hexdigest()==oid
  actual=subprocess.check_output(['git','hash-object','-w','-t',row['type'],'--stdin'],input=raw,cwd=source).decode().strip();assert actual==oid
 subprocess.run(['git','update-ref','HEAD',record['entry_commit']],cwd=source,check=True)
 (source/'.git/shallow').write_text(record['entry_commit']+'\n')
 subprocess.run(['git','add','--all'],cwd=source,check=True)
 spec=importlib.util.spec_from_file_location('builder',source/'scripts/build-service-runtime.py');builder=importlib.util.module_from_spec(spec);spec.loader.exec_module(builder)
 assert builder.source_identity()==record['native_source']
 soft,hard=resource.getrlimit(resource.RLIMIT_NOFILE);assert hard==resource.RLIM_INFINITY or hard>=4096;resource.setrlimit(resource.RLIMIT_NOFILE,(4096,hard))
 (run/'private-census').mkdir(mode=0o700)
 environment['TMPDIR']=str(Path(__import__('tempfile').gettempdir()).resolve())
 save('prerequisites.json',{'platform':platform.platform(),'logical_cpus':os.cpu_count(),'free_bytes':shutil.disk_usage(run).free,'rlimit_nofile_initial':[soft,hard],'rlimit_nofile_run':[4096,hard],'environment':{k:v for k,v in environment.items() if k in ['CARGO_TARGET_DIR','CARGO_BUILD_JOBS','CARGO_NET_OFFLINE','CARGO_INCREMENTAL','MACOSX_DEPLOYMENT_TARGET','AIRK_BUNDLED_MODEL_DIR','AIRK_BUNDLED_RUNTIME_DIR','AIRK_TEST_MODEL_DIR','AIRK_PRIVATE_CENSUS_BASE','TMPDIR']}})
 step('format',['cargo','fmt','--all','--check'])
 step('clippy',[python,'scripts/dev-check.py','-p','airk-cli','--all-features','--skip-fmt','--target-dir',str(target),'--incremental','off','--offline'])
 step('runtime-release',[python,'scripts/build-service-runtime.py','--profile','release','--destination','artifacts/neo/runtime'],release=True,timeout=5400)
 assert builder.source_identity()==record['native_source']
 # Reviewed CI-only script corrections can be delivered while native compilation runs.
 if (run/'overlay.json').is_file():
  delta=json.loads((run/'overlay.json').read_text());assert sha(run/'overlay.tar.gz')==delta['archive_sha256']
  allowed={'scripts/promote-vsix.py','scripts/package-extension.py','scripts/tests/test_release_promotion.py','scripts/tests/test_package_extension.py'}
  assert set(delta['files'])<=allowed
  for n,row in delta['files'].items():assert record['files'].get(n)==row['before'] and sha(source/n)==row['before'],n
  with tarfile.open(run/'overlay.tar.gz') as archive:
   assert {m.name for m in archive.getmembers()}==set(delta['files']);assert all(m.isfile() for m in archive.getmembers());archive.extractall(source,filter='data')
  for n,row in delta['files'].items():assert sha(source/n)==row['after'];record['files'][n]=row['after']
  save('snapshot-after-reviewed-overlay.json',record);save('applied-overlay.json',delta)
  assert builder.source_identity()==record['native_source']
 step('native-mac-contracts',[python,'-m','unittest','scripts.tests.test_service_runtime_build','scripts.tests.test_native_signing','scripts.tests.test_package_native','scripts.tests.test_service_install','-v'],timeout=600)
 main='artifacts/neo/runtime/AIRK.app/Contents/MacOS/airk'
 step('vector-model',[python,'scripts/check-transformer-model.py','--binary',main,'--report',str(evidence/'vector-model.json')],timeout=600)
 step('service-clients',[python,'scripts/check-service-clients.py','--runtime-dir','artifacts/neo/runtime','--files','16','--model','--report',str(evidence/'service-clients.json')],timeout=600)
 step('installed-runtime',[python,'scripts/check-service-install.py','--runtime','artifacts/neo/runtime'],timeout=900)
 step('npm-ci',['npm','ci','--ignore-scripts','--no-audit','--no-fund','--offline','--prefix','packages/vscode'],timeout=600)
 step('extension-tests',['npm','test','--prefix','packages/vscode'],timeout=600)
 step('package-contracts',[python,'-m','unittest','scripts.tests.test_package_extension','scripts.tests.test_release_promotion','-v'],timeout=600)
 signed=step('developer-id-sign',[python,'scripts/sign-native.py','--target','aarch64-apple-darwin','--runtime-dir','artifacts/neo/runtime','--identity','Developer ID Application: Ioka LLC (5PX7D99PXA)','--team-id','5PX7D99PXA','--keychain',str(Path.home()/'Library/Keychains/airk-signing.keychain-db'),'--output','artifacts/neo/signed'],required=False,timeout=900)==0
 if signed:step('signed-installed-runtime',[python,'scripts/check-service-install.py','--runtime','artifacts/neo/signed/runtime'],timeout=900)
 native=[python,'scripts/package-native.py','--target','aarch64-apple-darwin','--runtime-dir','artifacts/neo/runtime','--output','artifacts/neo/native']
 if signed:native+=['--signed-dir','artifacts/neo/signed']
 step('native-package',native,timeout=600)
 step('extension-package',[python,'scripts/package-extension.py','--target','aarch64-apple-darwin','--native-dir','artifacts/neo/native/publish','--output','artifacts/neo/vscode'],timeout=600)
 app=Path('/Applications/Visual Studio Code.app');electron=app/'Contents/MacOS/Electron';cli=app/'Contents/Resources/app/out/cli.js'
 step('installed-explorer',['node','scripts/check-installed-explorer.cjs',str(electron),str(cli),'artifacts/neo/vscode/publish/airk-1.8.12-darwin-arm64.vsix'],required=False,timeout=180)
 step('bootstrap-ui',['node','scripts/check-extension-update-editor.cjs',str(electron)],required=False,timeout=180)
 verify();assert builder.source_identity()==record['native_source'];code=0
except BaseException as error:
 failures.append({'error':str(error),'type':type(error).__name__});print('BATCH_FAILURE',repr(error),flush=True)
finally:
 # All synchronous stages have returned; preserve any remaining Cargo timing files before cleaning.
 timing_root=target/'cargo-timings'
 if timing_root.is_dir():
  for p in timing_root.glob('*.html'):
   if not (evidence/p.name).exists():shutil.copy2(p,evidence/p.name)
 compiler_lines=[line for line in subprocess.check_output(['ps','-axo','pid,pgid,comm,args'],text=True).splitlines() if (str(run) in line and ('rustc' in line or re.search(r'\bcargo\b',line))) and 'owner.py' not in line]
 cleanup={'compiler_descendants_before_clean':compiler_lines,'target':str(target),'source':str(source),'source_preserved':source.is_dir()}
 if source.is_dir() and (source/'Cargo.toml').is_file() and not compiler_lines:
  assert target.parent==run and target.name=='target'
  with (evidence/'cargo-clean.log').open('x') as out:result=subprocess.run(['cargo','clean','--manifest-path',str(source/'Cargo.toml'),'--target-dir',str(target)],cwd=source,env=environment,stdout=out,stderr=subprocess.STDOUT)
  cleanup['exit_code']=result.returncode;cleanup['target_absent']=not target.exists()
 else:cleanup['exit_code']=None;cleanup['target_absent']=not target.exists();code=1
 save('cleanup.json',cleanup)
 final={'schema':'airk.neo-1812-qualification/1','status':'passed' if code==0 and all(s['exit_code']==0 for s in steps) and cleanup.get('exit_code')==0 else 'incomplete_or_failed','required_steps_passed':code==0,'native_source':record['native_source'],'entry_commit':record['entry_commit'],'source_state':record['source_state'],'steps':steps,'failures':failures,'cleanup':cleanup,'limits':['Apple Silicon Neo only; Intel and oldest-supported macOS not qualified.','No notarization submission or release publication.','Current signed distribution/Gatekeeper qualification requires separate accepted notarization and transfer.']}
 save('qualification.json',final);(run/'phase.txt').write_text('complete');print(json.dumps(final),flush=True)
sys.exit(0 if code==0 and cleanup.get('exit_code')==0 else 1)
