# SPDX-License-Identifier: Apache-2.0
import hashlib,json,shutil
from pathlib import Path
r=Path.cwd();p=r/'artifacts/advanced-split/models/native-minilm-production-decision-r1/metadata-length-r1/manifest.json'
sha=lambda data:hashlib.sha256(data).hexdigest();assert sha(p.read_bytes())=='775fcb3e0d3fd607ed53c4b53b298fb6054a7137e185d434a0032e0ec621022b'
rows=[]
for item in json.loads(p.read_bytes())['files']:
 target=r/item['target'];data=(r/item['proposed']).read_bytes()
 assert sha(target.read_bytes())==item['base_sha256'] and sha(data)==item['proposed_sha256'];rows.append((target,data))
out=r/'artifacts/advanced-split/receipts/metadata-length-mount-r101';out.mkdir(parents=True,exist_ok=False)
for target,data in rows:
 saved=out/target.relative_to(r);saved.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(target,saved)
for target,data in sorted(rows,key=lambda row:row[0].suffix!='.md'):target.write_bytes(data)
(out/'mount.json').write_text(json.dumps({'manifest_sha256':sha(p.read_bytes()),'root_review':'Exact active neural reference lengths must match native catalog; resealed contradictory lengths error without payload reads. Actual private lease and retired-manifest classification remain unchanged. Contract first.','targets':[str(t.relative_to(r)) for t,_ in rows]},indent=2)+'\n')
print('mounted4')
