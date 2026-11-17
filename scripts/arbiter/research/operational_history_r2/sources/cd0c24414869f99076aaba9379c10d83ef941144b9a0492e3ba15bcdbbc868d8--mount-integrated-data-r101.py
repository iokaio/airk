# SPDX-License-Identifier: Apache-2.0
import hashlib,json,shutil,subprocess,sys
from pathlib import Path
r=Path.cwd().resolve();p=r/'artifacts/advanced-split/models/native-minilm-original-baseline-r1'
sha=lambda raw:hashlib.sha256(raw).hexdigest()
receipt=json.loads((r/'artifacts/advanced-split/receipts/stage-integration-r100/original-catalog-emitter.json').read_bytes())
assert receipt['exit_code']==0 and receipt['inputs_unchanged'] and receipt['selected_tests']==1
draft=p/'native-draft-r1';rows=[(r/'crates/airk-retrieval/src/model_catalog.json',(draft/'model_catalog.json').read_bytes()),(r/'fixtures/models/model-policy.json',(draft/'model-policy.json').read_bytes())]
membership=r/'artifacts/advanced-split/stage-service-preparation-r1/final-distribution-membership-r2/manifest.json'
assert sha(membership.read_bytes())=='cf1228e80e2e508f11928e490ec9d33c85927177eb0bb05b0685f5535d8033dd'
for x in json.loads(membership.read_bytes())['files']:
 target=r/x['target'];data=(r/x['proposed']).read_bytes()
 assert sha(target.read_bytes())==x['base_sha256'] and sha(data)==x['proposed_sha256'] and len(data)==x['bytes'];rows.append((target,data))
legal=p/'legal-consumers-manifest.json';assert sha(legal.read_bytes())=='74a021b02872d697b6bfe25ed2766ee4261106c143c6c533f5945701abce50cb'
l=json.loads(legal.read_bytes())
for target,before in l['before'].items():
 source=p/'legal-consumers'/target;data=source.read_bytes();assert sha(data)==l['copies'][str(Path('legal-consumers')/target)] and sha((r/target).read_bytes())==before;rows.append((r/target,data))
out=r/'artifacts/advanced-split/receipts/integrated-data-mount-r101';out.mkdir(parents=True,exist_ok=False);evidence=[]
for target,data in rows:
 assert target.resolve().is_relative_to(r) and not target.is_symlink()
 saved=out/target.relative_to(r);saved.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(target,saved)
 evidence.append({'target':str(target.relative_to(r)),'before':sha(saved.read_bytes()),'after':sha(data)})
for target,data in sorted(rows,key=lambda row:row[0].suffix!='.md'):target.write_bytes(data)
(out/'mount.json').write_text(json.dumps({'files':evidence,'root_review':'Root reviewed native-emitted exact original catalog/policy, original licensed payload inventory and independently reviewed singleton membership source-revocation frames/contract. No model/platform/quality qualification inferred.'},indent=2)+'\n',encoding='utf-8')
destination=r/'artifacts/advanced-split/bundled-model-v3';assert not destination.exists()
for precision in ['u8','s8']:
 command=[sys.executable,'scripts/provision-local-model.py','--destination',str(destination),'--profile',f'minilm-l6-v2-{precision}-v3','--source-directory',str(r/'artifacts/advanced-split/models/minilm-shipped-baseline-ort130-r1'/precision)]
 with (out/f'provision-{precision}.log').open('xb') as log:result=subprocess.run(command,stdout=log,stderr=subprocess.STDOUT)
 assert result.returncode==0,f'provision-{precision} failed; retain log'
print(json.dumps({'mounted':len(rows),'bundle':str(destination),'receipt':str(out/'mount.json')}))
