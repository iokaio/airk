# SPDX-License-Identifier: Apache-2.0
import hashlib,json,shutil
from pathlib import Path
r=Path.cwd();m=r/'artifacts/advanced-split/models/context-partition-identity-r1/mount-manifest.json'
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
assert sha(m)=='d258c0f180d1b18e260915d39e25d5f414745923c1d6e41c1a4e409ae5708177'
rows=json.loads(m.read_bytes())['files'];out=r/'artifacts/advanced-split/receipts/context-identity-mount-r147'
for name,row in rows.items():
    p=r/name;assert (sha(p)if p.exists()else None)==row['before_sha256'];assert sha(r/row['proposed'])==row['after_sha256']
out.mkdir()
for name,row in rows.items():
    p=r/name
    if p.exists():
        backup=out/'before'/name;backup.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(p,backup)
    p.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(r/row['proposed'],p)
(out/'review.json').write_text(json.dumps({'files':rows,'review':'Root independently reviewed delegated six-file implementation, current Supervisor purpose distinction and MCP sibling fixtures. Context digest remains inert accounting identity; actual installed index/fingerprint/original attempt remain mandatory. Ordinary Focus retains exact actual selection. No wire or source authority enlargement; no pass claimed until native gates.'},indent=2)+'\n',encoding='utf-8')
print(json.dumps({'mounted':len(rows)}))
