# SPDX-License-Identifier: Apache-2.0
import hashlib,json
from pathlib import Path
b=Path(__file__).resolve().parent
def sha(path):return hashlib.sha256(Path(path).read_bytes()).hexdigest()
proposal=b/'CAL-capture-release.postfix-r122.proposed.json'
assert sha(proposal)=='aa6340094ab8418633422a69e86a4465f23ac5814740c47b8944af9f66ef9067'
value=json.loads(proposal.read_bytes());old=json.loads((b/'CAL-capture-release.root-approved-r2.json').read_bytes())
for key in ('schema','maximum_questions','maximum_probes','Context_task_forward','Focus_profile','Context_bootstrap_profile','MiniLM_pipeline','task_policy','source_manifest_sha256','corpus_sha256','Context_candidate_requirements'):
    assert value[key]==old[key],key
assert value['approved'] is None
assert value['actual_binding_evidence']['native_source_group']=='eac7b9aac9a5a9943422a08406dd9602da55a35d2d7ed7cbceb24a2f4bdc29c5'
assert value['frozen_configuration']['revision']==3
ready=json.loads(Path(value['paths']['fixture_ready']).read_bytes())
assert ready['health']['process_id']==2132 and ready['health']['instance']=='51dfa80d8c69a09e1f92b0d2a1732068'
assert ready['health']['executable_sha256']=='e51772aca3d9bb3349395fb69c920807cb18a0a6715f6c12f91a53c7f5f61119'
status=json.loads(Path(value['actual_binding_evidence']['stage_status']['path']).read_bytes())
assert status['configuration']==value['frozen_configuration']
for stage in ('focus','context'):
    assert status[stage]['availability']=='ready'
    assert status[stage]['selected_profile']==status[stage]['active_profile']==status['configuration'][stage]['selected_profile']
value['approved']=True;value['status']='ROOT_APPROVED_POSTFIX_FIXED_CAL100_CAPTURE'
value['required_unresolved']=[]
value['root_review']={'decision':'Root independently reviewed new actual R122 image/source/Cargo and native Ready/configuration3/bindings, same whole CAL753source/767vectors and originalU8pipeline, all unchanged fixed100/500/scientific/cohort/model/resource values. Actual native background jobs establish desired setup state; failed explicit Configure and all failed warm notification controllers remain failed. This release requires genuine current Ready only and grants no coldwarm/controller timing pass. Defer journal sync_data fix to required post-fit build; original final fitted warm qualification remains mandatory.','scope':'Execute exactly one fixed100 FocusON CAL capture, at most500 verified source-reference probes, zero Context task forwards, original30s/2048token/eight-card limits and source authority unchanged. Root-reviewed1800s owned controller verifies source/absence/configuration and natural session/process closure. No Cargo or unrelated NN on this host.','qualification_granted':False,'proposal_sha256':sha(proposal)}
release=b/'CAL-capture-release.root-approved-postfix-r129.json';assert not release.exists()
release.write_text(json.dumps(value,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
packet=json.loads((b/'CAL-owned-controller-packet.root-approved-r2.json').read_bytes())
def pin(path):return {'path':str(path),'bytes':path.stat().st_size,'sha256':sha(path)}
packet['release']=pin(release)
packet['capture_driver']=pin(Path(packet['capture_driver']['path']))
packet['capture_output']=str(Path(packet['capture_output']).with_name('CAL-capture-postfix-r129'))
packet['owner_output']=str(b/'CAL-capture-owned-postfix-r129')
assert packet['owner_seconds']==1800 and packet['maximum_questions']==100 and packet['maximum_probes']==500
assert not Path(packet['capture_output']).exists() and not Path(packet['owner_output']).exists()
controller=b/'run_capture.py';assert sha(controller)=='5dc1822c577583bb0f2ac0cd72b9e1ff5027e5dac53f3e2ef8d317815f9a34b8'
output=b/'CAL-owned-controller-packet.root-approved-postfix-r129.json';assert not output.exists()
output.write_text(json.dumps(packet,indent=2)+'\n',encoding='utf-8')
(b/'root-capture-controller-decision-postfix-r129.json').write_text(json.dumps({'schema':'airk.private-root-cal-controller-decision/1','approved_packet_sha256':sha(output),'approved_controller_sha256':sha(controller),'approved_release_sha256':sha(release),'decision':'Root authored/reviewed exact new actual service-state/source bindings; unchanged reviewed controller and original1800s/100/500. Execute once; preserve prior failures, stop on failure, no release qualification or timing-pass granted.'},indent=2)+'\n',encoding='utf-8')
print(json.dumps({'release':str(release),'release_sha256':sha(release),'packet':str(output),'packet_sha256':sha(output)}))
