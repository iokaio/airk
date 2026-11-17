# SPDX-License-Identifier: Apache-2.0
import hashlib,json,shutil
from pathlib import Path
root=Path.cwd()
manifest=root/'artifacts/advanced-split/models/frontend-focus-purpose-r1/mount-manifest.json'
def sha(path):return hashlib.sha256(path.read_bytes()).hexdigest()
assert sha(manifest)=='2ddabaefc8a65f9dd738df160827e7385c0648bb84638a364a81309cb50b6db8'
rows=json.loads(manifest.read_bytes())['files']
out=root/'artifacts/advanced-split/receipts/purpose-fix-mount-r139'
assert not out.exists()
for name,row in rows.items():
    target=root/name;proposed=root/row['proposed']
    assert (sha(target) if target.exists() else None)==row['before_sha256'],name
    assert sha(proposed)==row['after_sha256'],name
out.mkdir(parents=True)
for name,row in rows.items():
    target=root/name
    if target.exists():
        backup=out/'before'/name;backup.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(target,backup)
    target.parent.mkdir(parents=True,exist_ok=True)
    shutil.copyfile(root/row['proposed'],target)
    assert sha(target)==row['after_sha256']
(out/'review.json').write_text(json.dumps({'manifest_sha256':sha(manifest),'files':rows,'review':'Root self-review plus FocusContract independent review; purpose-only correction, unchanged scope/budget/wire authority; no qualification claim'},indent=2)+'\n',encoding='utf-8')
print(json.dumps({'mounted':len(rows),'receipt':str(out/'review.json')}))
