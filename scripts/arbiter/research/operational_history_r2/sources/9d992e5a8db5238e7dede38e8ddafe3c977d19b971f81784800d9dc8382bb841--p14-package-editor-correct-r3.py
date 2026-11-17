# SPDX-License-Identifier: Apache-2.0
"""Correct only nonexistent root-lock input; no npm/package/native/editor work."""
from pathlib import Path
import ast,hashlib,importlib.util,json,sys
sys.dont_write_bytecode=True
A=Path(__file__).parent
h=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
old=A/'p14-package-editor-owner-r2.py'; prepared=A/'p14-package-editor-owner-r2-prepared.json'
s=old.read_text(encoding='utf-8')
needle="                 'packages/vscode/package-lock.json', 'packages/vscode/README.md',\n                 'package.json', 'package-lock.json'])"
assert s.count(needle)==1
s=s.replace(needle,"                 'packages/vscode/package-lock.json', 'packages/vscode/README.md']) # No repository-root npm package/lock exists.")
assert s.count("p14-package-editor-owner-r2-prepared.json")==1
s=s.replace('p14-package-editor-owner-r2-prepared.json','p14-package-editor-owner-r3-prepared.json')
ast.parse(s)
owner=A/'p14-package-editor-owner-r3.py'
assert not(A/'p14-package-editor-owner-r3-prepared.json').exists()
owner.write_bytes(s.encode())
m=json.loads(prepared.read_text(encoding='utf-8'))
assert len(m['pins'])==11 and all(h(Path(name))==expected for name,expected in m['pins'].items())
spec=importlib.util.spec_from_file_location('p14_editor_preflight_r3',owner)
module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
assert not(module.WORK/'package-lock.json').exists()
assert not(module.WORK/'package.json').exists()
assert not module.RECORD.exists() and not module.OUTPUT.exists() and not module.LEGACY_OWNER.exists()
inputs=module.extension_inputs() # Exact actual function; no main()/compile/package/editor work.
assert 'packages/vscode/package-lock.json' in inputs and 'package-lock.json' not in inputs
assert 'packages/vscode/package.json' in inputs and 'package.json' not in inputs
assert all((module.WORK/name).is_file() and h(module.WORK/name)==expected for name,expected in inputs.items())
catalog=A/'p14-package-editor-owner-r3-inputs.json'
with catalog.open('x',encoding='utf-8') as out:json.dump(inputs,out,indent=2);out.write('\n')
m['owner_sha256']=h(owner)
m['command'][-1]=str(owner.resolve())
m['execution']='R3 static correction ready, not launched. R2 failed preflight on nonexistent repository-root package-lock.json before record/output/stages. R2 history preserved; R3 actual extension_inputs() evaluated successfully without main/npm/native/package/editor execution.'
m['stages'][0]='WORK native packaging from actual current5d89 runtime, no build'
m['r3_correction']={'original_owner_sha256':h(old),'original_prepared_sha256':h(prepared),
    'pins_unchanged':len(m['pins']),'extension_inputs_count':len(inputs),'extension_inputs_catalog':str(catalog.resolve()),
    'extension_inputs_catalog_sha256':h(catalog),'output_paths_reused_fresh':True,
    'fresh_paths_confirmed_absent':[str(module.RECORD),str(module.OUTPUT),str(module.LEGACY_OWNER)],
    'change':'Remove exactly nonexistent repository-root package.json/package-lock.json assumptions; preserve required packages/vscode/package.json/package-lock.json and every actual input'}
output=A/'p14-package-editor-owner-r3-prepared.json'
with output.open('x',encoding='utf-8') as out:json.dump(m,out,indent=2);out.write('\n')
assert m['pins']==json.loads(prepared.read_text())['pins']
print(json.dumps({'owner_sha256':h(owner),'prepared_sha256':h(output),'inputs':len(inputs),'catalog_sha256':h(catalog),'pins_unchanged':11,'stages_executed':0}))
