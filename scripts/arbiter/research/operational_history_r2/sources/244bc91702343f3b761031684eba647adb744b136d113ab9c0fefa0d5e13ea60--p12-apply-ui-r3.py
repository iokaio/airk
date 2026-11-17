# SPDX-License-Identifier: Apache-2.0
import hashlib,json
from pathlib import Path
base=Path('D:/code/Github/airk/artifacts/v190');work=base/'p10-worktree';proposal=base/'p12-ui-composed-p11b-r3'
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest() if p.exists() else None
assert json.loads((base/'p11b-local-acceptance-commit-r1.json').read_text())['phase_accepted']
assert sha(proposal/'p12-ui-composed.patch')=='3f0f4f63f9f700e7cb8a8256cca204f18a9da13241f78fe58b3e9bddf1be2eac'
rows=json.loads((proposal/'composed-manifest.json').read_text());assert len(rows)==13
for name,row in rows.items():
    assert name.startswith('packages/vscode/src/') or name=='scripts/package-extension.py',name
    assert sha(work/name)==row['base_sha256'],name
    assert sha(proposal/'tree'/name)==row['composed_sha256'],name
for name in rows:
    path=work/name;path.parent.mkdir(parents=True,exist_ok=True);path.write_bytes((proposal/'tree'/name).read_bytes())
assert all(sha(work/name)==row['composed_sha256'] for name,row in rows.items())
with (base/'p12-ui-applied-r1.json').open('x',encoding='utf-8') as f:
    json.dump({'status':'13 reviewed UI files applied; compile/tests pending; native unchanged','files':rows,
        'patch_sha256':sha(proposal/'p12-ui-composed.patch')},f,indent=2);f.write('\n')
print('Applied13 exact UI files; native source unchanged; qualification pending.')
