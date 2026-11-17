# SPDX-License-Identifier: Apache-2.0
import hashlib,json
from pathlib import Path
root=Path.cwd()
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
manifest=root/'artifacts/advanced-split/models/selected-native-goal-driver-r1/cal-refusal-retention-r1/install-manifest.proposed.json'
assert sha(manifest)=='cf5bbbc37f33789e9a0e45616f79c7daf160a18810d2958aff787f43b1aa0b95'
rows=json.loads(manifest.read_bytes())['rows']
for r in rows:
    p=Path(r['target']);assert p.resolve().is_relative_to(root/'artifacts/advanced-split/models/selected-native-goal-driver-r1/source')
    assert not p.is_symlink() and (sha(p) if p.exists() else 'missing')==r['base_sha256']
    assert sha(Path(r['proposed']))==r['proposed_sha256']
out=root/'artifacts/advanced-split/receipts/cal-refusal-mount-r133';out.mkdir(exist_ok=False)
(out/'review.json').write_text(json.dumps({'schema':'airk.private-root-cal-refusal-mount/1','decision':'Root reviewed authenticated real R129 budget-refusal retention and explicit actual R2 worker-error refusal. Only parsed identity-bound verification_reply_budget with retryable=false and verification_performed=false may become UNKNOWN/no-request; no orphan admission association/probes/rawscores. All other errors remain fatal. All probe attempts, including refusals, count toward original5/500. Ten cheap controls passed; old sources and R129 failed exposure remain retained. New corrected-source full100 release is separate and not granted by this source install.','manifest_sha256':sha(manifest),'files':rows,'qualification_granted':False},indent=2)+'\n',encoding='utf-8')
for r in rows:Path(r['target']).write_bytes(Path(r['proposed']).read_bytes())
print(json.dumps({'installed':len(rows),'review':str(out/'review.json')}))
