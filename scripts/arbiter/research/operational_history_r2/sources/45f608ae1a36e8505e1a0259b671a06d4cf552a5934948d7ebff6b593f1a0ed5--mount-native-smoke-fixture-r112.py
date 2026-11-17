# SPDX-License-Identifier: Apache-2.0
import hashlib,json,shutil
from pathlib import Path
r=Path.cwd();p=r/'artifacts/advanced-split/local-delivery-preparation-r1/native-smoke-fixture-repair-r1';m=json.loads((p/'manifest.json').read_bytes());sha=lambda x:hashlib.sha256(x).hexdigest()
out=r/'artifacts/advanced-split/receipts/native-smoke-fixture-mount-r112';out.mkdir(parents=True,exist_ok=False);rows=[]
for target,row in m.items():
 dst=r/target;data=Path(row['private_candidate']).read_bytes();assert sha(data)==row['candidate_sha256'];assert (sha(dst.read_bytes()) if dst.exists() else 'missing')==row['original_sha256'];rows.append((dst,data))
for dst,data in rows:
 if dst.exists():saved=out/dst.name;shutil.copyfile(dst,saved)
 dst.parent.mkdir(parents=True,exist_ok=True);dst.write_bytes(data)
(out/'review.json').write_text(json.dumps({'review':'Root verified historical R2 identity retained/refused by current admission; separate current R104 exact native observed identity/pipeline and original key-order/foreign-manifest assertions retained. No Python-derived expected native identity replaces history.','files':m},indent=2)+'\n')
