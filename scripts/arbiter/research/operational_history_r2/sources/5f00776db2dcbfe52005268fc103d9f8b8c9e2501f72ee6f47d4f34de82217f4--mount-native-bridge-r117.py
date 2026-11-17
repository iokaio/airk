# SPDX-License-Identifier: Apache-2.0
import hashlib,json
from pathlib import Path
root=Path.cwd()
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
out=root/'artifacts/advanced-split/receipts/native-bridge-integration-r117'
assert not out.exists()
bridge=root/'artifacts/advanced-split/models/supervisor-sync-control-bridge-r1/mount-manifest.json'
cfg=root/'artifacts/advanced-split/warm-journal-cfg-preparation-r1/manifest.proposed.json'
assert sha(bridge)=='87468daea53ed7b230ec786c93c89c23fab6d7b6046c7671d5e2e43ca93ba152'
assert sha(cfg)=='02cd9ebe4d72e66f7625b1024da45f0cf8d0b52d0855ac3fbc50f4c9f23d1749'
rows=[]
for r in json.loads(bridge.read_bytes())['files']:
    rows.append({'target':r['target'],'source':r['source'],'before':r['target_before_sha256'],'after':r['source_sha256']})
for r in json.loads(cfg.read_bytes())['targets']:
    rows.append({'target':r['target'],'source':r['proposal'],'before':r['base_sha256'],'after':r['proposal_sha256']})
assert len({r['target'] for r in rows})==8
for r in rows:
    p=root/r['target'];assert p.resolve().is_relative_to(root.resolve()) and not p.is_symlink()
    assert (sha(p) if p.exists() else None)==r['before'] and sha(Path(r['source']))==r['after']
out.mkdir()
(out/'review.json').write_text(json.dumps({'schema':'airk.private-root-native-bridge-integration/1','decision':'Root independently reviewed the shared runtime bridge code, all five preserved callback request/deadline/error paths, six actual async-IO/once/deadline controls and owning contract. Same two-worker production runtime via block_in_place; local/joined driver fallback only for outside/current-thread callers, timers constructed inside driver. No detached work, permanent per-controller thread, new queue, request budget or retries. Corrected native-only journal cfg(test) gating prevents exposing unused fixture helpers to non-test all-feature builds; no warning suppression or new public exports. Contract-first mount; actual Clippy/tests remain pending.','files':rows,'qualification_granted':False},indent=2)+'\n',encoding='utf-8')
for r in rows:
    p=root/r['target']
    if p.exists():
        b=out/'before'/r['target'];b.parent.mkdir(parents=True,exist_ok=True);b.write_bytes(p.read_bytes())
for r in sorted(rows,key=lambda r:not r['target'].endswith('CONTRACT.md')):
    p=root/r['target'];p.parent.mkdir(parents=True,exist_ok=True);p.write_bytes(Path(r['source']).read_bytes());assert sha(p)==r['after']
print(json.dumps({'mounted':len(rows),'contracts_first':True,'review':str(out/'review.json'),'validation':'pending'}))
