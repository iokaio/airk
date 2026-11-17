# SPDX-License-Identifier: Apache-2.0
import hashlib,json,os,subprocess,time
from pathlib import Path
r=Path.cwd();out=r/'artifacts/advanced-split/receipts/cargo-vet-check-r109';out.mkdir(parents=True,exist_ok=False)
files=['supply-chain/config.toml','supply-chain/audits.toml','supply-chain/imports.lock','supply-chain/source-policy.json','Cargo.lock']
before={name:hashlib.sha256((r/name).read_bytes()).hexdigest() for name in files}
env=dict(os.environ);env.update(CARGO_NET_OFFLINE='true',GIT_CONFIG_COUNT='1',GIT_CONFIG_KEY_0='safe.directory',GIT_CONFIG_VALUE_0=str(r));env['PATH']=str(r/'artifacts/tools/cargo-vet')+';C:/Users/tdotn/.cargo/bin;'+env['PATH']
command=['C:/Users/tdotn/.cargo/bin/cargo.exe','vet','check','--locked','--no-minimize-exemptions','--no-registry-suggestions','--cache-dir',str(r/'artifacts/tools/cargo-vet/cache'),'--output-format=json']
started=time.monotonic()
with (out/'check.log').open('xb') as log:result=subprocess.run(command,env=env,stdout=log,stderr=subprocess.STDOUT)
after={name:hashlib.sha256((r/name).read_bytes()).hexdigest() for name in files}
(out/'result.json').write_text(json.dumps({'command':command,'exit_code':result.returncode,'elapsed_seconds':time.monotonic()-started,'before':before,'after':after,'inputs_unchanged':before==after,'all_features_default':True,'unfiltered_targets':True},indent=2)+'\n')
assert before==after,'check mutated its inputs'
raise SystemExit(result.returncode)
