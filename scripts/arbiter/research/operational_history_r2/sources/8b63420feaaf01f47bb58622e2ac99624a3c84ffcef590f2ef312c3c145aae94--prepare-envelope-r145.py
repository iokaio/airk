# SPDX-License-Identifier: Apache-2.0
import hashlib,json,shutil
from pathlib import Path
r=Path.cwd();out=r/'artifacts/advanced-split/receipts/envelope-mount-r145'
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
m=r/'artifacts/advanced-split/models/native-worker-error-envelope-r1/mount-manifest.json'
d=r/'artifacts/advanced-split/models/selected-native-goal-driver-r1/optional-model-delivery-docs-r1/proposal.json'
assert sha(m)=='57e10b420e35023b554263cdc39bdac8ad9645537a0acb448018f0d475ac24cb'
assert sha(d)=='6e354aced2452a6699c329561ee0865c125569dd0403301236053c339bd1179b'
rows=json.loads(m.read_bytes())['files']
for name,row in json.loads(d.read_bytes())['files'].items():
    rows[name]={'before_sha256':row['base_sha256'],'after_sha256':row['proposed_sha256'],'proposed':row['proposed_path']}
for name,row in rows.items():
    assert sha(r/name)==row['before_sha256'],name
    assert sha(r/row['proposed'])==row['after_sha256'],name
out.mkdir()
for name,row in rows.items():
    p=r/name;backup=out/'before'/name;backup.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(p,backup);shutil.copyfile(r/row['proposed'],p)
(out/'review.json').write_text(json.dumps({'files':rows,'review':'Root reviewed seven combined files. FocusContract independently reviewed the three native files. Omit only native SDK discriminator; preserve strict Kernel bounds and full error bodies. Documentation records user on-demand delivery requirement without claiming source availability or authorizing publication.'},indent=2)+'\n',encoding='utf-8')
print(json.dumps({'mounted':len(rows)}))
