# SPDX-License-Identifier: Apache-2.0
"""PRIVATE actual source-identity/path controls; no guard edits, Cargo, AIRK or NN."""
import importlib.util
import json
import os
from pathlib import Path
import sys

STATE=Path(__file__).parents[2]
spec=importlib.util.spec_from_file_location('actual_path_producer',STATE/'producer.py')
producer=importlib.util.module_from_spec(spec);spec.loader.exec_module(producer)
CANDIDATE=Path(r'D:\code\Github\airk\artifacts\v190\final-plan\candidate-worktree-r1')
ROOT=Path(__file__).parent
rows=[]

def builder(root,name):
    selected=producer.python_source_workspace(root)
    return producer.load_script(selected/'scripts/build-service-runtime.py',name)

plan=json.loads((STATE/'inputs/r3-prepared.json').read_bytes())
expected=json.loads(Path(plan['pins']['runtime_build']['path']).read_bytes())['source']
ordinary=builder(CANDIDATE,'ordinary_bound_source')
verbatim=builder(plan['source'],'native_bound_source')
assert ordinary.ROOT.samefile(verbatim.ROOT)
assert not str(verbatim.ROOT).startswith('\\\\?\\')
first,second=ordinary.source_identity(),verbatim.source_identity()
assert first==second==expected=={'sha256':'1c6bc37600bba4123046d76d8836a8ffda8f3ebad229bbb5da7c236bffbe7e92','files':1913}
assert producer.native_path(producer.python_source_workspace(plan['source']))==plan['source']
rows.append({'name':'both_spellings_bind_same_actual_candidate_and_exact_independent_shipping_c7','passed':True,'actual_identity':second,'native_canonical_paths_preserved':True})

fixture=ROOT/'literal-fixture';fixture.mkdir()
workspace=fixture/'workspace';(workspace/'crates/probe/src').mkdir(parents=True)
lib=workspace/'crates/probe/src/lib.rs';shared=workspace/'crates/probe/shared.txt';shared.write_bytes(b'independent literal source\n')
files={b'crates/probe/src/lib.rs'}
lib.write_text('const A: &str = include_str!("../shared.txt");\nconst B: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/shared.txt"));\n',encoding='utf-8')
# The actual unchanged source owner must inventory the same explicit expected input.
for supplied in [workspace,Path(producer.native_path(workspace))]:
    verbatim.ROOT=producer.python_source_workspace(supplied)
    assert verbatim.compiled_literal_inputs(files)=={b'crates/probe/shared.txt'}
rows.append({'name':'relative_dotdot_and_manifest_macros_keep_exact_literal_input_both_spellings','passed':True,'expected_dependency':'crates/probe/shared.txt'})

lib.write_text('const A: &str = include_str!("../missing.txt");\n',encoding='utf-8')
missing=False
try:verbatim.compiled_literal_inputs(files)
except RuntimeError as error:missing=str(error)=='Compiled source input is missing or escapes the runtime workspace'
assert missing
(fixture/'outside.txt').write_bytes(b'not in source workspace\n')
lib.write_text('const A: &str = include_str!("../../../../outside.txt");\n',encoding='utf-8')
escaped=False
try:verbatim.compiled_literal_inputs(files)
except RuntimeError as error:escaped=str(error)=='Compiled source input is missing or escapes the runtime workspace'
assert escaped
rows.append({'name':'unchanged_owner_refuses_missing_and_existing_escaped_inputs','passed':True,'missing_refused':missing,'existing_escape_refused':escaped})

refusals=[]
for path in [lib,workspace/'absent',str(workspace)+'\\..',str(workspace)+'\\.\\']:
    try:producer.python_source_workspace(path)
    except (ValueError,OSError):refusals.append(str(path))
assert len(refusals)==4
rows.append({'name':'source_workspace_refuses_file_missing_and_explicit_dot_ancestry','passed':True,'actual_refusals':refusals})
result={'schema':'airk.private.native-python-source-path-controls/1','selected':4,'passed':4,'failed':0,'actual_service_or_nn':False,'actual_guard_unchanged':True,'cases':rows}
with (ROOT/'result.json').open('x',encoding='utf-8') as stream:json.dump(result,stream,indent=2);stream.write('\n')
print(json.dumps({'selected':4,'passed':4,'failed':0,'receipt':str(ROOT/'result.json'),'exact_c7':second,'airk_or_nn':False}))
