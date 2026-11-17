# SPDX-License-Identifier: Apache-2.0
"""Transfer only reviewed public frozen inputs; fetch locked dependencies, never build."""
import hashlib,importlib.util,json,re,shlex,subprocess,time
from pathlib import Path
ROOT=Path.cwd(); BASE=ROOT/'artifacts/readiness-1817'
c=json.loads((ROOT/'artifacts/v190/neo-connection.local.json').read_text())
r=json.loads((BASE/'neo-readiness-1817-r1-prepared.json').read_text()); label=r['label'];assert re.fullmatch('readiness-1817-[a-z0-9-]+',label)
remote='/Users/tylerjensen/airk-builds/'+label;peer=c['user']+'@'+c['host'];ssh=['ssh','-i',c['identity_path'],*c['ssh_options'],peer]
def digest(path):
 with Path(path).open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
s=importlib.util.spec_from_file_location('freeze_owner',ROOT/'scripts/build-service-runtime.py');m=importlib.util.module_from_spec(s);s.loader.exec_module(m);assert m.source_identity()==r['native_source']
for name,value in r['pins'].items():assert digest(name)==value
create='from pathlib import Path; import os; os.umask(0o077); p=Path('+repr(remote)+'); assert p.parent==Path.home()/"airk-builds"; p.mkdir(mode=0o700); assert p.resolve()==p'
subprocess.run([*ssh,'/usr/bin/python3 -c '+shlex.quote(create)],check=True,timeout=30)
for local,name in r['transfers'].items():
 assert re.fullmatch('[a-z0-9.-]+',name)
 subprocess.run(['scp','-i',c['identity_path'],*c['ssh_options'],str(Path(local).relative_to(ROOT)),peer+':'+remote+'/'+name],check=True,timeout=120)
py='/Users/tylerjensen/airk-builds/timing-1.6.4.PutU1P/tools/python3.13'
started=time.monotonic()
with (BASE/'neo-dependency-fetch-remote.log').open('x',encoding='utf-8') as log:
 result=subprocess.run([*ssh,shlex.quote(py)+' '+shlex.quote(remote+'/dependency-fetch.py')],stdout=log,stderr=subprocess.STDOUT,timeout=300)
receipt={'remote':remote,'public_source':r['native_source'],'transferred_pins':r['pins'],'dependency_fetch_exit':result.returncode,'elapsed_seconds':time.monotonic()-started,'native_build_started':False,'log':'neo-dependency-fetch-remote.log'}
(BASE/'neo-transfer-and-prefetch.json').write_text(json.dumps(receipt,indent=2)+'\n');print(json.dumps({k:v for k,v in receipt.items() if k!='transferred_pins'}))
raise SystemExit(result.returncode)
