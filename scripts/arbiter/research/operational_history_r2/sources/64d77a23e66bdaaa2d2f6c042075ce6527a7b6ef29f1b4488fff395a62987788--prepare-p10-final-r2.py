# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import copy,hashlib,json
b=Path(__file__).resolve().parent
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
old=json.loads((b/'p10-final-qualification-command-map-r1.json').read_text())
stopped=json.loads((b/'p10-final-qualification-r1.json').read_text())
assert stopped['status']=='stopped_for_independent_review_finding' and stopped['source_gates_complete']
names=['fmt','lightweight-clippy','workspace-clippy','lightweight-check']
reused=[]
for name in names:
    receipt=b/f'p10-final-{name}-r1.receipt.json'
    log=b/f'p10-final-{name}-r1.log'
    r=json.loads(receipt.read_text())
    assert r['actual_exit_code']==r['exit_code']==0 and r['source_unchanged']
    assert r['source_before']==r['source_after']==stopped['source']
    assert sha(log)==r['log_sha256']
    assert Path(r['workdir'])==Path(old['cwd']) and Path(r['target'])==Path(old['persistent_target'])
    row=next(x for x in old['commands'] if x['name']==name)
    command=row['command'][3:]
    if 'cargo' in command and '--locked' in command and '--offline' not in command:
        command.insert(command.index('--locked')+1,'--offline')
    assert command==r['command']
    reused.append({'name':name,'receipt':str(receipt),'receipt_sha256':sha(receipt),
      'log':str(log),'log_sha256':sha(log),'command':r['command']})
m=copy.deepcopy(old)
m['status']='prepared_resume_not_run'
m['reused_source_gates']=reused
m['frozen_source_expected']=stopped['source']
m['stopped_predecessor']={'path':str(b/'p10-final-qualification-r1.json'),
 'sha256':sha(b/'p10-final-qualification-r1.json'),'status':stopped['status']}
m['commands']=[x for x in m['commands'] if x['name'] not in names]
for row in m['commands']:
    row['log']=row['log'].replace('-r1.log','-r2.log')
    row['command']=[x.replace('p10-final-','p10-final-').replace('-r1','-r2')
        if ('p10-final-' in x or 'p10-resource-final-' in x) and 'p10-runtime-final-' not in x else x
        for x in row['command']]
    row['environment']={k:v.replace('p10-resource-final-r1','p10-resource-final-r2') for k,v in row.get('environment',{}).items()}
m['resource_summary_commands']=[[x.replace('p10-resource-final-r1','p10-resource-final-r2') for x in command] for command in m['resource_summary_commands']]
m['source_freeze']='Reuse exact four successful native prerequisites from stopped r1 after validating immutable receipt/log hashes, actual/effective exits, exact command/workdir/target and unchanged source. Native source must still match frozen_source_expected. UI-only correction does not justify repeating native gates; UI checks are independently owned. Release runtime final-r1 remains unbuilt and is built exactly once.'
assert len(m['commands'])==17 and m['commands'][0]['name']=='release-build'
map_path=b/'p10-final-qualification-command-map-r2.json'
assert not map_path.exists()
map_path.write_text(json.dumps(m,indent=2)+'\n')
runner=(b/'p10-final-queue-r1.py').read_text()
runner=runner.replace("mapping = base / 'p10-final-qualification-command-map-r1.json'","mapping = base / 'p10-final-qualification-command-map-r2.json'")
runner=runner.replace("output = base / 'p10-final-qualification-r1.json'","output = base / 'p10-final-qualification-r2.json'")
needle="save()\ntry:\n    for row in plan['commands']:"
assert runner.count(needle)==1
replacement='''save()
try:
    prior = plan['stopped_predecessor']
    prior_path = Path(prior['path'])
    assert hashlib.sha256(prior_path.read_bytes()).hexdigest() == prior['sha256'], 'Stopped r1 record changed'
    assert json.loads(prior_path.read_text())['status'] == prior['status']
    expected = plan['frozen_source_expected']
    assert [gate['name'] for gate in plan['reused_source_gates']] == ['fmt','lightweight-clippy','workspace-clippy','lightweight-check']
    for gate in plan['reused_source_gates']:
        receipt_path, log_path = Path(gate['receipt']), Path(gate['log'])
        assert hashlib.sha256(receipt_path.read_bytes()).hexdigest() == gate['receipt_sha256'], 'Reused receipt drift'
        assert hashlib.sha256(log_path.read_bytes()).hexdigest() == gate['log_sha256'], 'Reused log drift'
        receipt = json.loads(receipt_path.read_text())
        assert receipt['actual_exit_code'] == receipt['exit_code'] == 0 and receipt['validation_error'] is None
        assert receipt['source_unchanged'] and receipt['source_before'] == receipt['source_after'] == expected
        assert receipt['command'] == gate['command'] and receipt['log_sha256'] == gate['log_sha256']
        assert Path(receipt['workdir']) == Path(plan['cwd']) and Path(receipt['target']) == Path(plan['persistent_target'])
        record['steps'].append({'name':gate['name'],'reused':True,'receipt':gate['receipt'],
            'receipt_sha256':gate['receipt_sha256'],'log_sha256':gate['log_sha256'],'exit_code':0})
    assert source_builder.source_identity() == expected, 'Source changed since four completed gates'
    record['source'] = expected
    record['source_gates_complete'] = True
    save()
    for row in plan['commands']:'''
runner=runner.replace(needle,replacement)
out=b/'p10-final-queue-r2.py'
assert not out.exists()
compile(runner,str(out),'exec')
out.write_text(runner)
print(json.dumps({'commands':len(m['commands']),'reused_gates':len(reused),'runner_sha256':sha(out),'map_sha256':sha(map_path)}))
