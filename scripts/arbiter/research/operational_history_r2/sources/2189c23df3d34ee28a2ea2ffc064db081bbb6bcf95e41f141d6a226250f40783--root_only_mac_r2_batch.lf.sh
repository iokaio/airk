# SPDX-License-Identifier: Apache-2.0
#!/bin/sh
set -eu
umask 077
/Users/tylerjensen/Code/airk/artifacts/tools/neo-python-3.13.18/bin/python3.13 -I -B - --root-reviewed-execute <<'AIRK_ROOT_BATCH'
"""ROOT ONLY: current eac7 Mac scoped platform batch; this file is preparation."""
from pathlib import Path
import hashlib,importlib.util,json,os,platform,shutil,subprocess,sys,time
SOURCE=Path('/Users/tylerjensen/airk-builds/advanced-split-local-delivery-r2/source')
BASE=SOURCE.parent
TARGET=SOURCE/'artifacts/v190/p10-target'
OUT=BASE/'receipts/native-platform-r2'
PREREQS=Path('/Users/tylerjensen/airk-builds/advanced-split-local-delivery-prerequisites-r1')
PYTHON=Path('/Users/tylerjensen/Code/airk/artifacts/tools/neo-python-3.13.18/bin/python3.13')
CARGO=Path('/Users/tylerjensen/.cargo/bin/cargo')
NODE=PREREQS/'tools/node-v24.15.0-darwin-arm64/bin/node'
STATIC=Path('/Users/tylerjensen/Code/airk/artifacts/advanced-split/ort130-arm64-env.json')
INVENTORY=PREREQS/'inputs/runtime-handoff-r1/admin-assets.json'
NATIVE_SHA='eac7b9aac9a5a9943422a08406dd9602da55a35d2d7ed7cbceb24a2f4bdc29c5'

def load(name,path):
 spec=importlib.util.spec_from_file_location(name,path);module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module);return module

def assert_plain(path):
 assert path.is_absolute() and path.resolve(strict=False)==path
 for parent in (path,*path.parents):assert not parent.is_symlink()

def main():
 assert sys.argv[1:]==['--root-reviewed-execute'],'Root must release and execute this reviewed remote batch'
 assert os.geteuid()==501 and platform.system()=='Darwin' and platform.machine()=='arm64'
 for path in [SOURCE,TARGET,OUT,PREREQS,INVENTORY]:assert_plain(path)
 assert SOURCE.is_dir() and not OUT.exists() and TARGET==SOURCE/'artifacts/v190/p10-target'
 OUT.mkdir(parents=True,mode=0o700)
 os.chdir(SOURCE)
 owner=load('mac_recovery_receipt_owner',SOURCE/'scripts/dev/receipt_runner.py')
 runtime=load('mac_recovery_runtime_owner',SOURCE/'scripts/build-service-runtime.py')
 initial=owner.observe(['native','harness','fixtures','frontend'])
 assert initial['native']['sha256']==NATIVE_SHA
 source_identity=runtime.source_identity()
 env=dict(os.environ,PATH=str(NODE.parent)+':'+str(CARGO.parent)+':'+os.environ.get('PATH',''),CARGO_TARGET_DIR=str(TARGET),CARGO_INCREMENTAL='0',CARGO_BUILD_JOBS='2',MACOSX_DEPLOYMENT_TARGET='13.3',AIRK_BUNDLED_MODEL_DIR=str(PREREQS/'inputs/bundled-model-v3'),NPM_CONFIG_CACHE=str(PREREQS/'cache/npm'),NPM_CONFIG_LOGS_DIR=str(PREREQS/'cache/npm/_logs'),NPM_CONFIG_UPDATE_NOTIFIER='false')
 # Missing locked Cargo packages may need ordinary registry access. No model
 # download is requested: the complete original v3 model closure is local.
 env.pop('CARGO_NET_OFFLINE',None)
 env.pop('CARGO_BUILD_TARGET',None)  # Verified arm64 native host; no inherited cross target
 static=load('mac_recovery_static_owner',SOURCE/'scripts/static-runtime-environment.py')
 env.update(static.load_environment_file(STATIC))
 nextest=subprocess.check_output([str(CARGO),'nextest','--version'],env=env,text=True).strip()
 assert nextest.split()[1]=='0.9.148',nextest
 cargo_started=False;rows=[];primary=None;cleanup=None;started=time.monotonic()
 stages=[
  ('scoped-clippy',[str(CARGO),'clippy','-p','airk-service','-p','airk-supervisor','-p','airk-kernel','--all-targets','--all-features','--locked','--timings','--','-D','warnings'],None),
  ('bridge-six',[str(CARGO),'nextest','run','--profile','ci','-p','airk-service','-p','airk-supervisor','-p','airk-kernel','--all-features','--locked','--timings','-E','test(runtime_metadata::arbiter::tests::sync_control_)'],6),
  ('matched-pair-model-smoke',[str(PYTHON),'-I','-B','scripts/build-service-runtime.py','--destination',str(BASE/'artifacts/runtime-recovery-r2'),'--profile','release-local','--admin-inventory',str(INVENTORY)],None),
 ]
 try:
  for name,command,expected in stages:
   cargo_started=True
   wrapped=[str(PYTHON),'-I','-B','scripts/dev/receipt_runner.py','--groups','native,harness,fixtures,frontend','--tier','T1' if expected else 'T2','--out',str(OUT),'--target-dir',str(TARGET),'--node',str(NODE),*(['--allow-no-tests'] if expected is None else []),name,*command]
   tick=time.monotonic()
   with (OUT/(name+'-wrapper.log')).open('x') as log:result=subprocess.run(wrapped,env=env,stdout=log,stderr=subprocess.STDOUT)
   row={'name':name,'command':wrapped,'exit_code':result.returncode,'elapsed_seconds':time.monotonic()-tick};rows.append(row)
   (OUT/'stages.json').write_text(json.dumps(rows,indent=2)+'\n')
   if result.returncode:raise RuntimeError(name+' failed; stop and preserve diagnostics')
   receipt=json.loads((OUT/(name+'.json')).read_bytes())
   assert receipt['inputs_unchanged'] and receipt['incremental']=='0'
   if expected is not None:assert receipt['selected_tests']==expected,'Exactly six actual bridge tests required'
 except BaseException as error:
  primary={'type':type(error).__name__,'message':str(error)}
 finally:
  try:
   if cargo_started:
    # Do not clean against an active compiler or a redirected/computed target.
    assert_plain(TARGET);assert TARGET==SOURCE/'artifacts/v190/p10-target'
    process_text=subprocess.check_output(['/bin/ps','-axo','pid=,ppid=,command='],text=True)
    active=[line for line in process_text.splitlines() if str(TARGET) in line and any(word in line for word in ['rustc','cargo','clang','cmake','ninja'])]
    if active:
     cleanup={'result':'refused_active_owned_compilers','processes':active}
    else:
     # Retain any other timing/JUnit evidence before the exact target is cleaned.
     if TARGET.exists():
      for pattern,folder in [('cargo-timings/*.html','cargo-timings'),('nextest/ci/*.xml','nextest')]:
       dest=OUT/folder;dest.mkdir(exist_ok=True)
       for path in TARGET.glob(pattern):shutil.copy2(path,dest/path.name)
     command=[str(CARGO),'clean','--locked','--target-dir',str(TARGET)];tick=time.monotonic()
     with (OUT/'cargo-clean.log').open('x') as log:result=subprocess.run(command,env=env,stdout=log,stderr=subprocess.STDOUT)
     cleanup={'command':command,'exit_code':result.returncode,'elapsed_seconds':time.monotonic()-tick,'target':str(TARGET),'compiler_descendants_absent':True}
  except BaseException as cleanup_error:
   cleanup={'result':'refused_or_failed_cleanup','type':type(cleanup_error).__name__,'message':str(cleanup_error)}
  final=owner.observe(['native','harness','fixtures','frontend'])
  result={'status':'passed_scoped_platform_candidate' if primary is None and cleanup and cleanup.get('exit_code')==0 else 'failed_or_cleanup_incomplete','native_source_sha256':NATIVE_SHA,'runtime_source_identity':source_identity,'inputs_unchanged':initial==final,'stages':rows,'error':primary,'cleanup':cleanup,'elapsed_seconds':time.monotonic()-started,'scope':'Mac arm64 scoped three-crate Clippy, six actual runtime bridge cases and canonical matched-pair MiniLM smoke; final qualified catalog, full platform and installed acceptance remain separate.','blanket_qualification_claim':False}
  (OUT/'result.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result),flush=True)
 return 0 if result['status']=='passed_scoped_platform_candidate' and result['inputs_unchanged'] else 1
if __name__=='__main__':raise SystemExit(main())

AIRK_ROOT_BATCH
