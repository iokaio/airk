# SPDX-License-Identifier: Apache-2.0
import hashlib,json,subprocess,sys
from pathlib import Path
r=Path.cwd();b=r/'artifacts/advanced-split/stage-service-preparation-r1/cal-execution-r102'
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def pin(p):return {'path':str(p),'bytes':p.stat().st_size,'sha256':sha(p)}
proposal=b/'CAL-capture-release.next-corrected-r142.proposed.json'
assert sha(proposal)=='fec556a6bbc0b78300f11edcf504b50257791b68fed80be31260c686116bf68e'
release=json.loads(proposal.read_bytes());previous=json.loads((b/'CAL-capture-release.root-approved-corrected-r137.json').read_bytes())
for key in ('Context_task_forward','maximum_questions','maximum_probes','Focus_profile','Context_bootstrap_profile','MiniLM_pipeline','task_policy','source_manifest_sha256','corpus_sha256','frozen_configuration','source_pins','absent_build_inputs'):
    assert release[key]==previous[key],key
assert release['maximum_questions']==100 and release['maximum_probes']==500 and release['Context_task_forward'] is False
assert release['actual_binding_evidence']['native_source_group']=='40c2871bc63b3f74ae67b1974aa6501bda1f04430fd8922f5b601a13aa5e6de5'
release.update(approved=True,status='ROOT_APPROVED_NEW_PURPOSE_CORRECTED_SOURCE_FIXED_CAL100',required_unresolved=[])
release['root_review']={
 'decision':'Root reviewed exact R141 actual Cargo/image/source closure, R140 full workspace Clippy, both actual IPC purpose regressions and R141 four MCP tests after correcting a test-only invalid scoped Status control. Root independently reviewed the five-file purpose correction; FocusContract independent review passed. Actual same-owner R142 process, genuine two warm completions/Status/Diagnostics and original MiniLM pipeline/index/source bindings are verified. Authorize NEW fixed CAL100 on this purpose-corrected source. Same previously partially exposed cohort; retain FAILED R129 and R137 outcomes and their actual Focus work separately, never relabel as fresh or successful. No refill, confirmation access, model/threshold search or changed scientific inputs.',
 'refusals':previous['root_review']['refusals'],
 'scope':previous['root_review']['scope'],
 'proposal_sha256':sha(proposal),'qualification_granted':False,'source_commit':'ccbe5096'}
approved=b/'CAL-capture-release.root-approved-purpose-corrected-r143.json'
with approved.open('x',encoding='utf-8',newline='\n') as f:json.dump(release,f,indent=2);f.write('\n')
packet_proposal=b/'CAL-owned-controller-packet.next-corrected-r142.proposed.json'
assert sha(packet_proposal)=='92d78d85143db8acafb176e7da402c60534bab3fc582d56e7b3695fcf67a8c96'
packet=json.loads(packet_proposal.read_bytes());packet['release']=pin(approved)
assert packet['owner_seconds']==1800 and packet['Context_task_forward'] is False
packet_path=b/'CAL-owned-controller-packet.root-approved-purpose-corrected-r143.json'
with packet_path.open('x',encoding='utf-8',newline='\n') as f:json.dump(packet,f,indent=2);f.write('\n')
controller=b/'run_capture.py';assert sha(controller)=='5dc1822c577583bb0f2ac0cd72b9e1ff5027e5dac53f3e2ef8d317815f9a34b8'
print(json.dumps({'release':pin(approved),'packet':pin(packet_path)}),flush=True)
result=subprocess.run([sys.executable,str(controller),'--packet',str(packet_path),'--packet-sha256',sha(packet_path)])
raise SystemExit(result.returncode)
