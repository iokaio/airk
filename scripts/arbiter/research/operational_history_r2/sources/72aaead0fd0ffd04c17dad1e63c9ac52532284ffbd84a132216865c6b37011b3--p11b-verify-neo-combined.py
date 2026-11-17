# SPDX-License-Identifier: Apache-2.0
import hashlib,json
from pathlib import Path
base=Path('D:/code/Github/airk/artifacts/v190');first='p11-native-p11b-20261106-r1';second='p11-native-p11b-20261106-r2-supplement'
def read(label,suffix):return json.loads((base/(label+suffix)).read_text())
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
source={'sha256':'e38277697881592edaea8b50311cfea628f6ce70c643c1a0bc0bfcf95c5d2ef1','files':954}
observations=[]
for label in [first,second]:
    snapshot=read(label,'-snapshot.json');plan=read(label,'-plan.json');q=read(label,'-qualification.json')
    cleanup=read(label,'-cleanup.json');steps=read(label,'-steps.json')
    assert snapshot['native_source']==plan['native_source']==q['native_source']==source
    assert q['archive_sha256']==snapshot['archive_sha256']==sha(base/(label+'.tar.gz'))
    assert q['plan_sha256']==sha(base/(label+'-plan.json')) and q['steps']==steps
    assert q['source_before']==q['source_after']=={'files':1778,'unchanged':True}
    assert cleanup['cargo_clean_exit_code']==0 and cleanup['target_removed'] and cleanup['source_preserved']
    assert cleanup['timing_copy_error'] is None and cleanup['clean_log_error'] is None
    if label==first:
        assert q['status']=='failed' and cleanup['test_exit_code']==1 and len(steps)==7
        assert all(row['exit_code']==row['actual_exit_code']==0 for row in steps[:6])
        assert steps[6]['passing_tests']==0 and steps[6]['actual_exit_code']==0 and steps[6]['exit_code']!=0
        passing=steps[:6]
    else:
        assert q['status']=='passed' and cleanup['test_exit_code']==0 and len(steps)==3
        assert all(row['exit_code']==row['actual_exit_code']==0 and row['passing_tests']==1 for row in steps)
        passing=steps
    for row in passing:
        expected=next(v for v in plan['steps'] if v['name']==row['name'])
        if expected.get('expected_passing_tests'):assert row['passing_tests']==expected['expected_passing_tests']
        assert (base/(label+'-'+row['name']+'.log')).is_file()
    observations.append({'label':label,'original_status':q['status'],'passing_commands':len(passing),
        'passing_tests':sum(v.get('passing_tests') or 0 for v in passing),
        'qualification_sha256':sha(base/(label+'-qualification.json')),'cleanup':cleanup})
assert read(first,'-snapshot.json')==read(second,'-snapshot.json')
assert sum(v['passing_tests'] for v in observations)==11
proof=json.loads((base/'p11b-final-native-equivalence-r1.json').read_text())
assert proof['historical_r3_and_mac_source']==source and proof['unchanged_native_inputs']==952
record={'status':'passed','native_source':source,'passing_commands':9,'passing_tests':11,
    'batches':observations,'current_windows_source':proof['current_source'],
    'native_equivalence_proof_sha256':sha(base/'p11b-final-native-equivalence-r1.json'),
    'history':'First batch stopped at wrong --bin target with zero matches. That failed result remains; the three unfinished checks passed against the identical archive with the correct --lib targets. No assertion/source change or original pass relabeling.',
    'limits':'Focused ARM macOS native and real tiny-helper checks; not full workspace, release package/signature, Intel/Linux/default ONNX or installed editor qualification.'}
with (base/'p11b-neo-combined-verification-r1.json').open('x',encoding='utf-8') as f:json.dump(record,f,indent=2);f.write('\n')
print(json.dumps({'status':'passed','tests':11,'cleaned_batches':2,'source':source}))
