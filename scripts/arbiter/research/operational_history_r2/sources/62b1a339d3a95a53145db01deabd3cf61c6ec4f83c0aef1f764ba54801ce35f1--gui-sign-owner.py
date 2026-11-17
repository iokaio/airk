# SPDX-License-Identifier: Apache-2.0
import hashlib,importlib.util,json,os,plistlib,signal,subprocess,time,uuid
from pathlib import Path
run=Path(__file__).resolve().parent;source=run/'source';evidence=run/'evidence'
assert run.parent==Path.home()/'airk-builds' and run.name.startswith('explorer-1812-')
assert not (run/'target').exists()
spec=importlib.util.spec_from_file_location('builder',source/'scripts/build-service-runtime.py');builder=importlib.util.module_from_spec(spec);spec.loader.exec_module(builder)
assert builder.source_identity()=={'sha256':'51536a705cf9cbabe4947cacddf43a8cb29b0af8bad4e23ca7f293aa6dd347fa','files':978}
native=source/'artifacts/neo/native/publish';native_receipt=json.loads((native/'manifest.json').read_text())
with (native/native_receipt['archive']).open('rb') as f:assert hashlib.file_digest(f,'sha256').hexdigest()=='e0a9ef2d9d36ff9da7262e3303f866b0c5df8b2030dc8cc644ddf1018d579388'
tools=Path.home()/'airk-builds/timing-1.6.4.PutU1P/tools';python=tools/'python3.13'
env=dict(os.environ,PATH=str(tools/'node-v24.15.0-darwin-arm64/bin')+':'+str(tools)+':'+str(Path.home()/'.cargo/bin')+':'+os.environ.get('PATH',''),PYTHONDONTWRITEBYTECODE='1',CARGO_NET_OFFLINE='true',MACOSX_DEPLOYMENT_TARGET='13.3')
uid=os.getuid();assert Path('/dev/console').stat().st_uid==uid
domain=f'gui/{uid}';subprocess.run(['/bin/launchctl','print',domain],check=True,stdout=subprocess.DEVNULL)
label='io.ioka.airk.build.sign-check.'+uuid.uuid4().hex;plist=run/'gui-sign-owned.plist';child=run/'gui-sign-child.py';result=evidence/'gui-sign-child-result.json'
assert not plist.exists() and not child.exists() and not result.exists()
command=[str(python),'scripts/sign-native.py','--target','aarch64-apple-darwin','--runtime-dir','artifacts/neo/runtime','--identity','Developer ID Application: Ioka LLC (5PX7D99PXA)','--team-id','5PX7D99PXA','--keychain',str(Path.home()/'Library/Keychains/airk-signing.keychain-db'),'--output','artifacts/neo/signed-gui1']
child.write_text('''import json,os,signal,subprocess,time
from pathlib import Path
run=Path(RUN);started=time.monotonic()
with (run/'evidence/gui-sign-command.log').open('x') as output:
 p=subprocess.Popen(COMMAND,cwd=run/'source',env=ENV,stdout=output,stderr=subprocess.STDOUT,start_new_session=True)
 timed=False
 try:code=p.wait(timeout=180)
 except subprocess.TimeoutExpired:
  timed=True;os.killpg(p.pid,signal.SIGTERM)
  try:code=p.wait(timeout=5)
  except subprocess.TimeoutExpired:os.killpg(p.pid,signal.SIGKILL);code=p.wait(timeout=5)
 try:os.killpg(p.pid,0);group_absent=False
 except ProcessLookupError:group_absent=True
 result={'exit_code':code,'timed_out':timed,'elapsed_seconds':round(time.monotonic()-started,3),'process_group_absent':group_absent,'session':subprocess.check_output(['/bin/launchctl','managername'],text=True).strip(),'command':COMMAND}
 (run/'evidence/gui-sign-child-result.json').write_text(json.dumps(result,indent=2)+'\\n')
'''.replace('RUN',repr(str(run))).replace('COMMAND',repr(command)).replace('ENV',repr(env)))
child.chmod(0o600)
plist.write_bytes(plistlib.dumps({'Label':label,'ProgramArguments':[str(python),str(child)],'RunAtLoad':True,'WorkingDirectory':str(run),'StandardOutPath':str(evidence/'gui-sign.stdout.log'),'StandardErrorPath':str(evidence/'gui-sign.stderr.log')}));plist.chmod(0o600)
report={'schema':'airk.neo-gui-sign-check/1','label':label,'uid':uid,'attempts':1,'notarization_submitted':False,'publication':False,'keychain_settings_changed':False,'original_native_producer_reused':True};rows=[]
def step(name,cmd,timeout):
 (run/'gui-sign-phase.txt').write_text(name);start=time.monotonic()
 with (evidence/(name+'.log')).open('x') as output:p=subprocess.run(cmd,cwd=source,env=env,stdout=output,stderr=subprocess.STDOUT,timeout=timeout)
 row={'name':name,'command':cmd,'exit_code':p.returncode,'elapsed_seconds':round(time.monotonic()-start,3)};rows.append(row);print(json.dumps(row),flush=True)
 if p.returncode:raise RuntimeError('Stage failed: '+name)
try:
 (run/'gui-sign-phase.txt').write_text('GUI-signing-single-attempt')
 subprocess.run(['/bin/launchctl','bootstrap',domain,str(plist)],check=True,timeout=15)
 deadline=time.monotonic()+205
 while not result.exists() and time.monotonic()<deadline:time.sleep(.25)
 report['signing']=json.loads(result.read_text()) if result.exists() else {'supervisor_timeout':True}
finally:
 stopped=subprocess.run(['/bin/launchctl','bootout',domain+'/'+label],capture_output=True,text=True,timeout=15)
 absent=subprocess.run(['/bin/launchctl','print',domain+'/'+label],capture_output=True,text=True,timeout=15)
 report['job_cleanup']={'bootout_exit':stopped.returncode,'job_absent':absent.returncode!=0}
 assert plist.parent==run and not plist.is_symlink();plist.unlink(missing_ok=True);report['job_cleanup']['plist_removed']=not plist.exists()
 (evidence/'gui-sign-job-receipt.json').write_text(json.dumps(report,indent=2)+'\n')
try:
 signing=report.get('signing',{});assert signing.get('exit_code')==0 and signing.get('process_group_absent') and report['job_cleanup']['job_absent']
 signed=json.loads((source/'artifacts/neo/signed-gui1/signing.json').read_text());assert signed.get('copied_launcher')
 step('signed-gui-installed-runtime',[str(python),'scripts/check-service-install.py','--runtime','artifacts/neo/signed-gui1/runtime'],900)
 step('signed-gui-service-clients',[str(python),'scripts/check-service-clients.py','--runtime-dir','artifacts/neo/signed-gui1/runtime','--files','16','--model','--report',str(evidence/'signed-gui-service-clients.json')],600)
 step('signed-gui-native-package',[str(python),'scripts/package-native.py','--target','aarch64-apple-darwin','--runtime-dir','artifacts/neo/runtime','--signed-dir','artifacts/neo/signed-gui1','--output','artifacts/neo/native-signed-gui1'],600)
 step('signed-gui-extension-package',[str(python),'scripts/package-extension.py','--target','aarch64-apple-darwin','--native-dir','artifacts/neo/native-signed-gui1/publish','--output','artifacts/neo/vscode-signed-gui1'],600)
 report['status']='passed'
except BaseException as error:report['status']='failed';report['error']={'type':type(error).__name__,'error':str(error)}
finally:
 with (evidence/'cargo-clean-gui-sign.log').open('x') as output:p=subprocess.run(['cargo','clean','--manifest-path',str(source/'Cargo.toml'),'--target-dir',str(run/'target')],cwd=source,env=env,stdout=output,stderr=subprocess.STDOUT)
 report['steps']=rows;report['cargo_clean_exit_code']=p.returncode;report['target_absent']=not (run/'target').exists()
 (run/'gui-sign-result.json').write_text(json.dumps(report,indent=2)+'\n');(run/'gui-sign-phase.txt').write_text('complete');print(json.dumps(report),flush=True)
