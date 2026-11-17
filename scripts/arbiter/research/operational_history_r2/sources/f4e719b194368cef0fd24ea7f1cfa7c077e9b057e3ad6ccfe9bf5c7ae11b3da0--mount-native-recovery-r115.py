# SPDX-License-Identifier: Apache-2.0
import hashlib,json
from pathlib import Path
root=Path.cwd()
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
output=root/'artifacts/advanced-split/receipts/native-recovery-integration-r115'
assert not output.exists()
warm=root/'artifacts/advanced-split/warm-journal-preparation-r1/manifest.proposed.json'
cfg=root/'artifacts/advanced-split/models/kernel-lightweight-focus-cfg-r1/mount-manifest.json'
assert sha(warm)=='19b1e73e07b37805c897601ee5403f5038ca908b8f725d422a741c8ace3b5e3e'
assert sha(cfg)=='9bd8fb7f7c6582cff9ed50668ac0fe23254e472d5e723c0b6c04fb142ca71a19'
rows=[]
for row in json.loads(warm.read_bytes())['targets']:
    rows.append({'target':row['target'],'source':row['proposal'],'before':row['base_sha256'],'after':row['proposal_sha256']})
for row in json.loads(cfg.read_bytes())['files']:
    rows.append({'target':row['target'],'source':row['source'],'before':row['target_before_sha256'],'after':row['source_sha256']})
assert len({r['target'] for r in rows})==10
for row in rows:
    target=root/row['target'];source=Path(row['source'])
    assert target.resolve().is_relative_to(root.resolve()) and not target.is_symlink()
    assert sha(source)==row['after']
    assert (sha(target) if target.exists() else None)==row['before']
output.mkdir()
decision={'schema':'airk.private-root-native-recovery-integration/1','decision':'Root reviewed ten exact source/contract changes and actual old fixture natural closure. Mount contract-first: three caller-matched cfg attributes and opt-in native warm-journal test seam. FocusContract independently reviewed corrected callback nonblocking/actual activation facts/stale intent/lost capture/writer closure/deadline intersections;12 Python controls passed, six Rust controls remain pending. No public wire, product budgets/model math/state format changes; no qualification granted. Runtime bridge repair remains a separate reviewed patch before Cargo.','old_fixture_closure_sha256':'f80138efda7cd1772a7411674499c245c8b3e118a8146ac75480243b95a1b5ff','files':rows,'validation':'NOT_YET_RUN'}
(output/'review.json').write_text(json.dumps(decision,indent=2)+'\n',encoding='utf-8')
for row in rows:
    target=root/row['target']
    if target.exists():
        backup=output/'before'/row['target'];backup.parent.mkdir(parents=True,exist_ok=True);backup.write_bytes(target.read_bytes())
for row in sorted(rows,key=lambda r:not r['target'].endswith('CONTRACT.md')):
    target=root/row['target'];target.parent.mkdir(parents=True,exist_ok=True);target.write_bytes(Path(row['source']).read_bytes())
    assert sha(target)==row['after']
print(json.dumps({'mounted':len(rows),'contracts_first':True,'review':str(output/'review.json'),'validation':'pending'}))
