# SPDX-License-Identifier: Apache-2.0
import copy,hashlib,json
from pathlib import Path
base=Path('D:/code/Github/airk/artifacts/v190');old='p11-native-p11b-20261106-r1';new='p11-native-p11b-20261106-r2-supplement'
def read(s):return json.loads((base/(old+s)).read_text())
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
cleanup=read('-cleanup.json');steps=read('-steps.json');snapshot=read('-snapshot.json');plan=read('-plan.json')
assert cleanup['cargo_clean_exit_code']==0 and cleanup['target_removed'] and cleanup['test_exit_code']==1
assert len(steps)==7 and all(r['exit_code']==0 for r in steps[:6]) and steps[6]['passing_tests']==0
assert sum(r.get('passing_tests',0) or 0 for r in steps[:6])==8
assert sha(base/(old+'.tar.gz'))==snapshot['archive_sha256']
supplement=copy.deepcopy(plan);supplement['steps']=copy.deepcopy(plan['steps'][6:])
for row in supplement['steps'][:2]:
    command=row['command'];index=command.index('--bin');assert command[index+1]=='airk'
    command[index:index+2]=['--lib']
assert len(supplement['steps'])==3 and sum(r['expected_passing_tests'] for r in supplement['steps'])==3
supplement['scope']='Only three unfinished native CLI/library/helper checks from r1; original eight passes remain separate. No source or assertion changes; correct --bin to owning --lib.'
artifacts={}
for suffix in ['.tar.gz','-snapshot.json','-runner.py']:
    destination=base/(new+suffix);assert not destination.exists()
    with destination.open('xb') as f:f.write((base/(old+suffix)).read_bytes())
    artifacts[destination.name]=sha(destination)
with (base/(new+'-plan.json')).open('x',encoding='utf-8') as f:json.dump(supplement,f,indent=2);f.write('\n')
artifacts[new+'-plan.json']=sha(base/(new+'-plan.json'))
with (base/(new+'-prepared.json')).open('x',encoding='utf-8') as f:
    json.dump({'phase':'P11b','native_source':snapshot['native_source'],'archive_reused_byte_identically_from':old,
        'prior_clean_verified':True,'prior_passing_tests':8,'expected_supplement_tests':3,'artifacts':artifacts,
        'no_source_changes':True,'no_test_assertion_changes':True,'status':'Prepared; not launched or qualified'},f,indent=2);f.write('\n')
print(new)
