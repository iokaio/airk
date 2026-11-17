# SPDX-License-Identifier: Apache-2.0
import json,os,subprocess,time
from pathlib import Path
root=Path(__file__).resolve().parents[3];os.chdir(root);base=Path(__file__).resolve().parent
os.environ.update(json.loads((base/'static-environment-supplement.json').read_text())['environment']);os.environ['PATH']='C:/Users/tdotn/.cargo/bin'+os.pathsep+os.environ['PATH'];os.environ.update(CARGO_TARGET_DIR=str(root/'artifacts/v190/p10-target'),CARGO_INCREMENTAL='1')
private=json.loads((base/'user-private-fixture-base.json').read_text())['path'];os.environ.update(TEMP=private,TMP=private,AIRK_PRIVATE_CENSUS_BASE=private)
args=['cargo','test','--locked','-p','airk-cli','--no-default-features','--features','embedding-worker','--lib','--timings','service_runtime::','--','--nocapture'];start=time.monotonic()
log=base/'index-access-retry-native-tests-r4.log'
with log.open('x') as f:r=subprocess.run(args,stdout=f,stderr=subprocess.STDOUT)
result={'arguments':args,'exit_code':r.returncode,'elapsed_seconds':time.monotonic()-start,'incremental':'on','target_directory':os.environ['CARGO_TARGET_DIR'],'nextest':'unavailable; documented Cargo test fallback'}
with (base/'index-access-retry-native-tests-r4.json').open('x') as f:json.dump(result,f,indent=2)
print(json.dumps(result),flush=True)
assert r.returncode==0 and 'running 0 tests' not in log.read_text(), 'No empty test selection qualifies'
r=subprocess.run(['python',str(base/'run-c8-r10.py')]);raise SystemExit(r.returncode)
