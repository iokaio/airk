# SPDX-License-Identifier: Apache-2.0
import hashlib,importlib.util,json
from pathlib import Path
base=Path('D:/code/Github/airk/artifacts/v190');work=base/'p10-worktree';proposal=base/'p11b-resource-observations-proposed'
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
assert sha(proposal/'observations.patch')=='0edee82d2d2dd9fd5a04bcdc6a2e5f468f919fca8ac3c385ab6d519770da8ba1'
assert json.loads((base/'p11b-focused-r3/qualification.json').read_text())['status']=='passed'
rows=json.loads((proposal/'manifest.json').read_text());assert len(rows)==2
for name,row in rows.items():
    assert sha(work/name)==row['base_sha256'] and sha(proposal/'source'/name)==row['proposed_sha256']
snapshot=json.loads((base/'p11-native-p11b-20261106-r1-snapshot.json').read_text())
for name,digest in snapshot['files'].items():assert sha(work/name)==digest,name
for name in rows:(work/name).write_bytes((proposal/'source'/name).read_bytes())
changed=[name for name,digest in snapshot['files'].items() if sha(work/name)!=digest]
assert set(changed)==set(rows)
spec=importlib.util.spec_from_file_location('builder',work/'scripts/build-service-runtime.py')
builder=importlib.util.module_from_spec(spec);spec.loader.exec_module(builder)
record={'source':builder.source_identity(),'mac_functional_source':snapshot['native_source'],
    'changed_external_test_inputs':rows,'other_public_inputs_unchanged':len(snapshot['files'])-2,
    'other_native_inputs_unchanged':952,'production_changes':False,
    'status':'Two reviewed optional resource hooks applied; focused verification pending, not a new52-case pass'}
output=base/'p11b-resource-hooks-applied-r1.json';assert not output.exists()
output.write_text(json.dumps(record,indent=2)+'\n')
print(json.dumps({'source':record['source'],'other_native_inputs_unchanged':952}))
