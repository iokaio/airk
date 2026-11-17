# SPDX-License-Identifier: Apache-2.0
import hashlib,json,os,shutil,subprocess,sys,time
from pathlib import Path
r=Path.cwd();out=r/'artifacts/advanced-split/receipts/cargo-vet-refresh-r106';out.mkdir(parents=True,exist_ok=False)
files=['supply-chain/config.toml','supply-chain/audits.toml','supply-chain/imports.lock','Cargo.lock']
before={}
for name in files:
 p=r/name;raw=p.read_bytes();before[name]=hashlib.sha256(raw).hexdigest();(out/p.name).write_bytes(raw)
env=dict(os.environ);env.update(CARGO_NET_OFFLINE='true',GIT_CONFIG_COUNT='1',GIT_CONFIG_KEY_0='safe.directory',GIT_CONFIG_VALUE_0=str(r));env['PATH']=str(r/'artifacts/tools/cargo-vet')+';C:/Users/tdotn/.cargo/bin;'+env['PATH']
command=['C:/Users/tdotn/.cargo/bin/cargo.exe','vet','fetch-imports','--no-minimize-exemptions','--no-registry-suggestions','--cache-dir',str(r/'artifacts/tools/cargo-vet/cache'),'--output-format=json']
started=time.monotonic()
with (out/'fetch.log').open('xb') as log:result=subprocess.run(command,env=env,stdout=log,stderr=subprocess.STDOUT)
after={name:hashlib.sha256((r/name).read_bytes()).hexdigest() for name in files}
(out/'result.json').write_text(json.dumps({'command':command,'exit_code':result.returncode,'elapsed_seconds':time.monotonic()-started,'before':before,'after':after,'scope':'Deliberate selection of already trusted immutable imports and actual publisher metadata; no graph/target filters, new auditor, exemption or criterion change authorized.'},indent=2)+'\n')
assert all(before[name]==after[name] for name in files if name!='supply-chain/imports.lock'),'unexpected source/audit/config mutation'
raise SystemExit(result.returncode)
