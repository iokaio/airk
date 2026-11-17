# SPDX-License-Identifier: Apache-2.0
"""Narrow static evidence-collection ordering fix. No archive/native/SSH execution."""
from pathlib import Path
import ast,json,hashlib
A=Path(__file__).parent
(A/'post-p14-neo-owner-r3.py').write_bytes((A/'post-p14-neo-owner-r2.py').read_bytes())
s=(A/'post-p14-neo-prepare-r2.py').read_text(encoding='utf-8').replace('post-p14-neo-owner-r2.py','post-p14-neo-owner-r3.py').replace('post-p14-neo-launch-r2.py','post-p14-neo-launch-r3.py')
ast.parse(s);(A/'post-p14-neo-prepare-r3.py').write_bytes(s.encode())
s=(A/'post-p14-neo-launch-r2.py').read_text(encoding='utf-8').replace('post-p14-neo-owner-r2.py','post-p14-neo-owner-r3.py')
old="    subprocess.run([*ssh,'test -f '+remote+'/cleanup.json && test ! -e '+remote+'/target'],check=True)"
new="    subprocess.run([*ssh,'test -f '+remote+'/cleanup.json'],check=True) # Completion receipt only; failed targets must remain collectable."
assert old in s;s=s.replace(old,new)
old="    # All available logs/reports/timings have been retained before refusing a run."
new="""    # All available logs/reports/timings have been retained before refusing a run.
    # A failed cleanup/containment must never block collection of its own evidence.
    actual_target_absence=subprocess.run([*ssh,'test ! -e '+remote+'/target']).returncode
    fresh=base/(label+'-collection-target-absence.json')
    observation={'remote':remote,'target_absence_exit_code':actual_target_absence,'evidence_downloaded_before_probe':True}
    content=(json.dumps(observation,sort_keys=True)+'\\n').encode()
    if fresh.exists():assert fresh.read_bytes()==content,'Changed immutable collection target observation'
    else:
        with fresh.open('xb') as output:output.write(content)
    assert actual_target_absence==0, 'Evidence preserved; exact owned target remains/absence unknown'"""
assert old in s;s=s.replace(old,new)
ast.parse(s);(A/'post-p14-neo-launch-r3.py').write_bytes(s.encode())
h=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
m=json.loads((A/'post-p14-neo-static-review-r2.json').read_text())
m['status']='STATIC_R3_READY_FUTURE_SOURCE_PENDING_NO_EXECUTION'
m['scripts']={name:h(A/name) for name in ['post-p14-neo-prepare-r3.py','post-p14-neo-owner-r3.py','post-p14-neo-launch-r3.py']}
m['r3_correction']='Collect after cleanup completion even if target remains; download evidence first, then actual remote target-absence probe plus all existing owner/cleanup/timing qualification checks'
m['owner_r2_byte_identical']=h(A/'post-p14-neo-owner-r3.py')==h(A/'post-p14-neo-owner-r2.py')
(A/'post-p14-neo-static-review-r3.json').write_bytes((json.dumps(m,indent=2)+'\n').encode())
p=json.loads((A/'post-p14-neo-parameters-r2.json').read_text())
p['prepare_command'][1]='artifacts/v190/post-p14-neo-prepare-r3.py'
p['launch_later_by_root_only']=p['launch_later_by_root_only'].replace('launch-r2','launch-r3')
p['collect_later_by_root_only']=p['collect_later_by_root_only'].replace('launch-r2','launch-r3')
(A/'post-p14-neo-parameters-r3.json').write_bytes((json.dumps(p,indent=2)+'\n').encode())
print(json.dumps(m['scripts']));print('review',h(A/'post-p14-neo-static-review-r3.json'))
