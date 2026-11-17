# SPDX-License-Identifier: Apache-2.0
import hashlib,importlib.util,json
from pathlib import Path
base=Path('D:/code/Github/airk/artifacts/v190');work=base/'p10-worktree';proposal=base/'p12-native-composed-p11b-r2'
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest() if p.exists() else None
assert json.loads((base/'p11b-local-acceptance-commit-r1.json').read_text())['phase_accepted']
assert sha(proposal/'p12-native-composed.patch')=='9fc4fe4a58f5fb5002c945bb3bd7400fae5b10526e1f23e9c9540a342f59308b'
rows=json.loads((proposal/'composed-manifest.json').read_text());assert len(rows)==30
for name,row in rows.items():
    assert name.startswith('crates/'),name
    assert sha(work/name)==row['base_sha256'],name
    assert sha(proposal/'tree'/name)==row['proposed_sha256'],name
for name in rows:
    path=work/name;path.parent.mkdir(parents=True,exist_ok=True);path.write_bytes((proposal/'tree'/name).read_bytes())
assert all(sha(work/name)==row['proposed_sha256'] for name,row in rows.items())
spec=importlib.util.spec_from_file_location('builder',work/'scripts/build-service-runtime.py')
builder=importlib.util.module_from_spec(spec);spec.loader.exec_module(builder)
record={'status':'30 reviewed native files applied; compilation and18new tests pending; not accepted',
    'files':rows,'source':builder.source_identity(),'patch_sha256':sha(proposal/'p12-native-composed.patch')}
with (base/'p12-native-applied-r1.json').open('x',encoding='utf-8') as f:json.dump(record,f,indent=2);f.write('\n')
print(json.dumps({'files':30,'source':record['source']}))
