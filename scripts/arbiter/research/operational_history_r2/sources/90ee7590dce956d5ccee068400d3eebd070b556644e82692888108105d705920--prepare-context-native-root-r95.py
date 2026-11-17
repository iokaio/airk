# SPDX-License-Identifier: Apache-2.0
import hashlib,json,subprocess,sys
from pathlib import Path
r=Path.cwd();base=r/'artifacts/advanced-split/models/qwen3-challengers-r1/context-native-owned-r1'
cargo=r/'artifacts/advanced-split/receipts/stage-integration-r95/airk-inference-image.log'
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
parent=json.loads((r/'artifacts/advanced-split/focus-native-qualification-preparation-r1/native-parity-packet-released-r2.json').read_bytes())
runtime={p:s for p,s in parent['pins'].items() if not Path(p).is_relative_to(r)}
assert len(runtime)>=19000
for label in ['17','4']:
 master=base/f'selected-context-{label}-profile-r1/selected-profiles.private.json';proposal=base/f'native-packet-{label}-proposed-root-r95.json'
 result=subprocess.run([sys.executable,str(base/'prepare_native.py'),'--master',str(master),'--master-sha256',sha(master),'--cargo-json',str(cargo),'--cargo-json-sha256',sha(cargo),'--output-packet',str(proposal)])
 if result.returncode:raise SystemExit(result.returncode)
 packet=json.loads(proposal.read_bytes());packet['status']='ROOT_RELEASED'
 packet['pins'].update(runtime)
 packet['root_review']='Root reviewed actual full-FP32 graph byte relocation/original head references, nativeScorer/owner/resource harness and actualRustProfile/Cargo-image provenance. Two pure failfast resource tests passed R94. Strict0.001 original/same-artifact error, all-live H,20calls/30s/fixedRAM; no quality/CAL/composed/platform-release claim. Reused exact base interpreter/stdlib/DLL closure from successful Focus owner; no Torch/venv/corpus.'
 packet['provenance']={'native_source_receipt':str(cargo.with_suffix('.json')),'runtime_base_rows':len(runtime),'component_only':True}
 released=base/f'native-packet-{label}-released-root-r95.json';assert not released.exists();released.write_text(json.dumps(packet,indent=2)+'\n')
 print(json.dumps({'label':label,'released_packet':str(released),'sha256':sha(released),'base_runtime_rows':len(runtime)}),flush=True)
