# SPDX-License-Identifier: Apache-2.0
"""Fresh single-TU CUDA static-core derivation; never confers qualification."""
import argparse
import datetime as dt
import hashlib
import importlib.util
import json
import math
import os
from pathlib import Path
import re
import subprocess
import sys
import time

sys.dont_write_bytecode = True
SID = 'S-1-5-21-3678370870-3858642417-826968105-1002'
PARENT = 'f9fac8eb2fe1a1cb5191118124503782792ca6d3aff14d02326167cd7276b178'
AGGREGATE = '042c970193bf3d2fc7c80f53b055f74130c653d561f7735142fc01b01cebac00'
RAW = 'airk.private.cuda-static-incremental-receipt/1'
PASS = 'PASS_INCREMENTAL_PRODUCER_ONLY_UNQUALIFIED'
STAGES = ('compile', 'session', 'aggregate', 'object-symbols', 'aggregate-directives')
CAPS = (120, 60, 300, 120, 120)
LIMITS = dict(whole_wall_seconds=600, compile_wall_seconds=120, session_wall_seconds=60,
              aggregate_wall_seconds=300, proof_wall_seconds=120, process_bytes=16 << 30,
              job_bytes=24 << 30, log_bytes=64 << 20, cleanup_seconds=5, parallelism=1)
EXPORTS = ['airk_ort_collect_final_placement_v1', 'airk_ort_collect_final_graph_metadata_v1']
TIMING = ('Original 600s covers preflight, commands, input rechecks, proof hashing and output audit; '
          'final receipt retention guard, encoding/write and postwrite owner audit are excluded.')
ANCESTRY = ('source_inventory_sha256', 'frontend_inventory_sha256', 'cuda_inventory_sha256',
            'frozen_input_closure_sha256', 'ninja', 'toolchain_pins')

def require(ok, message):
    if not ok:
        raise ValueError(message)

def finite(x):
    return type(x) in (int, float) and math.isfinite(x)

def canonical(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(',', ':'), ensure_ascii=False,
                                     allow_nan=False).encode()).hexdigest()

def read(path):
    p = Path(path)
    require(p.stat().st_size <= 32 << 20, 'JSON exceeds bound')
    return parse_json(p.read_bytes())

def parse_json(data):
    def pairs(rows):
        result = {}
        for key,value in rows:
            require(key not in result, 'duplicate JSON key: ' + key)
            result[key] = value
        return result
    def invalid(value):
        raise ValueError('nonfinite JSON: '+value)
    return json.loads(data,object_pairs_hook=pairs,parse_constant=invalid)

def pin(path, deadline=None):
    p = Path(path).absolute()
    for ancestor in [p, *p.parents]:
        require(not ancestor.is_symlink() and not getattr(ancestor.stat(), 'st_file_attributes', 0) & 0x400,
                'reparse input refused: ' + str(ancestor))
    require(p.is_file(), 'regular file required')
    h = hashlib.sha256()
    size = 0
    with p.open('rb') as f:
        while data := f.read(4 << 20):
            if deadline is not None:
                require(time.monotonic() < deadline, 'original deadline during hashing')
            h.update(data)
            size += len(data)
    require(size == p.stat().st_size, 'file changed while hashing')
    return dict(path=str(p), bytes=size, sha256=h.hexdigest())

def check_pin(row, deadline=None):
    require(set(row) == {'path', 'bytes', 'sha256'} and type(row['bytes']) is int
            and isinstance(row['sha256'], str) and re.fullmatch('[0-9a-f]{64}', row['sha256'])
            and Path(row['path']).is_absolute(), 'invalid absolute FilePin')
    require(pin(row['path'], deadline) == row, 'pinned file changed: ' + row['path'])
    return Path(row['path'])

def load(path, name):
    spec = importlib.util.spec_from_file_location(name, path)
    obj = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(obj)
    return obj


def deadline_check(deadline):
    if deadline is not None:
        require(time.monotonic() < deadline, 'original deadline during historical verification')

def bound_module(module, deadline):
    """Only this privately loaded module's Path/load bindings; no global monkeypatch."""
    if deadline is None or getattr(module, '_incremental_bound', False):
        return module
    module._incremental_bound = True
    class Stream:
        def __init__(self, stream):
            self.stream = stream
        def __enter__(self):
            self.stream.__enter__()
            return self
        def __exit__(self, *args):
            return self.stream.__exit__(*args)
        def __getattr__(self, name):
            value = getattr(self.stream,name)
            if not callable(value):
                return value
            def call(*args,**kwargs):
                deadline_check(deadline)
                result = value(*args,**kwargs)
                deadline_check(deadline)
                return result
            return call
    class BoundedPath(type(Path())):
        def open(self,*args,**kwargs):
            mode = args[0] if args else kwargs.get('mode','r')
            require(not any(x in mode for x in ('w','a','x','+')), 'historical verifier attempted mutation')
            deadline_check(deadline)
            return Stream(super().open(*args,**kwargs))
    if hasattr(module,'Path'):
        module.Path = BoundedPath
    for name in ('load','module','base'):
        previous = getattr(module,name,None)
        if not callable(previous):
            continue
        def loader(*args,_fn=previous,**kwargs):
            deadline_check(deadline)
            loaded = _fn(*args,**kwargs)
            deadline_check(deadline)
            return bound_module(loaded,deadline)
        setattr(module,name,loader)
    return module

def all_pins(value):
    if isinstance(value, dict):
        if set(value) == {'path', 'bytes', 'sha256'}:
            yield value
        else:
            for child in value.values():
                yield from all_pins(child)
    elif isinstance(value, list):
        for child in value:
            yield from all_pins(child)

def frozen_inputs(recipe, deadline=None):
    seen = {}
    for row in all_pins(recipe):
        key = os.path.normcase(row['path'])
        require(key not in seen or seen[key] == row, 'conflicting input pins')
        if key not in seen:
            check_pin(row, deadline)
            seen[key] = row
    for row in recipe['response_files']:
        check_pin(dict(path=row['proposal_path'], bytes=row['bytes'], sha256=row['sha256']), deadline)

def validate_recipe(r):
    out = Path(r['fresh_output'])
    require(out.is_absolute() and r['proposed_limits'] == LIMITS, 'fresh root/original bounds differ')
    require(r['old_producer']['sha256'] == PARENT and r['old_aggregate']['sha256'] == AGGREGATE,
            'historical parent authority differs')
    require(len(r['compiler_dependencies']) == 482 and len(r['session_objects']) == 40
            and len(r['aggregate_libraries']) == 96, 'preserved input roster count differs')
    for key in ('compiler_dependencies', 'session_objects', 'aggregate_libraries'):
        require(len({os.path.normcase(x['path']) for x in r[key]}) == len(r[key]), 'duplicate input')
    obj = str(out / 'objects/onnxruntime_c_api.cc.obj')
    session = str(out / 'session/onnxruntime_session.lib')
    for rows, paths, fresh, suffix in (
        (r['session_objects'], r['session_input_paths'], obj, 'onnxruntime_c_api.cc.obj'),
        (r['aggregate_libraries'], r['aggregate_input_paths'], session, 'onnxruntime_session.lib')):
        old = [x['path'] for x in rows]
        replaced = [p for p in old if Path(p).name == suffix]
        require(len(replaced) == 1 and paths == [fresh if p == replaced[0] else p for p in old],
                'exact single input substitution differs')
    original = r['original_compile_argv']
    command = r['commands']['compile']
    expected = [command[0]] + [('/Fo' + obj if x.startswith('/Fo') else
        '/Fd' + str(out / 'objects/onnxruntime_session.pdb') if x.startswith('/Fd') else
        str(out / 'source/onnxruntime_c_api.cc') if x == r['base_source']['path'] else x)
        for x in original[1:]]
    expected.insert(next(i for i,x in enumerate(expected) if x.startswith(('-I','/I'))),
                    '/I' + str(Path(r['base_source']['path']).parent))
    require(command == expected and '/showIncludes' in command and '-c' in command,
            'compile differs beyond reviewed relocations')
    require(r['required_exports'] == EXPORTS, 'collector export pair differs')
    require(r['source_patch']['base'] == r['base_source']
        and r['source_patch']['after']['sha256'] == r['new_source_sha256']
        and r['source_patch']['after']['bytes'] == r['new_source_bytes'],
        'reviewed source patch authority differs')
    supplemental = r['approved_new_dependencies']
    if supplemental:
        require(len(supplemental) == 324 and 'base_header_observation' in r,
                'the complete reviewed toolchain header supplement is required')
        roots = tuple(Path(path) for path in r['environment']['INCLUDE'].split(';') if path)
        headers = [os.path.normcase(str(Path(row['path']))) for row in
                   r['compiler_dependencies'] + supplemental]
        require(len(set(headers)) == len(headers), 'duplicate project or toolchain header')
        require(all(any(Path(row['path']).is_relative_to(root) for root in roots)
                    for row in supplemental), 'supplemental header outside recorded INCLUDE roots')
    else:
        require('base_header_observation' not in r, 'empty toolchain header supplement')
    tool_files = {x['path'] for x in r['compiler_tool_files']}
    require(all(cmd[0] in tool_files for cmd in r['commands'].values()), 'unfrozen compiler tool')
    require(r['commands']['session'] == [r['commands']['session'][0], '/nologo', '/machine:x64',
        '/OUT:' + session, '@' + str(out / 'session-inputs.rsp')], 'session command differs')
    require(r['commands']['aggregate'] == [r['commands']['aggregate'][0], '/NOLOGO',
        '/OUT:' + str(out / 'aggregate/onnxruntime.lib'), '@' + str(out / 'aggregate-inputs.rsp')],
        'aggregate command differs')
    require(r['commands']['object_symbols'] == [r['commands']['object_symbols'][0], '/symbols', obj]
        and r['commands']['aggregate_directives'] == [r['commands']['aggregate_directives'][0],
        '/directives', str(out / 'aggregate/onnxruntime.lib')], 'proof commands differ')
    require(r['environment']['CL'] == r['environment']['_CL_'] == ''
        and r['environment']['VSLANG'] == '1033'
        and all(Path(r['environment'][x]) == out / 'tmp' for x in ('TEMP','TMP')),
        'compiler environment differs')
    for row, paths in zip(r['response_files'], (r['session_input_paths'], r['aggregate_input_paths'])):
        data = ('\r\n'.join('"' + p + '"' for p in paths) + '\r\n').encode()
        require(Path(row['future_path']) == out / row['name'] and len(data) == row['bytes']
            and hashlib.sha256(data).hexdigest() == row['sha256'], 'response roster bytes differ')
    return out

def settled(owned, executable):
    exact = dict(exit_code=0, timed_out=False, log_oversized=False, owned_descendants_settled=True,
        owned_job_forced_termination=False, owned_job_final_active_processes=0,
        owned_job_final_member_pids=[], parent_assigned_to_job=False, child_assigned_before_resume=True,
        unexpected_third_process=False, whole_process_deadline_not_refreshed=True)
    require(all(type(owned.get(k)) is type(v) and owned[k] == v for k,v in exact.items()),
            'command failed or forced/unsettled')
    require(type(owned.get('owned_total_processes')) is int and 1 <= owned['owned_total_processes'] <= 2
        and type(owned.get('owned_leader_pid')) is int and owned['owned_leader_pid'] > 0
        and type(owned.get('peak_host_committed_bytes')) is int
        and 0 < owned['peak_host_committed_bytes'] <= LIMITS['job_bytes'], 'Job count/commit proof differs')
    close = owned.get('handle_close_proof', {})
    require(all(type(close.get(k)) is type(v) and close[k] == v for k,v in
        dict(forced=False, settled=True, final_active=0, members_before=[], final_member_pids=[]).items()),
        'retained HANDLE close proof differs')
    for rows in (owned.get('retained_member_exits'), close.get('retained_exits')):
        require(isinstance(rows,list) and rows and all(x.get('exit_observed') is True
            and type(x.get('exit_code')) is int and x['exit_code'] == 0 for x in rows),
            'actual HANDLE exit proof missing')
    require(any(x.get('pid') == owned['owned_leader_pid'] for x in close['retained_exits']),
            'leader HANDLE not retained through settlement')
    s = owned.get('suspended_assignment', {})
    require(s.get('pid') == owned['owned_leader_pid'] and s.get('execution_user_sid') == SID
        and s.get('default_owner_sid') == SID and s.get('image') ==
        {'path':executable['path'], 'sha256':executable['sha256']}
        and s.get('component_job') == dict(process_bytes=16 << 30, job_bytes=24 << 30,
                                         flags=0x2308, active_limit=2), 'suspended identity/Job differs')

def base_header_command(recipe, preprocessed):
    original = recipe["original_compile_argv"]
    require(original.count("-c") == 1 and original.count("/showIncludes") == 1,
            "unchanged compiler input must be the single observed translation unit")
    require(original[-1] == recipe["base_source"]["path"], "original compiler source differs")
    require(sum(arg.startswith("/Fo") for arg in original) == 1
            and sum(arg.startswith("/Fd") for arg in original) == 1,
            "expected original object and PDB options")
    require(not any(arg.startswith(("@", "/Fi", "/Fa", "/Fe")) for arg in original[1:]),
            "unexpected additional compiler output or response file")
    return [recipe["commands"]["compile"][0], "/P", "/Fi" + str(preprocessed), *(
        arg for arg in original[1:]
        if arg not in ("-c", "/FS") and not arg.startswith(("/Fo", "/Fd"))
    )]


def verify_base_header_observation(recipe, deadline):
    require(recipe.get('approved_new_dependencies') and 'base_header_observation' in recipe,
            'executing or verifying the collector requires the complete base header observation')
    observation = read(check_pin(recipe['base_header_observation'], deadline))
    baseline = read(check_pin(observation['recipe'], deadline))
    original_keys = ('base_source', 'original_compile_argv', 'parent_context', 'compiler_tool_files')
    require(all(baseline[key] == recipe[key] for key in original_keys),
            'independent baseline recipe inputs differ')
    base_environment = {key: value for key, value in baseline['environment'].items()
                        if key not in ('TEMP', 'TMP')}
    compile_environment = {key: value for key, value in recipe['environment'].items()
                           if key not in ('TEMP', 'TMP')}
    require(base_environment == compile_environment, 'independent baseline environment differs')
    probe = Path(__file__).resolve().parent / 'cuda-static-incremental/probe_headers.py'
    require(pin(probe, deadline) == observation['probe_source'], 'reviewed header probe differs')
    compiler = next(row for row in recipe['compiler_tool_files']
                    if row['path'] == recipe['commands']['compile'][0])
    require(observation['schema'] == 'airk.private.collector-base-header-observation/1'
            and observation['qualification'] is False
            and observation['skip_devshell_telemetry'] is False
            and finite(observation['elapsed_seconds'])
            and 0 < observation['elapsed_seconds'] < 120
            and observation['source'] == recipe['base_source']
            and observation['compiler'] == compiler
            and observation['compiler_dependencies'] == len(recipe['compiler_dependencies'])
            and observation['all_header_count'] ==
                len(recipe['compiler_dependencies']) + len(recipe['approved_new_dependencies'])
            and observation['toolchain_headers'] == recipe['approved_new_dependencies'],
            'unchanged source header observation differs from recipe')
    settled(observation['owned'], compiler)
    log = check_pin(observation['log'], deadline)
    expected_cwd = Path(recipe['parent_context']['destination']) / 'b/Release'
    require(Path(observation['cwd']) == expected_cwd
            and observation['command'] == base_header_command(recipe, log.parent / 'base.i'),
            'unchanged source preprocessing command differs')
    include_recipe = dict(recipe, fresh_output=observation['cwd'])
    includes(log.read_bytes(), include_recipe)
    deadline_check(deadline)


def validate_clocks(receipt, recipe):
    a,z,d = (receipt[k] for k in ('started_monotonic','finished_monotonic','original_deadline_monotonic'))
    require(all(finite(x) for x in (a,z,d)) and d == a + 600 and a <= z < d
        and receipt['elapsed_seconds'] == z-a, 'original whole deadline differs')
    utc = [dt.datetime.fromisoformat(receipt[k]) for k in ('started_utc','finished_utc','original_deadline')]
    require(all(x.utcoffset() == dt.timedelta(0) for x in utc)
        and utc[0] <= utc[1] < utc[2] and utc[2]-utc[0] == dt.timedelta(seconds=600), 'UTC chronology differs')
    commands = receipt['commands']
    require(len(commands) == len(STAGES), 'command roster differs')
    prior = a
    for cmd, stage, cap in zip(commands, STAGES, CAPS):
        start,end,cutoff = (cmd[k] for k in ('started_monotonic','finished_monotonic','absolute_deadline_monotonic'))
        require(all(finite(x) for x in (start,end,cutoff)) and prior <= start <= end <= z and end < cutoff <= d
            and cutoff == min(d,start+cap) and cmd['phase_limit_seconds'] == cap
            and cmd['stage'] == stage and cmd['argv'] == recipe['commands'][stage.replace('-','_')]
            and cmd['cwd'] == recipe['fresh_output'], 'phase order/deadline/argv differs')
        require(dt.datetime.fromisoformat(cmd['started_utc']) <= dt.datetime.fromisoformat(cmd['finished_utc']),
                'command UTC order differs')
        prior = end

def includes(log, recipe):
    rows = {}
    for line in log.decode('utf-8', errors='strict').splitlines():
        if line.startswith('Note: including file:'):
            path = Path(line.split('Note: including file:',1)[1].strip())
            if not path.is_absolute():
                path = Path(recipe['fresh_output']) / path
            rows[os.path.normcase(str(path.resolve()))] = path.resolve()
    expected = {os.path.normcase(str(Path(x['path']).resolve())):x
                for x in recipe['compiler_dependencies'] + recipe['approved_new_dependencies']}
    require(set(rows) == set(expected), 'actual showIncludes differs from frozen complete header roster')
    return [expected[k] for k in sorted(expected)]

def complete_stage(stage, audit, observe_includes, deadline):
    # This runs before any downstream archive/proof dispatch.
    audit()
    if stage == 'compile':
        result = observe_includes()
        deadline_check(deadline)
        return result
    deadline_check(deadline)
    return None

def archive_proof(recipe, objpins, libpins, archive):
    out = Path(recipe['fresh_output'])
    expected_objects = [{'kind':'static','object_bodies':[dict(sha256=x['sha256'],bytes=x['bytes'],count=1)]}
                        for x in objpins]
    session = archive.inspect_archive(out / 'session/onnxruntime_session.lib')
    archive.verify_member_closure(expected_objects, session)
    libraries = [archive.inspect_archive(x['path']) for x in libpins]
    aggregate = archive.inspect_archive(out / 'aggregate/onnxruntime.lib')
    archive.verify_member_closure(libraries, aggregate)
    require(session['kind'] == aggregate['kind'] == 'static'
        and all(x in session['export_directives'] and x in aggregate['export_directives'] for x in EXPORTS),
        'actual static collector exports missing')
    return dict(session=session, aggregate=aggregate, required_exports=EXPORTS)

def validate_parent_toolchain(recipe, parent):
    tools = parent['toolchain_pins']
    compiler = [x for x in recipe['compiler_tool_files'] if x['path'] == recipe['commands']['compile'][0]]
    require(len(tools) == 6 and compiler == [tools[0]] and recipe['ninja'] == tools[2]
        and recipe['vcvars'] == tools[5], 'actual compiler/Ninja/vcvars differ from inherited parent toolchain')
    require(recipe['environment']['VCTOOLSVERSION'] == '14.44.35207'
        and recipe['environment']['WINDOWSSDKVERSION'] == '10.0.26100.0\\',
        'exact retained compiler/SDK environment differs')

def parent_proof(recipe, repo, deadline=None):
    deadline_check(deadline)
    ck = bound_module(load(repo / 'scripts/cuda-static-checkpointed.py', 'incremental_historical_checkpoint'),deadline)
    p = recipe['parent_context']
    parent = ck.verify_checkpointed(Path(p['destination']), Path(p['composite_root']),
        Path(p['proposal']), PARENT, repo)
    require(parent == read(check_pin(recipe['parent_build_proof'],deadline)), 'canonical historical proof differs')
    validate_parent_toolchain(recipe,parent)
    deadline_check(deadline)
    return parent

def modules(recipe, repo, deadline=None):
    for row in recipe['helper_pins']:
        check_pin(row,deadline)
    c = load(repo / 'scripts/cuda-component-execution.py', 'incremental_existing_component')
    return c, bound_module(load(repo / 'scripts/cuda-static-archive.py', 'incremental_archive_owner'),deadline)

def verify_incremental(destination, incremental_root, incremental_proposal, receipt_sha, repo_root, *, deadline=None):
    observed = time.monotonic()
    deadline = observed+600 if deadline is None else deadline
    require(finite(deadline) and deadline <= observed+600, 'verification deadline must be finite and at most600s')
    deadline_check(deadline)
    file_pin = lambda path: pin(path,deadline)
    checked = lambda row: check_pin(row,deadline)
    root, proposal, repo = Path(incremental_root), Path(incremental_proposal), Path(repo_root)
    require(Path(destination) == root, 'new producer destination differs')
    raw_path = root / 'gpu-incremental-build-receipt.json'
    raw_pin = file_pin(raw_path)
    require(raw_pin['sha256'] == receipt_sha and receipt_sha != PARENT, 'fresh receipt pin differs')
    v = read(raw_path)
    require(v['schema'] == RAW and v['status'] == PASS and v['mode'] == 'incremental-finalgraph'
        and v['failure'] is None and v['qualification'] is False
        and v['cold_build_qualified'] is False and v['activation_pins_available'] is False,
        'completed unqualified incremental receipt required')
    r = read(checked(v['recipe']))
    require(Path(v['recipe']['path']) == proposal / 'recipe.json'
        and validate_recipe(r) == root and Path(r['repo_root']) == repo and v['producer_source'] == r['producer_source']
        and file_pin(__file__) == r['producer_source'], 'reviewed producer/recipe binding differs')
    frozen_inputs(r, deadline)
    verify_base_header_observation(r, deadline)
    require(Path(r['source_patch']['after']['path']).read_bytes().startswith(Path(r['base_source']['path']).read_bytes()), 'historical TU prefix changed')
    parent = parent_proof(r, repo,deadline)
    _, archive = modules(r, repo,deadline)
    for key, value in [('parent_producer',r['old_producer']),('parent_build_proof',r['parent_build_proof']),
                       ('parent_aggregate',r['old_aggregate']),('source_patch',r['source_patch'])]:
        require(v[key] == value, 'derivation parent/source differs')
    for key, relative in [('new_source','source/onnxruntime_c_api.cc'),
                         ('new_object','objects/onnxruntime_c_api.cc.obj'),
                         ('new_session','session/onnxruntime_session.lib'),
                         ('new_aggregate','aggregate/onnxruntime.lib')]:
        require(v[key] == file_pin(root / relative), 'actual fresh output differs')
    require(v['new_source']['sha256'] == r['source_patch']['after']['sha256']
        and v['new_aggregate']['sha256'] != AGGREGATE, 'new source/aggregate derivation absent')
    require(v['session_input_pins'] == [file_pin(x) for x in r['session_input_paths']]
        and v['aggregate_input_pins'] == [file_pin(x) for x in r['aggregate_input_paths']], 'replacement inputs drift')
    validate_clocks(v,r)
    for cmd in v['commands']:
        settled(cmd['owned'], file_pin(cmd['argv'][0]))
        checked(cmd['stdout'])
        require(cmd['stdout']['bytes'] <= 64 << 20
            and Path(cmd['stdout']['path']) == root / 'logs' / (cmd['stage']+'.log'), 'command log differs')
    require(v['actual_include_pins'] == includes(Path(v['commands'][0]['stdout']['path']).read_bytes(),r),
            'actual include proof differs')
    require(v['archive_proof'] == archive_proof(r,v['session_input_pins'],v['aggregate_input_pins'],archive),
            'actual archive member proof differs')
    require(v['input_recheck_errors'] == [] and v['limits'] == LIMITS
        and v['execution_user_sid'] == v['default_owner_sid'] == SID
        and v['identity']['UserSid'] == v['identity']['OwnerSid'] == SID
        and SID in v['identity']['InteractiveOwnerSids'] and v['identity']['SessionId'] > 0
        and v['timing_scope'] == TIMING, 'identity/input/clock policy differs')
    outputs = {str(p):None for p in [root,*root.rglob('*')] if p != raw_path}
    require(v['output_owner_audit'] == outputs, 'output custody roster differs')
    c,_ = modules(r,repo,deadline)
    c.guard_many(r,[root,*root.rglob('*')],True,deadline=deadline)
    libraries = []
    oldroot = Path(r['parent_context']['destination'])
    for old, new in zip(r['aggregate_libraries'],v['aggregate_input_pins']):
        libraries.append(dict(path=Path(old['path']).relative_to(oldroot).as_posix(),
                              bytes=new['bytes'],sha256=new['sha256']))
    require(len(libraries) == 96, 'canonical libraries count differs')
    result = dict(schema='airk.cuda-static-build-proof/3', target=parent['target'], ort_version=parent['ort_version'],
        build_receipt=raw_pin, producer_source=v['producer_source'], parent_producer=v['parent_producer'],
        parent_build_proof=v['parent_build_proof'], parent_aggregate=v['parent_aggregate'], recipe=v['recipe'],
        source_patch=v['source_patch'], compile_dependencies=r['compiler_dependencies'],
        actual_include_pins=v['actual_include_pins'], provider_dlls=parent['provider_dlls'], libraries=libraries,
        qualification=False, activation_pins_available=False, cold_build_qualified=False)
    for k in ('new_source','new_object','new_session','new_aggregate','session_input_pins','aggregate_input_pins'):
        result[k] = v[k]
    result.update({k:parent[k] for k in ANCESTRY})
    require(file_pin(raw_path) == raw_pin, 'fresh receipt changed during verification')
    deadline_check(deadline)
    return result

def verify_incremental_bundle(destination, incremental_root, incremental_proposal, receipt_sha, repo_root, *, deadline=None):
    observed = time.monotonic()
    deadline = observed+600 if deadline is None else deadline
    require(finite(deadline) and deadline <= observed+600, 'verification deadline must be finite and at most600s')
    deadline_check(deadline)
    build = verify_incremental(destination,incremental_root,incremental_proposal,receipt_sha,repo_root,deadline=deadline)
    deadline_check(deadline)
    value = read(check_pin(build['build_receipt'],deadline))
    recipe = read(check_pin(build['recipe'],deadline))
    aggregate = dict(schema='airk.cuda-static-aggregate-proof/1',target=build['target'],
        producer_proof_sha256=canonical(build),receipt_sha256=build['build_receipt']['sha256'],
        output=dict(path='aggregate/onnxruntime.lib',bytes=build['new_aggregate']['bytes'],
                    sha256=build['new_aggregate']['sha256']),plan_sha256=canonical(recipe),
        coff=value['archive_proof']['aggregate'],qualification=False)
    deadline_check(deadline)
    return {'build':build,'aggregate':aggregate}


def verify_aggregate(destination, build, core, *, deadline=None):
    observed = time.monotonic()
    deadline = observed+600 if deadline is None else deadline
    require(finite(deadline) and deadline <= observed+600, 'verification deadline must be finite and at most600s')
    deadline_check(deadline)
    require(build.get('schema') == 'airk.cuda-static-build-proof/3', 'incremental proof/3 required')
    recipe = read(check_pin(build['recipe'],deadline))
    bundle = verify_incremental_bundle(destination,recipe['fresh_output'],Path(build['recipe']['path']).parent,
        build['build_receipt']['sha256'],recipe['repo_root'],deadline=deadline)
    require(bundle['build'] == build, 'canonical incremental proof differs')
    return bundle['aggregate']


def execute(recipe_path, expected_sha, repo_root):
    start = time.monotonic()
    began = dt.datetime.now(dt.timezone.utc)
    deadline = start + 600
    recipe_pin = pin(recipe_path,deadline)
    require(recipe_pin['sha256'] == expected_sha, 'externally reviewed recipe pin differs')
    r = read(recipe_path)
    out = validate_recipe(r)
    require(r['execution_authorized'] is True and not out.exists(), 'proposal/unfresh output cannot execute')
    require(pin(__file__,deadline) == r['producer_source'], 'reviewed runner changed')
    repo = Path(repo_root)
    frozen_inputs(r, deadline)
    verify_base_header_observation(r, deadline)
    require(Path(r['source_patch']['after']['path']).read_bytes().startswith(Path(r['base_source']['path']).read_bytes()), 'historical TU prefix changed')
    deadline_check(deadline)
    c,archive = modules(r,repo,deadline)
    c.guard(r,deadline=deadline)
    parent_proof(r,repo,deadline)
    code = ("$ErrorActionPreference='Stop'; $v=[Console]::In.ReadToEnd()|ConvertFrom-Json; "
            ". $v.guard; Assert-AirkHumanExecution -ExpectedSid $v.sid | ConvertTo-Json -Depth 5 -Compress")
    response = subprocess.run([r['powershell']['path'],'-NoProfile','-Command',code],
                              input=json.dumps({'guard':r['checked_guard']['path'],'sid':SID}),
                              capture_output=True,text=True,check=True,timeout=c.audit_timeout(deadline))
    identity = json.loads(response.stdout)
    c.mkdir(r,out,deadline=deadline)
    c.guard(r,out/'gpu-incremental-build-receipt.json',deadline=deadline)
    for name in ('source','objects','session','aggregate','logs','tmp'):
        c.mkdir(r,out/name,deadline=deadline)
    def write(path,data):
        c.guard(r,path,deadline=deadline)
        with path.open('xb') as f:
            f.write(data)
        c.guard(r,path,True,deadline=deadline)
    write(out/'source/onnxruntime_c_api.cc',check_pin(r['source_patch']['after'],deadline).read_bytes())
    for row in r['response_files']:
        write(Path(row['future_path']),Path(row['proposal_path']).read_bytes())
    write(out/'stdin.txt',b'')
    require(list((out/'source').iterdir()) == [out/'source/onnxruntime_c_api.cc'], 'fresh quote search polluted')
    v = dict(schema=RAW,status='FAILED_PRESERVED_NO_RETRY',mode='incremental-finalgraph',
        recipe=recipe_pin,producer_source=r['producer_source'],parent_producer=r['old_producer'],
        parent_build_proof=r['parent_build_proof'],parent_aggregate=r['old_aggregate'],source_patch=r['source_patch'],
        started_utc=began.isoformat(),started_monotonic=start,original_deadline=(began+dt.timedelta(seconds=600)).isoformat(),
        original_deadline_monotonic=deadline,execution_user_sid=SID,default_owner_sid=SID,identity=identity,
        limits=LIMITS,commands=[],input_recheck_errors=[],qualification=False,activation_pins_available=False,
        cold_build_qualified=False,failure=None,timing_scope=TIMING)
    parent,owner = c.base().helpers(r)
    require(identity['UserSid'] == identity['OwnerSid'] == SID and SID in identity['InteractiveOwnerSids'],
            'actual parent identity differs')
    try:
        for stage,cap in zip(STAGES,CAPS):
            argv = r['commands'][stage.replace('-','_')]
            a = time.monotonic()
            cutoff = min(deadline,a+cap)
            require(a < cutoff, 'original phase exhausted')
            log = out/'logs'/(stage+'.log')
            c.guard(r,log,deadline=cutoff)
            # Compiler/linker outputs and temp roots were preaudited; audit again before dispatch.
            c.guard_many(r,[out/'objects',out/'session',out/'aggregate',out/'tmp'],True,deadline=cutoff)
            executable = pin(argv[0],cutoff)
            def authenticate(api, job, info, child_deadline):
                proof = parent.suspended_authentication(api,info,cutoff,child_deadline,SID,executable,owner.image_proof)
                require(c.token_owner(info.process) == SID, 'child default owner differs')
                proof.update(default_owner_sid=SID,component_job=c.configure_job(api,job))
                return proof
            cmd = dict(stage=stage,argv=argv,cwd=str(out),started_utc=dt.datetime.now(dt.timezone.utc).isoformat(),
                started_monotonic=a,absolute_deadline_monotonic=cutoff,phase_limit_seconds=cap)
            v['commands'].append(cmd)
            with (out/'stdin.txt').open('rb') as initial,log.open('xb') as output:
                cmd['owned'] = owner.run_windows_owned_training(argv,out,output,cutoff-time.monotonic(),
                    r['environment'],initial,16<<30,before_resume=authenticate,cleanup_seconds=5,absolute_deadline=cutoff)
            # Every output, including PDB/temp, is audited before another phase can consume it.
            def audit_outputs():
                c.guard_many(r, [out, *out.rglob('*')], True, deadline=cutoff)
                cmd['stdout'] = pin(log, cutoff)

            try:
                observed = complete_stage(
                    stage, audit_outputs, lambda: includes(log.read_bytes(), r), cutoff)
            finally:
                cmd['finished_utc'] = dt.datetime.now(dt.timezone.utc).isoformat()
                cmd['finished_monotonic'] = time.monotonic()
            if stage == 'compile':
                v['actual_include_pins'] = observed
            require(cmd['finished_monotonic'] < cutoff, 'command final observation expired')
            settled(cmd['owned'],executable)

        for key,relative in [('new_source','source/onnxruntime_c_api.cc'),('new_object','objects/onnxruntime_c_api.cc.obj'),
                             ('new_session','session/onnxruntime_session.lib'),('new_aggregate','aggregate/onnxruntime.lib')]:
            v[key] = pin(out/relative,deadline)
        v['session_input_pins'] = [pin(x,deadline) for x in r['session_input_paths']]
        v['aggregate_input_pins'] = [pin(x,deadline) for x in r['aggregate_input_paths']]
        v['archive_proof'] = archive_proof(r,v['session_input_pins'],v['aggregate_input_pins'],archive)
        v['status'] = PASS
    except BaseException as error:
        v['failure'] = dict(type=type(error).__name__,message=str(error))
        if hasattr(error,'owned_cleanup'):
            v['actual_cleanup_failure'] = error.owned_cleanup
        raise
    finally:
        original = sys.exc_info()[1]
        try:
            frozen_inputs(r,deadline)
            v['output_owner_audit'] = c.guard_many(r,[out,*out.rglob('*')],True,deadline=deadline)
            c.guard(r,out/'gpu-incremental-build-receipt.json',deadline=deadline)
        except BaseException as error:
            v['input_recheck_errors'].append(dict(type=type(error).__name__,message=str(error)))
            v['status'] = 'FAILED_PRESERVED_NO_RETRY'
        v['finished_utc'] = dt.datetime.now(dt.timezone.utc).isoformat()
        v['finished_monotonic'] = time.monotonic()
        v['elapsed_seconds'] = v['finished_monotonic']-start
        if v['finished_monotonic'] >= deadline or v['failure'] is not None:
            v['status'] = 'FAILED_PRESERVED_NO_RETRY'
        try:
            path = out/'gpu-incremental-build-receipt.json'
            # Explicit bounded retention-only guard; never write after a refused guard.
            c.guard(r,path)
            with path.open('xb') as f:
                f.write((json.dumps(v,sort_keys=True,indent=2,allow_nan=False)+'\n').encode())
            c.guard(r,path,True)
        except BaseException as error:
            if original is not None:
                original.add_note('Receipt retention failed: '+str(error))
            else:
                raise
        if original is None:
            require(v['status'] == PASS, 'producer final proof failed')
    return v

def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--execute-reviewed-producer',action='store_true',required=True)
    p.add_argument('--recipe',type=Path,required=True)
    p.add_argument('--recipe-sha256',required=True)
    p.add_argument('--repo-root',type=Path,required=True)
    args = p.parse_args()
    print(json.dumps(execute(args.recipe,args.recipe_sha256,args.repo_root),sort_keys=True))

if __name__ == '__main__':
    main()
