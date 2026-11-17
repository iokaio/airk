# SPDX-License-Identifier: Apache-2.0
import ast,hashlib,json
from pathlib import Path
base=Path(__file__).resolve().parent
prior=json.loads((base/'p11a-final-qualification-r2.json').read_text())
assert prior['status']=='failed' and prior['failure']=='workspace-tests' and prior['source_gates_complete']
resolution_path=base/'p11a-workspace-doc-resolution-r1.json'
resolution=json.loads(resolution_path.read_text())
assert resolution['status']=='sole_documentation_failure_resolved_by_focused_normal_check' and resolution['source']==prior['source']
plan=json.loads((base/'p11a-final-qualification-command-map-r2.json').read_text())
names={'fmt','lightweight-clippy','workspace-clippy','lightweight-check','workspace-build'}
plan['adopted_prerequisites']=[row for row in prior['steps'] if row['name'] in names]
assert len(plan['adopted_prerequisites'])==5
remaining=[]
for row in plan['commands']:
 if row['name'] in names|{'workspace-tests'}:continue
 row['log']=row['log'].replace('-r2.log','-r3.log')
 row['command']=[a.replace('-r2','-r3') if isinstance(a,str) and 'p11a-final-' in a else a for a in row['command']]
 if 'expected_report' in row:
  row['expected_report']['path']=row['expected_report']['path'].replace('-r2','-r3')
 remaining.append(row)
assert len(remaining)==12
plan['commands']=remaining
plan['workspace_resolution_proof']=str(resolution_path)
plan['workspace_resolution_proof_sha256']=hashlib.sha256(resolution_path.read_bytes()).hexdigest()
plan['qualification_change']='P11a R3 continuation: adopt five passed R2 source/build gates and both R4 actual transport checks; preserve complete R2workspace2571pass/1staleCLIreferencefailure/53skip, with sole documentation failure repaired by canonical generation and exact normal verification. No production changes or broad rerun; execute only remaining12 stages. Original runtime identity remains33e6, test source979.'
mapping=base/'p11a-final-qualification-command-map-r3.json';assert not mapping.exists()
mapping.write_text(json.dumps(plan,indent=2)+'\n')
s=(base/'p11a-final-queue-r2.py').read_text()
s=s.replace('p11a-final-qualification-command-map-r2.json',mapping.name).replace('p11a-final-qualification-r2.json','p11a-final-qualification-r3.json').replace('p11a-final-qualification-owner-r2.lock','p11a-final-qualification-owner-r3.lock').replace('p11a-runtime-adoption-r2-seal.json','p11a-runtime-adoption-r3-seal.json')
anchor="    assert source_builder.source_identity() == plan['source_expected'], 'Source changed before P11a gates'"
assert s.count(anchor)==1
addition="""    record['adopted_prerequisites']=[]
    for adopted in plan['adopted_prerequisites']:
        rp=Path(adopted['receipt']); r=json.loads(rp.read_text())
        lp=rp.with_name(rp.name.replace('.receipt.json','.log'))
        assert r['actual_exit_code']==r['exit_code']==0 and r['source_before']==r['source_after']==plan['source_expected'] and r['source_unchanged']
        assert hashlib.sha256(lp.read_bytes()).hexdigest()==r['log_sha256']
        record['adopted_prerequisites'].append({'name':adopted['name'],'receipt':str(rp),'log_sha256':r['log_sha256'],'executed_in_this_queue':False})
    resolution_path=Path(plan['workspace_resolution_proof'])
    assert hashlib.sha256(resolution_path.read_bytes()).hexdigest()==plan['workspace_resolution_proof_sha256']
    resolution=json.loads(resolution_path.read_text())
    assert resolution['source']==plan['source_expected'] and resolution['status']=='sole_documentation_failure_resolved_by_focused_normal_check'
    repair=json.loads(Path(resolution['repair']['receipt']).read_text())
    assert repair['exit_code']==repair['actual_exit_code']==0 and repair['passing_tests']==1
    assert repair['source_before']==repair['source_after']==plan['source_expected']
    record['resolved_workspace']=resolution
    record['source_gates_complete']=True
"""
s=s.replace(anchor,addition+anchor)
s=s.replace("junit=base/'p10-target/nextest/ci/junit.xml'","junit=base/'p10-worktree/target/nextest/ci/junit.xml'")
ast.parse(s)
output=base/'p11a-final-queue-r3.py';assert not output.exists()
output.write_text(s,encoding='utf-8')
print('Prepared remaining12 stages; preserved original full workspace failure and independently verified focused repair')
