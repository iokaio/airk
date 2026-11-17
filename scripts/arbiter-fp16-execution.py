# SPDX-License-Identifier: Apache-2.0
"""Reviewed single-model FP16 conversion custody; no inference or admission authority."""
import argparse
import ctypes
from datetime import datetime, timedelta, timezone
import hashlib
import importlib.util
import json
import math
import os
from pathlib import Path
import subprocess
import sys
import time

SID = 'S-1-5-21-3678370870-3858642417-826968105-1002'
SCHEMA = 'airk.private.fp16-conversion-plan/1'
LIMITS = {'wall_seconds': 600, 'host_committed_bytes': 8 << 30, 'log_bytes': 64 << 20, 'cleanup_seconds': 5}
HELPERS = {'reference_owned.py': '2d7a31a64eaba745e32ed83adfbeedb75983ab6f9b5c4cc82714e9371c1895a1',
           'reference_parent_no_site_r2.py': 'fe4cc990649d92e041b79cccb3fd7ef1129979a26432b7f02b2d8d28b586ed8d',
           'reference_worker.py': '9c04be28ef06c0a06b8e2cffd4c274348ff141046114cc493d725ce527b694c7'}
OUTPUT_NAMES = {'model.onnx', 'weights.bin', 'export-manifest.private.json'}
PROOFS = ('graph', 'weights', 'manifest', 'stdout', 'worker', 'inputs')


def require(value, message):
    if not value:
        raise ValueError(message)


def regular(path):
    path = Path(path)
    require(path.is_absolute() and path.resolve(strict=True) == path, 'canonical absolute input required')
    for part in [path, *path.parents]:
        value = part.lstat()
        require(not part.is_symlink() and not getattr(value, 'st_file_attributes', 0) & 0x400, 'aliased input refused')
    require(path.is_file(), 'regular file required')
    return path


def pin(path, check=lambda: None):
    path = regular(path)
    value = hashlib.sha256()
    with path.open('rb') as source:
        while chunk := source.read(4 << 20):
            check()
            value.update(chunk)
    check()
    return {'path': str(path), 'bytes': path.stat().st_size, 'sha256': value.hexdigest()}


def pinned(row, check=lambda: None):
    require(set(row) == {'path', 'bytes', 'sha256'} and pin(row['path'], check) == row, 'complete input byte pin differs')
    return Path(row['path'])


def read(path):
    path = regular(path)
    require(path.stat().st_size <= 32 << 20, 'bounded JSON required')
    return json.loads(path.read_bytes())


def read_observed(path):
    path = regular(path)
    require(path.stat().st_size <= 32 << 20, 'bounded output JSON required')
    with path.open('rb') as source:
        raw = source.read((32 << 20) + 1)
    require(len(raw) <= 32 << 20, 'output JSON grew beyond bound')
    row = {'path': str(path), 'bytes': len(raw), 'sha256': hashlib.sha256(raw).hexdigest()}
    return json.loads(raw), row

def load(path, name):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


def file_roster(root):
    root = Path(root)
    require(root.is_absolute() and root.resolve(strict=True) == root and root.is_dir(), 'canonical runtime root required')
    result = set()
    stack = [root]
    while stack:
        directory = stack.pop()
        require(not directory.is_symlink() and not directory.is_junction(), 'runtime directory alias')
        for child in directory.iterdir():
            require(not child.is_symlink() and not child.is_junction(), 'runtime member alias')
            if child.is_dir():
                if child.name != '__pycache__':
                    stack.append(child)
            else:
                result.add(str(regular(child)))
    return result


def validate_plan(plan):
    require(plan.get('schema') == SCHEMA and plan.get('human_sid') == SID and plan.get('limits') == LIMITS, 'fixed conversion schema/identity/bounds required')
    require(plan.get('model') in ('focus', 'compact', 'large'), 'exact single selected model required')
    require(set(plan['prerequisite_evidence']) == {'first_fp32', 'native_g5'}, 'reviewed prerequisite evidence pins required; this owner cannot grant admission')
    require(type(plan['expected_weights_bytes']) is int and plan['expected_weights_bytes'] > 0, 'exact expected sidecar size required')
    require(plan['expected_attention_count'] == (36 if plan['model'] == 'large' else 28), 'exact model attention count required')
    require(set(plan['helpers']) == set(HELPERS), 'complete original helper roster required')
    for name, row in plan['helpers'].items():
        require(Path(row['path']).name == name and row['sha256'] == HELPERS[name], 'original helper identity differs')
    paths = [row['path'] for row in plan['input_files']]
    require(len(paths) == len(set(paths)) and len(paths) <= 50000, 'unique bounded complete input census required')
    by_path = {row['path']: row for row in plan['input_files']}
    required = [plan[key] for key in ('python', 'powershell', 'wrapper', 'guard', 'exporter', 'source_graph', 'source_closure', 'runtime_manifest')]
    required += list(plan['helpers'].values()) + list(plan['prerequisite_evidence'].values())
    require(all(by_path.get(row['path']) == row for row in required), 'required source absent from input census')
    require(Path(plan['wrapper']['path']) == Path(__file__).resolve(), 'actual maintained wrapper differs')
    root = Path(plan['repository_root']); output = Path(plan['output_root'])
    require(root.is_absolute() and root.resolve(strict=True) == root and output.is_absolute() and output.resolve(strict=False) == output and output.is_relative_to(root / 'artifacts') and output != root / 'artifacts', 'fresh canonical repository artifact output required')
    require(not output.exists() and output.parent.is_dir(), 'fresh output with existing reviewed parent required')
    require(Path(plan['cache_prefix']).is_absolute() and not Path(plan['cache_prefix']).exists(), 'absent nonwriting Python cache prefix required')
    return by_path


def authenticate(plan, check=lambda: None):
    by_path = validate_plan(plan)
    for row in plan['input_files']:
        pinned(row, check)
    runtime = read(plan['runtime_manifest']['path'])
    require(set(runtime) == {'roots', 'files', 'import_paths'}, 'complete explicit runtime closure required')
    actual = set().union(*(file_roster(root) for root in runtime['roots']))
    require(actual == set(runtime['files']), 'runtime complete file roster changed')
    for path, row in runtime['files'].items():
        require(path == row['path'] and by_path.get(path) == row, 'runtime member omitted from input census')
    for directory in runtime['import_paths']:
        require(Path(directory).is_dir() and Path(directory).resolve(strict=True) == Path(directory) and any(Path(directory).is_relative_to(Path(root)) for root in runtime['roots']), 'canonical import path outside sealed runtime roots')
    require(not any(Path(plan['output_root']).is_relative_to(Path(root)) for root in runtime['roots']), 'output cannot mutate sealed runtime roots')
    closure = read(plan['source_closure']['path'])
    parent = Path(plan['source_graph']['path']).parent
    for name, value in closure.items():
        require(name not in ('', '.', '..') and Path(name).name == name and '/' not in name and '\\' not in name and ':' not in name, 'unsafe source sidecar name')
        require(by_path.get(str(parent / name)) == dict(value, path=str(parent / name)), 'source sidecar omitted from input census')
    check()
    return runtime


def loaded_runtime(files):
    observed = {}
    for module in list(sys.modules.values()):
        value = getattr(module, '__file__', None)
        if not value or value.startswith('<'):
            continue
        path = Path(value).resolve(strict=True)
        if path.suffix == '.pyc':
            raise ValueError('cached module import refused')
        row = files.get(str(path))
        require(row is not None and pin(path) == row, 'actual imported module outside sealed closure: ' + str(path))
        observed[str(path)] = row['sha256']
    return observed


def windows_owner(path=None, process=None):
    """Read-only SID/default-token-owner proof; never changes security metadata."""
    require(os.name == 'nt', 'Windows owner verification required')
    from ctypes import wintypes as w
    adv = ctypes.WinDLL('advapi32', use_last_error=True)
    kernel = ctypes.WinDLL('kernel32', use_last_error=True)
    adv.ConvertSidToStringSidW.argtypes = [ctypes.c_void_p, ctypes.POINTER(w.LPWSTR)]
    adv.ConvertSidToStringSidW.restype = w.BOOL
    kernel.LocalFree.argtypes = [ctypes.c_void_p]; kernel.LocalFree.restype = ctypes.c_void_p
    def sid_text(pointer):
        text = w.LPWSTR()
        if not adv.ConvertSidToStringSidW(pointer, ctypes.byref(text)):
            raise ctypes.WinError(ctypes.get_last_error())
        try:
            return text.value
        finally:
            kernel.LocalFree(ctypes.cast(text, ctypes.c_void_p))
    if path is not None:
        owner = ctypes.c_void_p(); descriptor = ctypes.c_void_p()
        adv.GetNamedSecurityInfoW.argtypes = [w.LPWSTR, ctypes.c_int, w.DWORD, ctypes.POINTER(ctypes.c_void_p), ctypes.c_void_p, ctypes.c_void_p, ctypes.c_void_p, ctypes.POINTER(ctypes.c_void_p)]
        adv.GetNamedSecurityInfoW.restype = w.DWORD
        code = adv.GetNamedSecurityInfoW(str(path), 1, 1, ctypes.byref(owner), None, None, None, ctypes.byref(descriptor))
        if code:
            raise ctypes.WinError(code)
        try:
            require(sid_text(owner) == SID, 'foreign output owner')
            return SID
        finally:
            kernel.LocalFree(descriptor)
    kernel.GetCurrentProcess.restype = w.HANDLE
    kernel.CloseHandle.argtypes = [w.HANDLE]; kernel.CloseHandle.restype = w.BOOL
    adv.OpenProcessToken.argtypes = [w.HANDLE, w.DWORD, ctypes.POINTER(w.HANDLE)]; adv.OpenProcessToken.restype = w.BOOL
    adv.GetTokenInformation.argtypes = [w.HANDLE, ctypes.c_int, ctypes.c_void_p, w.DWORD, ctypes.POINTER(w.DWORD)]; adv.GetTokenInformation.restype = w.BOOL
    token = w.HANDLE()
    if not adv.OpenProcessToken(process or kernel.GetCurrentProcess(), 8, ctypes.byref(token)):
        raise ctypes.WinError(ctypes.get_last_error())
    try:
        result = []
        for kind in (1, 4):
            size = w.DWORD(); adv.GetTokenInformation(token, kind, None, 0, ctypes.byref(size))
            buffer = ctypes.create_string_buffer(size.value)
            if not adv.GetTokenInformation(token, kind, buffer, size, ctypes.byref(size)):
                raise ctypes.WinError(ctypes.get_last_error())
            result.append(sid_text(ctypes.cast(buffer, ctypes.POINTER(ctypes.c_void_p))[0]))
        require(result == [SID, SID], 'effective/default token owner differs')
        return {'user_sid': result[0], 'owner_sid': result[1]}
    finally:
        kernel.CloseHandle(token)


def guard(plan, path, created=False, *, deadline, clock=time.monotonic, runner=subprocess.run):
    pinned(plan['guard']); pinned(plan['powershell'])
    environment = dict(os.environ, AIRK_GUARD_FILE=plan['guard']['path'], AIRK_GUARD_PATH=str(path), AIRK_GUARD_ROOT=plan['repository_root'], AIRK_GUARD_SID=SID)
    function = 'Assert-AirkCreatedOutput' if created else 'Assert-AirkOutputParent'
    script = "$ErrorActionPreference='Stop'; . $env:AIRK_GUARD_FILE; " + function + ' -Path $env:AIRK_GUARD_PATH -Root $env:AIRK_GUARD_ROOT -ExpectedSid $env:AIRK_GUARD_SID | ConvertTo-Json -Depth 4'
    remaining = deadline - clock()
    if not math.isfinite(remaining) or remaining <= 0:
        raise TimeoutError('original conversion deadline expired before guard')
    result = runner([plan['powershell']['path'], '-NoProfile', '-NonInteractive', '-Command', script], env=environment, capture_output=True, text=True, check=True, timeout=remaining, creationflags=getattr(subprocess, 'CREATE_NO_WINDOW', 0))
    observation = json.loads(result.stdout)
    if clock() >= deadline:
        raise TimeoutError('original conversion deadline expired during guard')
    return observation


def output_access(event, arguments, output, owner=windows_owner):
    if event in ('subprocess.Popen', 'os.system', 'os.posix_spawn', 'socket.connect'):
        raise ValueError('conversion worker cannot spawn or connect')
    if event not in ('open', 'os.mkdir'):
        return
    raw = arguments[0]
    if isinstance(raw, int):
        return
    path = Path(raw).resolve(strict=False)
    writing = event == 'os.mkdir' or (len(arguments) > 2 and bool(arguments[2] & (os.O_WRONLY | os.O_RDWR | os.O_CREAT | os.O_TRUNC | os.O_APPEND)))
    if writing:
        require(path.is_relative_to(output), 'worker write outside fresh output refused')
        owner(path.parent)
    if path.is_relative_to(output) and path.exists():
        require(not path.is_symlink() and not path.is_junction(), 'output alias refused')
        owner(path)


def validate_manifest(manifest, plan, graph, weights):
    require(manifest.get('qualification') is False and manifest.get('native_execution') is False, 'conversion cannot claim qualification')
    require(weights['bytes'] == plan['expected_weights_bytes'], 'exact converted sidecar size differs')
    require(0 < graph['bytes'] <= 16 << 20, 'converted graph exceeds original metadata bound')
    require(manifest['graph_sha256'] == graph['sha256'], 'actual graph bytes differ')
    expected_op = 'MultiHeadAttention' if plan['model'] == 'focus' else 'GroupQueryAttention'
    expected = [{'name': f'/airk_g5/attention/{i}/efficient', 'operator': expected_op, 'domain': 'com.microsoft'} for i in range(plan['expected_attention_count'])]
    require(manifest['attention_nodes'] == expected, 'actual emitted attention roster differs')


def finish(receipt, started, deadline, callbacks, clock=time.monotonic, utc_clock=lambda: datetime.now(timezone.utc)):
    """Attempt every required proof, then sample the original clock last."""
    receipt['final_proofs'] = {}; receipt['final_errors'] = []
    for name in PROOFS:
        try:
            row = callbacks[name]()
            require(isinstance(row, dict) and set(row) == {'path', 'bytes', 'sha256'} and Path(row['path']).is_absolute() and type(row['bytes']) is int and row['bytes'] >= 0 and isinstance(row['sha256'], str) and len(row['sha256']) == 64 and set(row['sha256']) <= set('0123456789abcdef'), 'missing exact final proof row')
            require(name not in receipt.get('observed_proofs', {}) or receipt['observed_proofs'][name] == row, 'final proof changed after validation')
            receipt['final_proofs'][name] = row
        except BaseException as error:
            receipt['final_errors'].append({'proof': name, 'type': type(error).__name__, 'message': str(error)})
    if receipt.get('failure') is not None or receipt['final_errors'] or set(receipt['final_proofs']) != set(PROOFS):
        receipt['complete'] = False
    receipt['ended_utc'] = utc_clock().isoformat()
    now = clock()
    receipt['elapsed_seconds'] = now - started
    receipt['whole_phase_expired'] = now >= deadline
    if not math.isfinite(now) or not 0 <= receipt['elapsed_seconds'] < LIMITS['wall_seconds'] or receipt['whole_phase_expired']:
        receipt['complete'] = False
    return receipt


def worker(plan, deadline):
    require(sys.flags.no_site == 1 and sys.dont_write_bytecode and sys.pycache_prefix == plan['cache_prefix'] and not Path(sys.pycache_prefix).exists(), 'actual no-site nonwriting bootstrap differs')
    require(os.environ.get('AIRK_FP16_CHILD') == '1' and plan['schema'] == SCHEMA and plan['limits'] == LIMITS, 'owned conversion child gate required')
    windows_owner()
    output = Path(plan['output_root']); require(output.is_dir(), 'parent-owned fresh output required')
    runtime = read(plan['runtime_manifest']['path'])
    sys.path[:] = runtime['import_paths']
    files = {row['path']: row for row in plan['input_files']}
    def check():
        require(time.monotonic() < deadline, 'original conversion deadline expired')
    sys.addaudithook(lambda event, args: output_access(event, args, output))
    import numpy
    import onnx
    require(numpy.__version__ == '2.3.2' and onnx.__version__ == '1.19.0', 'original conversion dependency versions differ')
    loaded_runtime(files)
    exporter = load(plan['exporter']['path'], 'reviewed_fp16_exporter')
    candidate = output / 'candidate'
    original_checker = onnx.checker.check_model
    def checked_model(path, **kwargs):
        for artifact in (candidate, candidate / 'weights.bin', candidate / 'model.onnx'):
            windows_owner(artifact)
        return original_checker(path, **kwargs)
    onnx.checker.check_model = checked_model
    manifest = exporter.export_after_owner_admission(plan['source_graph']['path'], plan['model'], read(plan['source_closure']['path']), candidate, onnx, numpy, deadline)
    graph = onnx.load_model(str(candidate / 'model.onnx'), load_external_data=False)
    manifest['attention_nodes'] = [{'name': node.name, 'operator': node.op_type, 'domain': node.domain} for node in graph.graph.node if node.op_type in ('MultiHeadAttention', 'GroupQueryAttention')]
    manifest['loaded_runtime'] = loaded_runtime(files)
    check()
    with (candidate / 'export-manifest.private.json').open('x', encoding='utf-8') as target:
        json.dump(manifest, target, indent=2); target.write('\n')
    for artifact in candidate.iterdir():
        windows_owner(artifact)
    with (output / 'worker.json').open('x', encoding='utf-8') as target:
        json.dump({'complete': True, 'model_inference': False, 'qualification': False, 'pid': os.getpid(), 'token': windows_owner(), 'loaded_runtime': loaded_runtime(files)}, target)
    check()


def run(path, expected):
    started_utc = datetime.now(timezone.utc)
    started = time.monotonic(); deadline = started + LIMITS['wall_seconds']
    def check():
        require(time.monotonic() < deadline, 'original conversion deadline expired')
    require(pin(path)['sha256'] == expected, 'reviewed plan hash differs')
    plan = read(path); runtime = authenticate(plan, check)
    require(pin(sys.executable) == plan['python'] and sys.flags.no_site == 1 and sys.dont_write_bytecode and sys.pycache_prefix == plan['cache_prefix'] and not Path(sys.pycache_prefix).exists(), 'actual parent interpreter/bootstrap differs')
    windows_owner()
    output = Path(plan['output_root']); policy = guard(plan, output, deadline=deadline)
    for name, row in plan['helpers'].items():
        pinned(row, check)
    sys.path.insert(0, str(Path(plan['helpers']['reference_owned.py']['path']).parent))
    parent = load(plan['helpers']['reference_parent_no_site_r2.py']['path'], 'fp16_conversion_parent')
    owner = load(plan['helpers']['reference_owned.py']['path'], 'fp16_conversion_owner')
    require(pin(parent.worker.__file__) == plan['helpers']['reference_worker.py'], 'ownership dependency shadowed')
    files = {row['path']: row for row in plan['input_files']}
    loaded_runtime(files)
    output.mkdir(); guard(plan, output, True, deadline=deadline)
    log = output / 'stdout.log'; worker_result = output / 'worker.json'; candidate = output / 'candidate'
    argv = [plan['python']['path'], '-B', '-S', '-X', 'pycache_prefix=' + plan['cache_prefix'], plan['wrapper']['path'], '--worker', '--plan', str(path), '--plan-sha256', expected, '--deadline', repr(deadline)]
    environment = {key: value for key, value in os.environ.items() if not key.upper().startswith(('AIRK_', 'ORT_', 'PYTHON', 'CUDA_', 'CUDNN_'))}
    environment.update(AIRK_FP16_CHILD='1', PYTHONDONTWRITEBYTECODE='1', CUDA_VISIBLE_DEVICES='', PYTHONPYCACHEPREFIX=plan['cache_prefix'], TEMP=str(output), TMP=str(output), TMPDIR=str(output))
    receipt = {'started_utc': started_utc.isoformat(), 'original_deadline_utc': (started_utc + timedelta(seconds=LIMITS['wall_seconds'])).isoformat(), 'original_deadline_monotonic': deadline, 'timing_scope': 'Includes every final proof and input census; excludes final receipt encoding/write and its owner audit only', 'schema': 'airk.private.fp16-conversion-execution/1', 'plan': pin(path), 'command': argv, 'limits': LIMITS, 'output_policy': policy, 'complete': False, 'failure': None, 'qualification': False, 'activation': False, 'model_inference': False}
    try:
        def suspended(api, job, info, child_deadline):
            token = windows_owner(process=info.process)
            proof = parent.suspended_authentication(api, info, deadline, child_deadline, SID, plan['python'], owner.image_proof)
            return dict(proof, token=token)
        guard(plan, log, deadline=deadline)
        guard(plan, output / 'execution.json', deadline=deadline)
        with log.open('xb') as target, open(os.devnull, 'rb') as stdin:
            receipt['owned'] = owner.run_windows_owned_training(argv, Path(plan['repository_root']), target, deadline - time.monotonic(), environment, stdin, 8 << 30, before_resume=suspended, absolute_deadline=deadline)
        require(parent.settled(receipt['owned']), 'original child did not naturally settle')
        require(receipt['owned']['peak_host_committed_bytes'] <= 8 << 30, 'original memory bound exceeded')
        for artifact in [candidate, *candidate.iterdir(), worker_result, log]:
            windows_owner(artifact)
        require({item.name for item in candidate.iterdir()} == OUTPUT_NAMES, 'unexpected output roster')
        graph = pin(candidate / 'model.onnx', check); weights = pin(candidate / 'weights.bin', check)
        manifest, manifest_pin = read_observed(candidate / 'export-manifest.private.json')
        validate_manifest(manifest, plan, graph, weights)
        require(log.stat().st_size <= LIMITS['log_bytes'], 'original log bound exceeded')
        result, worker_pin = read_observed(worker_result)
        require(result['token'] == {'user_sid': SID, 'owner_sid': SID}, 'actual child token proof differs')
        require(result['complete'] is True and result['model_inference'] is False and result['qualification'] is False and result['pid'] == receipt['owned']['owned_leader_pid'], 'actual worker result differs')
        receipt['observed_proofs'] = {'graph': graph, 'weights': weights, 'manifest': manifest_pin, 'worker': worker_pin, 'stdout': pin(log, check)}
        receipt['complete'] = True
    except BaseException as error:
        receipt['failure'] = {'type': type(error).__name__, 'message': str(error)}
        if hasattr(error, 'owned_cleanup'):
            receipt['owned_cleanup'] = error.owned_cleanup
    finally:
        paths = {'graph': candidate / 'model.onnx', 'weights': candidate / 'weights.bin', 'manifest': candidate / 'export-manifest.private.json', 'stdout': log, 'worker': worker_result}
        def proof(path):
            windows_owner(path)
            return pin(path)
        def input_proof():
            for row in plan['input_files']:
                pinned(row)
            require(set().union(*(file_roster(root) for root in runtime['roots'])) == set(runtime['files']), 'runtime final census drift')
            row = pin(path)
            require(row['sha256'] == expected, 'plan changed')
            return row
        callbacks = {key: lambda path=value: proof(path) for key, value in paths.items()}
        callbacks['inputs'] = input_proof
        finish(receipt, started, deadline, callbacks)
        with (output / 'execution.json').open('x', encoding='utf-8') as target:
            json.dump(receipt, target, indent=2); target.write('\n')
        windows_owner(output / 'execution.json')
    require(receipt['complete'], 'conversion nonpass; original evidence retained')
    return receipt


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument('--inspect-plan', action='store_true')
    mode.add_argument('--execute-reviewed-conversion', action='store_true')
    mode.add_argument('--worker', action='store_true')
    parser.add_argument('--plan', type=Path, required=True)
    parser.add_argument('--plan-sha256', required=True)
    parser.add_argument('--deadline', type=float)
    args = parser.parse_args()
    if not args.execute_reviewed_conversion:
        require(pin(args.plan)['sha256'] == args.plan_sha256, 'reviewed plan differs')
    if args.worker:
        require(args.deadline is not None and math.isfinite(args.deadline), 'original parent deadline required')
        worker(read(args.plan), args.deadline)
    elif args.inspect_plan:
        authenticate(read(args.plan))
        print(json.dumps({'inputs_authenticated': True, 'prerequisites_require_root_review': True, 'conversion_executed': False, 'qualification': False}))
    else:
        result = run(args.plan, args.plan_sha256)
        print(json.dumps({'complete': result['complete'], 'elapsed_seconds': result['elapsed_seconds'], 'qualification': False}))


if __name__ == '__main__':
    main()
