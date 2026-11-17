# SPDX-License-Identifier: Apache-2.0
import hashlib,json,os,zipfile
from pathlib import Path
p=Path('/Users/tylerjensen/airk-builds/p6a-20261103.jBR4Jh');r=p/'source-r2';e=json.loads((p/'p6e-smoke-inputs.json').read_text());old=json.loads((p/'p6d-accepted-a87-inputs.json').read_text());z=zipfile.ZipFile(p/'p6e-native-delta.zip')
for name,h in old['files'].items():assert hashlib.sha256((r/name).read_bytes()).hexdigest()==h,name
changed={name for name,h in e['files'].items() if old['files'].get(name)!=h};assert len(changed)==4 and changed==set(z.namelist())
for name in changed:
 assert name.startswith('crates/') and name.endswith('.rs')
 data=z.read(name);assert hashlib.sha256(data).hexdigest()==e['files'][name]
 dest=r/name;assert dest.resolve().is_relative_to(r.resolve());dest.write_bytes(data);os.utime(dest,None)
print(json.dumps({'changed_only_reviewed_rust':sorted(changed)}))
