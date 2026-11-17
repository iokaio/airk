# SPDX-License-Identifier: Apache-2.0
"""Apply reviewed help/harness-only bytes; reuse the immutable native producer."""
import hashlib,importlib.util,json,os,plistlib,re,shutil,signal,subprocess,tarfile,time
from pathlib import Path
run=Path(__file__).resolve().parent;source=run/'source';evidence=run/'evidence'
assert run.parent==Path.home()/'airk-builds' and run.name.startswith('explorer-1812-')
assert json.loads((run/'cleanup.json').read_text())['exit_code']==0 and not (run/'target').exists()
def sha(p):
 with p.open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
record=json.loads((run/'snapshot.json').read_text());delta=json.loads((run/'help-ui-delta.json').read_text())
allowed={'packages/vscode/src/explorerHelp/recovering-errors.md','scripts/check-extension-update-editor.cjs'}
assert set(delta['files'])==allowed and sha(run/'help-ui-delta.tar.gz')==delta['archive_sha256']
for n,row in delta['files'].items():assert sha(source/n)==row['before'] and record['files'][n]==row['before']
with tarfile.open(run/'help-ui-delta.tar.gz') as archive:
 members=archive.getmembers();assert {m.name for m in members}==allowed and all(m.isfile() for m in members);archive.extractall(source,filter='data')
for n,row in delta['files'].items():assert sha(source/n)==row['after'];record['files'][n]=row['after']
spec=importlib.util.spec_from_file_location('builder',source/'scripts/build-service-runtime.py');builder=importlib.util.module_from_spec(spec);spec.loader.exec_module(builder)
assert builder.source_identity()==record['native_source']
original_native=source/'artifacts/neo/native/publish';native_receipt=json.loads((original_native/'manifest.json').read_text());archive_before=sha(original_native/native_receipt['archive'])
assert archive_before==native_receipt['archive_sha256']
tools=Path.home()/'airk-builds/timing-1.6.4.PutU1P/tools';node=tools/'node-v24.15.0-darwin-arm64/bin/node';python=tools/'python3.13'
env=dict(os.environ,PATH=str(node.parent)+':'+str(tools)+':'+str(Path.home()/'.cargo/bin')+':'+os.environ.get('PATH',''),PYTHONDONTWRITEBYTECODE='1')
contents=Path('/Applications/Visual Studio Code.app/Contents');executable=contents/'MacOS'/plistlib.loads((contents/'Info.plist').read_bytes())['CFBundleExecutable'];cli=contents/'Resources/app/out/cli.js'
rows=[];error=None
def step(name,command,required=True,timeout=180):
 (run/'help-ui-phase.txt').write_text(name);start=time.monotonic()
 with (evidence/(name+'.log')).open('x') as out:
  child=subprocess.Popen(command,cwd=source,env=env,stdout=out,stderr=subprocess.STDOUT,start_new_session=True);timed_out=False
  try:code=child.wait(timeout=timeout)
  except subprocess.TimeoutExpired:
   timed_out=True;os.killpg(child.pid,signal.SIGTERM)
   try:code=child.wait(timeout=10)
   except subprocess.TimeoutExpired:os.killpg(child.pid,signal.SIGKILL);code=child.wait(timeout=10)
 row={'name':name,'command':command,'exit_code':code,'required':required,'timed_out':timed_out,'elapsed_seconds':round(time.monotonic()-start,3)};rows.append(row)
 (run/'help-ui-steps.json').write_text(json.dumps(rows,indent=2)+'\n');print(json.dumps(row),flush=True)
 if code and required:raise RuntimeError('Required stage failed: '+name)
try:
 step('extension-tests-help-corrected',['npm','test','--prefix','packages/vscode'],timeout=600)
 assert builder.source_identity()==record['native_source']
 step('extension-package-help-corrected',[str(python),'scripts/package-extension.py','--target','aarch64-apple-darwin','--native-dir','artifacts/neo/native/publish','--output','artifacts/neo/vscode-help-corrected'],timeout=600)
 vsix='artifacts/neo/vscode-help-corrected/publish/airk-1.8.12-darwin-arm64.vsix'
 step('installed-explorer-corrected',[str(node),'scripts/check-installed-explorer.cjs',str(executable),str(cli),vsix],required=False,timeout=180)
 step('bootstrap-ui-corrected',[str(node),'scripts/check-extension-update-editor.cjs',str(executable)],required=False,timeout=180)
 assert sha(original_native/native_receipt['archive'])==archive_before
 assert builder.source_identity()==record['native_source']
 for n,h in record['files'].items():assert sha(source/n)==h,n
except BaseException as failure:error={'type':type(failure).__name__,'error':str(failure)}
finally:
 assert (run/'target').parent==run and (run/'target').name=='target'
 with (evidence/'cargo-clean-followup.log').open('x') as out:clean=subprocess.run(['cargo','clean','--manifest-path',str(source/'Cargo.toml'),'--target-dir',str(run/'target')],cwd=source,env=env,stdout=out,stderr=subprocess.STDOUT)
 result={'schema':'airk.neo-help-ui-followup/1','status':'passed' if not error and all(r['exit_code']==0 for r in rows) and clean.returncode==0 else 'failed','native_source':record['native_source'],'native_producer_reused':True,'native_archive_sha256_before':archive_before,'native_archive_sha256_after':sha(original_native/native_receipt['archive']),'reviewed_public_delta':delta,'actual_code_executable':str(executable),'steps':rows,'error':error,'cargo_clean_exit_code':clean.returncode,'target_absent':not (run/'target').exists(),'scope':'Actual packaged production controller/UI uses native execution disabled; separate fictional managed-entry bootstrap fixture; original ad-hoc native producer is unchanged; Developer ID signing remains separately unverified.'}
 (run/'help-ui-followup.json').write_text(json.dumps(result,indent=2)+'\n');(run/'help-ui-phase.txt').write_text('complete');print(json.dumps(result),flush=True)
raise SystemExit(0 if result['status']=='passed' else 1)
