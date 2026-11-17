# SPDX-License-Identifier: Apache-2.0
import hashlib,json
from pathlib import Path
b=Path(__file__).resolve().parent
def sha(p):return hashlib.sha256(Path(p).read_bytes()).hexdigest()
proposal=b/'CAL-capture-release.restart-r136.proposed.json'
assert sha(proposal)=='f0bfce30c2ae68fa07d58a1be6bf0e02abb352570874093b639fdf88dcba09ca'
value=json.loads(proposal.read_bytes());old=json.loads((b/'CAL-capture-release.root-approved-postfix-r129.json').read_bytes())
for k in ('schema','maximum_questions','maximum_probes','Context_task_forward','Focus_profile','Context_bootstrap_profile','MiniLM_pipeline','task_policy','source_manifest_sha256','corpus_sha256','Context_candidate_requirements'):
    assert value[k]==old[k],k
assert value['approved'] is None
assert value['actual_binding_evidence']['native_source_group']=='3eb341b4c762059a7c1855970f8f540f30c6ca9f10f3029ac6808ecee5b27381'
ready=json.loads(Path(value['paths']['fixture_ready']).read_bytes())
assert ready['health']['process_id']==67408 and ready['health']['instance']=='b20d86836be950ed47ac8b816e369f5b'
assert ready['health']['executable_sha256']=='44f728d6b09113b6530bdebcde0a1db66a4533be3c44df22736da756760d7c7b'
assert sha(value['paths']['fixture_ready'])=='36f6b3feb2ed812ba96a54d8c15d8019906fb57116c9e7ab1df0e30b8cc2819c'
status=json.loads(Path(value['actual_binding_evidence']['stage_status']['path']).read_bytes())
assert status['configuration']==value['frozen_configuration'] and status['configuration']['revision']==3
for stage in ('focus','context'):
    assert status[stage]['availability']=='ready' and status[stage]['queued']==status[stage]['running']==0
    assert status[stage]['selected_profile']==status[stage]['active_profile']==status['configuration'][stage]['selected_profile']
value['approved']=True;value['status']='ROOT_APPROVED_NEW_CORRECTED_SOURCE_FIXED_CAL100'
value['required_unresolved']=[]
value['root_review']={'decision':'Root reviewed successful corrected-source R135 Clippy/nine regressions/Cargo/image provenance, actual same-owner restart lease/catalog/configuration and genuine restored Ready/journals/status/diagnostics/newbindings. Authorize NEW full fixedCAL100 on this corrected source, not continuation or reclassification of R129. The SAME cohort is already partly exposed: retain R129 FAILED first-query refusal/two Focus forwards1024tokens/zero Context forwards separately. No cohort refill, model/threshold choice or confirmation access. All scientific data/profile/CPU/memory/budget values remain unchanged.','refusals':'Reviewed R133 collector retains ONLY authenticated nonretryable verification_reply_budget with verification_performed=false as UNKNOWN/no-request, never associates/probes/scores undelivered admissions, counts all probe attempts toward5/500. Other identity/parse/timeout/ownership/unknown errors remain fatal.','scope':'Exactly100questions/atmost500probes/zeroContexttaskforwards; original30-second calls/2048tokens/eightcards/262144requested operation bytes; unchanged1800-second owned phase and actual natural process/session/config/source closure. No local Cargo or unrelated NN. No timing or release qualification granted.','proposal_sha256':sha(proposal),'qualification_granted':False}
release=b/'CAL-capture-release.root-approved-corrected-r137.json';assert not release.exists()
release.write_text(json.dumps(value,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
packet_path=b/'CAL-owned-controller-packet.restart-r136.proposed.json'
assert sha(packet_path)=='03b136fa7ddc2aa1302075d2dd93e0336fd3d4bbb26b1882e17c82724986bf04'
packet=json.loads(packet_path.read_bytes())
packet['release']={'path':str(release),'bytes':release.stat().st_size,'sha256':sha(release)}
assert packet['owner_seconds']==1800 and packet['maximum_questions']==100 and packet['maximum_probes']==500 and packet['Context_task_forward'] is False
assert not Path(packet['capture_output']).exists() and not Path(packet['owner_output']).exists()
controller=b/'run_capture.py';assert sha(controller)=='5dc1822c577583bb0f2ac0cd72b9e1ff5027e5dac53f3e2ef8d317815f9a34b8'
output=b/'CAL-owned-controller-packet.root-approved-corrected-r137.json';assert not output.exists()
output.write_text(json.dumps(packet,indent=2)+'\n',encoding='utf-8')
(b/'root-capture-controller-decision-corrected-r137.json').write_text(json.dumps({'schema':'airk.private-root-cal-controller-decision/1','approved_packet_sha256':sha(output),'approved_controller_sha256':sha(controller),'approved_release_sha256':sha(release),'decision':'Root reviewed exact new corrected-source fixed100 closure; same partially exposed CAL cohort, all old failures retained; original1800s/100/500 and no Context taskforward; execute once, stop on fatal error, no qualification granted.'},indent=2)+'\n',encoding='utf-8')
print(json.dumps({'release':str(release),'release_sha256':sha(release),'packet':str(output),'packet_sha256':sha(output)}))
