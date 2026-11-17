# SPDX-License-Identifier: Apache-2.0
import hashlib,json,os,zipfile
from pathlib import Path
parent=Path('/Users/tylerjensen/airk-builds/p6a-20261103.jBR4Jh');root=parent/'source-r2'
e=json.loads((parent/'p6d-native-inputs.json').read_text());old=json.loads((parent/'p6c-native-inputs.json').read_text());z=zipfile.ZipFile(parent/'p6d-native-delta.zip')
expected={n for n,h in e['files'].items() if old['files'].get(n)!=h}
assert len(expected)==14 and set(z.namelist())==expected
for n in expected:
 assert n.startswith('crates/') and n.endswith('.rs')
 assert hashlib.sha256(z.read(n)).hexdigest()==e['files'][n]
for n,h in json.loads((parent/'p6d-smoke-inputs.json').read_text()).items():assert hashlib.sha256((root/n).read_bytes()).hexdigest()==h,n
for n in expected:
 destination=root/n
 assert destination.resolve().is_relative_to(root.resolve())
 destination.write_bytes(z.read(n));os.utime(destination,None)
print('Applied and timestamped exactly14 reviewed public Rust inputs; native-clients smoke bytes also verified')
