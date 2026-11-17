# SPDX-License-Identifier: Apache-2.0
import hashlib,importlib.util,json,os,plistlib,shutil,subprocess,time,uuid
from pathlib import Path
run=Path(__file__).resolve().parent;source=run/'source';evidence=run/'evidence';assert evidence.is_dir()
assert run.parent==Path.home()/'airk-builds' and run.name=='explorer-notary-1812-ee5ad77-20261107'
original=Path.home()/'airk-builds/explorer-1812-20261107-1794068515/source/artifacts/neo/runtime'
expected={'sha256':'51536a705cf9cbabe4947cacddf43a8cb29b0af8bad4e23ca7f293aa6dd347fa','files':978};commit='ee5ad77d14307800ac3894381039856d13708736'
tools=Path.home()/'airk-builds/timing-1.6.4.PutU1P/tools';python=tools/'python3.13';nodebin=tools/'node-v24.15.0-darwin-arm64/bin'
env=dict(os.environ,PATH=str(nodebin)+':'+str(tools)+':'+str(Path.home()/'.cargo/bin')+':'+os.environ.get('PATH',''),PYTHONDONTWRITEBYTECODE='1',CARGO_NET_OFFLINE='true',MACOSX_DEPLOYMENT_TARGET='13.3')
def sha(p):
 with p.open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
def module(name,path):
 spec=importlib.util.spec_from_file_location(name,path);m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m);return m
rows=[];report={'schema':'airk.neo-local-notarized-candidate/1','source_commit':commit,'native_compilation':'reused original producer; no Rust build','original_runtime':str(original),'notarization_authorized':True,'publication':False,'keychain_settings_changed':False}
def save():
 (run/'notary2-steps.json').write_text(json.dumps(rows,indent=2)+'\n')
def step(name,command,timeout=600,cwd=None):
 (run/'notary2-phase.txt').write_text(name);start=time.monotonic()
 with (evidence/(name+'.log')).open('x') as output:p=subprocess.run(command,cwd=cwd or source,env=env,stdout=output,stderr=subprocess.STDOUT,timeout=timeout)
 row={'name':name,'command':list(map(str,command)),'exit_code':p.returncode,'elapsed_seconds':round(time.monotonic()-start,3)};rows.append(row);save();print(json.dumps(row),flush=True)
 if p.returncode:raise RuntimeError(name+' failed')
try:
 report['free_bytes_before']=shutil.disk_usage(run).free
 difference=json.loads((evidence/'clone-native-source-difference.json').read_text());assert difference['all_differences_only_crlf']
 old_source=original.parents[3]
 for row in difference['differences']:
  target=source/row['path'];origin=old_source/row['path'];assert sha(target)==row['current'] and sha(origin)==row['original'];target.write_bytes(origin.read_bytes());assert sha(target)==row['original']
 report['native_line_ending_restoration']=difference
 report['prior_failed_boundary']='pre-sign-failure1/notary-result.json'
 head=subprocess.check_output(['git','rev-parse','HEAD'],cwd=source,text=True).strip();assert head==commit,head
 builder=module('builder',source/'scripts/build-service-runtime.py');assert builder.source_identity()==expected
 package=module('package',source/'scripts/package-native.py');files,executables=package.validate_runtime(original,'1.8.12','aarch64-apple-darwin',expected)
 report['native_source']=expected;report['original_runtime_complete_inventory']=files;report['original_build_metadata']=json.loads((original/'airk-build.json').read_text());report['original_install_sha256']=sha(original/'airk-install.json')
 report['tool_sha256']={n:sha(source/n) for n in ['scripts/sign-native.py','scripts/ci-macos-sign.py','scripts/package-native.py','scripts/package-extension.py','packages/vscode/package.json','packages/vscode/package-lock.json']}
 (evidence/'producer-and-source.json').write_text(json.dumps(report,indent=2)+'\n')
 uid=os.getuid();assert uid==501 and Path('/dev/console').stat().st_uid==uid;domain=f'gui/{uid}'
 subprocess.run(['/bin/launchctl','print',domain],check=True,stdout=subprocess.DEVNULL)
 label='io.ioka.airk.build.notarize.'+uuid.uuid4().hex;plist=run/'notary-owned.plist';child=run/'notary-child.py';result=evidence/'notary-child-result.json'
 command=[str(python),'scripts/sign-native.py','--target','aarch64-apple-darwin','--runtime-dir',str(original),'--identity','4d8496dcab73794378194d040d9177fb28a0f982','--team-id','5PX7D99PXA','--notarize','--keychain-profile','airk-notary','--keychain',str(Path.home()/'Library/Keychains/airk-signing.keychain-db'),'--output','artifacts/neo/signed-notarized']
 code='''import json,os,signal,subprocess,time
from pathlib import Path
r=Path(RUN);start=time.monotonic()
with (r/'evidence/notary-sign-command.log').open('x') as output:
 p=subprocess.Popen(COMMAND,cwd=r/'source',env=ENV,stdin=subprocess.DEVNULL,stdout=output,stderr=subprocess.STDOUT,start_new_session=True)
 timed=False
 try:ret=p.wait(timeout=1500)
 except subprocess.TimeoutExpired:
  timed=True;os.killpg(p.pid,signal.SIGTERM)
  try:ret=p.wait(timeout=5)
  except subprocess.TimeoutExpired:os.killpg(p.pid,signal.SIGKILL);ret=p.wait(timeout=5)
 try:os.killpg(p.pid,0);absent=False
 except ProcessLookupError:absent=True
 report={'exit_code':ret,'timed_out':timed,'process_group_absent':absent,'elapsed_seconds':round(time.monotonic()-start,3),'session':subprocess.check_output(['/bin/launchctl','managername'],text=True).strip(),'command':COMMAND}
 (r/'evidence/notary-child-result.json').write_text(json.dumps(report,indent=2)+'\\n')
'''.replace('RUN',repr(str(run))).replace('COMMAND',repr(command)).replace('ENV',repr(env))
 child.write_text(code);child.chmod(0o600);plist.write_bytes(plistlib.dumps({'Label':label,'ProgramArguments':[str(python),str(child)],'RunAtLoad':True,'WorkingDirectory':str(run),'StandardOutPath':str(evidence/'notary-gui.stdout.log'),'StandardErrorPath':str(evidence/'notary-gui.stderr.log')}));plist.chmod(0o600)
 try:
  (run/'notary2-phase.txt').write_text('DeveloperID-sign-and-notarize-Aqua')
  subprocess.run(['/bin/launchctl','bootstrap',domain,str(plist)],check=True,timeout=15);deadline=time.monotonic()+1520
  while not result.exists() and time.monotonic()<deadline:time.sleep(.5)
  report['signing']=json.loads(result.read_text()) if result.exists() else {'supervisor_timeout':True}
 finally:
  stopped=subprocess.run(['/bin/launchctl','bootout',domain+'/'+label],capture_output=True,text=True,timeout=15);absent=subprocess.run(['/bin/launchctl','print',domain+'/'+label],capture_output=True,text=True,timeout=15)
  assert plist.parent==run and not plist.is_symlink();plist.unlink(missing_ok=True)
  report['gui_cleanup']={'bootout_exit':stopped.returncode,'job_absent':absent.returncode!=0,'plist_removed':not plist.exists()}
  (evidence/'notary-job-receipt.json').write_text(json.dumps(report['gui_cleanup'],indent=2)+'\n')
 assert report['signing'].get('exit_code')==0 and report['signing'].get('process_group_absent') and report['gui_cleanup']['job_absent']
 signed=source/'artifacts/neo/signed-notarized';receipt=json.loads((signed/'signing.json').read_text());assert receipt['copied_launcher']['strict_signature']
 submissions={v['notarization']['id'] for v in receipt['components'].values()};assert len(submissions)==1 and all(v['notarization']['status']=='Accepted' and v['notarization']['stapled'] for v in receipt['components'].values());report['accepted_notarization_ids']=sorted(submissions)
 guard=module('ci_guard',source/'scripts/ci-macos-sign.py');guard.verify_signed_output(signed,True,'4d8496dcab73794378194d040d9177fb28a0f982')
 step('notarized-staple-validation',['/usr/bin/xcrun','stapler','validate',str(signed/'runtime/AIRK.app')],60)
 step('notarized-gatekeeper-assessment',['/usr/sbin/spctl','--assess','--type','execute','--verbose=4',str(signed/'runtime/AIRK.app')],90)
 step('offline-npm-ci',[str(nodebin/'npm'),'ci','--ignore-scripts','--no-audit','--no-fund','--offline','--prefix','packages/vscode'],120)
 step('extension-compile-contracts',[str(nodebin/'npm'),'test','--prefix','packages/vscode'],240)
 step('package-signing-python-contracts',[str(python),'-m','unittest','scripts.tests.test_package_extension','scripts.tests.test_release_promotion','scripts.tests.test_ci_macos_signing','scripts.tests.test_native_signing'],180)
 step('notarized-native-installed-lifecycle',[str(python),'scripts/check-service-install.py','--runtime',str(signed/'runtime')],900)
 step('notarized-service-clients-model',[str(python),'scripts/check-service-clients.py','--runtime-dir',str(signed/'runtime'),'--files','16','--model','--report',str(evidence/'notarized-service-clients-model.json')],600)
 step('package-notarized-native',[str(python),'scripts/package-native.py','--target','aarch64-apple-darwin','--runtime-dir',str(original),'--signed-dir',str(signed),'--output','artifacts/neo/native-notarized'],180)
 step('package-notarized-extension',[str(python),'scripts/package-extension.py','--target','aarch64-apple-darwin','--native-dir','artifacts/neo/native-notarized/publish','--output','artifacts/neo/vscode-notarized'],180)
 assert builder.source_identity()==expected and all(sha(original/n)==h for n,h in files.items())
 report['original_runtime_unchanged']=True;report['status']='passed'
except BaseException as error:report['status']='failed';report['error']={'type':type(error).__name__,'error':str(error)}
finally:
 report['steps']=rows;report['target_absent']=not (run/'target').exists() and not (source/'target').exists();report['free_bytes_after']=shutil.disk_usage(run).free
 (run/'notary2-result.json').write_text(json.dumps(report,indent=2)+'\n');(run/'notary2-phase.txt').write_text('complete');print(json.dumps({k:v for k,v in report.items() if k not in ['original_runtime_complete_inventory','original_build_metadata']}),flush=True)
