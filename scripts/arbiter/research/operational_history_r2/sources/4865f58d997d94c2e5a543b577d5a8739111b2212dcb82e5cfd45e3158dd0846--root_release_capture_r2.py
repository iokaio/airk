# SPDX-License-Identifier: Apache-2.0
import hashlib,json
from pathlib import Path
base=Path(__file__).resolve().parent
def sha(path):return hashlib.sha256(Path(path).read_bytes()).hexdigest()
old=json.loads((base/'CAL-capture-release.root-approved-r1.json').read_bytes())
proposal=base/'CAL-capture-release.raw-status2-r2.proposed.json'
assert sha(proposal)=='4a10b85e2a681c36c84fd346bfa5910fabcc8f99de255b5e7b62a92e6d732b09'
new=json.loads(proposal.read_bytes())
allowed={'approved','status','private_amendment','supersedes_failed_release_sha256','private_status2_source_updates','input_pins','source_pins'}
assert all(old.get(k)==new.get(k) for k in set(old)|set(new) if k not in allowed)
amendment=base/'private-raw-status2-amendment-r1/changes.proposed.json'
assert sha(amendment)=='c193345413e689fb49150876ad1006059b5bfb23b5a1ac0ac2762766ebb36c9e'
changes=json.loads(amendment.read_bytes())['changes']
for row in changes:assert sha(row['path'])==row['new_sha256']
for kind in ('input_pins','source_pins'):
    assert set(old[kind])==set(new[kind])
    actual={p for p in old[kind] if old[kind][p]!=new[kind][p]}
    expected={r['path'] for r in changes if r['path'] in old[kind]}
    assert actual==expected and len(actual)==4
    for row in changes:
        if row['path'] in actual:
            assert old[kind][row['path']]==row['old_sha256']
            assert new[kind][row['path']]==row['new_sha256']
new['approved']=True
new['status']='ROOT_APPROVED_RAW_STATUS2_CORRECTED_CAL100_CAPTURE'
new['root_review']['private_status2_amendment']='Root reviewed all five actual diffs: raw Status2 preserved by adapter/capture/confirmation/controller, existing health wrapper unchanged, three genuine native test profiles replace obsolete opaque four-profile fixture. Changed only exact reviewed private source pins; source3a64, actual service/configuration3, cohort/data/model/resource/product/wholephase bounds unchanged. Prior failed capture had zero question calls and no neural outcomes; preserve its failure. Reviewed affected29 passing controls and three absence passes/one OS-symlink skip; extra historical QASPER final-expectation failure remains separately disclosed.'
release=base/'CAL-capture-release.root-approved-r2.json'
assert not release.exists()
release.write_text(json.dumps(new,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
packet=json.loads((base/'CAL-owned-controller-packet.proposed.json').read_bytes())
def pin(path):return {'path':str(path),'bytes':path.stat().st_size,'sha256':sha(path)}
packet['release']=pin(release)
packet['capture_driver']=pin(Path(packet['capture_driver']['path']))
packet['capture_output']=str(Path(packet['capture_output']).with_name('CAL-capture-r2'))
packet['owner_output']=str(base/'CAL-capture-owned-r2')
assert packet['owner_seconds']==1800 and packet['maximum_questions']==100 and packet['maximum_probes']==500
assert not Path(packet['capture_output']).exists() and not Path(packet['owner_output']).exists()
controller=base/'run_capture.py'
assert sha(controller)=='5dc1822c577583bb0f2ac0cd72b9e1ff5027e5dac53f3e2ef8d317815f9a34b8'
output=base/'CAL-owned-controller-packet.root-approved-r2.json'
assert not output.exists()
output.write_text(json.dumps(packet,indent=2)+'\n',encoding='utf-8')
decision={'schema':'airk.private-root-cal-controller-decision/1','approved_packet_sha256':sha(output),'approved_controller_sha256':sha(controller),'approved_release_sha256':sha(release),'decision':'Root authored and reviewed this exact additive raw-Status2 correction. Execute once, same immutable runtime/owner and1800seconds/100questions/500probes; fresh protected output only. Native source and all scientific inputs unchanged. No Cargo or unrelated NN; stop on failure. No release qualification granted.'}
(base/'root-capture-controller-decision-r2.json').write_text(json.dumps(decision,indent=2)+'\n',encoding='utf-8')
print(json.dumps({'release':str(release),'release_sha256':sha(release),'packet':str(output),'packet_sha256':sha(output),'controller_sha256':sha(controller)}))
