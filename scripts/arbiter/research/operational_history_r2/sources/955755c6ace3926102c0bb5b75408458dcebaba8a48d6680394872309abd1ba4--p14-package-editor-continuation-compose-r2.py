# SPDX-License-Identifier: Apache-2.0
"""Prepare only a fresh editor continuation with the two reviewed driver changes."""
from pathlib import Path
import ast,hashlib,json,re
A=Path(__file__).parent;W=A/'p10-worktree'
h=lambda p:hashlib.sha256(Path(p).read_bytes()).hexdigest()
read=lambda p:json.loads(Path(p).read_text(encoding='utf-8'))
old=A/'p14-package-editor-continuation-r1.py';s=old.read_text(encoding='utf-8')
s=s.replace('p14-package-editor-continuation-r1.json','p14-package-editor-continuation-r2.json').replace('p14-package-editor-continuation-r1-prepared.json','p14-package-editor-continuation-r2-prepared.json')
assert "label='p14-package-continuation-'+name+'-r1.log'" in s
s=s.replace("label='p14-package-continuation-'+name+'-r1.log'","label='p14-package-continuation-'+name+'-r2.log'")
old_line="    drivers=previous['driver_hashes']"
new_line='''    drivers=dict(previous['driver_hashes'])
    adoption=read(BASE/'p14-editor-tools-live-adoption-r1.json')
    assert adoption['native_source_unchanged']==EXPECTED and adoption['existing_vsix_unchanged']==vsix['sha256']
    changed={row['path']:row for row in adoption['files'] if row['path'] in drivers}
    assert set(changed)=={'scripts/editor-ui.cjs','scripts/check-native-editor.cjs'}
    for name,row in changed.items():
        assert drivers[name]==row['base_sha256']
        assert sha(WORK/name)==row['proposed_sha256']
        drivers[name]=row['proposed_sha256']
    cleanup=read(BASE/'p14-editor-failed-fixture-cleanup-r2.json')
    assert cleanup['qualified'] is True and cleanup['lifecycle_stop_claimed'] is False and cleanup['ui_stop_receipt_created'] is False
    prior_attempt=read(BASE/'p14-package-editor-continuation-r1.json');assert prior_attempt['status']=='failed' '''
assert old_line in s;s=s.replace(old_line,new_line)
anchor="    def save():RECORD.write_text"
insert="""    record['prior_failed_continuation']={'path':str(BASE/'p14-package-editor-continuation-r1.json'),'sha256':sha(BASE/'p14-package-editor-continuation-r1.json'),'status':'failed'}
    record['prior_native_cleanup']={'path':str(BASE/'p14-editor-failed-fixture-cleanup-r2.json'),'sha256':sha(BASE/'p14-editor-failed-fixture-cleanup-r2.json'),'scope':'native quiescence only; no lifecycle/UI Stop receipt claimed'}
    record['reviewed_driver_adoption']={'path':str(BASE/'p14-editor-tools-live-adoption-r1.json'),'sha256':sha(BASE/'p14-editor-tools-live-adoption-r1.json'),'changed_drivers':changed}
    def save():RECORD.write_text"""
assert anchor in s;s=s.replace(anchor,insert)
ast.parse(s);owner=A/'p14-package-editor-continuation-r2.py'
with owner.open('xb') as output:output.write(s.encode())
prepared=read(A/'p14-package-editor-continuation-r1-prepared.json')
adoption=read(A/'p14-editor-tools-live-adoption-r1.json');manifest=read(A/'p14-editor-ui-contract-proposed-r1/manifest.json')
assert adoption['patch_sha256']==manifest['patch_sha256']
pins=dict(prepared['pins']);changed_pins={}
for row in adoption['files']:
 path=(W/row['path']).resolve();assert h(path)==row['proposed_sha256']
 if str(path) in pins:
  assert pins[str(path)]==row['base_sha256'];changed_pins[str(path)]={'before':pins[str(path)],'after':row['proposed_sha256']}
  pins[str(path)]=row['proposed_sha256']
 else:
  assert row['path']=='scripts/tests/editor-ui.test.cjs'
  pins[str(path)]=row['proposed_sha256']
assert len(changed_pins)==2
evidence=['p14-editor-tools-live-adoption-r1.json','p14-editor-ui-contract-proposed-r1/manifest.json',
 'p14-editor-ui-contract-proposed-r1/editor-ui.patch','p14-editor-ui-contract-proposed-r1/source-io-independent-review-r1.json',
 'p14-editor-ui-syntax-r1.receipt.json','p14-editor-ui-syntax-r1.log','p14-editor-runner-syntax-r1.receipt.json','p14-editor-runner-syntax-r1.log',
 'p14-editor-ui-node-tests-r2.receipt.json','p14-editor-ui-node-tests-r2.log','p14-editor-failed-fixture-cleanup-r2.json','p14-package-editor-continuation-r1.json',
 'p14-package-editor-continuation-r1-prepared.json']
for name in ['p14-editor-ui-syntax-r1','p14-editor-runner-syntax-r1','p14-editor-ui-node-tests-r2']:
 receipt=read(A/(name+'.receipt.json'))
 assert receipt['exit_code']==receipt['actual_exit_code']==0 and receipt['validation_error'] is None
 assert receipt['source_before']==receipt['source_after']==adoption['native_source_unchanged'] and receipt['source_unchanged'] is True
 assert h(A/(name+'.log'))==receipt['log_sha256']
test_log=(A/'p14-editor-ui-node-tests-r2.log').read_text(encoding='utf-8')
for name,count in [('tests',6),('pass',6),('fail',0),('cancelled',0),('skipped',0)]:assert re.search(r'^\u2139 '+name+r' '+str(count)+r'$',test_log,re.M)
assert read(A/'p14-editor-failed-fixture-cleanup-r2.json')['qualified'] is True
for name in evidence:pins[str((A/name).resolve())]=h(A/name)
assert all(h(Path(p))==expected for p,expected in pins.items())
assert not(A/'p14-package-editor-continuation-r2.json').exists() and not(W/'artifacts/p14-legacy-upgrade-editor-owner-r2').exists()
for name in ['five-private-scenarios','legacy-fixture','legacy-private-scenario']:assert not(A/('p14-package-continuation-'+name+'-r2.log')).exists()
prepared.update({'status':'STATIC_R2_READY_NOT_EXECUTED','owner_sha256':h(owner),'pins':pins,'command':['python','-X','utf8',str(owner.resolve())],
 'changed_existing_driver_pins':changed_pins,'other_existing_pins_unchanged':True,
 'original_failed_continuation_preserved':{'path':str((A/'p14-package-editor-continuation-r1.json').resolve()),'status':'failed'},
 'focused_validation':'Two syntax checks passed; exact six Node tests passed in clean R2 wrapper receipt. Prior Unicode-print wrapper failure remains preserved.',
 'cleanup_basis':'Separate actual owned native quiescence/repair/inspect qualified; no lifecycleStopped/UI Stop claim',
 'execution':'No actual fresh editor/native/package/compile steps executed by preparation. Reuse exact49170VSIX and original three passing package stages.'})
prepared.pop('original11pins_unchanged',None)
out=A/'p14-package-editor-continuation-r2-prepared.json'
with out.open('x',encoding='utf-8') as f:json.dump(prepared,f,indent=2);f.write('\n')
print(json.dumps({'owner':h(owner),'prepared':h(out),'pins':len(pins),'changed_existing_pins':len(changed_pins),'editor_stages_executed':0}))
