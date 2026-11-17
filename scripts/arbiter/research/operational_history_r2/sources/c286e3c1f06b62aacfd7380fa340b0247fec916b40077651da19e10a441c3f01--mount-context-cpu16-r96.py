# SPDX-License-Identifier: Apache-2.0
import hashlib,json,shutil
from pathlib import Path
r=Path.cwd();p=r/'artifacts/advanced-split/models/qwen3-challengers-r1/context-native-owned-r2'
sha=lambda x:hashlib.sha256(x.read_bytes()).hexdigest()
review=p/'amendment-review.json'
assert sha(review)=='ce0f5da5c12241293415327d19ee36c3ab37067a2b60e7a8f1eaf80a258a7dbe'
source=r/'crates/airk-inference/src/inference_engine/pool/arbiter/qualification/context.rs'
proposed=p/'context.rs.proposed.txt'
assert sha(source)=='d5fc918eeb21540f07fee6af35092392772cda8104a46463b5015a0714f17e64'
assert sha(proposed)=='f3ad3c1480b6a7a25166aeb0d6772c76d8a20c51412f5825ffc241c8a53fd551'
backup=r/'artifacts/advanced-split/receipts/context-cpu16-mount-r96';backup.mkdir(parents=True,exist_ok=False)
contract=r/'crates/airk-inference/src/inference_engine/pool/arbiter/CONTRACT.md'
shutil.copyfile(contract,backup/'CONTRACT.before.md');shutil.copyfile(source,backup/'context.before.rs')
decision={'schema':'airk.private-root-context-hardware-decision/1','decision':'One CPU16 Large requirement candidate authorized; CPU8 failed and remains failed. No threshold/memory/precision/model changes. Component validation only; genuine composed quality/fit/platform gates remain.','review_sha256':sha(review),'cpu16_inventory_sha256':sha(p/'profile-request-4-cpu16-r2.json'),'source_before':sha(source),'source_after':sha(proposed),'execution_authorized':True}
(backup/'decision.json').write_text(json.dumps(decision,indent=2)+'\n',encoding='utf-8')
with contract.open('ab') as stream:stream.write(b'\n\n'+(p/'CONTRACT.delta.proposed.md').read_bytes())
source.write_bytes(proposed.read_bytes())
print(json.dumps(decision))
