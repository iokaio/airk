# SPDX-License-Identifier: Apache-2.0
import hashlib,json,os,zipfile
from pathlib import Path
p=Path('/Users/tylerjensen/airk-builds/p6a-20261103.jBR4Jh');r=p/'source-r2';e=json.loads((p/'p6d-fairness-inputs.json').read_text());old=json.loads((p/'p6d-native-inputs.json').read_text());z=zipfile.ZipFile(p/'p6d-fairness-delta.zip')
for n,h in old['files'].items():assert hashlib.sha256((r/n).read_bytes()).hexdigest()==h,n
names=set(z.namelist());assert names=={'scripts/check-service-source-fairness.py','scripts/tests/test_service_source_fairness.py','scripts/check-service-pool.py','scripts/package-native.py'}
changed=[]
for n in names:
 data=z.read(n);assert hashlib.sha256(data).hexdigest()==e['files'][n]
 dest=r/n;assert dest.resolve().is_relative_to(r.resolve())
 if not dest.is_file() or hashlib.sha256(dest.read_bytes()).hexdigest()!=e['files'][n]:dest.write_bytes(data);changed.append(n)
base=p/'artifacts/p6d-fairness-private';base.mkdir(mode=0o700,exist_ok=False)
assert base.stat().st_uid==os.getuid() and base.stat().st_mode & 0o077==0
print(json.dumps({'changed_only_reviewed_python':sorted(changed),'private_fixture_base':str(base),'private_mode':oct(base.stat().st_mode & 0o777)}))
