# SPDX-License-Identifier: Apache-2.0
"""One authorized retry after the user explicitly unlocked the existing keychain."""
import hashlib,importlib.util,json,os,subprocess,time
from pathlib import Path
run=Path(__file__).resolve().parent;source=run/'source';evidence=run/'evidence'
assert run.parent==Path.home()/'airk-builds' and run.name.startswith('explorer-1812-')
assert json.loads((run/'cleanup.json').read_text())['exit_code']==0 and not (run/'target').exists()
assert json.loads((run/'help-ui-followup.json').read_text())['target_absent']
tools=Path.home()/'airk-builds/timing-1.6.4.PutU1P/tools';python=tools/'python3.13';nodebin=tools/'node-v24.15.0-darwin-arm64/bin'
env=dict(os.environ,PATH=str(nodebin)+':'+str(tools)+':'+str(Path.home()/'.cargo/bin')+':'+os.environ.get('PATH',''),PYTHONDONTWRITEBYTECODE='1',CARGO_NET_OFFLINE='true',MACOSX_DEPLOYMENT_TARGET='13.3')
spec=importlib.util.spec_from_file_location('builder',source/'scripts/build-service-runtime.py');builder=importlib.util.module_from_spec(spec);spec.loader.exec_module(builder)
expected={'sha256':'51536a705cf9cbabe4947cacddf43a8cb29b0af8bad4e23ca7f293aa6dd347fa','files':978};assert builder.source_identity()==expected
native=source/'artifacts/neo/native/publish';receipt=json.loads((native/'manifest.json').read_text())
def sha(p):
 with p.open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
original=sha(native/receipt['archive']);assert original==receipt['archive_sha256']
keychain=Path.home()/'Library/Keychains/airk-signing.keychain-db'
state=subprocess.run(['/usr/bin/security','show-keychain-info',str(keychain)],capture_output=True,text=True,timeout=30)
(evidence/'keychain-after-user-unlock.json').write_text(json.dumps({'schema':'airk.neo-keychain-after-user-unlock/1','user_explicit_unlock_confirmed':True,'settings_query_exit':state.returncode,'settings_output':(state.stdout+state.stderr)[-4096:],'programmatic_trust_identity_acl_partition_search_list_changes':False},indent=2)+'\n')
rows=[];error=None
def step(name,command,timeout):
 (run/'sign-retry-phase.txt').write_text(name);started=time.monotonic()
 with (evidence/(name+'.log')).open('x') as output:
  result=subprocess.run(command,cwd=source,env=env,stdout=output,stderr=subprocess.STDOUT,timeout=timeout)
 row={'name':name,'command':command,'exit_code':result.returncode,'elapsed_seconds':round(time.monotonic()-started,3)};rows.append(row);(run/'sign-retry-steps.json').write_text(json.dumps(rows,indent=2)+'\n');print(json.dumps(row),flush=True)
 if result.returncode:raise RuntimeError('Stage failed: '+name)
try:
 step('developer-id-sign-retry1',[str(python),'scripts/sign-native.py','--target','aarch64-apple-darwin','--runtime-dir','artifacts/neo/runtime','--identity','Developer ID Application: Ioka LLC (5PX7D99PXA)','--team-id','5PX7D99PXA','--keychain',str(keychain),'--output','artifacts/neo/signed-retry1'],1200)
 signed=json.loads((source/'artifacts/neo/signed-retry1/signing.json').read_text());assert signed.get('copied_launcher')
 step('signed-installed-runtime-retry1',[str(python),'scripts/check-service-install.py','--runtime','artifacts/neo/signed-retry1/runtime'],900)
 step('signed-service-clients-retry1',[str(python),'scripts/check-service-clients.py','--runtime-dir','artifacts/neo/signed-retry1/runtime','--files','16','--model','--report',str(evidence/'signed-service-clients-retry1.json')],600)
 step('signed-native-package-retry1',[str(python),'scripts/package-native.py','--target','aarch64-apple-darwin','--runtime-dir','artifacts/neo/runtime','--signed-dir','artifacts/neo/signed-retry1','--output','artifacts/neo/native-signed-retry1'],600)
 step('signed-extension-package-retry1',[str(python),'scripts/package-extension.py','--target','aarch64-apple-darwin','--native-dir','artifacts/neo/native-signed-retry1/publish','--output','artifacts/neo/vscode-signed-retry1'],600)
 assert builder.source_identity()==expected and sha(native/receipt['archive'])==original
except BaseException as failure:error={'type':type(failure).__name__,'error':str(failure)}
finally:
 with (evidence/'cargo-clean-sign-retry.log').open('x') as out:clean=subprocess.run(['cargo','clean','--manifest-path',str(source/'Cargo.toml'),'--target-dir',str(run/'target')],cwd=source,env=env,stdout=out,stderr=subprocess.STDOUT)
 report={'schema':'airk.neo-authorized-signing-retry/1','status':'passed' if not error and all(row['exit_code']==0 for row in rows) and clean.returncode==0 else 'failed','native_source':expected,'original_native_archive_sha256':original,'native_archive_after_sha256':sha(native/receipt['archive']),'native_producer_reused':True,'user_explicit_unlock_confirmed':True,'signing_attempts_this_followup':1,'notarization_submitted':False,'publication':False,'keychain_acl_trust_identity_partition_search_list_changed':False,'steps':rows,'error':error,'cargo_clean_exit_code':clean.returncode,'target_absent':not (run/'target').exists()}
 (run/'sign-retry-result.json').write_text(json.dumps(report,indent=2)+'\n');(run/'sign-retry-phase.txt').write_text('complete');print(json.dumps(report),flush=True)
raise SystemExit(0 if report['status']=='passed' else 1)
