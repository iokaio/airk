# SPDX-License-Identifier: Apache-2.0
import json,os,subprocess,time
from pathlib import Path
root=Path(__file__).resolve().parents[3];os.chdir(root);base=Path(__file__).resolve().parent
os.environ.update(json.loads((base/'static-environment-supplement.json').read_text())['environment'])
os.environ['PATH']=str(Path('C:/Users/tdotn/.cargo/bin'))+os.pathsep+os.environ['PATH']
os.environ.update(CARGO_TARGET_DIR=str(root/'artifacts/v190/p10-target'),CARGO_INCREMENTAL='1',GIT_CONFIG_COUNT='1',GIT_CONFIG_KEY_0='safe.directory',GIT_CONFIG_VALUE_0=root.as_posix())
private=json.loads((base/'user-private-fixture-base.json').read_text())['path'];os.environ.update(TEMP=private,TMP=private,AIRK_PRIVATE_CENSUS_BASE=private)
def gate(name,args):
 start=time.monotonic()
 with (base/(name+'.log')).open('x') as log:r=subprocess.run(args,stdout=log,stderr=subprocess.STDOUT)
 receipt={'arguments':args,'elapsed_seconds':time.monotonic()-start,'exit_code':r.returncode,'target_directory':os.environ['CARGO_TARGET_DIR'],'incremental':'on','scope':'index/catalog busy retry shared native boundary; Windows debug only'}
 with (base/(name+'.json')).open('x') as f:json.dump(receipt,f,indent=2)
 print(json.dumps(receipt),flush=True)
 if r.returncode:raise SystemExit(r.returncode)
gate('index-access-retry-format-r1',['cargo','fmt','--all'])
gate('index-access-retry-clippy-r1',['python','scripts/dev-check.py','-p','airk-cli','--features','local-inference','--target-dir',os.environ['CARGO_TARGET_DIR'],'--incremental','on','--skip-fmt'])
gate('index-access-retry-native-tests-r1',['cargo','nextest','run','--locked','-p','airk-cli','--no-default-features','--features','local-inference','--bin','airk','--profile','ci','--timings','-E','test(registry_retry_tests) | test(console_client::tests) | test(client_requests::tests) | test(service_management::tests) | test(connection_failure_tests)'])
gate('windows-debug-runtime-r22',['python','scripts/build-service-runtime.py','--profile','debug','--destination',str(base/'windows-debug-r22/runtime')])
