# SPDX-License-Identifier: Apache-2.0
"""Freeze only a fresh external-test-qualified editor continuation. Never run it."""
from pathlib import Path
import ast,hashlib,json,re
A=Path(__file__).parent;W=A/'p10-worktree'
h=lambda p:hashlib.sha256(Path(p).read_bytes()).hexdigest();read=lambda p:json.loads(Path(p).read_text(encoding='utf-8'))
s=(A/'p14-package-editor-continuation-r2.py').read_text(encoding='utf-8')
s=s.replace("RECORD=BASE/'p14-package-editor-continuation-r2.json'","RECORD=BASE/'p14-package-editor-continuation-r3.json'")
s=s.replace("PREPARED=BASE/'p14-package-editor-continuation-r2-prepared.json'","PREPARED=BASE/'p14-package-editor-continuation-r3-prepared.json'")
s=s.replace("label='p14-package-continuation-'+name+'-r2.log'","label='p14-package-continuation-'+name+'-r3.log'")
old="    ui=read(BASE/'p14-package-editor-owner-r3-inputs.json')\n    assert len(ui)==256 and prior_owner.extension_inputs()==ui==previous['extension_input_hashes']"
new="""    original_ui=read(BASE/'p14-package-editor-owner-r3-inputs.json')
    compiled=read(BASE/'p14-installed-editor-compile-r2.json')
    assert compiled['status']=='passed' and compiled['actual_exit_code']==0 and compiled['error'] is None
    assert compiled['native_source']==EXPECTED and compiled['production_source_unchanged'] is True and compiled['production_out_unchanged'] is True
    assert compiled['vsix_contains_test_outputs'] is False and compiled['vsix_sha256']==previous['vsix']['sha256']
    external=read(BASE/'p14-installed-editor-live-adoption-r2.json')
    targets={row['path'] for row in external['files'] if row['path'].startswith('packages/vscode/')}
    assert targets=={'packages/vscode/src/test/installedEditor.ts','packages/vscode/src/test/installedEditorContracts.ts','packages/vscode/src/test/installedEditorContracts.test.ts'}
    ui=compiled['ui_input_hashes'];assert len(ui)==258 and prior_owner.extension_inputs()==ui
    assert previous['extension_input_hashes']==original_ui and len(original_ui)==256
    assert {name for name in set(original_ui)|set(ui) if original_ui.get(name)!=ui.get(name)}==targets
    baseline=read(BASE/'p14-installed-editor-compile-r2-before.json')
    assert baseline['source']==EXPECTED and baseline['vsix_sha256']==previous['vsix']['sha256']
    def product_files(root):return {p.relative_to(root).as_posix():sha(p) for p in root.rglob('*') if p.is_file() and p.relative_to(root).parts[0]!='test'}
    assert product_files(WORK/'packages/vscode/src')==baseline['production_sources']
    assert product_files(WORK/'packages/vscode/out')==baseline['production_outputs']"""
assert old in s;s=s.replace(old,new)
s=s.replace("        assert sha(WORK/name)==row['proposed_sha256']\n        drivers[name]=row['proposed_sha256']", "        drivers[name]=row['proposed_sha256']")
anchor="    cleanup=read(BASE/'p14-editor-failed-fixture-cleanup-r2.json')"
insert="""    later={row['path']:row for row in external['files'] if row['path'] in drivers}
    assert set(later)=={'scripts/check-native-editor.cjs'}
    for name,row in later.items():
        assert drivers[name]==row['base_sha256'];drivers[name]=row['proposed_sha256']
    assert all(sha(WORK/name)==expected for name,expected in drivers.items())
    second_cleanup=read(BASE/'p14-editor-cleanup-native-installed-XGFMHQ-r1.json')
    assert second_cleanup['qualified'] is True and 'no restart or Stop claim' in second_cleanup['proof_kind']
    assert read(BASE/'p14-package-editor-continuation-r2.json')['status']=='failed'
    cleanup=read(BASE/'p14-editor-failed-fixture-cleanup-r2.json')"""
assert anchor in s;s=s.replace(anchor,insert)
anchor="    def save():RECORD.write_text"
insert="""    record['external_test_adoption']={'path':str(BASE/'p14-installed-editor-live-adoption-r2.json'),'sha256':sha(BASE/'p14-installed-editor-live-adoption-r2.json'),'driver_layer':later,'test_only_input_delta':sorted(targets),'compile_proof_sha256':sha(BASE/'p14-installed-editor-compile-r2.json')}
    record['second_failed_continuation']={'path':str(BASE/'p14-package-editor-continuation-r2.json'),'sha256':sha(BASE/'p14-package-editor-continuation-r2.json'),'status':'failed','separate_native_cleanup_sha256':sha(BASE/'p14-editor-cleanup-native-installed-XGFMHQ-r1.json')}
    def save():RECORD.write_text"""
assert anchor in s;s=s.replace(anchor,insert)
s=s.replace("        assert prior_owner.extension_inputs()==ui", "        assert prior_owner.extension_inputs()==ui\n        assert product_files(WORK/'packages/vscode/src')==baseline['production_sources']\n        assert product_files(WORK/'packages/vscode/out')==baseline['production_outputs']")
ast.parse(s);owner=A/'p14-package-editor-continuation-r3.py'
with owner.open('xb') as out:out.write(s.encode())
previous=read(A/'p14-package-editor-continuation-r2-prepared.json');pins=dict(previous['pins'])
external=read(A/'p14-installed-editor-live-adoption-r2.json');proposal=read(A/'p14-installed-editor-contract-proposed-r2/manifest.json')
assert external['patch_sha256']==proposal['patch_sha256']
changed_pins={}
for row in external['files']:
 actual=W/row['path'];assert h(actual)==row['proposed_sha256']
 if row['path']=='scripts/check-native-editor.cjs':
  key=str(actual.resolve());assert pins[key]==row['base_sha256'];changed_pins[key]={'before':pins[key],'after':row['proposed_sha256']};pins[key]=row['proposed_sha256']
 else:pins[str(actual.resolve())]=row['proposed_sha256']
assert len(changed_pins)==1
proof=read(A/'p14-installed-editor-compile-r2.json');assert proof['status']=='passed' and proof['production_source_unchanged'] and proof['production_out_unchanged'] and not proof['vsix_contains_test_outputs']
assert len(proof['ui_input_hashes'])==258
evidence=['p14-installed-editor-live-adoption-r2.json','p14-installed-editor-contract-proposed-r2/manifest.json','p14-installed-editor-contract-proposed-r2/installed-editor.patch',
 'p14-installed-editor-contract-proposed-r2/source-io-independent-delta-review-r2.json','p14-installed-editor-compile-r2.json','p14-installed-editor-compile-r2-before.json',
 'p14-installed-editor-compile-r2-emit.json','p14-installed-editor-compile-r2.log','p14-installed-editor-target-compile-r2.cjs','p14-installed-editor-compile-proof-r2.py',
 'p14-installed-editor-node-tests-r1.log','p14-installed-editor-node-tests-r1.receipt.json','p14-editor-cleanup-native-installed-XGFMHQ-r1.json',
 'p14-package-editor-continuation-r2.json','p14-package-editor-continuation-r2-prepared.json']
node=read(A/'p14-installed-editor-node-tests-r1.receipt.json');assert node['exit_code']==node['actual_exit_code']==0 and node['validation_error'] is None
assert h(A/'p14-installed-editor-node-tests-r1.log')==node['log_sha256']=='fc582edef3e4d8295646d517c701142bdcce07f45e30b3bc73442fef3d443149'
log=(A/'p14-installed-editor-node-tests-r1.log').read_text(encoding='utf-8')
for name,count in [('tests',9),('pass',9),('fail',0),('cancelled',0),('skipped',0)]:assert re.search(r'^\u2139 '+name+r' '+str(count)+r'$',log,re.M)
for name in evidence:pins[str((A/name).resolve())]=h(A/name)
assert len(proof['emission']['emitted'])==3 and proof['emission']['production_emit_allowed'] is False
for name in proof['emission']['emitted']:
 path=Path(name);assert path.parent.resolve()==(W/'packages/vscode/out/test').resolve();pins[str(path.resolve())]=h(path)
assert all(h(Path(name))==expected for name,expected in pins.items())
assert not(A/'p14-package-editor-continuation-r3.json').exists() and not(W/'artifacts/p14-legacy-upgrade-editor-owner-r2').exists()
for name in ['five-private-scenarios','legacy-fixture','legacy-private-scenario']:assert not(A/('p14-package-continuation-'+name+'-r3.log')).exists()
previous.update({'status':'STATIC_R3_READY_NOT_EXECUTED','owner_sha256':h(owner),'pins':pins,'command':['python','-X','utf8',str(owner.resolve())],
 'changed_existing_driver_pins_r3':changed_pins,'external_ui_input_count':258,'external_ui_delta':proof['external_test_delta'],
 'driver_lineage':'check-native-editor original8ac925 ->reviewed e582cf ->reviewed ae4265; editor-ui originalf42c95 ->reviewed40719a. All other original driver pins unchanged.',
 'compile_proof':{'path':str((A/'p14-installed-editor-compile-r2.json').resolve()),'sha256':h(A/'p14-installed-editor-compile-r2.json'),'production_source_unchanged':True,'production_out_unchanged':True,'native_unchanged':True,'vsix_test_outputs_excluded':True},
 'focused_validation':'Exact9 external consumer Node tests PASS; target-only TS semantic compilePASS,3 test-only emits. Prior failed compile/type-root receipt stays historical.',
 'execution':'No native/npm/package/editor execution by preparation. Adopt only original three passed package stages; preserve original/R1/R2 editor failures and separate native cleanup proofs.'})
out=A/'p14-package-editor-continuation-r3-prepared.json'
with out.open('x',encoding='utf-8') as f:json.dump(previous,f,indent=2);f.write('\n')
print(json.dumps({'owner':h(owner),'prepared':h(out),'pins':len(pins),'current_ui_inputs':258,'stages_executed':0}))
