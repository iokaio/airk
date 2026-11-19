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
    required = [plan[key] for key in ('python', 'powershell', 'wrapper', 'guard', 'exporter', 'source_graph', 'source_closure', 'runtime_manifest', 'source_plan', 'runtime_baseline', 'prerequisites_manifest')]
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
    authenticate_prerequisites(plan, by_path, check)
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
            authenticate_prerequisites(plan, {row['path']: row for row in plan['input_files']})
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


SOURCE_PLAN_SHA = 'cdf57c6c86b490cf2b6656fa8c48fbb4ff3b6ccb4d5c5be9705a71a2866b735c'
RUNTIME_BASELINE_SHA = '23f9fbfa770d18ee2549dbd6c1b553e204ab8e387ba51b92b620dd24c10189cb'
G5_TESTS = frozenset(
    'arbiter::runtime::cuda::fp16::tests::' + name for name in (
        'inherited_exact_settings_pass_and_late_setter_or_drift_refuses',
        'fp32_policy_remains_distinct_from_each_model_fp16_policy',
        'context_boundaries_and_exact_original_mask_positions_refuse_unsupported_input',
        'focus_all_live_and_extreme_left_pad_pass_without_renumbering',
        'half_initializer_bit_loading_preserves_half_values_without_rounding',
        'actual_final_placement_needs_exact_complete_attention_roster',
        'qualified_product_math_does_not_force_attention_debug_logging')) | frozenset(
    'arbiter::external_data::g5::tests::' + name for name in (
        'three_literal_selected_rosters_admit_actual_half_boundaries',
        'wrong_scale_grouping_causal_and_bias_refuse_before_session',
        'wrong_half_cast_and_missing_layer_refuse', 'focus_bias_and_wrong_head_count_refuse',
        'all_f32_and_overlap_are_not_half_weight_closure',
        'large_half_metadata_is_additive_and_never_allocates_tensor_bytes',
        'unknown_quantized_half_closure_refuses',
        'focus_unwritten_padded_queries_require_explicit_sanitization')) | frozenset(
    'arbiter::runtime::cpu_source_transition::tests::' + name for name in (
        'closed_profile_refuses_neural_metadata_fit_and_source_drift',
        'cuda_bridge_and_driver_closure_preserve_closed_cpu_runtime_source',
        'current_runtime_predicate_preserves_the_exact_legacy_cpu_literal',
        'exact_legacy_descriptor_and_public_sources_remain_byte_stable',
        'native_version_scope_is_pre_190_exact_release_only',
        'source_row_refuses_wrong_current_core_or_task_policy',
        'publisher_census_row_refuses_old_task_and_preserves_legacy_payload',
        'transition_never_admits_context_or_relabels_a_current_profile',
        'witness_rechecks_the_exact_descriptor_and_keeps_new_input_provenance',
        'g5_supplement_binds_exact_current_source_without_relabelling_old_row'))
G5_SOURCES = ('Cargo.lock', 'crates/airk-retrieval/Cargo.toml', 'crates/airk-retrieval/src/onnx.rs') + tuple(
    'crates/airk-retrieval/src/arbiter/' + name for name in (
        'external_data.rs', 'external_data/g5.rs', 'external_data/g5/tests.rs',
        'runtime.rs', 'compiler.rs', 'compiler/focus.rs', 'runtime/cuda/manifest.rs',
        'runtime/cuda.rs', 'runtime/cuda/fp16.rs',
        'runtime/cuda/fp16/tests.rs', 'runtime/cuda/fp16/CONTRACT.md',
        'runtime/cpu_source_transition.rs', 'runtime/cpu_source_transition/tests.rs',
        'runtime/cpu_source_transition/CONTRACT.md', 'runtime/cuda/windows.rs',
        'runtime/cuda/modules.rs', 'runtime/cuda/tests.rs',
        'runtime/cuda/CONTRACT.md'))


FP32 = {
    'compact': {
        'scoring': '1abbd515e534f4056e3c48380fffd290654e0b92dea67088ae291512f02cf2a2',
        'resource': '941f1fc208d0379320801eecf0b6f55427c9d5b5cab37d0734e7de5852258b81',
        'profile': '840bd478675cbf03f2477c170401af75027d55de82b6ab69933f971a1104531d',
        'outer': 'airk.private.owned-compact-bounded-hybrid-profile/2',
        'timing': 'airk.private.owned-compact-bounded-hybrid-resource20/1',
        'native': 'airk.private.cuda-compact-bounded-hybrid-',
        'selector': 'cuda_compact_fp32_bounded_hybrid_',
        'host_gib': 32, 'device_gib': 24, 'arena': (22, 16, 2), 'lane': 'same_artifact',
    },
    'large': {
        'scoring': 'ea9aefbf7e9cefb91bca8384924237f95655131cb49ac571e6900e55f3d6d820',
        'resource': '8ff0d3ed5152ea533a46ab1dc258b96b536aa7df2948b49400f275aabda8fc7b',
        'profile': '7e4481caaf7034a70779702edad73a0eb08321418d1b785a4b7f4f891522bdac',
        'outer': 'airk.private.owned-large-cuda-batch-view-scoring/1',
        'timing': 'airk.private.owned-large-cuda-batch-view-resource20/1',
        'native': 'airk.private.cuda-large-batch-view-bounded-hybrid-',
        'selector': 'cuda_large_fp32_batch_view_bounded_hybrid_',
        'host_gib': 48, 'device_gib': 44, 'arena': (40, 8, 4), 'lane': 'equivalent_predecessor',
    },
}
SCORING_OWNERS = {
    'journal': 'scripts/cuda-component-execution/bounded_hybrid.py',
    'watchdog': 'scripts/cuda-component-execution/resource20_watch.py',
    'settlement': 'scripts/cuda-native-execution.py',
}


def successful_fp32(receipt, expected, *, timing=False):
    schema = expected['timing' if timing else 'outer']
    elapsed = receipt.get('elapsed_seconds')
    limit = 3600 if timing else 120
    require(receipt.get('schema') == schema and receipt.get('complete') is True
            and receipt.get('failure', 'missing') is None and receipt.get('execution_user_sid') == SID
            and receipt.get('qualification') is False
            and type(elapsed) in (int, float) and math.isfinite(elapsed) and 0 < elapsed < limit,
            'genuine successful FP32 receipt and original timing required')
    if expected['lane'] == 'same_artifact' or timing:
        required = ['native_result', 'operations', 'stdout']
        if not timing:
            required.append('runtime_profile')
        require(receipt.get('final_proof_success') == required and receipt.get('final_proof_errors') == []
                and receipt.get('final_guard_errors') == {} and receipt.get('whole_phase_expired') is False,
                'FP32 final proof roster or original timing differs')
    if expected['lane'] == 'same_artifact':
        custody = receipt.get('graph_custody', {})
        require(receipt.get('product_admission') is False and receipt.get('whole_phase_deadline_not_refreshed') is True
                and type(custody.get('opened_handles')) is int
                and custody['opened_handles'] == custody.get('closed_handles') and custody['opened_handles'] > 0
                and custody.get('close_errors') == [],
                'FP32 Compact input custody or original timing differs')
    else:
        require(receipt.get('reference_basis') == expected['lane'] and receipt.get('model_requests_dispatched') is True,
                'FP32 corrected Large reference basis differs')


def core_pin(pin_row):
    return {key: pin_row[key] for key in ('path', 'sha256')}


def validate_fp32_process(receipt, native, image, expected, settled):
    owned = receipt['owned']
    pid = owned.get('owned_leader_pid')
    proof = owned.get('suspended_assignment', {})
    host = expected['host_gib'] << 30
    require(type(pid) is int and pid > 0 and proof.get('pid') == native.get('pid') == pid
            and proof.get('execution_user_sid') == proof.get('default_owner_sid') == SID
            and native.get('execution_user') == native.get('default_owner') == SID
            and proof.get('image') == core_pin(image)
            and native.get('native_binary_sha256') == image['sha256'],
            'actual suspended FP32 PID/image/token proof differs')
    if expected['lane'] == 'same_artifact':
        require(proof.get('component_job') == {'process_bytes': host, 'job_bytes': host,
                                               'flags': 0x2308, 'active_limit': 2},
                'actual suspended FP32 Compact Job proof differs')
    require(settled(owned) and type(owned.get('peak_host_committed_bytes')) is int
            and 0 < owned['peak_host_committed_bytes'] <= host
            and native.get('actual_job') == {'process_memory_limit_bytes': host, 'job_memory_limit_bytes': host,
                                            'flags': 0x2308}
            and native.get('native_host_reservation_bytes') == host
            and type(native.get('peak_process_rss_bytes')) is int and 0 < native['peak_process_rss_bytes'] <= host,
            'actual FP32 Job, host reservation or process RSS differs')
    arena, warm, headroom = expected['arena']
    require(native.get('applied_capacity') == {'arena_limit_bytes': arena << 30, 'warm_extra_bytes': warm << 30,
                                               'headroom_bytes': headroom << 30}
            and native.get('bounds') == {'schema': 'airk.arbiter.gpu-probe-reservation/1', 'deadline_ms': 120000,
                                         'device_limit_bytes': expected['device_gib'] << 30, 'host_limit_bytes': host}
            and native.get('profile') == expected['profile'] and native.get('complete') is True
            and native.get('error', 'missing') is None and native.get('native_control_cleanup_error', 'missing') is None
            and all(native.get(key) is True for key in ('session_teardown_observed', 'native_host_charge_release_observed',
                                                       'native_control_cleanup_observed'))
            and all(native.get(key) is False for key in ('qualified', 'product_admission', 'process_cuda_poisoned',
                                                         'native_control_retained_for_unknown_cuda')),
            'FP32 profile, device reservation or native teardown differs')


def validate_fp32_rows(native, rows, lane):
    require([row['tokens'] for row in native['rows']] == [32, 128, 1024, 8192]
            and [row['tokens'] for row in native['profiled_runs']] == [32, 128, 1024, 8192]
            and len(rows) == 4, 'four exact FP32 numeric observations required')
    for actual, expected in zip(native['rows'], rows):
        require(actual['name'] == expected['name']
                and actual['tokens'] == len(expected['input']['input_ids'])
                and actual['input_digest'] == expected['input']['input_digest']
                and actual['argmax_matches'] is True and len(actual['raw_scores']) == 8
                and all(type(score) in (int, float) and math.isfinite(score) for score in actual['raw_scores'])
                and all(type(actual[key]) in (int, float) and 0 <= actual[key] <= limit for key, limit in (
                    (lane + '_raw_error', .001), ('original_fp32_raw_error', .001),
                    (lane + '_mass_error', .05), ('original_fp32_mass_error', .05))),
                'FP32 independent score comparison differs')


def fp32_facts(evidence, selected_source, take, core, owners):
    expected = FP32[evidence['model']]
    compact = evidence['model'] == 'compact'
    require(evidence['first_fp32']['sha256'] == expected['scoring']
            and evidence['resource_fp32']['sha256'] == expected['resource'],
            'exact selected model FP32 receipt identities required')
    first, timing = (read(take(evidence[key])) for key in ('first_fp32', 'resource_fp32'))
    historical_group = scored_scope = None
    for timing_run, outer in ((False, first), (True, timing)):
        successful_fp32(outer, expected, timing=timing_run)
        native = read(take(outer['native_result']))
        suffix = 'resource' if timing_run else 'profile'
        require(native['schema'] == expected['native'] + suffix + '/1', 'exact FP32 native receipt schema required')
        if compact:
            plan = read(take(outer['plan']))
            context = plan['context']
            image, request, runtime = (context[key] for key in ('native_image', 'request', 'descriptor'))
            build = read(take(context['native_build_receipt']))
            require(plan['schema'] == ('airk.private.compact-bounded-hybrid-resource20-plan/1' if timing_run
                                      else 'airk.private.compact-bounded-hybrid-plan/2')
                    and outer['input_seals'] == dict(context['input_seals'], **{
                        outer['plan']['path']: outer['plan']['sha256']})
                    and outer['parent_python'] == context['parent_python']
                    and context['native_group'] == build['groups']['native']['sha256'],
                    'retained Compact plan or native build source differs')
            take(context['parent_python'])
            if timing_run:
                require(plan['profile_execution'] == evidence['first_fp32']
                        and plan['profile_result'] == first['native_result'],
                        'Compact timing did not use the selected scoring evidence')
            profile, native_input = context['profile'], context['native_input']
            graph = plan['graph']
        else:
            image, request, runtime = (outer[key] for key in ('image', 'request', 'runtime_manifest'))
            build = read(take(outer['build']))
            log = take(outer['build_log'])
            require(log.parent == Path(outer['build']['path']).parent and log.name == build['log']
                    and outer['wall_seconds'] == (3600 if timing_run else 120)
                    and outer['host_bytes'] == expected['host_gib'] << 30, 'retained Large build or limits differ')
            candidate, scoring = outer['candidate_inputs'], outer['scoring_inputs']
            profile, native_input = candidate['profile_pin'], candidate['native_input']
            graph = scoring['reviewed_graph']
            if timing_run:
                require(outer['resource_inputs']['scoring_execution'] == evidence['first_fp32']
                        and outer['resource_inputs']['scoring_result'] == first['native_result'],
                        'Large timing did not use the selected scoring evidence')
            else:
                proof = read(take(scoring['reference_equivalence']))
                require(native['reference_basis'] == expected['lane']
                        and native['reference_equivalence'] == core_pin(scoring['reference_equivalence'])
                        and native['reference_request'] == core_pin(scoring['reference_request'])
                        and proof['predecessor_graph'] == selected_source['original_graph']['sha256']
                        and proof['candidate_graph'] == candidate['graph']['sha256']
                        and proof['exact_inverse'] is proof['operand_order_unchanged'] is True
                        and proof['verified_mask_adds'] == 36,
                        'Large scoring must bind the original export through the reviewed inverse proof')
                take(scoring['reference_request'])
                take(candidate['graph'])
        require(build['passed'] is True and build['inputs_unchanged'] is True, 'actual passing FP32 native build required')
        if not timing_run:
            historical_group = build['groups']['native']['sha256']
        selector = ('inference_engine::pool::arbiter::qualification::software_v2::cuda_qualification::'
                    'bounded_hybrid::' + expected['selector'] + ('resource20' if timing_run else 'profile'))
        command = ([image['path'], '--exact', selector, '--ignored', '--nocapture', '--test-threads=1'] if compact
                   else [image['path'], '--ignored', '--exact', selector, '--test-threads=1']
                        + (['--nocapture'] if timing_run else []))
        require(outer['command'] == command, 'exact retained FP32 native selector required')
        take(image)
        validate_fp32_process(outer, native, image, expected, owners['settlement'].settled)
        for label, row in (('request', request), ('runtime_manifest', runtime), ('native_input', native_input),
                           ('reviewed_graph', graph)):
            require(native[label] == core_pin(row), 'native FP32 input pin differs: ' + label)
            take(row)
        request_value, input_value = read(request['path']), read(native_input['path'])
        profile_value = read(take(profile))
        require(request_value['gpu_input'] == core_pin(native_input)
                and request_value['runtime_manifest'] == core_pin(runtime)
                and request_value['bounds'] == native['bounds']
                and request_value['job_limit_bytes'] == expected['host_gib'] << 30
                and input_value['profile'] == core_pin(profile)
                and input_value['source_manifest'] == native['source_manifest']
                and profile_value['manifest'] == expected['profile'],
                'FP32 Probe/profile/source custody differs')
        core(native['source_manifest'])
        if compact:
            require(profile_value['device_execution']['graph'] == selected_source['original_graph']['sha256'],
                    'Compact scoring profile does not bind the original export graph')
        limits = {'native_result': 16 << 20, 'operations': 128 << 10, 'stdout': 64 << 20}
        if not timing_run:
            limits['runtime_profile'] = 64 << 20
            require(outer['runtime_profile'] == native['runtime_profile'], 'FP32 runtime profile pin differs')
        for key, cap in limits.items():
            require(0 < outer[key]['bytes'] <= cap, 'FP32 bounded output proof required: ' + key)
            if key != 'native_result':
                take(outer[key])
        raw_operations = Path(outer['operations']['path']).read_bytes()
        operations = [json.loads(line) for line in raw_operations.splitlines()]
        scope = [native[key] for key in ('profile', 'request', 'reviewed_graph', 'runtime_manifest')]
        if timing_run:
            watch = outer['owned']['before_GO']['operation_watch']
            series = native['resource']
            require([*scope, native['input_digest']] == scored_scope
                    and native['profiling_enabled'] is False and native['tokens'] == 8192
                    and native['operation_deadline_ms'] == 120_000 and series['complete'] is True
                    and series['attempted_samples'] == len(series['samples']) == 20
                    and series['reservation_bytes'] == expected['host_gib'] << 30
                    and watch['actual_process_exit_observed'] is True and watch['exit_code'] == 0
                    and watch['completed_return_markers'] == 22 and watch['observed_events'] == operations,
                    'complete independently watched FP32 Resource20 evidence required')
            identity = {'input_sha256': native['request']['sha256'], 'profile': native['profile'],
                        'process_id': native['pid'], 'native_binary_sha256': native['native_binary_sha256']}
            state = owners['watchdog'].Operations(identity, native['input_digest'],
                                                  native['original_whole_expires_monotonic_ns'])
            prefix = b''
            for line, event in zip(raw_operations.splitlines(keepends=True), operations):
                prefix += line
                state.feed(prefix, event['observed_monotonic_ns'])
            state.prove_exit(0)
        else:
            fixtures = [read(core(native[key])) for key in ('retained_fixture', 'supplemental_fixture')]
            for key, fixture in zip(('retained_producer', 'supplemental_producer'), fixtures):
                require(native[key] == fixture['reference_producer'], 'FP32 reference producer differs')
                core(native[key])
            validate_fp32_rows(native, owners['journal'].selected_rows(*fixtures), expected['lane'])
            owners['journal'].journal(operations, native)
            scored_scope = [*scope, native['rows'][-1]['input_digest']]
    return historical_group


def successful_readiness(receipt, kind):
    require(receipt.get('schema') == 'airk-readiness-command/1'
            and receipt.get('status') == 'completed' and receipt.get('passed') is True
            and receipt.get('exit_code') == 0 and receipt.get('runner_exit_code') == 0
            and receipt.get('interrupted') is False, 'successful native G5 readiness receipt required')
    for key in ('inputs_unchanged', 'runner_unchanged', 'runtime_unchanged', 'command_inputs_unchanged'):
        require(receipt.get(key) is True, 'native G5 changed inputs: ' + key)
    command = receipt['command']
    require(isinstance(command, list) and 'airk-retrieval' in command and '--all-features' in command,
            'native G5 feature/crate scope missing')
    if kind == 'clippy':
        require(any(Path(x).name == 'dev-check.py' for x in command), 'actual scoped Clippy receipt required')
    else:
        require(command[1:3] == ['nextest', 'run'] and '--locked' in command
                and '--profile' in command and command[command.index('--profile') + 1] == 'ci',
                'actual pinned Nextest receipt required')
        require(type(receipt.get('selected_tests')) is int and receipt['selected_tests'] > 0,
                'positive native G5 selected test count required')


def validate_junit(raw, selected):
    import xml.etree.ElementTree as ET
    require(len(raw) <= 32 << 20 and b'<!DOCTYPE' not in raw and b'<!ENTITY' not in raw,
            'bounded literal JUnit required')
    tree = ET.fromstring(raw)
    cases = list(tree.iter('testcase'))
    require(len(cases) == selected and len({(x.get('classname'), x.get('name')) for x in cases}) == selected,
            'JUnit selected count/unique identities differ')
    require(all(not list(x) or all(child.tag == 'system-out' for child in x) for x in cases),
            'JUnit failure/skipped/error evidence refused')
    passed = {x.get('name') for x in cases if x.get('classname') == 'airk-retrieval'}
    require(G5_TESTS <= passed, 'required G5/CPU-transition passing test identities absent')


def g5_source_facts(group, root, check=lambda: None):
    rows = {}
    for relative in G5_SOURCES:
        row = pin(root / relative, check)
        require(group['files'].get(relative) == row['sha256'], 'required current G5 source absent/changed: ' + relative)
        rows[row['path']] = row
    return rows

def fp32_owners(validators, take):
    root = Path(__file__).resolve().parents[1]
    require(set(validators) == set(SCORING_OWNERS), 'complete prerequisite validator roster required')
    owners = {}
    for key, relative in SCORING_OWNERS.items():
        row = validators[key]
        require(row['path'] == str(root / relative), 'maintained FP32 prerequisite validator required: ' + key)
        owners[key] = load(take(row), 'fp16_fp32_' + key)
    native = owners['settlement']
    require(native.HELPERS == HELPERS, 'original settlement owner differs')
    for name, sha in HELPERS.items():
        take(dict(native.pin(native.MAINTAINED_HELPER_ROOT / name), sha256=sha))
    owners['settlement'], _ = native.helpers({'ownership_helper_root': str(native.MAINTAINED_HELPER_ROOT)})
    return owners


def prerequisite_facts(evidence, selected_source, check=lambda: None):
    """Reopen sealed model-specific FP32 and current G5 evidence without dispatch."""
    root = Path(__file__).resolve().parents[1]
    rows = {}
    def take(row):
        if row['path'] not in rows:
            windows_owner(path=row['path'])
            pinned(row, check)
            rows[row['path']] = row
        require(rows[row['path']] == row, 'conflicting prerequisite pin for one path')
        return Path(row['path'])
    def core(row):
        if row['path'] not in rows:
            windows_owner(path=row['path'])
        full = rows.get(row['path']) or pin(row['path'], check)
        require(full['sha256'] == row['sha256'], 'prerequisite core pin changed')
        return take(full)
    owners = fp32_owners(evidence['validators'], take)
    historical_group = fp32_facts(evidence, selected_source, take, core, owners)
    tests = read(take(evidence['native_g5']))
    clippy = read(take(evidence['native_g5_clippy']))
    successful_readiness(tests, 'tests'); successful_readiness(clippy, 'clippy')
    require(tests['groups']['native'] == clippy['groups']['native'], 'G5 Clippy/tests source groups differ')
    group = tests['groups']['native']
    require(group['sha256'] != historical_group, 'G5 cannot retag the original FP32 source epoch')
    for row in g5_source_facts(group, root, check).values():
        take(row)
    junit = tests['junit']; leaf = Path(junit['path'])
    require(leaf.name == junit['path'] and leaf.name not in ('', '.', '..'), 'JUnit must be a retained sibling leaf')
    junit_path = core(dict(junit, path=str(Path(evidence['native_g5']['path']).parent / leaf)))
    validate_junit(junit_path.read_bytes(), tests['selected_tests'])
    check()
    return rows


def source_facts(source, model, check=lambda: None):
    require(model in FP32, 'only Compact and Large have genuine FP32 prerequisite validators')
    require(source['sha256'] == SOURCE_PLAN_SHA, 'original export source plan identity differs')
    original = read(pinned(source, check))
    require(original.get('schema') == 'airk.private.g5-original3-export-run-plan/1', 'original source schema differs')
    models = [row for row in original['models'] if row['model'] == model]
    require(len(models) == 1, 'exact original selected source required')
    selected = models[0]
    for key in ('original_graph', 'source_closure', 'source_closure_authority'):
        pinned(selected[key], check)
    return original, selected


def runtime_facts(baseline, roots, check=lambda: None):
    require(baseline['sha256'] == RUNTIME_BASELINE_SHA, 'original runtime baseline identity differs')
    value = read(pinned(baseline, check))
    require(value.get('schema') == 'airk.private.exact-python-runtime-files/1', 'runtime baseline schema differs')
    files = {path: pin(path, check) for path in sorted(set().union(*(file_roster(root) for root in roots)))}
    outside = {}
    for path, sha in value['files'].items():
        row = files.get(path) or pin(path, check)
        require(row['sha256'] == sha, 'retained runtime baseline member changed/missing')
        if path not in files:
            outside[path] = row
    return {'roots': [str(x) for x in roots], 'files': files, 'import_paths': [str(x) for x in roots]}, outside


def authenticate_prerequisites(plan, by_path, check=lambda: None):
    original, selected = source_facts(plan['source_plan'], plan['model'], check)
    require(by_path.get(selected['source_closure_authority']['path']) == selected['source_closure_authority'], 'original source authority omitted from input census')
    require(plan['source_graph'] == selected['original_graph'] and plan['source_closure'] == selected['source_closure']
            and plan['expected_weights_bytes'] == selected['expected_weights_bin_bytes']
            and plan['expected_attention_count'] == len(selected['expected_attention_nodes'])
            and plan['python'] == original['python'] and plan['exporter']['sha256'] == original['reviewed_library']['sha256'],
            'conversion differs from authentic original source data')
    evidence = read(pinned(plan['prerequisites_manifest'], check))
    require(evidence.get('schema') == 'airk.private.fp16-prerequisites/2' and evidence.get('model') == plan['model']
            and {key: evidence[key] for key in ('first_fp32', 'native_g5')} == plan['prerequisite_evidence'],
            'prerequisite manifest identity differs')
    for path, row in prerequisite_facts(evidence, selected, check).items():
        require(by_path.get(path) == row, 'prerequisite dependency omitted from input census')
    baseline = plan['runtime_baseline']
    require(baseline['sha256'] == RUNTIME_BASELINE_SHA, 'original runtime baseline identity differs')
    value = read(pinned(baseline, check))
    require(value.get('schema') == 'airk.private.exact-python-runtime-files/1', 'original runtime baseline schema differs')
    for path, sha in value['files'].items():
        require(by_path.get(path, {}).get('sha256') == sha, 'retained runtime baseline file omitted/changed')

def merge_input_pins(*groups):
    pins = {}
    for group in groups:
        for path, row in group.items():
            require(path == row['path'] and pins.setdefault(path, row) == row,
                    'conflicting input pin for one canonical path')
    return pins


def prepare(args):
    deadline = time.monotonic() + LIMITS['wall_seconds']
    require(sys.flags.no_site == 1 and sys.dont_write_bytecode, 'nonwriting no-site preparer required')
    windows_owner()
    def check():
        require(time.monotonic() < deadline, 'preparation metadata deadline expired')
    root = Path(__file__).resolve().parents[1]
    source = pin(args.source_plan, check)
    original, selected = source_facts(source, args.model, check)
    evidence = {'schema': 'airk.private.fp16-prerequisites/2', 'model': args.model,
                'first_fp32': pin(args.first_fp32, check), 'resource_fp32': pin(args.resource_fp32, check),
                'native_g5': pin(args.native_g5, check), 'native_g5_clippy': pin(args.native_g5_clippy, check),
                'validators': {key: pin(root / relative, check) for key, relative in SCORING_OWNERS.items()}}
    prerequisite_rows = prerequisite_facts(evidence, selected, check)
    python = original['python']; pinned(python, check)
    require(pin(sys.executable, check) == python, 'original interpreter required for preparation')
    home = Path(python['path']).parent
    runtime, outside = runtime_facts(pin(args.runtime_baseline, check), [home / 'Lib', home / 'DLLs', Path(original['python_site'])], check)
    plan_path = Path(args.plan)
    require(plan_path.is_absolute() and plan_path.resolve(strict=False) == plan_path
            and plan_path.is_relative_to(root / 'artifacts') and plan_path.parent.is_dir(), 'fresh canonical artifact metadata parent required')
    runtime_path = plan_path.with_name(plan_path.stem + '-runtime.json')
    evidence_path = plan_path.with_name(plan_path.stem + '-prerequisites.json')
    require(not any(x.exists() for x in (plan_path, runtime_path, evidence_path)), 'preparation metadata must be fresh')
    helper_root = root / 'scripts/cuda-native-execution/helpers'
    plan = {'schema': SCHEMA, 'human_sid': SID, 'limits': dict(LIMITS), 'model': args.model,
            'repository_root': str(root), 'output_root': str(args.output_root), 'cache_prefix': str(args.cache_prefix),
            'expected_weights_bytes': selected['expected_weights_bin_bytes'],
            'expected_attention_count': len(selected['expected_attention_nodes']),
            'python': python, 'powershell': pin(args.powershell, check), 'wrapper': pin(__file__, check),
            'guard': pin(root / 'scripts/dev/assert-human-execution.ps1', check),
            'exporter': pin(root / 'scripts/arbiter-fp16-export.py', check),
            'source_graph': selected['original_graph'], 'source_closure': selected['source_closure'],
            'source_plan': source, 'runtime_baseline': pin(args.runtime_baseline, check),
            'helpers': {name: pin(helper_root / name, check) for name in HELPERS},
            'prerequisite_evidence': {'first_fp32': evidence['first_fp32'], 'native_g5': evidence['native_g5']}}
    require(plan['exporter']['sha256'] == original['reviewed_library']['sha256'], 'original reviewed exporter differs')
    rows = merge_input_pins(runtime['files'], outside, prerequisite_rows)
    for row in [source, plan['runtime_baseline'], selected['source_closure_authority'], *plan['helpers'].values()]:
        rows[row['path']] = row
    for key in ('python', 'powershell', 'wrapper', 'guard', 'exporter', 'source_graph', 'source_closure'):
        rows[plan[key]['path']] = plan[key]
    for name, item in read(plan['source_closure']['path']).items():
        require(Path(name).name == name and name not in ('', '.', '..') and not any(x in name for x in ('/', '\\', ':')), 'unsafe original source sidecar')
        row = dict(item, path=str(Path(plan['source_graph']['path']).parent / name))
        pinned(row, check); rows[row['path']] = row
    def encoded(value):
        raw = (json.dumps(value, indent=2) + '\n').encode('utf-8')
        require(len(raw) <= 32 << 20, 'bounded metadata exceeded')
        return raw
    def prospective(path, raw):
        return {'path': str(path), 'bytes': len(raw), 'sha256': hashlib.sha256(raw).hexdigest()}
    runtime_raw, evidence_raw = encoded(runtime), encoded(evidence)
    plan['runtime_manifest'] = prospective(runtime_path, runtime_raw)
    plan['prerequisites_manifest'] = prospective(evidence_path, evidence_raw)
    for row in (plan['runtime_manifest'], plan['prerequisites_manifest']):
        rows[row['path']] = row
    plan['input_files'] = list(rows.values())
    validate_plan(plan)
    plan_raw = encoded(plan)
    for target in (plan_path, runtime_path, evidence_path, Path(plan['output_root'])):
        guard(plan, target, deadline=deadline)
    for target, raw in ((runtime_path, runtime_raw), (evidence_path, evidence_raw), (plan_path, plan_raw)):
        check()
        with target.open('xb') as stream:
            stream.write(raw); stream.flush(); os.fsync(stream.fileno())
        guard(plan, target, True, deadline=deadline)
    authenticate(plan, check)
    check()
    return pin(plan_path, check)

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument('--prepare', action='store_true')
    mode.add_argument('--inspect-plan', action='store_true')
    mode.add_argument('--execute-reviewed-conversion', action='store_true')
    mode.add_argument('--worker', action='store_true')
    parser.add_argument('--plan', type=Path, required=True)
    parser.add_argument('--plan-sha256')
    for name in ('source-plan', 'first-fp32', 'resource-fp32', 'native-g5', 'native-g5-clippy', 'runtime-baseline', 'output-root', 'cache-prefix', 'powershell'):
        parser.add_argument('--' + name, type=Path)
    parser.add_argument('--model', choices=('focus', 'compact', 'large'))
    parser.add_argument('--deadline', type=float)
    args = parser.parse_args()
    if args.prepare:
        require(all(getattr(args, name) is not None for name in ('source_plan', 'first_fp32', 'resource_fp32', 'native_g5', 'native_g5_clippy', 'runtime_baseline', 'output_root', 'cache_prefix', 'powershell', 'model')), 'complete prepare arguments required')
        print(json.dumps({'plan': prepare(args), 'conversion_executed': False, 'qualification': False}))
        return
    require(args.plan_sha256 is not None, 'reviewed plan hash required')
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
