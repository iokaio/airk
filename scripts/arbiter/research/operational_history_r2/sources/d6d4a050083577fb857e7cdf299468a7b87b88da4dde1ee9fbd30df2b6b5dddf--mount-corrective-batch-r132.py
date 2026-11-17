# SPDX-License-Identifier: Apache-2.0
import hashlib,json
from pathlib import Path
root=Path.cwd()
def sha(path):return hashlib.sha256(path.read_bytes()).hexdigest()
out=root/'artifacts/advanced-split/receipts/corrective-batch-r132';assert not out.exists()
rows=[]
head=root/'artifacts/advanced-split/models/context-verification-headroom-r1/mount-manifest.json'
assert sha(head)=='8e508aa098dcc54f07e685b4f0e9fc78fb0566143409832d863d996ffe253f7a'
for target,r in json.loads(head.read_bytes())['files'].items():
    rows.append({'target':target,'source':str(root/r['proposed']),'before':r['before_sha256'],'after':r['after_sha256']})
restart=root/'artifacts/advanced-split/stage-service-preparation-r1/same-owner-restart-r1/manifest-r4.proposed.json'
assert sha(restart)=='925fc2bb481bcf9edb5d0c352f5e07c2d8908ce94b8be0f046c4c1b47690aaff'
for r in json.loads(restart.read_bytes())['rows']:
    rows.append({'target':r['target'],'source':r['proposal'],'before':r['base_sha256'],'after':r['proposed_sha256']})
flush=root/'artifacts/advanced-split/warm-journal-flush-preparation-r1/manifest.proposed.json'
assert sha(flush)=='0edc8258e965bc74b93ff128ec34afbf47ee700c098bf048158388ae531d01ee'
for r in json.loads(flush.read_bytes())['targets']:
    rows.append({'target':str(Path(r['target']).relative_to(root)).replace('\\','/'),'source':r['proposal'],'before':r['base_sha256'],'after':r['proposal_sha256']})
mac=root/'artifacts/advanced-split/mac-platform-typefix-preparation-r1/preparation.json'
for r in json.loads(mac.read_bytes())['files']:
    rows.append({'target':r['target'],'source':str(root/r['proposed']),'before':r['base_sha256'],'after':r['proposed_sha256']})
for r in rows:
    p=root/r['target'];assert p.resolve().is_relative_to(root.resolve()) and not p.is_symlink()
    assert (sha(p) if p.exists() else None)==r['before'],r['target']
    assert sha(Path(r['source']))==r['after'],r['source']
grouped={}
for r in rows:grouped.setdefault(r['target'],[]).append(r)
assert len(rows)==16 and len(grouped)==15
composed={}
for target,items in grouped.items():
    if len(items)==1:composed[target]=Path(items[0]['source']).read_bytes();continue
    assert target=='crates/airk-service/src/service_runtime/arbiter/CONTRACT.md' and len(items)==2
    base=(root/target).read_text(encoding='utf-8');data=base
    for item in items:
        proposal=Path(item['source']).read_text(encoding='utf-8');assert proposal.startswith(base)
        data+=proposal[len(base):]
    composed[target]=data.encode('utf-8')
# Equivalent let-chain keeps the reviewed startup fence before every supervisor.
target='crates/airk-service/src/service_runtime.rs'
text=composed[target].decode('utf-8').replace('\r\n','\n')
old='    if let OwnerFactory::Evaluation(_, guard) = &mut owner_factory {\n        if let Some(guard) = guard.take() {\n            guard(&_ownership)?;\n        }\n    }'
new='    if let OwnerFactory::Evaluation(_, guard) = &mut owner_factory\n        && let Some(guard) = guard.take()\n    {\n        guard(&_ownership)?;\n    }'
assert text.count(old)==1
composed[target]=text.replace(old,new).encode('utf-8')
out.mkdir()
(out/'review.json').write_text(json.dumps({'schema':'airk.private-root-corrective-batch-review/1','decision':'Root reviewed coherent derived verification-reserve correction with independent FocusContract review, test-only same-owner restart with actual early retained-lease queue/catalog/configuration fences, writer-only Windows journal sync_data, and two diagnosed Mac type fixes. Contracts land before behavior. Compose both exact appends to the same Service contract rather than overwriting. Equivalent startup let-chain is a mechanical lint correction. No request/frame/quality limit increase, new wire/state/digest format, data refill, namespace adoption, model choice or fake pass. Prior failures and exact costs remain retained. Original fixture closed naturally before mutation; all checks/build/real corrected-source CAL execution remain pending.','old_fixture_owned_closure_sha256':'f55e3620321a38e0581f13f0f235e6bce4029a5ea879b0c55a0dea68ccadf7a3','rows':rows,'composed_after_sha256':{k:hashlib.sha256(v).hexdigest() for k,v in composed.items()},'qualification_granted':False},indent=2)+'\n',encoding='utf-8')
for target in composed:
    p=root/target
    if p.exists():
        backup=out/'before'/target;backup.parent.mkdir(parents=True,exist_ok=True);backup.write_bytes(p.read_bytes())
for target in sorted(composed,key=lambda name:not name.endswith('CONTRACT.md')):
    p=root/target;p.parent.mkdir(parents=True,exist_ok=True);p.write_bytes(composed[target])
print(json.dumps({'mounted':len(composed),'contracts_first':True,'review':str(out/'review.json'),'checks':'pending'}))
