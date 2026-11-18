# SPDX-License-Identifier: Apache-2.0
"""Own exactly four native CUDA metadata stages, with the existing NN0 custody."""
import argparse
import ctypes
import datetime
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import time
import types

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
SCRIPTS = ROOT / 'scripts'
SID = 'S-1-5-21-3678370870-3858642417-826968105-1002'
ANCHOR = Path('C:/Users/tdotn/AppData/Local/airk-final-exec-20261116T052942Z')
CAMPAIGN = datetime.datetime(2026, 11, 19, 5, 29, 42, tzinfo=datetime.timezone.utc)
LIMITS = {'wall_seconds': 120, 'host_committed_bytes': 8 << 30, 'log_bytes': 64 << 20}
PLAN_SCHEMA = 'airk.private.cuda-profile-nn0-plan/1'
REQUEST_SCHEMA = 'airk.private.cuda-component-probe-request/1'
RECEIPT_SCHEMA = 'airk.private.owned-cuda-profile-nn0/1'
GUARD_SHA = 'cfb22eb4058820994980e2e5b4a1b651672823b721bc8804b4243cc8631af795'
LOCK_SHA = '549be258fd21be030f994f6da7f74146a9e9edd3bb611bee83305d9a4deade6d'
PREFIX = 'inference_engine::pool::arbiter::qualification::software_v2::cuda_qualification::'
SELECTORS = {
    'assembly-request': 'emit_cuda_component_probe_request',
    'assembly': 'emit_cuda_runtime_manifest_from_held_native_closure',
    'request': 'emit_cuda_component_probe_request',
    'metadata': 'emit_cuda_fp32_probe_metadata',
}
PROFILE_LEAVES = ('profile.json', 'native-input.json', 'probe-request.json',
                  'commands.json', 'metadata-receipt.json')
QUAL_INPUT = 'AIRK_CUDA_QUALIFICATION_INPUT'
REQUEST_OUT = 'AIRK_CUDA_REQUEST_OUTPUT'
MANIFEST_OUT = 'AIRK_CUDA_QUALIFICATION_OUTPUT'
METADATA_OUT = 'AIRK_CUDA_METADATA_DIRECTORY'
BUILD_PROOF = 'AIRK_CUDA_CHECKPOINTED_PROOF'
CPU_INPUT = 'AIRK_SOFTWARE_V2_NATIVE_INPUT'
MANIFEST_INPUT = 'AIRK_CUDA_RUNTIME_MANIFEST'
DEADLINE_MS = 'AIRK_CUDA_DEADLINE_MS'
BOUNDS = {
    'host_limit_bytes': 'AIRK_CUDA_HOST_LIMIT_BYTES',
    'device_limit_bytes': 'AIRK_CUDA_DEVICE_LIMIT_BYTES',
    'cold_bytes': 'AIRK_CUDA_COLD_BYTES',
    'warm_bytes': 'AIRK_CUDA_WARM_BYTES',
    'headroom_bytes': 'AIRK_CUDA_HEADROOM_BYTES',
    'job_limit_bytes': 'AIRK_CUDA_JOB_LIMIT_BYTES',
}
ASSEMBLY_LISTS = {
    'module_names': 'AIRK_CUDA_MODULE_NAMES',
    'load_order': 'AIRK_CUDA_LOAD_ORDER',
    'system_imports': 'AIRK_CUDA_SYSTEM_IMPORTS',
}
ENV_PINS = {'assembly-request': (BUILD_PROOF,), 'assembly': (QUAL_INPUT,),
            'request': (CPU_INPUT, MANIFEST_INPUT), 'metadata': (QUAL_INPUT,)}
ENV_FIELDS = {
    'assembly-request': {'AIRK_CUDA_REQUEST_KIND', 'AIRK_CUDA_RUNTIME_ROOT',
                         'AIRK_CUDA_PRODUCER_NAME', 'AIRK_CUDA_PRODUCER_SHA256',
                         *ASSEMBLY_LISTS.values(), DEADLINE_MS, REQUEST_OUT},
    'assembly': {MANIFEST_OUT},
    'request': {'AIRK_CUDA_REQUEST_KIND', DEADLINE_MS, REQUEST_OUT, *BOUNDS.values()},
    'metadata': {METADATA_OUT},
}
OWNER_PATHS = {
    'wrapper': HERE / 'metadata.py',
    'custody_owner': SCRIPTS / 'cuda-native-execution.py',
    'component_owner': SCRIPTS / 'cuda-component-execution.py',
    'checked_guard': SCRIPTS / 'dev/assert-human-execution.ps1',
    'output_guard': SCRIPTS / 'cuda-static-runtime/build/r8/provision_cuda_runtime.py',
    'output_guard_lock': SCRIPTS / 'cuda-static-runtime/build/r8/cuda-runtime-producer-lock.json',
}
JSON_CAP = 16 << 20
ASSEMBLY_CAP = 128 << 10


def require(ok, message):
    if not ok:
        raise ValueError(message)


def check(deadline, clock=time.monotonic):
    if clock() >= deadline:
        raise TimeoutError('original metadata phase expired')


def sha256(value):
    return (isinstance(value, str) and len(value) == 64
            and all(ch in '0123456789abcdef' for ch in value))


def canonical_file(path):
    path = Path(path)
    require(path.is_absolute() and path.resolve(strict=True) == path and path.is_file(),
            'canonical absolute regular input required')
    for current in (path, *path.parents):
        require(not current.is_symlink() and not getattr(current.lstat(), 'st_file_attributes', 0) & 0x400,
                'input alias refused')
    return path


def file_observation(path, deadline, *, cap=None, keep=False):
    check(deadline)
    path = canonical_file(path)
    digest = hashlib.sha256()
    data = bytearray() if keep else None
    with path.open('rb') as stream:
        before = os.fstat(stream.fileno())
        require(cap is None or before.st_size <= cap, f'input exceeds byte bound: {path}')
        size = 0
        while True:
            check(deadline)
            block = stream.read(min(1 << 20, before.st_size - size + 1))
            if not block:
                break
            size += len(block)
            require(size <= before.st_size, f'input grew while reading: {path}')
            digest.update(block)
            if data is not None:
                data.extend(block)
        after = os.fstat(stream.fileno())
    require(size == before.st_size and identity(before) == identity(after) == identity(path.stat()),
            f'input changed while reading: {path}')
    check(deadline)
    pin = {'path': str(path), 'bytes': size, 'sha256': digest.hexdigest()}
    return pin, bytes(data) if data is not None else None


def identity(st):
    return st.st_dev, st.st_ino, st.st_size, st.st_mtime_ns, st.st_ctime_ns


def read_json(path, deadline, cap=JSON_CAP):
    row, data = file_observation(path, deadline, cap=cap, keep=True)
    value = json.loads(data)
    check(deadline)
    return row, value


def load_reviewed(row, path, name, deadline):
    observed, data = file_observation(path, deadline, cap=1 << 20, keep=True)
    require(observed == row, f'exact maintained owner pin differs: {path}')
    module = types.ModuleType(name)
    module.__file__ = observed['path']
    exec(compile(data, module.__file__, 'exec'), module.__dict__)
    check(deadline)
    return module


def env_pin(env, name):
    return {'path': env[name], 'sha256': env[name + '_SHA256']}


def unsigned(value):
    require(isinstance(value, str) and value.isascii() and value.isdecimal() and len(value) <= 20,
            'invalid Rust metadata unsigned environment value')
    result = int(value)
    require(result < 1 << 64, 'out-of-range Rust metadata environment value')
    return result


def safe_leaf(value):
    return (isinstance(value, str) and value not in ('', '.', '..')
            and all(ch not in value for ch in ('/', '\\', ':', '\0')) and Path(value).name == value)


def equivalent(expected, actual):
    return (isinstance(actual, dict) and set(actual) == {'path', 'sha256'}
            and Path(actual['path']) == Path(expected['path']) and actual['sha256'] == expected['sha256'])


def request_value(value, kind):
    require(isinstance(value, dict) and value.get('schema') == REQUEST_SCHEMA
            and value.get('kind') == kind, f'exact native {kind} request required')
    return value


def expected_prepare(env):
    numbers = {name: unsigned(env[key]) for name, key in BOUNDS.items()}
    bounds = {'schema': 'airk.arbiter.gpu-probe-reservation/1',
              'deadline_ms': unsigned(env[DEADLINE_MS])}
    for name in ('host_limit_bytes', 'device_limit_bytes'):
        bounds[name] = numbers.pop(name)
    return {'schema': REQUEST_SCHEMA, 'kind': 'prepare', 'cpu_input': env_pin(env, CPU_INPUT),
            'runtime_manifest': env_pin(env, MANIFEST_INPUT), 'bounds': bounds, **numbers}


def assembly_input(env):
    lists = {}
    for name, key in ASSEMBLY_LISTS.items():
        items = env[key].split(';') if env[key] else []
        require(len(items) <= 256 and len(set(items)) == len(items)
                and all(safe_leaf(item) for item in items), 'invalid native assembly name roster')
        lists[name] = sorted(items) if name == 'system_imports' else items
    require(2 <= len(lists['module_names']) <= 64
            and set(lists['load_order']) == set(lists['module_names'])
            and safe_leaf(env['AIRK_CUDA_PRODUCER_NAME'])
            and sha256(env['AIRK_CUDA_PRODUCER_SHA256']), 'invalid fresh assembly runtime plan')
    return {'directory': env['AIRK_CUDA_RUNTIME_ROOT'],
            'producer_name': env['AIRK_CUDA_PRODUCER_NAME'],
            'producer_sha256': env['AIRK_CUDA_PRODUCER_SHA256'],
            'checkpointed_proof': env_pin(env, BUILD_PROOF), **lists}


def stage_paths(plan, current_request=None):
    kind, env = plan['kind'], plan['environment']
    require(kind in SELECTORS and plan['selector'] == PREFIX + SELECTORS[kind], 'native stage selector differs')
    keys = ENV_FIELDS[kind] | {key for base in ENV_PINS[kind] for key in (base, base + '_SHA256')}
    require(isinstance(env, dict) and set(env) == keys
            and all(isinstance(value, str) and '\0' not in value for value in env.values()),
            'exact stage-specific environment required')
    root = Path(plan['output_root'])
    require(root.is_absolute() and root.parent == ANCHOR and safe_leaf(root.name),
            'fresh direct child of the reviewed campaign root required')
    create = None
    if kind in ('assembly-request', 'request'):
        mode = 'assembly' if kind == 'assembly-request' else 'prepare'
        require(env['AIRK_CUDA_REQUEST_KIND'] == mode and env[DEADLINE_MS] == '120000',
                'native request mode or deadline differs')
        outputs = [Path(env[REQUEST_OUT])]
        require(outputs[0].parent == root and safe_leaf(outputs[0].name)
                and outputs[0].name.lower() not in {'execution.json', 'stdin.txt', 'stdout-stderr.log', 'temp'},
                'request must be create-only in its own stage root')
    elif kind == 'assembly':
        manifest = Path(env[MANIFEST_OUT])
        require(manifest.name == 'airk-cuda-runtime.json'
                and (manifest.is_relative_to(ANCHOR) or manifest.is_relative_to(ROOT))
                and not manifest.is_relative_to(root), 'exact create-only runtime descriptor path required')
        if current_request is not None:
            expected = Path(request_value(current_request, 'assembly')['assembly']['directory'])
            require(expected == manifest.parent, 'manifest directory differs from native assembly input')
        outputs = [manifest, manifest.with_suffix('.receipt.json')]
    else:
        create = Path(env[METADATA_OUT])
        require(create.parent == root and safe_leaf(create.name)
                and create.name.lower() not in {'execution.json', 'stdin.txt', 'stdout-stderr.log', 'temp'},
                'six-file metadata directory must be fresh under the guarded stage root')
        outputs = [create / name for name in PROFILE_LEAVES]
    require([str(path) for path in outputs] == plan['output_pins_expected'],
            'exact expected native output roster differs')
    return root, create, outputs


def output_values(kind, outputs, rows, plan, inputs, owned, read):
    env = plan['environment']
    if kind in ('assembly-request', 'request'):
        observed = read(outputs[0])
        expected = expected_prepare(env) if kind == 'request' else {
            'schema': REQUEST_SCHEMA, 'kind': 'assembly', 'assembly': assembly_input(env),
            'deadline_ms': unsigned(env[DEADLINE_MS]),
        }
        require(observed == expected, 'native emitted request differs from reviewed metadata inputs')
        return
    inner = read(outputs[-1 if kind == 'assembly' else 4])
    elapsed = inner.get('elapsed_us')
    require(type(elapsed) is int and 0 <= elapsed < 120_000_000
            and inner.get('model_requests_dispatched') is False
            and inner.get('forward_count_observed', 'missing') is None,
            'inner native receipt scope or clock differs')
    if kind == 'assembly':
        manifest = read(outputs[0], ASSEMBLY_CAP)
        assembly = inputs[QUAL_INPUT]['assembly']
        require(inner.get('schema') == 'airk.private.cuda-runtime-assembly-emission/1'
                and inner.get('approved') is False and inner.get('fits_empty') is True
                and inner.get('runtime_admission_dispatched') is False
                and type(inner.get('pid')) is int and inner['pid'] == owned['owned_leader_pid']
                and inner.get('execution_user') == SID
                and equivalent(env_pin(env, QUAL_INPUT), inner.get('input'))
                and equivalent(rows[0], inner.get('manifest')),
                'actual native assembly emission identity or scope differs')
        producer = assembly['producer_name']
        sealed = plan['input_seals']
        directory = Path(assembly['directory'])
        require(manifest.get('schema') == 'airk.native.cuda-runtime-manifest/1'
                and manifest.get('ort_version') == '1.30.0' and manifest.get('fits') == []
                and manifest.get('static_core_build') == manifest.get('producer_sha256') == assembly['producer_sha256']
                and manifest.get('producer_name') == producer
                and manifest.get('load_order') == assembly['load_order']
                and manifest.get('system_imports') == assembly['system_imports']
                and [row['name'] for row in manifest['files']] == assembly['module_names']
                and manifest.get('producer_bytes') == inputs['runtime_pins'][producer]['bytes']
                and all(row['sha256'] == sealed.get(str(directory / row['name']))
                        and row['bytes'] == inputs['runtime_pins'][row['name']]['bytes']
                        for row in manifest['files']), 'native RuntimeManifest differs from held files')
        return
    profile, native, probe = (read(path) for path in outputs[:3])
    calibration = rows[5]
    asset = calibration_asset(profile)
    prepared = inputs[QUAL_INPUT]
    original = inputs[CPU_INPUT]
    expected_files = dict(original['files'], **{asset['name']: calibration['path']})
    require(set(native) == set(original)
            and all(native[key] == value for key, value in original.items() if key not in ('profile', 'files'))
            and set(native['files']) == set(expected_files)
            and all(Path(native['files'][key]) == Path(path) for key, path in expected_files.items())
            and profile.get('model') == inputs['cpu_model'],
            'native GPU input or profile changed original CPU source identity')
    require(inner.get('schema') == 'airk.private.cuda-probe-metadata/1'
            and inner.get('qualified') is False
            and equivalent(env_pin(env, QUAL_INPUT), inner.get('request'))
            and equivalent(prepared['cpu_input'], inner.get('cpu_input'))
            and equivalent(rows[2], inner.get('probe_request'))
            and inner.get('profile') == profile.get('manifest') and inner.get('model') == profile.get('model')
            and equivalent(rows[0], native.get('profile'))
            and (calibration['bytes'], calibration['sha256']) == (asset['bytes'], asset['sha256'])
            and equivalent(rows[1], request_value(probe, 'probe').get('gpu_input'))
            and all(probe.get(key) == prepared.get(key) for key in ('runtime_manifest', 'bounds',
                    'cold_bytes', 'warm_bytes', 'headroom_bytes', 'job_limit_bytes')),
            'native profile, calibration, input and probe links differ')
    selectors = (['cuda_selected_focus_fp32_512_component'] if profile['model'] == 'arbiter-focus-qwen3-reranker-06b'
                 else ['cuda_original_context_fp32_t17_component', 'cuda_original_context_fp32_t128_component'])
    commands = read(outputs[3])
    require(isinstance(commands, list) and len(commands) == len(selectors)
            and all(command.get('args') == ['--exact', PREFIX + selector, '--ignored', '--nocapture']
                    and Path(command['program']) == Path(plan['native_image']['path'])
                    and isinstance(command.get('env'), dict)
                    and set(command['env']) == {QUAL_INPUT, QUAL_INPUT + '_SHA256'}
                    and equivalent(rows[2], env_pin(command['env'], QUAL_INPUT))
                    for command, selector in zip(commands, selectors)),
            'native emitted command metadata differs from held image and probe')


def calibration_asset(profile):
    assets = [row for row in profile.get('assets', []) if row.get('role') == 'calibration']
    require(len(assets) == 1 and safe_leaf(assets[0].get('name'))
            and assets[0]['name'].lower() not in PROFILE_LEAVES, 'one distinct native calibration leaf required')
    return assets[0]


def seal(row, seals):
    require(Path(row['path']).is_absolute() and sha256(row['sha256'])
            and seals.get(str(Path(row['path']))) == row['sha256'],
            'required original metadata/source/image seal missing')


def verify_seals(seals, deadline, observe=None):
    drift = []
    for path, expected in seals.items():
        row, _ = file_observation(path, deadline)
        if row['sha256'] != expected:
            drift.append(path)
        if observe is not None:
            observe[path] = row
    return drift


def sealed_json(row, seals, deadline, cap=JSON_CAP):
    seal(row, seals)
    actual, value = read_json(row['path'], deadline, cap)
    require(equivalent(actual, row), 'retained original JSON changed while opening')
    return value


def cpu_closure(row, seals, deadline):
    native = sealed_json(row, seals, deadline, 1 << 20)
    require(native.get('schema') == 'airk.arbiter.resource-run/2', 'native CPU input schema differs')
    profile = sealed_json(native['profile'], seals, deadline, 1 << 20)
    seal(native['source_manifest'], seals)
    if native['numeric_fixture'] is not None:
        seal(native['numeric_fixture'], seals)
    files = native['files']
    assets = {asset['name']: asset['sha256'] for asset in profile['assets']}
    require(set(files) == set(assets) and len(assets) == len(profile['assets']),
            'CPU input does not name the exact original profile assets')
    for name, path in files.items():
        seal({'path': path, 'sha256': assets[name]}, seals)
    return native, profile['model']


def runtime_closure(assembly, seals, deadline):
    producer = assembly['producer_name']
    directory = Path(assembly['directory'])
    modules = assembly['module_names']
    require(directory.is_absolute() and producer not in modules and safe_leaf(producer)
            and 2 <= len(modules) <= 64 and len(set(modules)) == len(modules)
            and all(safe_leaf(name) for name in modules)
            and {'onnxruntime_providers_shared.dll', 'onnxruntime_providers_cuda.dll'} <= set(modules),
            'fresh runtime producer and provider roster differs')
    proof = sealed_json(assembly['checkpointed_proof'], seals, deadline, 1 << 20)
    require(proof.get('schema') == 'airk.private.cuda-fresh-static-build-proof/1'
            and proof.get('qualification') is False
            and proof['build_receipt']['sha256'] == assembly['producer_sha256'],
            'current-toolchain proof does not bind the original compile bytes')
    producer_path = str(directory / producer)
    seal({'path': producer_path, 'sha256': assembly['producer_sha256']}, seals)
    names = [producer, *modules]
    for name in names:
        path = str(directory / name)
        require(sha256(seals.get(path)), 'reviewed runtime file seal missing')
    return {name: str(directory / name) for name in names}


def manifest_scope(raw):
    require(raw.get('schema') == 'airk.native.cuda-runtime-manifest/1' and raw.get('ort_version') == '1.30.0'
            and raw.get('fits') == [] and sha256(raw.get('static_core_build'))
            and raw.get('producer_sha256') == raw['static_core_build'],
            'original actual fits-empty runtime manifest required')


def initial_inputs(plan, deadline):
    kind, env, seals = plan['kind'], plan['environment'], plan['input_seals']
    for name in ENV_PINS[kind]:
        seal(env_pin(env, name), seals)
    if kind == 'assembly-request':
        runtime_closure(assembly_input(env), seals, deadline)
        return {}
    if kind == 'request':
        prepare = expected_prepare(env)
    else:
        request = sealed_json(env_pin(env, QUAL_INPUT), seals, deadline, 1 << 20)
        if kind == 'assembly':
            assembly = request_value(request, 'assembly')['assembly']
            require(request.get('deadline_ms') == 120_000 and 'incremental_proof' not in assembly
                    and 'custody_copies' not in assembly and 'historical_tool_copies' not in assembly,
                    'exact fresh assembly request and original deadline required')
            runtime = runtime_closure(assembly, seals, deadline)
            return {QUAL_INPUT: request, 'runtime_paths': runtime}
        prepare = request_value(request, 'prepare')
    require(prepare['bounds'].get('deadline_ms') == 120_000, 'original native profile deadline differs')
    cpu_native, cpu_model = cpu_closure(prepare['cpu_input'], seals, deadline)
    raw = sealed_json(prepare['runtime_manifest'], seals, deadline, ASSEMBLY_CAP)
    manifest_scope(raw)
    return {QUAL_INPUT: prepare, CPU_INPUT: cpu_native, 'cpu_model': cpu_model}


def protected_policy(plan, deadline):
    owner = load_reviewed(plan['output_guard'], OWNER_PATHS['output_guard'], 'metadata_output_policy', deadline)
    lock = sealed_json({key: plan['output_guard_lock'][key] for key in ('path', 'sha256')},
                       plan['input_seals'], deadline)
    cpu_path = SCRIPTS / 'provision-static-onnx-runtime.py'
    cpu_lock = SCRIPTS / 'static-onnx-runtime-lock.json'
    cpu_pin, _ = file_observation(cpu_path, deadline, cap=1 << 20)
    lock_pin, cpu_value = read_json(cpu_lock, deadline)
    require(cpu_pin['sha256'] == lock['cpu_owner_sha256'] and lock_pin['sha256'] == lock['cpu_lock_sha256'],
            'original CPU output-policy source differs')
    seal(cpu_pin, plan['input_seals'])
    seal(lock_pin, plan['input_seals'])
    cpu = load_reviewed(cpu_pin, cpu_path, 'metadata_cpu_output_policy', deadline)
    require(cpu_value == cpu.LOCK, 'CPU policy loaded a different lock')
    cpu.validate_lock(cpu_value)

    def bounded_run(argv, **options):
        check(deadline)
        if argv[0] == 'pwsh':
            argv = [plan['powershell']['path'], *argv[1:]]
        options['timeout'] = min(options['timeout'], deadline - time.monotonic())
        require(options['timeout'] > 0, 'original policy observation deadline expired')
        result = subprocess.run(argv, **options)
        check(deadline)
        return result

    owner.subprocess = types.SimpleNamespace(run=bounded_run)
    root = Path(plan['output_root'])
    lock['build_path']['fresh_child'] = root.name
    return owner.identity_and_parent(cpu, root, lock)


def bootstrap(plan, main_sha, deadline):
    require(plan.get('schema') == PLAN_SCHEMA and plan.get('human_sid') == SID
            and Path(plan['repository_root']) == ROOT and plan.get('limits') == LIMITS,
            'exact reviewed NN0 metadata plan and bounds required')
    seals = plan['input_seals']
    require(isinstance(seals, dict) and 0 < len(seals) <= 16_380
            and all(Path(path).is_absolute() and str(Path(path)) == path and sha256(sha)
                    for path, sha in seals.items()),
            'bounded canonical original input seals required')
    for key, path in OWNER_PATHS.items():
        row = plan[key]
        require(Path(row['path']) == path and file_observation(path, deadline, cap=1 << 20)[0] == row,
                f'actual maintained owner differs: {key}')
        seal(row, seals)
    require(plan['wrapper']['sha256'] == main_sha
            and plan['output_guard']['sha256'] == GUARD_SHA and plan['output_guard_lock']['sha256'] == LOCK_SHA,
            'reviewed wrapper and protected output policy required')
    for key in ('parent_python', 'powershell', 'native_image', 'image_packet', 'native_build_receipt'):
        row = plan[key]
        require(file_observation(row['path'], deadline)[0] == row, f'original metadata pin differs: {key}')
        seal(row, seals)
    require(Path(plan['powershell']['path']).name.lower() == 'pwsh.exe'
            and Path(plan['parent_python']['path']) == Path(sys.executable),
            'actual reviewed Windows parent and PowerShell required')
    base = load_reviewed(plan['custody_owner'], OWNER_PATHS['custody_owner'], 'metadata_package_owner', deadline)
    component = load_reviewed(plan['component_owner'], OWNER_PATHS['component_owner'],
                              'metadata_component_custody_owner', deadline)
    base.digest = lambda path: file_observation(path, deadline)[0]['sha256']
    base.pin = lambda path: file_observation(path, deadline)[0]
    base.read = lambda path, limit=JSON_CAP: read_json(path, deadline, limit)[1]
    require(base.CAMPAIGN == CAMPAIGN and base.SID == component.SID == SID
            and Path(plan['ownership_helper_root']) == base.MAINTAINED_HELPER_ROOT,
            'shared original ownership constants or maintained source root differ')
    for name, sha in base.HELPERS.items():
        seal({'path': str(base.MAINTAINED_HELPER_ROOT / name), 'sha256': sha}, seals)
    for relative, sha in plan['required_native_sources'].items():
        source = Path(relative)
        require(not source.is_absolute() and '..' not in source.parts, 'native source must be repository relative')
        seal({'path': str(ROOT / source), 'sha256': sha}, seals)
    component.guard(plan, deadline=deadline)
    require(component.token_owner(ctypes.c_void_p(-1)) == SID, 'parent default token owner differs')
    base.source_image(plan)
    parent, owner = base.helpers(plan)
    require(parent.worker.windows_sid() == SID, 'actual parent SID differs')
    owner.digest = base.digest
    return component, parent, owner


def final_outputs(plan, paths, inputs, owned, guard, out, deadline):
    kind = plan['kind']
    log = out / 'stdout-stderr.log'
    guard([log, out / 'stdin.txt', *paths], True)
    if kind == 'metadata':
        directory = paths[0].parent
        guard([directory], True)
        _, profile = read_json(paths[0], deadline)
        paths = [*paths, directory / calibration_asset(profile)['name']]
        require({child.name for child in directory.iterdir()} == {path.name for path in paths},
                'exact native six-file metadata roster required')
        guard(paths[-1:], True)
    guard([out / 'temp'], True)
    require(not any((out / 'temp').iterdir()), 'native child left temporary outputs')
    pins = [file_observation(path, deadline, cap=JSON_CAP)[0] for path in paths]
    by_path = {row['path']: row for row in pins}

    def read(path, cap=JSON_CAP):
        row, value = read_json(path, deadline, cap)
        require(row == by_path[str(path)], 'native emission changed during independent verification')
        return value

    output_values(kind, paths, pins, plan, inputs, owned, read)
    return pins, file_observation(log, deadline, cap=LIMITS['log_bytes'])[0]


def run(plan_path, plan_sha, approved_script_sha):
    started = time.monotonic()
    utc = datetime.datetime.now(datetime.timezone.utc)
    cutoff = min(utc + datetime.timedelta(seconds=LIMITS['wall_seconds']), CAMPAIGN)
    deadline = started + (cutoff - utc).total_seconds()
    require(os.name == 'nt' and sys.flags.dont_write_bytecode and sys.flags.isolated,
            'explicit isolated Windows Python with bytecode writes disabled is required')
    check(deadline)
    main_pin, _ = file_observation(OWNER_PATHS['wrapper'], deadline, cap=1 << 20)
    plan_pin, plan = read_json(plan_path, deadline, 4 << 20)
    require(main_pin['sha256'] == approved_script_sha and plan_pin['sha256'] == plan_sha,
            'exact reviewed wrapper and metadata plan SHA required')
    root, create, paths = stage_paths(plan)
    require(not root.exists(), 'reviewed metadata run root must be fresh')
    component, parent, owner = bootstrap(plan, approved_script_sha, deadline)
    require(plan_pin['path'] not in plan['input_seals'], 'plan cannot self-seal')
    seals = dict(plan['input_seals'], **{plan_pin['path']: plan_pin['sha256']})
    inputs = initial_inputs(plan, deadline)
    kind = plan['kind']
    stage_paths(plan, inputs.get(QUAL_INPUT))
    require(not any(path.exists() for path in paths), 'native create-only output already exists')
    observed = {} if kind == 'assembly' else None
    drift = verify_seals(seals, deadline, observed)
    require(not drift, f'original metadata input drift: {drift}')
    if observed is not None:
        inputs['runtime_pins'] = {name: observed[path] for name, path in inputs['runtime_paths'].items()}
    policy = protected_policy(plan, deadline)

    def guard(targets, created=False):
        return component.guard_many(plan, targets, created, deadline=deadline)

    if kind == 'assembly':
        guard(paths)
    component.mkdir(plan, root, deadline=deadline)
    argv = [plan['native_image']['path'], '--exact', plan['selector'], '--ignored', '--nocapture', '--test-threads=1']
    receipt = {'schema': RECEIPT_SCHEMA, 'plan': plan_pin, 'selector': plan['selector'], 'kind': kind,
               'command': argv, 'input_seals': seals, 'execution_user_sid': SID, 'parent_pid': os.getpid(),
               'parent_python': plan['parent_python'], 'output_policy': policy,
               'started_utc': utc.isoformat(), 'original_deadline': cutoff.isoformat(),
               'original_deadline_monotonic': deadline,
               'complete': False, 'failure': None, 'post_input_drift': [],
               'whole_phase_deadline_not_refreshed': True, 'whole_phase_expired': False,
               'activation': False, 'qualification': False}

    def authenticate(api, job, info, child_deadline):
        proof = parent.suspended_authentication(api, info, deadline, child_deadline, SID,
                                                plan['native_image'], owner.image_proof)
        require(component.token_owner(info.process) == SID, 'suspended native default owner differs')
        proof['default_owner_sid'] = SID
        check(deadline)
        return proof

    error = None
    try:
        temp = root / 'temp'
        component.mkdir(plan, temp, deadline=deadline)
        log, stdin = root / 'stdout-stderr.log', root / 'stdin.txt'
        guarded_outputs = paths if kind != 'metadata' else [create]
        guard([stdin, log, *guarded_outputs])
        env = {key: value for key, value in os.environ.items()
               if not key.upper().startswith(('AIRK_', 'ORT_', 'PYTHON', 'CUDA_', 'CUDNN_', 'TEMP', 'TMP'))
               and key.upper() != 'PATH'}
        env.update(plan['environment'])
        env.update(PATH='C:/Windows/System32;C:/Windows', TEMP=str(temp), TMP=str(temp), TMPDIR=str(temp))
        with stdin.open('xb'):
            pass
        guard([stdin], True)
        with stdin.open('rb') as initial, log.open('xb') as stdout:
            guard([log], True)
            check(deadline)
            receipt['owned'] = owner.run_windows_owned_training(
                argv, ROOT, stdout, deadline - time.monotonic(), env, initial, LIMITS['host_committed_bytes'],
                before_resume=authenticate, absolute_deadline=deadline)
        owned = receipt['owned']
        commit = owned.get('peak_host_committed_bytes')
        suspended = owned.get('suspended_assignment', {})
        require(parent.settled(owned) and type(commit) is int and 0 < commit <= LIMITS['host_committed_bytes']
                and suspended.get('pid') == owned['owned_leader_pid']
                and suspended.get('execution_user_sid') == SID and suspended.get('default_owner_sid') == SID
                and equivalent(plan['native_image'], suspended.get('image')),
                'actual child failed natural one-process settlement, image, default-owner or memory evidence')
        rows, stdout = final_outputs(plan, paths, inputs, owned, guard, root, deadline)
        receipt['post_input_drift'] = verify_seals(seals, deadline)
        require(not receipt['post_input_drift'], 'post-execution original metadata input drift')
        guard([Path(row['path']) for row in [*rows, stdout]], True)
        if kind == 'metadata':
            require({p.name for p in create.iterdir()} == {Path(row['path']).name for row in rows},
                    'native profile output roster changed after validating original inputs')
        for row in [*rows, stdout]:
            cap = LIMITS['log_bytes'] if row is stdout else JSON_CAP
            actual, _ = file_observation(row['path'], deadline, cap=cap)
            require(actual == row, 'native output changed after validating original inputs')
        receipt.update(outputs=rows, stdout=stdout, complete=True)
    except BaseException as exc:
        error = exc
        receipt['failure'] = {'type': type(exc).__name__, 'message': str(exc)}
        if hasattr(exc, 'owned_cleanup'):
            receipt['actual_cleanup_failure'] = exc.owned_cleanup
        raise
    finally:
        parent.final_phase_observation(receipt, deadline, started)
        try:
            component.write_new(plan, root / 'execution.json', receipt)
        except Exception as retention_error:
            if error is None:
                raise
            error.add_note(f'owned metadata receipt retention failed: {retention_error}')
        if error is None and not receipt['complete']:
            raise TimeoutError('original metadata window expired during final verification')
    return receipt


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--execute-reviewed-nn0', required=True, action='store_true')
    parser.add_argument('--approved-script-sha256', required=True)
    parser.add_argument('--plan', required=True, type=Path)
    parser.add_argument('--plan-sha256', required=True)
    args = parser.parse_args()
    run(args.plan, args.plan_sha256, args.approved_script_sha256)


if __name__ == '__main__':
    main()
