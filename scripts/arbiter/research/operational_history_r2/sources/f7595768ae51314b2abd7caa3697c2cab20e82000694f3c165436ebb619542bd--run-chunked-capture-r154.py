# SPDX-License-Identifier: Apache-2.0
import hashlib,json,subprocess,sys
from pathlib import Path
r=Path.cwd();base=r/'artifacts/advanced-split/stage-service-preparation-r1/cal-execution-r102';h=r/'artifacts/advanced-split/models/selected-native-goal-driver-r1/chunked-cal-r1'
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def pin(p):return {'path':str(p),'bytes':p.stat().st_size,'sha256':sha(p)}
proposal=base/'CAL-common-release-cooperative-r153.proposed.json';assert sha(proposal)=='ea49e1a22e1781a8b21614d623567c5b1bf78f8e1d5d2b377b9890785ece9038'
n=json.loads(proposal.read_bytes());previous=json.loads((base/'CAL-capture-release.root-approved-purpose-corrected-r143.json').read_bytes())
for k in ('Context_task_forward','maximum_questions','maximum_probes','Focus_profile','Context_bootstrap_profile','MiniLM_pipeline','task_policy','source_manifest_sha256','corpus_sha256','frozen_configuration','absent_build_inputs'):assert n[k]==previous[k],k
plan=str(r/'docs/split/codex-plan.md')
for p,digest in previous['source_pins'].items():
    if p==plan:assert n['source_pins'][p]==sha(Path(p))
    else:assert n['source_pins'][p]==digest,p
assert n['chunk_protocol_sha256']=='da5eb4de2642deb952cdbc608b302d99137965ed43969bfafd5dcc07c187ac99'
assert n['actual_binding_evidence']['native_source_group']=='13dc85349c783077aec1ad4eae194e17542a3205639c0fe7d927b165a6a0cf25'
n.update(approved=True,status='ROOT_APPROVED_FIXED_CAL100_THIRTEEN_BOUNDED_PHASES',required_unresolved=[])
n['root_review']={'decision':'Root approves this exact common release and source closure after genuine R153 same-owner Ready/two warm completions/config3/753sources/767vectors/original MiniLM pipeline/cal-live binding, current R148 full Clippy/six regressions/original actual combined transport pass and immutable Cargo images. Combined reviewed harness327119fb03cdc78cdfabc662f474eab79f8fcda9ebad8f1fc5809f15aee2af69 preserves original census/source/model/budget/authority. Same partially exposed CAL cohort; old R129/R137/R143 failed attempts and costs remain separate, no refill/confirmation access/model selection. New exact same-selection grants only between completed questions, real per-question nonces/fingerprints retained; no uncertain-read replay.','bounds':'Thirteen distinct naturally owned phases:12x8+4 questions; each<=1800s, each native call<=30s, total100/500probes/zeroContextTaskNN; SAME original fixture absolute10800 expiry, never extended. Aggregate validates every original phase and unchanged source/config/model/pipeline.','source_changes':'Only reviewed native fixes, actual new fixture/build bindings and37 frozen private harness/proof/consumer pins were added/refreshed. Plan text pin refreshed solely for user-required on-demand delivery; task policy, model choices, rubric, quality/resource thresholds and source corpus unchanged.','proposal_sha256':sha(proposal),'qualification_granted':False,'source_commit':'3d5f8847'}
release=base/'CAL-common-release.root-approved-r154.json'
with release.open('x',encoding='utf-8',newline='\n')as f:json.dump(n,f,indent=2);f.write('\n')
plan_dir=base/'CAL-chunks-plan-r154';execution=r/'artifacts/advanced-split/models/actual-composed-service-r2/CAL-capture-chunked-r154'
cmd=[sys.executable,'-B','-X','utf8',str(h/'collector/prepare_packets.py'),'--release',str(release),'--protocol',str(h/'proof/protocol.proposed.json'),'--fixture-bound',n['original_fixture_bound']['path'],'--runtime-python',n['capture_python'],'--node',str(r/'artifacts/tools/node-v24.15.0-win-x64/node.exe'),'--owner',str(r/'scripts/provision-static-onnx-runtime.py'),'--output',str(plan_dir),'--execution',str(execution)]
result=subprocess.run(cmd);assert result.returncode==0
packet=plan_dir/'controller-packet.json';decision=base/'CAL-controller-decision.root-approved-r154.json'
value={'schema':'airk.private-selected-cal-chunk-controller-decision/1','approved':True,'controller_packet_sha256':sha(packet),'root_review':'Exact approved common release, protocol466/da5, reviewed frozen collector/proof/start inputs and original absolute bound. Only the13 fixed phases; no retries, stop on first failed/stopped phase. No model/publication qualification granted.','qualification_granted':False}
with decision.open('x',encoding='utf-8',newline='\n')as f:json.dump(value,f,indent=2);f.write('\n')
print(json.dumps({'release':pin(release),'controller':pin(packet),'decision':pin(decision),'execution':str(execution)}),flush=True)
result=subprocess.run([sys.executable,'-B','-X','utf8',str(h/'collector/run_controller.py'),'--packet',str(packet),'--packet-sha256',sha(packet),'--decision',str(decision),'--decision-sha256',sha(decision)])
raise SystemExit(result.returncode)
