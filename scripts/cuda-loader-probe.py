# SPDX-License-Identifier: Apache-2.0
"""Diagnostic DLL load order only; no ORT API, cuInit, session, tensor or model."""
import argparse
import ctypes
import datetime
import importlib.util
import json
import math
import shutil
import os
from pathlib import Path
import subprocess
import sys
import time

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[1]
SCHEMA = 'airk.private.cuda-loader-probe-plan/1'
EVENT = 'airk.private.cuda-loader-probe-event/1'
DEFAULT_FLAGS = 0xA00  # application directory + System32, same as native owner
LOAD_FLAGS = 0x900  # DLL load directory + System32, same as native owner
CUDA_PROVIDER = 'onnxruntime_providers_cuda.dll'
SHARED_PROVIDER = 'onnxruntime_providers_shared.dll'
LIMITS = {'wall_seconds': 120, 'process_bytes': 16 << 30, 'job_bytes': 24 << 30,
          'log_bytes': 64 << 20, 'cleanup_seconds': 5}


def require(ok, message):
    if not ok:
        raise ValueError(message)


def component():
    path = ROOT / 'scripts/cuda-component-execution.py'
    spec = importlib.util.spec_from_file_location('loader_existing_component', path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def roster(plan, b):
    manifest = b.read(b.pinned(plan['descriptor']))
    require(manifest.get('schema') == 'airk.native.cuda-runtime-manifest/1', 'exact scientific runtime required')
    names = manifest['load_order']
    require(len(names) == 13 and len(set(names)) == 13 and set(names) == {x['name'] for x in manifest['files']}
            and names[-1] == CUDA_PROVIDER and names.index(SHARED_PROVIDER) < names.index(CUDA_PROVIDER),
            'complete original native load roster required')
    for name in names:
        require(Path(name).name == name and '/' not in name and '\\' not in name and ':' not in name,
                'direct runtime module names required')
    require(Path(plan['driver']['path']) == Path('C:/Windows/System32/nvcuda.dll'), 'system driver path differs')
    return [('nvcuda.dll', Path(plan['driver']['path']))] + [(name, Path(plan['runtime_root']) / name) for name in names]


def verify_origin_facts(receipt, capture, stdout, old, sid):
    require(receipt.get('schema') == 'airk.private.owned-cuda-component/1' and receipt.get('complete') is False
            and receipt.get('failure') is not None and receipt.get('whole_phase_expired') is False
            and receipt.get('whole_phase_deadline_not_refreshed') is True
            and receipt.get('post_input_drift') == [] and receipt.get('final_input_errors') == [], 'original failed custody differs')
    owned = receipt['owned']
    expected = {'exit_code': 101, 'timed_out': False, 'log_oversized': False,
                'owned_descendants_settled': True, 'owned_job_forced_termination': False,
                'owned_job_final_active_processes': 0, 'owned_job_final_member_pids': [],
                'parent_assigned_to_job': False, 'child_assigned_before_resume': True,
                'owned_leader_pid': 40388, 'owned_total_processes': 1, 'unexpected_third_process': False,
                'whole_process_deadline_not_refreshed': True}
    require(all(type(owned.get(k)) is type(v) and owned[k] == v for k, v in expected.items()), 'actual original natural101 Job proof differs')
    require(owned.get('retained_member_exits') == [{'exit_observed': True, 'exit_code': 101}], 'original retained exit101 absent')
    close = owned.get('handle_close_proof', {})
    expected_close = {'forced': False, 'settled': True, 'final_active': 0, 'members_before': [], 'final_member_pids': []}
    require(all(type(close.get(k)) is type(v) and close[k] == v for k, v in expected_close.items())
            and close.get('retained_exits') == [{'pid': 40388, 'exit_observed': True, 'exit_code': 101}],
            'original actual HANDLE closure proof absent')
    suspended = owned.get('suspended_assignment', {})
    require(suspended.get('pid') == 40388 and suspended.get('execution_user_sid') == sid
            and suspended.get('default_owner_sid') == sid
            and suspended.get('image') == {k: old['native_image'][k] for k in ['path', 'sha256']}
            and suspended.get('component_job') == {'process_bytes': 16 << 30, 'job_bytes': 24 << 30,
                                                   'flags': 8968, 'active_limit': 2}, 'original suspended identity/Job differs')
    require(receipt.get('final_proof_success') == ['native_result', 'stdout']
            and receipt.get('final_proof_errors') == [{'label': 'operations', 'type': 'ValueError',
                                                       'message': 'mandatory bounded final SHA proof absent'}],
            'original empty-journal failure must remain explicit')
    error = 'authenticated GPU module load failed: A dynamic link library (DLL) initialization routine failed. (os error 1114)'
    require(capture.get('schema') == 'airk.private.cuda-component-result/1' and capture.get('complete') is False
            and capture.get('pid') == 40388 and capture.get('execution_user') == sid
            and capture.get('native_binary_sha256') == old['native_image']['sha256']
            and capture.get('error') == error and capture.get('rows') == [] and 'load_us' not in capture,
            'actual original pre-model1114 capture required')
    require(error in stdout and old['selector'] in stdout
            and 'running 1 test' in stdout and 'test result: FAILED. 0 passed; 1 failed;' in stdout,
            'actual original1114 native stdout required')


ORIGIN_CONTEXT = ('descriptor', 'runtime_root', 'runtime_files', 'ownership_helper_root', 'output_guard', 'output_guard_lock')


def origin_context(plan, old):
    require(all(key in plan and key in old and plan[key] == old[key] for key in ORIGIN_CONTEXT),
            'diagnostic runtime/helper/policy differs from original failed component')


def authenticate_origin(plan, c, b):
    receipt = b.read(b.pinned(plan['origin_component_receipt']))
    old = b.read(b.pinned(plan['origin_component_plan']))
    origin_context(plan, old)
    require(receipt['plan'] == plan['origin_component_plan']
            and receipt['stdout'] == plan['origin_stdout'] and receipt['native_result'] == plan['origin_native_result'],
            'origin output pins differ')
    stdout = b.pinned(plan['origin_stdout']).read_text(encoding='utf-8')
    capture = b.read(b.pinned(plan['origin_native_result']))
    require(b.pinned(plan['origin_operations']).stat().st_size == 0, 'original pre-load journal is not empty')
    verify_origin_facts(receipt, capture, stdout, old, c.SID)


def bounded_guard(plan, c, path=None, created=False, *, deadline, clock=time.monotonic, run=subprocess.run):
    b = c.base()
    b.pinned(plan['checked_guard'])
    b.pinned(plan['powershell'])
    remaining = deadline - clock()
    require(remaining > 0, 'original probe deadline expired before guard')
    code = "$ErrorActionPreference='Stop'; $v=[Console]::In.ReadToEnd()|ConvertFrom-Json; . $v.guard; "
    if path is None:
        code += 'Assert-AirkHumanExecution -ExpectedSid $v.sid | Out-Null'
    else:
        code += ('Assert-AirkCreatedOutput' if created else 'Assert-AirkOutputParent')
        code += ' -Path $v.path -Root $v.root -ExpectedSid $v.sid | Out-Null'
    root = ROOT if path is not None and Path(path).is_relative_to(ROOT) else c.ANCHOR
    payload = {'guard': plan['checked_guard']['path'], 'sid': c.SID, 'path': str(path), 'root': str(root)}
    run([plan['powershell']['path'], '-NoLogo', '-NoProfile', '-NonInteractive', '-Command', code],
        input=json.dumps(payload), text=True, check=True, timeout=min(10, remaining), capture_output=True)
    require(clock() < deadline, 'original probe deadline expired during guard')


def bounded_policy(policy, cpu, out, lock, deadline, powershell, clock=time.monotonic, resolve=shutil.which):
    # The sealed identity_and_parent calls only its own subprocess.run twice.
    # Returned CPU owner plain() only reads path metadata. Never mutate the
    # shared subprocess module or any retained helper bytes.
    original = policy.subprocess
    class Scoped:
        @staticmethod
        def run(argv, **kwargs):
            if argv[0] == 'pwsh':
                observed = resolve('pwsh')
                require(observed is not None and Path(observed).resolve() == Path(powershell).resolve(),
                        'retained policy PowerShell resolution differs from sealed executable')
            else:
                require(argv[0] == 'C:/Windows/System32/whoami.exe', 'unexpected retained policy subprocess')
            remaining = deadline - clock()
            require(remaining > 0, 'original probe deadline expired before protected-policy observation')
            timeout = kwargs.get('timeout')
            require(timeout is None or type(timeout) in (int, float) and math.isfinite(timeout) and timeout > 0,
                    'invalid retained policy timeout')
            kwargs['timeout'] = min(timeout, remaining) if timeout is not None else remaining
            result = original.run(argv, **kwargs)
            require(clock() < deadline, 'original probe deadline expired during protected-policy observation')
            return result
    policy.subprocess = Scoped
    try:
        return policy.identity_and_parent(cpu, out, lock)
    finally:
        policy.subprocess = original


def mkdir_bounded(plan, c, path, deadline):
    bounded_guard(plan, c, path, deadline=deadline)
    Path(path).mkdir()
    bounded_guard(plan, c, path, True, deadline=deadline)


def write_receipt_after_clock(plan, c, path, receipt, original, writer=None, audit=None):
    """Only create-new serialization/fsync and owner audit are outside measured phase."""
    def write():
        with Path(path).open('x', encoding='utf-8', newline='\n') as stream:
            json.dump(receipt, stream, indent=2, allow_nan=False)
            stream.write('\n')
            stream.flush()
            os.fsync(stream.fileno())
    try:
        (writer or write)()
        (audit or (lambda: c.guard(plan, path, True)))()
    except BaseException as error:
        fact = {'type': type(error).__name__, 'message': str(error)}
        receipt['receipt_retention_error'] = fact
        if original is not None:
            original.add_note('Diagnostic receipt retention also failed: ' + json.dumps(fact))
        else:
            raise


def preflight(plan, c, b, fresh):
    require(plan.get('schema') == SCHEMA and plan.get('limits') == LIMITS and plan.get('human_sid') == c.SID,
            'original diagnostic identity/bounds required')
    require(plan['wrapper'] == b.pin(__file__) and plan['component_owner'] == b.pin(ROOT / 'scripts/cuda-component-execution.py')
            and plan['custody_owner'] == b.pin(ROOT / 'scripts/cuda-native-execution.py')
            and plan['contract'] == b.pin(ROOT / 'scripts/cuda-loader-probe/CONTRACT.md'), 'maintained source changed')
    out = Path(plan['output_root'])
    require(out.parent == c.ANCHOR and (not fresh or not out.exists()), 'fresh approved private child required')
    require(plan['runtime_files'] == b.census(plan['runtime_root']) and plan['descriptor'] in plan['runtime_files'],
            'exact retained runtime census differs')
    required = [v for v in plan.values() if isinstance(v, dict) and set(v) == {'path', 'bytes', 'sha256'}]
    required += plan['runtime_files'] + [b.pin(Path(plan['ownership_helper_root']) / name) for name in b.HELPERS]
    lock = b.read(b.pinned(plan['output_guard_lock']))
    for name, key in [('provision-static-onnx-runtime.py', 'cpu_owner_sha256'), ('static-onnx-runtime-lock.json', 'cpu_lock_sha256')]:
        required.append({'path': str(ROOT / 'scripts' / name), 'sha256': lock[key]})
    require(len(plan['input_seals']) <= 16380, 'bounded source closure required')
    for row in required:
        require(plan['input_seals'].get(row['path']) == row['sha256'], 'required original seal missing')
    for path, sha in plan['input_seals'].items():
        require(b.digest(b.regular(path)) == sha, 'sealed diagnostic input changed: ' + path)
    authenticate_origin(plan, c, b)
    rows = roster(plan, b)
    require(plan['load_roster'] == [{'name': n, 'path': str(p)} for n, p in rows], 'literal native load order differs')
    return b.helpers(plan)


def prepare(args):
    c = component()
    b = c.base()
    origin = b.pin(args.component_receipt)
    require(origin['sha256'] == args.component_receipt_sha256, 'reviewed failed component receipt differs')
    receipt = b.read(origin['path'])
    old_pin = receipt['plan']
    old = b.read(b.pinned(old_pin))
    require(receipt['schema'] == 'airk.private.owned-cuda-component/1' and receipt['complete'] is False
            and receipt['owned']['owned_leader_pid'] == 40388 and receipt['owned']['exit_code'] == 101
            and receipt['post_input_drift'] == [] and receipt['final_input_errors'] == [], 'original R1 failed custody differs')
    plan = {key: old[key] for key in ['ownership_helper_root', 'output_guard', 'output_guard_lock',
                                    'descriptor', 'runtime_root', 'runtime_files', 'powershell', 'checked_guard']}
    plan.update(schema=SCHEMA, human_sid=c.SID, limits=LIMITS, wrapper=b.pin(__file__),
                component_owner=b.pin(ROOT / 'scripts/cuda-component-execution.py'),
                custody_owner=b.pin(ROOT / 'scripts/cuda-native-execution.py'),
                contract=b.pin(ROOT / 'scripts/cuda-loader-probe/CONTRACT.md'),
                origin_component_receipt=origin, origin_component_plan=old_pin,
                origin_stdout=receipt['stdout'], origin_native_result=receipt['native_result'],
                origin_operations=b.pin(Path(receipt['native_result']['path']).with_suffix('.operations.jsonl')),
                parent_python=b.pin(sys.executable), driver=b.pin('C:/Windows/System32/nvcuda.dll'),
                output_root=str(Path(args.output_root)), repository_root=str(ROOT))
    plan['load_roster'] = [{'name': name, 'path': str(path)} for name, path in roster(plan, b)]
    required = [v for v in plan.values() if isinstance(v, dict) and set(v) == {'path', 'bytes', 'sha256'}]
    required += plan['runtime_files'] + [b.pin(Path(plan['ownership_helper_root']) / name) for name in b.HELPERS]
    lock = b.read(plan['output_guard_lock']['path'])
    for name, key in [('provision-static-onnx-runtime.py', 'cpu_owner_sha256'), ('static-onnx-runtime-lock.json', 'cpu_lock_sha256')]:
        required.append({'path': str(ROOT / 'scripts' / name), 'sha256': lock[key]})
    plan['input_seals'] = {row['path']: row['sha256'] for row in required}
    preflight(plan, c, b, True)
    c.guard(plan)
    c.write_new(plan, Path(args.plan), plan)
    return b.pin(args.plan)


def exercise(rows, load, get_host, emit):
    """Observation state machine: a load failure is a diagnostic outcome, never a pass."""
    loaded = {}
    host_null = None
    for index, row in enumerate(rows):
        name, path = row['name'], row['path']
        if name == CUDA_PROVIDER:
            require(SHARED_PROVIDER in loaded, 'shared provider must precede CUDA provider')
            pointer = get_host(loaded[SHARED_PROVIDER])
            require(type(pointer) is int and pointer >= 0, 'invalid observed provider host pointer')
            host_null = pointer == 0
            emit({'kind': 'provider_host', 'module': name, 'pointer': pointer, 'is_null': host_null})
        emit({'kind': 'attempt', 'ordinal': index, 'module': name, 'path': path})
        handle, error = load(path)
        require(type(handle) is int and type(error) is int and handle >= 0 and error >= 0,
                'invalid loader observation')
        emit({'kind': 'result', 'ordinal': index, 'module': name, 'loaded': handle != 0, 'handle': handle, 'error': error})
        if not handle:
            require(error != 0, 'failed load has no native error')
            emit({'kind': 'terminal', 'outcome': 'load_failed', 'failed_module': name, 'error': error,
                  'attempted': index + 1, 'loaded': len(loaded), 'provider_host_null': host_null})
            return
        require(error == 0, 'successful load has a stale error')
        loaded[name] = handle
    emit({'kind': 'terminal', 'outcome': 'loaded_all', 'failed_module': None, 'error': 0,
          'attempted': len(rows), 'loaded': len(loaded), 'provider_host_null': host_null})


def validate_events(events, plan, pid):
    require(events and all(e.get('schema') == EVENT for e in events), 'typed complete diagnostic events required')
    first = events[0]
    require(first.get('kind') == 'start' and first.get('pid') == pid and first.get('user_sid') == plan['human_sid']
            and first.get('default_owner_sid') == plan['human_sid'] and first.get('manifest') == plan['descriptor']
            and first.get('model_execution') is False and first.get('cuinit_called') is False
            and first.get('ort_api_called') is False, 'worker identity/scope differs')
    require(len(events) >= 5 and events[1] == {'schema': EVENT, 'kind': 'search_policy',
            'default_flags': DEFAULT_FLAGS, 'load_flags': LOAD_FLAGS}, 'exact native search policy required')
    cursor = 2
    supplied = []
    def emit(expected):
        nonlocal cursor
        require(cursor < len(events) and events[cursor] == dict(schema=EVENT, **expected), 'diagnostic order/content differs')
        cursor += 1
    def host(_):
        require(cursor < len(events) and events[cursor].get('kind') == 'provider_host', 'host observation missing')
        return events[cursor].get('pointer')
    def load(_):
        require(cursor < len(events) and events[cursor].get('kind') == 'result', 'module result missing')
        value = events[cursor]
        supplied.append(value)
        return value.get('handle'), value.get('error')
    exercise(plan['load_roster'], load, host, emit)
    require(cursor == len(events), 'unexpected events after terminal result')
    return events[-1]


def worker(args):
    c = component()
    b = c.base()
    pp = b.pin(args.plan)
    require(pp['sha256'] == args.plan_sha256, 'worker plan differs')
    plan = b.read(pp['path'])
    require(plan['wrapper'] == b.pin(__file__) and plan['parent_python'] == b.pin(sys.executable), 'worker code/interpreter differs')
    parent, _ = preflight(plan, c, b, False)
    require(parent.worker.windows_sid() == c.SID and c.token_owner(ctypes.c_void_p(-1)) == c.SID, 'worker token differs')
    from ctypes import wintypes as w
    api = ctypes.WinDLL('kernel32', use_last_error=True)
    api.SetDefaultDllDirectories.argtypes = [w.DWORD]
    api.SetDefaultDllDirectories.restype = w.BOOL
    api.LoadLibraryExW.argtypes = [w.LPCWSTR, w.HANDLE, w.DWORD]
    api.LoadLibraryExW.restype = w.HMODULE
    api.GetProcAddress.argtypes = [w.HMODULE, ctypes.c_char_p]
    api.GetProcAddress.restype = ctypes.c_void_p
    def emit(value):
        print(json.dumps(dict(schema=EVENT, **value), allow_nan=False), flush=True)
        os.fsync(sys.stdout.fileno())
    emit({'kind': 'start', 'pid': os.getpid(), 'user_sid': c.SID, 'default_owner_sid': c.SID,
          'manifest': plan['descriptor'], 'model_execution': False, 'cuinit_called': False, 'ort_api_called': False})
    if not api.SetDefaultDllDirectories(DEFAULT_FLAGS):
        raise ctypes.WinError(ctypes.get_last_error())
    emit({'kind': 'search_policy', 'default_flags': DEFAULT_FLAGS, 'load_flags': LOAD_FLAGS})
    def load(path):
        ctypes.set_last_error(0)
        handle = api.LoadLibraryExW(path, None, LOAD_FLAGS)
        return int(handle or 0), 0 if handle else ctypes.get_last_error()
    def host(shared):
        address = api.GetProcAddress(shared, b'Provider_GetHost')
        require(address, 'Provider_GetHost export missing')
        return int(ctypes.CFUNCTYPE(ctypes.c_void_p)(address)() or 0)
    # Raw handles are deliberately retained through process exit. No FreeLibrary,
    # Provider_SetHost, driver entry point, ORT API or model call is made.
    exercise(plan['load_roster'], load, host, emit)


def retain_log(receipt, log, hash_file, audit):
    """Always attempt final owner/hash proof even if another final input failed."""
    try:
        audit(log)
        final = hash_file(log)
        require(isinstance(final, dict) and set(final) == {'path', 'bytes', 'sha256'}
                and final['path'] == str(log) and type(final['bytes']) is int and 0 < final['bytes'] <= 64 << 20
                and isinstance(final['sha256'], str) and len(final['sha256']) == 64
                and all(x in '0123456789abcdef' for x in final['sha256'])
                and ('stdout' not in receipt or final == receipt['stdout']), 'mandatory final diagnostic log proof differs')
        receipt['stdout'] = final
    except BaseException as error:
        receipt['final_proof_errors'].append({'message': str(error)})


def execute(args):
    started = time.monotonic()
    utc = datetime.datetime.now(datetime.timezone.utc)
    c = component()
    b = c.base()
    end_utc = min(utc + datetime.timedelta(seconds=120), b.CAMPAIGN)
    deadline = started + (end_utc - utc).total_seconds()
    pp = b.pin(args.plan)
    require(pp['sha256'] == args.plan_sha256, 'reviewed plan differs')
    plan = b.read(pp['path'])
    require(plan['parent_python'] == b.pin(sys.executable), 'parent interpreter differs')
    bounded_guard(plan, c, deadline=deadline)
    require(c.token_owner(ctypes.c_void_p(-1)) == c.SID, 'parent default owner differs')
    parent, owner = preflight(plan, c, b, True)
    out = Path(plan['output_root'])
    policy = b.load(plan['output_guard']['path'], 'loader_existing_output_policy')
    lock = b.read(plan['output_guard_lock']['path'])
    cpu = policy.cpu_owner(lock)
    local_lock = json.loads(json.dumps(lock))
    local_lock['build_path']['fresh_child'] = out.name
    observed_policy = bounded_policy(policy, cpu, out, local_lock, deadline, plan['powershell']['path'])
    mkdir_bounded(plan, c, out, deadline)
    mkdir_bounded(plan, c, out / 'temp', deadline)
    log, initial = out / 'events-and-stderr.log', out / 'stdin.txt'
    for path in [log, initial, out / 'execution.json']:
        bounded_guard(plan, c, path, deadline=deadline)
    with initial.open('xb'):
        pass
    bounded_guard(plan, c, initial, True, deadline=deadline)
    env = dict(os.environ)
    for key in list(env):
        if key.upper().startswith(('AIRK_', 'ORT_', 'PYTHON', 'CUDA_', 'CUDNN_')):
            env.pop(key)
    env.update(PATH='C:/Windows/System32;C:/Windows', TEMP=str(out / 'temp'), TMP=str(out / 'temp'), TMPDIR=str(out / 'temp'))
    argv = [plan['parent_python']['path'], '-B', '-S', plan['wrapper']['path'], '--worker', '--plan', pp['path'],
            '--plan-sha256', pp['sha256'], '--approved-script-sha256', plan['wrapper']['sha256']]
    receipt = {'schema': 'airk.private.owned-cuda-loader-probe/1', 'plan': pp, 'command': argv,
               'started_utc': utc.isoformat(), 'original_deadline': end_utc.isoformat(), 'original_deadline_monotonic': deadline,
               'execution_user_sid': c.SID, 'parent_pid': os.getpid(), 'parent_python': plan['parent_python'],
               'output_policy': observed_policy, 'input_seals': dict(plan['input_seals'], **{pp['path']: pp['sha256']}),
               'diagnostic_complete': False, 'failure': None, 'model_execution': False, 'qualification': False,
               'whole_phase_deadline_not_refreshed': True, 'post_input_drift': []}
    try:
        def authenticate(api, job, info, child_deadline):
            proof = parent.suspended_authentication(api, info, deadline, child_deadline, c.SID, plan['parent_python'], owner.image_proof)
            require(c.token_owner(info.process) == c.SID, 'suspended worker default owner differs')
            proof['default_owner_sid'] = c.SID
            proof['component_job'] = c.configure_job(api, job)
            require(time.monotonic() < deadline, 'original probe deadline expired before resume')
            return proof
        remaining = deadline - time.monotonic()
        require(remaining > 0, 'original probe cutoff expired before launch')
        with initial.open('rb') as stdin, log.open('xb') as stdout:
            receipt['owned'] = owner.run_windows_owned_training(argv, ROOT, stdout, remaining, env, stdin,
                16 << 30, before_resume=authenticate, cleanup_seconds=5, absolute_deadline=deadline)
        require(parent.settled(receipt['owned']) and c.finite(receipt['owned'].get('peak_host_committed_bytes'), 1, 24 << 30),
                'diagnostic worker failed or did not naturally settle')
        bounded_guard(plan, c, log, True, deadline=deadline)
        require(log.stat().st_size <= 64 << 20, 'original log cap exceeded')
        receipt['stdout'] = b.pin(log)
        events = [json.loads(line) for line in log.read_text(encoding='utf-8').splitlines()]
        receipt['observed_loader_outcome'] = validate_events(events, plan, receipt['owned']['owned_leader_pid'])
        receipt['diagnostic_complete'] = True
    except BaseException as error:
        receipt['failure'] = {'type': type(error).__name__, 'message': str(error)}
        if hasattr(error, 'owned_cleanup'):
            receipt['actual_cleanup_failure'] = error.owned_cleanup
        raise
    finally:
        original = sys.exc_info()[1]
        receipt['final_proof_errors'] = []
        for path, sha in receipt['input_seals'].items():
            try:
                require(b.digest(b.regular(path)) == sha, 'input changed')
            except BaseException as error:
                receipt['post_input_drift'].append(path)
                receipt['final_proof_errors'].append({'path': path, 'message': str(error)})
        try:
            require(b.census(plan['runtime_root']) == plan['runtime_files'], 'runtime roster changed')
            for path in [out, *out.rglob('*')]:
                bounded_guard(plan, c, path, True, deadline=deadline)
        except BaseException as error:
            receipt['final_proof_errors'].append({'message': str(error)})
        retain_log(receipt, log, b.pin, lambda path: bounded_guard(plan, c, path, True, deadline=deadline))
        try:
            bounded_guard(plan, c, out / 'execution.json', deadline=deadline)
        except BaseException as error:
            receipt['final_proof_errors'].append({'message': 'final receipt-path preaudit: ' + str(error)})
        if receipt['final_proof_errors']:
            receipt['diagnostic_complete'] = False
            receipt['failure'] = receipt.get('failure') or receipt['final_proof_errors'][0]
        receipt['timing_scope'] = 'Through final input/output proofs and receipt-path preaudit; excludes subsequent create-new receipt serialization/fsync and owner audit'
        receipt['finished_utc'] = datetime.datetime.now(datetime.timezone.utc).isoformat()
        now = time.monotonic()
        receipt['elapsed_seconds'] = now - started
        receipt['whole_phase_expired'] = now >= deadline
        if receipt['whole_phase_expired']:
            receipt['diagnostic_complete'] = False
            receipt['failure'] = receipt.get('failure') or {'message': 'original probe cutoff expired'}
        write_receipt_after_clock(plan, c, out / 'execution.json', receipt, original)
        if original is None and not receipt['diagnostic_complete']:
            raise ValueError(receipt['failure'])
    return receipt


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument('--prepare', action='store_true')
    mode.add_argument('--execute-reviewed-probe', action='store_true')
    mode.add_argument('--worker', action='store_true')
    parser.add_argument('--approved-script-sha256', required=True)
    parser.add_argument('--plan', type=Path, required=True)
    parser.add_argument('--plan-sha256')
    parser.add_argument('--component-receipt', type=Path)
    parser.add_argument('--component-receipt-sha256')
    parser.add_argument('--output-root', type=Path)
    args = parser.parse_args()
    require(component().base().digest(__file__) == args.approved_script_sha256, 'reviewed diagnostic source SHA required')
    if args.worker:
        worker(args)
    else:
        value = prepare(args) if args.prepare else execute(args)
        print(json.dumps(value if args.prepare else {'diagnostic_complete': value['diagnostic_complete'],
                                                    'outcome': value['observed_loader_outcome']}))


if __name__ == '__main__':
    main()
