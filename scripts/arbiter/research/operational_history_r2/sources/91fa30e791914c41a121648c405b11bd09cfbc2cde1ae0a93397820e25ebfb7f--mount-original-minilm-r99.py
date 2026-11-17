# SPDX-License-Identifier: Apache-2.0
import hashlib,json,shutil
from pathlib import Path
r=Path.cwd().resolve();p=r/'artifacts/advanced-split/models/native-minilm-original-baseline-r1'
sha=lambda data:hashlib.sha256(data).hexdigest()
manifest=p/'coherent-source-mount-r1.json';assert sha(manifest.read_bytes())=='882fd3848d98ccb3b2b785492dbe0e7c74c54bd49f8eb7d6df3448b3221d1ff6'
m=json.loads(manifest.read_bytes());assert len(m['files'])==17
assert sha((r/'artifacts/advanced-split/models/original-minilm-retention-decision-r1.json').read_bytes())==m['recorded_decision_sha256']
prepared=[]
for row in m['files']:
 target=(r/row['target']).resolve();source=Path(row['source']).resolve()
 assert target.is_relative_to(r) and not target.is_relative_to(r/'artifacts') and source.is_relative_to(p)
 assert not source.is_symlink() and not target.is_symlink()
 data=source.read_bytes();assert sha(data)==row['source_sha256']
 assert (sha(target.read_bytes()) if target.exists() else None)==row['target_before_sha256']
 prepared.append((target,data))
s8=r/'artifacts/advanced-split/models/original-s8-native-qualification-r1'
data=(s8/'original_s8.rs.proposed.txt').read_bytes();assert sha(data)=='7f55aed887947fc6c223b44f5453420dc34e25bf55b0cdf9c92c7f9b8e9ddc87'
child=r/'crates/airk-kernel/src/inference_owner/evaluation_tests/original_s8.rs';assert not child.exists();prepared.append((child,data))
parent=r/'crates/airk-kernel/src/inference_owner/evaluation_tests.rs';old=parent.read_bytes();assert b'mod original_s8;' not in old
prepared.append((parent,old+b'\nmod original_s8;\n'))
contract=r/'crates/airk-kernel/src/inference_owner/evaluation_tests/CONTRACT.md';prepared.append((contract,contract.read_bytes()+b'\n\n'+(s8/'CONTRACT.delta.proposed.md').read_bytes()))
backup=r/'artifacts/advanced-split/receipts/original-minilm-source-mount-r99';backup.mkdir(parents=True,exist_ok=False)
rows=[]
for target,data in prepared:
 before=None
 if target.exists():
  saved=backup/target.relative_to(r);saved.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(target,saved);before=sha(saved.read_bytes())
 rows.append({'path':str(target.relative_to(r)),'before':before,'after':sha(data)})
for target,data in sorted(prepared,key=lambda pair:pair[0].suffix!='.md'):
 target.parent.mkdir(parents=True,exist_ok=True);target.write_bytes(data)
(backup/'mount.json').write_text(json.dumps({'manifest_sha256':sha(manifest.read_bytes()),'root_review':'Root self-review + independent metadata and S8 reviews. Exact original baseline, no epoch1 promotion; current Runtime preserved, qualification remains pending. Native Rust draft precedes catalog/data consumers; contracts first. Scope includes full same-runtime metadata/cache/readiness refusal and existing regeneration ownership.','files':rows},indent=2)+'\n',encoding='utf-8')
print(json.dumps({'mounted':len(rows),'receipt':str(backup/'mount.json')}))
