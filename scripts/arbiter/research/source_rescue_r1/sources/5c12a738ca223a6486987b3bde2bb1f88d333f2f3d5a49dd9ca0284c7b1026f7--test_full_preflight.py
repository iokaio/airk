# SPDX-License-Identifier: Apache-2.0
"""PRIVATE metadata-only full path preflight: zero subprocess and zero Fixture construction."""
import copy
import ctypes
from ctypes import wintypes as w
import hashlib
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
from unittest import mock

ROOT=Path(__file__).parent
STATE=ROOT.parents[1]
sys.path.insert(0,str(STATE/'execution-owner-r1'))  # actual runner invocation directory
def load(path,name):
    spec=importlib.util.spec_from_file_location(name,path);module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module);return module
producer=load(STATE/'producer.py','full_path_actual_producer')
runner=load(STATE/'execution-owner-r1/runner.py','full_path_actual_runner')
plan=copy.deepcopy(json.loads((STATE/'inputs/r4-prepared.json').read_bytes()))
plan['name']='dry_full_path_r5'
quality=producer.load_script(producer.python_source_workspace(plan['source'])/'scripts/check-retrieval-quality.py','full_path_actual_candidate')
rows=[]
with mock.patch.object(subprocess,'Popen',side_effect=AssertionError('Dry preflight started a child')) as children, mock.patch.object(quality.clients.Fixture,'__init__',side_effect=AssertionError('Dry preflight constructed Fixture')) as constructor:
    actual=producer.preflight_paths(plan,quality)
    assert actual['fixture'].is_relative_to(producer.unaliased(STATE))
    assert actual['runtime_image'].relative_to(actual['runtime']).as_posix()=='airk.exe'
    assert actual['cli_image'].relative_to(actual['runtime']).as_posix()=='airkctl.exe'
    assert all(not str(value).startswith('\\\\?\\') for key,value in actual.items() if isinstance(value,Path))
    for name in ['fixture','source','model_directory']:
        raw=plan['source'] if name=='source' else plan[name]
        assert producer.unaliased(raw).samefile(Path(raw))
        assert producer.native_path(producer.unaliased(raw))==raw
    for name in ['runtime_image','cli_image','diagnostic_image']:
        pin=plan['pins'][name]
        assert producer.native_path(producer.unaliased(pin['path']))==pin['path']
    for phase in ['transport','native-pins','native-compare']:
        log,receipt=runner.phase_output_paths(plan,phase)
        assert log.parent.samefile(actual['evidence']) and receipt.parent.samefile(actual['evidence'])
    assert children.call_count==0 and constructor.call_count==0
    rows.append({'name':'actual_full_source_runtime_state_model_output_fences','passed':True,'child_calls':children.call_count,'fixture_constructors':constructor.call_count,'actual_paths':{key:str(value) for key,value in actual.items() if isinstance(value,Path)},'native_FilePins_preserved':True})
    # Parent creates its own log before child preflight; this must not become a
    # child-output collision. Parent still refuses replay through its own fence.
    parent_log,parent_receipt=runner.phase_output_paths(plan,'transport')
    with parent_log.open('xb') as stream:stream.write(b'owned parent log marker\n')
    try:
        producer.preflight_paths(plan,quality)
        parent_refused=False
        try:runner.phase_output_paths(plan,'transport')
        except ValueError:parent_refused=True
        assert parent_refused
        rows.append({'name':'actual_parent_to_child_log_lifetime_and_parent_replay_refusal','passed':True,'child_preflight_accepts_live_parent_log':True,'parent_replay_refused':True})
    finally:
        assert parent_log.resolve().is_relative_to(STATE.resolve())
        parent_log.unlink()  # exact newly created owned dry marker only
    scratch=ROOT/'owned-nonempty-fixture';scratch.mkdir();(scratch/'marker.txt').write_text('do not overwrite',encoding='utf-8')
    bad=copy.deepcopy(plan);bad['fixture']=producer.native_path(scratch)
    nonempty=False
    try:producer.preflight_paths(bad,quality)
    except ValueError:nonempty=True
    assert nonempty and (scratch/'marker.txt').read_text()=='do not overwrite'
    bad=copy.deepcopy(plan);bad['name']='../escape'
    escape=False
    try:producer.preflight_paths(bad,quality)
    except ValueError:escape=True
    assert escape
    rows.append({'name':'nonempty_fixture_and_output_escape_remain_refused','passed':True,'original_nonempty_bytes_preserved':True,'output_escape_refused':True})
    # Exercise the unchanged generic image proof on this actual process object,
    # using the same local ordinary view the native runner now supplies.
    reference_path=producer.ROOT/'artifacts/advanced-split/models/weights-v2/software-v2-independent-numeric-r1/reference_owned.py'
    reference=producer.load_script(reference_path,'full_path_actual_reference_owner')
    api=ctypes.WinDLL('kernel32',use_last_error=True)
    api.GetCurrentProcess.restype=w.HANDLE
    api.QueryFullProcessImageNameW.argtypes=[w.HANDLE,w.DWORD,w.LPWSTR,ctypes.POINTER(w.DWORD)]
    api.QueryFullProcessImageNameW.restype=w.BOOL
    native_pin={'path':producer.native_path(sys.executable),'sha256':producer.sha(sys.executable)}
    local={**native_pin,'path':str(producer.unaliased(native_pin['path']))}
    proof=reference.image_proof(api,api.GetCurrentProcess(),local)
    assert proof['sha256']==native_pin['sha256'] and Path(proof['path']).samefile(Path(native_pin['path']))
    assert native_pin['path'].startswith('\\\\?\\')
    assert children.call_count==0 and constructor.call_count==0
    rows.append({'name':'unchanged_image_guard_uses_same_actual_file_local_view','passed':True,'actual_proof':proof,'original_native_FilePin_preserved':native_pin,'child_calls':0,'fixture_constructors':0})
result={'schema':'airk.private.full-path-flow-preflight/1','selected':4,'passed':4,'failed':0,'subprocesses_started':0,'fixture_constructors':0,'actual_service_or_nn':False,'cases':rows}
with (ROOT/'result.json').open('x',encoding='utf-8') as stream:json.dump(result,stream,indent=2);stream.write('\n')
print(json.dumps({'selected':4,'passed':4,'failed':0,'subprocesses':0,'fixture_constructors':0,'actual_service_or_nn':False,'receipt':str(ROOT/'result.json')}))
