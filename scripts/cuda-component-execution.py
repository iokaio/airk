# SPDX-License-Identifier: Apache-2.0
"""One original Compact17 CUDA component; see cuda-component-execution/CONTRACT.md."""
import argparse
import ctypes
import datetime
import importlib.util
import json
import math
import os
from pathlib import Path
import subprocess
import sys
import time

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[1]
SID = 'S-1-5-21-3678370870-3858642417-826968105-1002'
ANCHOR = Path('C:/Users/tdotn/AppData/Local/airk-final-exec-20261116T052942Z')
SELECTOR = 'inference_engine::pool::arbiter::qualification::software_v2::cuda_qualification::cuda_original_context_fp32_t17_component'
SCHEMA = 'airk.private.cuda-component-plan/1'
LIMITS = {'wall_seconds': 120, 'process_bytes': 16 << 30, 'job_bytes': 24 << 30,
          'log_bytes': 64 << 20, 'cleanup_seconds': 5}
BOUNDS = {'schema': 'airk.arbiter.gpu-probe-reservation/1', 'host_limit_bytes': 16 << 30,
          'device_limit_bytes': 24 << 30, 'deadline_ms': 120000}
PROOFS = {'native_result': 16 << 20, 'operations': 1 << 20, 'stdout': 64 << 20}
SOURCE_KEYS = (
    'crates/airk-inference/src/inference_engine/pool/arbiter/qualification/software_v2/cuda_qualification.rs',
    'crates/airk-inference/src/inference_engine/pool/arbiter/qualification/software_v2/cuda_qualification/CONTRACT.md',
    'crates/airk-inference/src/inference_engine/pool/arbiter/qualification/software_v2/operation.rs',
    'crates/airk-retrieval/src/arbiter/runtime/cuda/windows.rs',
    'crates/airk-retrieval/src/arbiter/runtime/cuda/placement.rs',
    'crates/airk-host/src/gpu_runtime.rs',
)


def require(ok, message):
    if not ok:
        raise ValueError(message)


def base():
    path = ROOT / 'scripts/cuda-native-execution.py'
    spec = importlib.util.spec_from_file_location('component_existing_nn0', path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def finite(value, low=0, high=math.inf):
    return type(value) in (int, float) and math.isfinite(value) and low <= value <= high


def exact(value, expected, label):
    require(type(value) is type(expected) and value == expected, label)


def guard(plan, path=None, created=False):
    """Read-only checked guard in an inherited same-user PowerShell process."""
    b = base()
    b.pinned(plan['checked_guard'])
    b.pinned(plan['powershell'])
    code = "$ErrorActionPreference='Stop'; $v=[Console]::In.ReadToEnd()|ConvertFrom-Json; . $v.guard; "
    if path is None:
        code += 'Assert-AirkHumanExecution -ExpectedSid $v.sid | Out-Null'
    else:
        code += ('Assert-AirkCreatedOutput' if created else 'Assert-AirkOutputParent')
        code += ' -Path $v.path -Root $v.root -ExpectedSid $v.sid | Out-Null'
    root = ROOT if path is not None and Path(path).is_relative_to(ROOT) else ANCHOR
    payload = {'guard': plan['checked_guard']['path'], 'sid': SID, 'path': str(path), 'root': str(root)}
    subprocess.run([plan['powershell']['path'], '-NoLogo', '-NoProfile', '-NonInteractive', '-Command', code],
                   input=json.dumps(payload), text=True, check=True, timeout=10, capture_output=True)


def mkdir(plan, path):
    guard(plan, path)
    Path(path).mkdir()
    guard(plan, path, True)


def write_new(plan, path, value):
    guard(plan, path)
    with Path(path).open('x', encoding='utf-8', newline='\n') as stream:
        json.dump(value, stream, indent=2, allow_nan=False)
        stream.write('\n')
        stream.flush()
        os.fsync(stream.fileno())
    guard(plan, path, True)


def metadata(plan, b, parent):
    """Authenticate actual metadata custody; neither old image nor receipt is relabeled."""
    receipt = b.read(b.pinned(plan['metadata_receipt']))
    old = b.read(b.pinned(receipt['plan']))
    require(receipt.get('schema') == 'airk.private.owned-cuda-profile-nn0/1'
            and receipt.get('complete') is True and receipt.get('failure') is None
            and receipt.get('whole_phase_expired') is False
            and receipt.get('post_input_drift') == [] and parent.settled(receipt.get('owned', {}))
            and finite(receipt.get('elapsed_seconds'), 0, 120), 'actual successful metadata custody required')
    require(receipt.get('kind') == 'metadata' and receipt.get('activation') is False
            and receipt.get('qualification') is False and receipt.get('execution_user_sid') == SID,
            'metadata parent scope differs')
    b.source_image(old)
    seals = dict(old['input_seals'], **{receipt['plan']['path']: receipt['plan']['sha256']})
    require(receipt['input_seals'] == seals, 'metadata input closure differs')
    for path, sha in seals.items():
        require(b.digest(b.regular(path)) == sha, 'historical metadata source changed: ' + path)
    outputs = receipt['outputs']
    require(len(outputs) == 6 and len({x['path'] for x in outputs}) == 6, 'native six-file metadata required')
    for row in outputs:
        b.pinned(row)
    roster = {Path(x['path']).name: x for x in outputs}
    for key, name in [('request', 'probe-request.json'), ('native_input', 'native-input.json'), ('profile', 'profile.json')]:
        require(plan[key] == roster[name], 'metadata output identity differs: ' + name)
    emitted = b.read(roster['metadata-receipt.json']['path'])
    require(emitted.get('schema') == 'airk.private.cuda-probe-metadata/1'
            and emitted.get('qualified') is False and emitted.get('model_requests_dispatched') is False
            and emitted.get('forward_count_observed', 'absent') is None
            and b.core_pin_matches(emitted.get('probe_request'), plan['request']), 'native metadata emission differs')
    return old, outputs


def input_facts(plan, b):
    request = b.read(b.pinned(plan['request']))
    require(request.get('schema') == 'airk.private.cuda-component-probe-request/1'
            and request.get('kind') == 'probe' and request.get('bounds') == BOUNDS
            and request.get('cold_bytes') == 16 << 30 and request.get('warm_bytes') == 8 << 30
            and request.get('headroom_bytes') == 2 << 30 and request.get('job_limit_bytes') == 24 << 30,
            'original Compact17 request/bounds differ')
    require(b.core_pin_matches(request['gpu_input'], plan['native_input'])
            and b.core_pin_matches(request['runtime_manifest'], plan['descriptor']), 'native request pins differ')
    native = b.read(b.pinned(plan['native_input']))
    profile = b.read(b.pinned(plan['profile']))
    require(b.core_pin_matches(native['profile'], plan['profile']) and profile['model'] == 'arbiter-context-qwen3-17b',
            'original Compact profile required')
    fixture_pin = native['numeric_fixture']
    require(b.digest(b.regular(fixture_pin['path'])) == fixture_pin['sha256'], 'fixture changed')
    fixture = b.read(fixture_pin['path'])
    require(len(fixture['rows']) == 3 and [len(x['input']['input_ids']) for x in fixture['rows']] == [17, 128, 8192],
            'complete original17/128/8192 denominator required')
    row = fixture['rows'][0]
    require(row['name'] == 'all-live-0'
            and row['input']['input_digest'] == '3df7cf8912d78bdddbaa9cf5c02b64d06ade9f740bacb28181189b12447b545e'
            and len(row['input']['candidate_positions']) == 3, 'frozen literal17 input differs')
    for key in ['reference_same_artifact', 'reference_original_fp32']:
        require(len(row[key]) == 3 and all(finite(x, -math.inf) for x in row[key]), 'full finite independent references required')
    return native, profile, fixture


def preflight(plan, b, *, before):
    require(plan.get('schema') == SCHEMA and plan.get('human_sid') == SID
            and plan.get('selector') == SELECTOR and plan.get('limits') == LIMITS,
            'exact Context17 plan and original bounds required')
    require(plan['repository_root'] == str(ROOT) and plan['wrapper'] == b.pin(__file__)
            and plan['custody_owner'] == b.pin(ROOT / 'scripts/cuda-native-execution.py')
            and plan['contract'] == b.pin(ROOT / 'scripts/cuda-component-execution/CONTRACT.md'), 'maintained source identity differs')
    require(Path(plan['output_root']).parent == ANCHOR, 'fresh direct approved private output required')
    require(Path(plan['private_census']).parent == Path(plan['output_root']), 'private census must belong to this run')
    if before:
        require(not Path(plan['output_root']).exists(), 'fresh output already exists')
    b.source_image(plan)
    require(set(SOURCE_KEYS) <= set(plan['required_native_sources']), 'component native source roster omitted')
    for path, sha in plan['required_native_sources'].items():
        require(b.digest(b.regular(ROOT / path)) == sha, 'frozen native source differs: ' + path)
    parent, owner = b.helpers(plan)
    old, outputs = metadata(plan, b, parent)
    require(plan['output_guard'] == old['output_guard'] and plan['output_guard_lock'] == old['output_guard_lock'],
            'retained protected-output policy differs')
    require(b.census(plan['runtime_root']) == plan['runtime_files']
            and Path(plan['descriptor']['path']).parent == Path(plan['runtime_root'])
            and plan['descriptor'] in plan['runtime_files'], 'complete frozen runtime roster differs')
    required = [plan[k] for k in ['wrapper', 'custody_owner', 'contract', 'parent_python', 'powershell', 'checked_guard',
                                 'metadata_receipt', 'image_packet', 'native_build_receipt', 'native_image',
                                 'request', 'native_input', 'profile', 'descriptor', 'output_guard', 'output_guard_lock']]
    required += outputs + plan['runtime_files'] + [b.pin(Path(plan['ownership_helper_root']) / n) for n in b.HELPERS]
    native, profile, fixture = input_facts(plan, b)
    required += [b.pin(path) for path in native['files'].values()]
    required += [b.pin(native['source_manifest']['path']), b.pin(native['numeric_fixture']['path']),
                 b.pin(fixture['reference_producer']['path'])]
    for row in required:
        b.pinned(row)
        require(plan['input_seals'].get(row['path']) == row['sha256'], 'required seal omitted: ' + row['path'])
    require(len(plan['input_seals']) <= 16380, 'input closure exceeds existing bound')
    for path, sha in plan['input_seals'].items():
        require(b.digest(b.regular(path)) == sha, 'original input seal changed: ' + path)
    return parent, owner, native, profile, fixture


def prepare(args):
    b = base()
    metadata_pin = b.pin(args.metadata_receipt)
    require(metadata_pin['sha256'] == args.metadata_receipt_sha256, 'reviewed metadata receipt differs')
    receipt = b.read(metadata_pin['path'])
    old = b.read(b.pinned(receipt['plan']))
    packet_pin = b.pin(args.image_packet)
    require(packet_pin['sha256'] == args.image_packet_sha256, 'reviewed preserved image packet differs')
    packet = b.read(packet_pin['path'])
    build_pin = b.pin(packet['actual_build_receipt']['path'])
    require(build_pin['sha256'] == packet['actual_build_receipt']['sha256'], 'native build receipt differs')
    build = b.read(build_pin['path'])
    images = [x for x in packet['images'] if x['binary_id'] == 'airk-inference']
    require(len(images) == 1, 'one actual native Inference image required')
    roster = {Path(x['path']).name: x for x in receipt['outputs']}
    request = b.read(roster['probe-request.json']['path'])
    descriptor = b.pin(request['runtime_manifest']['path'])
    out = Path(args.output_root)
    require(out.is_absolute() and out.parent == ANCHOR, 'canonical private direct-child output required')
    plan = {k: old[k] for k in ['repository_root', 'ownership_helper_root', 'output_guard', 'output_guard_lock']}
    plan.update(schema=SCHEMA, human_sid=SID, selector=SELECTOR, limits=LIMITS,
                wrapper=b.pin(__file__), custody_owner=b.pin(ROOT / 'scripts/cuda-native-execution.py'),
                contract=b.pin(ROOT / 'scripts/cuda-component-execution/CONTRACT.md'),
                metadata_receipt=metadata_pin, image_packet=packet_pin, native_build_receipt=build_pin,
                native_image={k: images[0][k] for k in ['path', 'bytes', 'sha256']}, native_group=packet['native_group'],
                request=roster['probe-request.json'], native_input=roster['native-input.json'], profile=roster['profile.json'],
                descriptor=descriptor, runtime_root=str(Path(descriptor['path']).parent),
                output_root=str(out), private_census=str(out / 'private-census'),
                parent_python=b.pin(sys.executable), powershell=b.pin(args.powershell),
                checked_guard=b.pin(ROOT / 'scripts/dev/assert-human-execution.ps1'))
    plan['runtime_files'] = b.census(plan['runtime_root'])
    plan['required_native_sources'] = dict(old['required_native_sources'])
    for key in SOURCE_KEYS:
        plan['required_native_sources'][key] = build['groups']['native']['files'][key]
    seals = dict(old['input_seals'])
    seals[receipt['plan']['path']] = receipt['plan']['sha256']
    rows = [v for v in plan.values() if isinstance(v, dict) and set(v) == {'path', 'bytes', 'sha256'}]
    rows += receipt['outputs'] + plan['runtime_files']
    native, _, fixture = input_facts(plan, b)
    rows += [b.pin(p) for p in native['files'].values()]
    rows += [b.pin(native['source_manifest']['path']), b.pin(native['numeric_fixture']['path']), b.pin(fixture['reference_producer']['path'])]
    for row in rows:
        seals[row['path']] = row['sha256']
    for name in b.HELPERS:
        row = b.pin(Path(plan['ownership_helper_root']) / name)
        seals[row['path']] = row['sha256']
    for path, sha in plan['required_native_sources'].items():
        seals[str(ROOT / path)] = sha
    plan['input_seals'] = seals
    preflight(plan, b, before=True)
    guard(plan)
    write_new(plan, Path(args.plan), plan)
    return b.pin(args.plan)


def validate_native(value, plan, pid, native, profile, fixture):
    require(value.get('schema') == 'airk.private.cuda-component-result/1'
            and value.get('complete') is True and value.get('qualified') is False
            and value.get('product_admission') is False and value.get('measured_peak_device_bytes', 'missing') is None,
            'native component success/scope absent')
    b = base()
    for key, expected in [('request', plan['request']), ('native_input', plan['native_input']),
                          ('runtime_manifest', plan['descriptor']), ('fixture', native['numeric_fixture']),
                          ('reference_producer', fixture['reference_producer']), ('source_manifest', native['source_manifest'])]:
        require(b.core_pin_matches(value.get(key), expected), 'native capture identity differs: ' + key)
    require(value.get('profile') == profile['manifest'] and value.get('model') == 'arbiter-context-qwen3-17b'
            and type(value.get('pid')) is int and value['pid'] == pid and value.get('execution_user') == SID
            and value.get('native_binary_sha256') == plan['native_image']['sha256'], 'actual native PID/image/model differs')
    require(value.get('bounds') == BOUNDS and value.get('native_host_reservation_bytes') == 16 << 30,
            'native resource charge differs')
    job = value.get('actual_job', {})
    require(job.get('process_memory_limit_bytes') == 16 << 30 and job.get('job_memory_limit_bytes') == 24 << 30
            and type(job.get('flags')) is int and job['flags'] & 0x2300 == 0x2300, 'actual native Job limits differ')
    for key, expected in [('native_control_retained_for_unknown_cuda', False), ('native_control_cleanup_observed', True),
                          ('native_host_charge_release_observed', True), ('process_cuda_poisoned', False),
                          ('native_control_cleanup_error', None), ('final_host_peak_error', None)]:
        require(key in value, 'missing native final proof: ' + key)
        exact(value[key], expected, 'native final settlement differs: ' + key)
    require('error' not in value and value.get('final_report_retention_exceeded_original_deadline', False) is False,
            'native failure retained')
    require(finite(value.get('peak_process_rss_bytes'), 1, 16 << 30)
            and finite(value.get('elapsed_us_to_final_report_start'), 0, 119999999)
            and finite(value.get('load_us'), 0, 120000000), 'native original time/RSS proof absent')
    place = value.get('final_placement', {})
    nodes = place.get('nodes', [])
    require(place.get('scope') == 'onnxruntime-final-session-state-ep-placement' and place.get('complete') is True
            and place.get('shim_status') == 0 and place.get('ort_error_code') == 0
            and 0 < len(nodes) <= 65536 and place.get('reported_nodes') == len(nodes), 'complete final placement absent')
    total = 0
    for node in nodes:
        require(type(node.get('subgraph_index')) is int and node['subgraph_index'] >= 0
                and node.get('ep') == 'CUDAExecutionProvider' and isinstance(node.get('op'), str) and node['op'],
                'unexpected native graph placement')
        for key in ['name', 'domain', 'op', 'ep']:
            require(isinstance(node.get(key), str) and len(node[key].encode()) <= 4096, 'placement string bound')
            total += len(node[key].encode())
    require(total == place.get('reported_string_bytes') and total <= 16 << 20, 'placement byte coverage differs')
    identity = value.get('execution_identity')
    require(isinstance(identity, list) and len(identity) == 2 and isinstance(identity[0], str)
            and identity[0].startswith(profile['runtime'] + '/device-execution/')
            and len(identity[0].rsplit('/', 1)[-1]) == 64
            and all(x in '0123456789abcdef' for x in identity[0].rsplit('/', 1)[-1])
            and type(identity[1]) is int and identity[1] >= profile['device_execution']['host_fit']['minimum_physical_cores'],
            'native opaque execution tuple missing')
    fixed = ['uuid', 'ordinal', 'compute_major', 'compute_minor', 'driver_api_version', 'driver_image_sha256', 'physical_total_bytes']
    before, after = value.get('adapter_before'), value.get('adapter_after')
    require(isinstance(before, dict) and isinstance(after, dict), 'actual adapter observations missing')
    for observed in [before, after]:
        uuid = observed.get('uuid')
        require(isinstance(uuid, list) and len(uuid) == 16 and any(uuid)
                and all(type(x) is int and 0 <= x <= 255 for x in uuid), 'actual adapter UUID absent')
        require(all(type(observed.get(k)) is int and observed[k] >= 0 for k in
                    ['ordinal', 'compute_major', 'compute_minor', 'driver_api_version', 'physical_total_bytes', 'total_bytes', 'free_bytes', 'epoch'])
                and observed['driver_api_version'] >= 12080
                and 0 < observed['total_bytes'] <= observed['physical_total_bytes']
                and observed['free_bytes'] <= observed['total_bytes'], 'actual adapter capacity observation invalid')
        sha = observed.get('driver_image_sha256')
        require(isinstance(sha, str) and len(sha) == 64 and all(x in '0123456789abcdef' for x in sha), 'driver image proof missing')
    require(all(before[k] == after[k] for k in fixed), 'adapter identity changed during component')
    rows = value.get('rows', [])
    require(len(rows) == 1, 'exactly one17 row required')
    row = rows[0]
    require(row.get('name') == fixture['rows'][0]['name'] and row.get('tokens') == 17
            and row.get('input_digest') == fixture['rows'][0]['input']['input_digest'], 'selected17 row identity differs')
    require(len(row.get('raw_scores', [])) == 3 and all(finite(x, -math.inf) for x in row['raw_scores'])
            and row.get('argmax_matches') is True, 'finite complete scores/argmax required')
    for key, bound in [('same_artifact_raw_error', .001), ('same_artifact_mass_error', .05),
                       ('original_fp32_raw_error', .001), ('original_fp32_mass_error', .05)]:
        require(finite(row.get(key), 0, bound), 'original numeric gate refused: ' + key)
    require(finite(row.get('elapsed_us'), 0, 120000000)
            and finite(row.get('native_call_elapsed_us'), 0, row['elapsed_us']), 'native row timing absent')


def validate_operations(events, value):
    require(len(events) == 4, 'exact load/numeric start-return journal required')
    identity = {'input_sha256': value['request']['sha256'], 'profile': value['profile'],
                'process_id': value['pid'], 'native_binary_sha256': value['native_binary_sha256']}
    previous = 0
    for i, event in enumerate(events):
        kind, ordinal = ('load', 0) if i < 2 else ('numeric', 1)
        require(event.get('schema') == 'airk.arbiter.native-operation/1' and event.get('sequence') == i + 1
                and event.get('identity') == identity and event.get('kind') == kind and event.get('ordinal') == ordinal
                and event.get('phase') == ('started' if i % 2 == 0 else 'returned')
                and event.get('clock') == 'windows_performance_counter', 'native operation identity/order differs')
        exact(event.get('operation_input_digest'), None if i < 2 else value['rows'][0]['input_digest'], 'operation input differs')
        started, observed, expiry = (event.get(k) for k in ['started_monotonic_ns', 'observed_monotonic_ns', 'expires_monotonic_ns'])
        require(all(type(x) is int for x in [started, observed, expiry]) and previous <= observed
                and 0 < started <= observed < expiry == value['original_expires_monotonic_ns'], 'original operation clock differs')
        if i % 2:
            require(event.get('native_success') is True and finite(event.get('native_call_elapsed_us'), 0, 120000000)
                    and started == events[i - 1]['started_monotonic_ns'], 'native return proof absent')
        else:
            require(started == observed and event.get('native_success') is None
                    and event.get('native_call_elapsed_us') is None, 'start event differs')
        previous = observed


def token_owner(process):
    from ctypes import wintypes as w
    api = ctypes.WinDLL('advapi32', use_last_error=True)
    kernel = ctypes.WinDLL('kernel32', use_last_error=True)
    api.OpenProcessToken.argtypes = [w.HANDLE, w.DWORD, ctypes.POINTER(w.HANDLE)]
    api.GetTokenInformation.argtypes = [w.HANDLE, ctypes.c_int, ctypes.c_void_p, w.DWORD, ctypes.POINTER(w.DWORD)]
    api.ConvertSidToStringSidW.argtypes = [ctypes.c_void_p, ctypes.POINTER(w.LPWSTR)]
    kernel.CloseHandle.argtypes = [w.HANDLE]
    kernel.LocalFree.argtypes = [ctypes.c_void_p]
    handle = w.HANDLE()
    if not api.OpenProcessToken(process, 8, ctypes.byref(handle)):
        raise ctypes.WinError(ctypes.get_last_error())
    try:
        needed = w.DWORD()
        api.GetTokenInformation(handle, 4, None, 0, ctypes.byref(needed))  # TokenOwner
        require(0 < needed.value <= 65536, 'token owner size unavailable')
        storage = ctypes.create_string_buffer(needed.value)
        if not api.GetTokenInformation(handle, 4, storage, len(storage), ctypes.byref(needed)):
            raise ctypes.WinError(ctypes.get_last_error())
        pointer = ctypes.cast(storage, ctypes.POINTER(ctypes.c_void_p))[0]
        text = w.LPWSTR()
        if not api.ConvertSidToStringSidW(pointer, ctypes.byref(text)):
            raise ctypes.WinError(ctypes.get_last_error())
        try:
            return text.value
        finally:
            kernel.LocalFree(ctypes.cast(text, ctypes.c_void_p))
    finally:
        kernel.CloseHandle(handle)


def configure_job(api, job):
    from ctypes import wintypes as w
    class Basic(ctypes.Structure):
        _fields_ = [('process_time', ctypes.c_longlong), ('job_time', ctypes.c_longlong), ('flags', w.DWORD),
                    ('minimum', ctypes.c_size_t), ('maximum', ctypes.c_size_t), ('active', w.DWORD),
                    ('affinity', ctypes.c_size_t), ('priority', w.DWORD), ('scheduling', w.DWORD)]
    class IO(ctypes.Structure):
        _fields_ = [(n, ctypes.c_ulonglong) for n in ['read_ops', 'write_ops', 'other_ops', 'read', 'write', 'other']]
    class Limits(ctypes.Structure):
        _fields_ = [('basic', Basic), ('io', IO), ('process', ctypes.c_size_t), ('job', ctypes.c_size_t),
                    ('peak_process', ctypes.c_size_t), ('peak_job', ctypes.c_size_t)]
    limits = Limits()
    def query():
        if not api.QueryInformationJobObject(job, 9, ctypes.byref(limits), ctypes.sizeof(limits), None):
            raise ctypes.WinError(ctypes.get_last_error())
    query()
    require(limits.process == limits.job == 16 << 30 and limits.basic.flags == 0x2308
            and limits.basic.active == 2, 'original suspended Job policy differs')
    limits.job = 24 << 30
    if not api.SetInformationJobObject(job, 9, ctypes.byref(limits), ctypes.sizeof(limits)):
        raise ctypes.WinError(ctypes.get_last_error())
    query()
    require(limits.process == 16 << 30 and limits.job == 24 << 30 and limits.basic.flags == 0x2308
            and limits.basic.active == 2, 'frozen component Job bounds readback differs')
    return {'process_bytes': limits.process, 'job_bytes': limits.job, 'flags': limits.basic.flags, 'active_limit': limits.basic.active}


def finish(receipt, paths, started, deadline, hash_file, clock=time.monotonic,
           utc_now=lambda: datetime.datetime.now(datetime.timezone.utc)):
    receipt['final_proof_success'] = []
    receipt['final_proof_errors'] = []
    receipt['complete'] = receipt.get('complete') is True and receipt.get('failure') is None
    for label, limit in PROOFS.items():
        try:
            row = hash_file(paths[label])
            require(isinstance(row, dict) and set(row) == {'path', 'bytes', 'sha256'}
                    and row['path'] == str(paths[label]) and type(row['bytes']) is int and 0 < row['bytes'] <= limit
                    and isinstance(row['sha256'], str) and len(row['sha256']) == 64
                    and all(x in '0123456789abcdef' for x in row['sha256']), 'mandatory bounded final SHA proof absent')
            require(label not in receipt or receipt[label] == row, 'output drift after validation: ' + label)
            receipt[label] = row
            receipt['final_proof_success'].append(label)
        except BaseException as error:
            fact = {'label': label, 'type': type(error).__name__, 'message': str(error)}
            receipt['final_proof_errors'].append(fact)
            receipt['failure'] = receipt.get('failure') or fact
            receipt['complete'] = False
    # Capture UTC before the final original monotonic observation; no work or
    # wall-clock conversion may move that monotonic observation earlier.
    receipt['finished_utc'] = utc_now().isoformat()
    now = clock()
    receipt['elapsed_seconds'] = now - started
    receipt['whole_phase_expired'] = now >= deadline
    if receipt['whole_phase_expired']:
        receipt['complete'] = False
        receipt['failure'] = receipt.get('failure') or {'type': 'TimeoutError', 'message': 'original component deadline expired'}


def run(args):
    started = time.monotonic()
    started_utc = datetime.datetime.now(datetime.timezone.utc)
    b = base()
    original_deadline = min(started_utc + datetime.timedelta(seconds=120), b.CAMPAIGN)
    deadline = started + (original_deadline - started_utc).total_seconds()
    plan_pin = b.pin(args.plan)
    require(plan_pin['sha256'] == args.plan_sha256, 'reviewed component plan differs')
    plan = b.read(plan_pin['path'])
    require(plan['parent_python'] == b.pin(sys.executable), 'actual parent interpreter differs')
    guard(plan)
    require(token_owner(ctypes.c_void_p(-1)) == SID, 'parent token default owner differs')
    parent, owner, native, profile, fixture = preflight(plan, b, before=True)
    require(parent.worker.windows_sid() == SID and time.monotonic() < deadline, 'parent identity/original deadline refused')
    output_guard = b.load(plan['output_guard']['path'], 'component_protected_output_guard')
    lock = b.read(plan['output_guard_lock']['path'])
    cpu = output_guard.cpu_owner(lock)
    out = Path(plan['output_root'])
    policy_lock = json.loads(json.dumps(lock))
    policy_lock['build_path']['fresh_child'] = out.name
    policy = output_guard.identity_and_parent(cpu, out, policy_lock)
    mkdir(plan, out)
    for name in ['temp', 'private-census']:
        mkdir(plan, out / name)
    paths = {'native_result': out / 'native-result.json', 'operations': out / 'native-result.operations.jsonl',
             'stdout': out / 'stdout-stderr.log'}
    for path in [*paths.values(), out / 'stdin.txt', out / 'execution.json']:
        guard(plan, path)
    env = dict(os.environ)
    for key in list(env):
        if key.upper().startswith(('AIRK_', 'ORT_', 'PYTHON', 'CUDA_', 'CUDNN_')):
            env.pop(key)
    env.update(PATH='C:/Windows/System32;C:/Windows', TEMP=str(out / 'temp'), TMP=str(out / 'temp'), TMPDIR=str(out / 'temp'),
               AIRK_PRIVATE_CENSUS_BASE=plan['private_census'], AIRK_CUDA_QUALIFICATION_INPUT=plan['request']['path'],
               AIRK_CUDA_QUALIFICATION_INPUT_SHA256=plan['request']['sha256'], AIRK_CUDA_QUALIFICATION_OUTPUT=str(paths['native_result']))
    argv = [plan['native_image']['path'], '--exact', SELECTOR, '--ignored', '--nocapture', '--test-threads=1']
    receipt = {'schema': 'airk.private.owned-cuda-component/1', 'plan': plan_pin, 'command': argv,
               'started_utc': started_utc.isoformat(), 'original_deadline': original_deadline.isoformat(),
               'original_deadline_monotonic': deadline,
               'execution_user_sid': SID, 'parent_pid': os.getpid(), 'parent_python': b.pin(sys.executable),
               'output_policy': policy, 'input_seals': dict(plan['input_seals'], **{plan_pin['path']: plan_pin['sha256']}),
               'whole_phase_deadline_not_refreshed': True, 'complete': False, 'failure': None,
               'post_input_drift': [], 'qualification': False, 'product_admission': False}
    try:
        def authenticate(api, job, info, child_deadline):
            proof = parent.suspended_authentication(api, info, deadline, child_deadline, SID, plan['native_image'], owner.image_proof)
            require(token_owner(info.process) == SID, 'suspended native default token owner differs')
            proof['default_owner_sid'] = SID
            proof['component_job'] = configure_job(api, job)
            require(time.monotonic() < deadline, 'original phase expired during suspended Job authentication')
            return proof
        with (out / 'stdin.txt').open('xb') as stream:
            stream.flush()
        guard(plan, out / 'stdin.txt', True)
        remaining = deadline - time.monotonic()
        require(remaining > 0, 'original phase expired before dispatch')
        with (out / 'stdin.txt').open('rb') as initial, paths['stdout'].open('xb') as stdout:
            receipt['owned'] = owner.run_windows_owned_training(argv, ROOT, stdout, remaining, env, initial,
                16 << 30, before_resume=authenticate, cleanup_seconds=5, absolute_deadline=deadline)
        require(parent.settled(receipt['owned']) and finite(receipt['owned'].get('peak_host_committed_bytes'), 1, 24 << 30),
                'original child did not settle naturally within bounds')
        for label, path in paths.items():
            guard(plan, path, True)
            receipt[label] = b.pin(path)
            require(receipt[label]['bytes'] <= PROOFS[label], 'native output cap exceeded')
        value = b.read(paths['native_result'])
        validate_native(value, plan, receipt['owned']['owned_leader_pid'], native, profile, fixture)
        events = [json.loads(line) for line in paths['operations'].read_text(encoding='utf-8').splitlines()]
        validate_operations(events, value)
        receipt['complete'] = True
    except BaseException as error:
        receipt['failure'] = {'type': type(error).__name__, 'message': str(error)}
        if hasattr(error, 'owned_cleanup'):
            receipt['actual_cleanup_failure'] = error.owned_cleanup
        raise
    finally:
        original = sys.exc_info()[1]
        receipt['final_input_errors'] = []
        # Attempt every original input after failure too; missing inputs cannot
        # disappear behind the first native error or become successful cleanup.
        for path, sha in receipt['input_seals'].items():
            try:
                require(b.digest(b.regular(path)) == sha, 'original input drift')
            except BaseException as error:
                receipt['post_input_drift'].append(path)
                receipt['final_input_errors'].append({'path': path, 'type': type(error).__name__, 'message': str(error)})
        try:
            require(b.census(plan['runtime_root']) == plan['runtime_files'], 'final runtime roster drift')
            for path in [out, *out.rglob('*')]:
                guard(plan, path, True)
        except BaseException as error:
            receipt['final_input_errors'].append({'type': type(error).__name__, 'message': str(error)})
        if receipt['final_input_errors']:
            receipt['complete'] = False
            receipt['failure'] = receipt.get('failure') or receipt['final_input_errors'][0]
        def final_hash(path):
            guard(plan, path, True)
            return b.pin(path)
        finish(receipt, paths, started, deadline, final_hash)
        write_new(plan, out / 'execution.json', receipt)
        if original is None and not receipt['complete']:
            raise ValueError(receipt['failure'])
    return receipt


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    modes = parser.add_mutually_exclusive_group(required=True)
    modes.add_argument('--prepare', action='store_true')
    modes.add_argument('--execute-reviewed-component', action='store_true')
    parser.add_argument('--plan', type=Path, required=True)
    parser.add_argument('--plan-sha256')
    parser.add_argument('--approved-script-sha256', required=True)
    parser.add_argument('--metadata-receipt', type=Path)
    parser.add_argument('--metadata-receipt-sha256')
    parser.add_argument('--image-packet', type=Path)
    parser.add_argument('--image-packet-sha256')
    parser.add_argument('--output-root', type=Path)
    parser.add_argument('--powershell', type=Path)
    args = parser.parse_args()
    require(base().digest(__file__) == args.approved_script_sha256, 'reviewed maintained script SHA required')
    result = prepare(args) if args.prepare else run(args)
    print(json.dumps(result if args.prepare else {'complete': result['complete'], 'elapsed_seconds': result['elapsed_seconds']}))


if __name__ == '__main__':
    main()
