# SPDX-License-Identifier: Apache-2.0
import hashlib,json,os,zipfile
from pathlib import Path
p=Path('/Users/tylerjensen/airk-builds/p6a-20261103.jBR4Jh');r=p/'source-r2';e=json.loads((p/'p6d-claim-inputs.json').read_text());old=json.loads((p/'p6d-fairness-interval-inputs.json').read_text());z=zipfile.ZipFile(p/'p6d-claim-delta.zip')
for n,h in old['files'].items():assert hashlib.sha256((r/n).read_bytes()).hexdigest()==h,n
changed={n for n,h in e['files'].items() if old['files'].get(n)!=h};assert len(changed)==3 and changed==set(z.namelist())
for n in changed:
 assert n.startswith('crates/airk-store/src/') and n.endswith('.rs')
 data=z.read(n);assert hashlib.sha256(data).hexdigest()==e['files'][n]
 dest=r/n;assert dest.resolve().is_relative_to(r.resolve());dest.write_bytes(data);os.utime(dest,None)
base=p/'artifacts/p6d-fairness-claim-private';base.mkdir(mode=0o700,exist_ok=False);assert base.stat().st_uid==os.getuid() and base.stat().st_mode & 0o077==0
print(json.dumps({'changed_only_reviewed_rust':sorted(changed),'private_fixture_base':str(base),'private_mode':oct(base.stat().st_mode & 0o777)}))
