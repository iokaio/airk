# SPDX-License-Identifier: Apache-2.0
import hashlib,json,shutil
from pathlib import Path
r=Path.cwd();p=r/'artifacts/advanced-split/models/native-parent-bag-native-run-preparation-r1'
review=json.loads((p/'preparation-review-r1.json').read_bytes());pins={x['path']:x['sha256'] for x in review['files']}
base=json.loads((p/'amendment-review.json').read_bytes())['public_source_before'];rows=[]
for name in ['native_parent_bag.rs','development.rs']:
 target='crates/airk-kernel/src/inference_owner/evaluation_tests/'+name
 proposal=p/(name+'.formatted.proposed.txt');key=str(proposal.relative_to(r))
 assert hashlib.sha256(proposal.read_bytes()).hexdigest()==pins[key]
 assert hashlib.sha256((r/target).read_bytes()).hexdigest()==base[target.replace('/','\\')]
 rows.append((r/target,proposal.read_bytes()))
manifest=r/'artifacts/advanced-split/models/selected-native-goal-driver-r1/native-execution-preparation-r1/vector-draft-emitter.manifest.json'
assert hashlib.sha256(manifest.read_bytes()).hexdigest()=='c4606423218a1352df8085dd3763bf6290f1558af263c237cfc97739187ca54d'
for item in json.loads(manifest.read_bytes())['files']:
 target=r/item['target'];source=r/item['proposed']
 assert hashlib.sha256(target.read_bytes()).hexdigest()==item['base_sha256']
 assert hashlib.sha256(source.read_bytes()).hexdigest()==item['sha256']
 rows.append((target,source.read_bytes()))
backup=r/'artifacts/advanced-split/receipts/source-mount-r94';backup.mkdir(parents=True,exist_ok=False)
result=[]
for target,content in rows:
 saved=backup/target.relative_to(r);saved.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(target,saved)
 target.write_bytes(content);result.append({'path':str(target.relative_to(r)),'before':hashlib.sha256(saved.read_bytes()).hexdigest(),'after':hashlib.sha256(content).hexdigest()})
contract=r/'crates/airk-kernel/src/inference_owner/evaluation_tests/CONTRACT.md';shutil.copyfile(contract,backup/'kernel-evaluation-CONTRACT.md')
with contract.open('a',encoding='utf-8') as stream:stream.write('\n\n'+(p/'CONTRACT-addition.proposed.md').read_text(encoding='utf-8'))
contract=r/'crates/airk-service/src/service_runtime/CONTRACT.md';shutil.copyfile(contract,backup/'service-runtime-CONTRACT.md')
with contract.open('a',encoding='utf-8') as stream:stream.write('\n\nThe ignored selected vector configuration emitter serializes actual Core Vectors, InstallModel and VectorSelection drafts, plus the inspected CPU-selected MiniLM object plan/pipeline identity, before the private fixture starts. It reads no task data, runs no neural session, grants no readiness or qualification and emits only under a fresh native private artifact root. Actual service status and configured native inference must independently match that identity.\n')
(backup/'mount.json').write_text(json.dumps({'root_review':'Root self-review of private-base source/launcher/contract and pure vector-draft emitter; no runtime wire or persisted shape change','files':result},indent=2)+'\n')
print(json.dumps(result))
