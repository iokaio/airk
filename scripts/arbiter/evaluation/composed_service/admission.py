# SPDX-License-Identifier: Apache-2.0
"""Composed adapter preflight; no process execution or qualification authority."""
import hashlib
import json
import math
from pathlib import Path
import stat
import types

ROOT = Path(__file__).resolve().parents[4]
HERE = Path(__file__).resolve().parent
OBSERVER = ROOT / 'scripts/arbiter/evaluation/research_history/composed_orchestration/os_measurement--c81d48c34df4.py'
OBSERVER_SHA = '75355bdecbeb75e436053b1118841268a0387cf96552d5b083e5b4f46a31e196'
SOURCE_NAMES = ('admission.py', 'prepare.py', 'run_composed.py', 'native_policy.py',
                'native_delivery.py', 'session_controller.cjs', 'native_policy.cjs',
                'native_delivery.cjs', 'native_eval_session.cjs', 'evidence_checks.py',
                'assemble_observations.py')
MAX_METADATA = 16 << 20


def require(value, message):
    if not value:
        raise ValueError(message)


def object_pairs(pairs):
    value = {}
    for key, item in pairs:
        require(key not in value, 'duplicate JSON key')
        value[key] = item
    return value


def parse(raw):
    require(len(raw) <= MAX_METADATA, 'bounded metadata required')
    return json.loads(raw, object_pairs_hook=object_pairs,
                      parse_constant=lambda value: (_ for _ in ()).throw(ValueError('nonfinite JSON')))


def regular(path):
    path = Path(path)
    require(path.is_absolute() and path.resolve(strict=True) == path, 'canonical absolute input required')
    for item in (path, *path.parents):
        info = item.lstat()
        require(not item.is_symlink() and not getattr(info, 'st_file_attributes', 0) & 0x400,
                'aliased input refused')
    require(stat.S_ISREG(path.stat().st_mode), 'ordinary input required')
    return path


def digest(path):
    with regular(path).open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def captured(pin):
    path = regular(pin['path'])
    with path.open('rb') as stream:
        raw = stream.read(MAX_METADATA + 1)
    require(len(raw) <= MAX_METADATA, 'bounded metadata required')
    require(hashlib.sha256(raw).hexdigest() == pin['sha256'], 'consumed bytes differ from pin')
    if 'bytes' in pin:
        require(type(pin['bytes']) is int and pin['bytes'] == len(raw), 'consumed length differs')
    return raw


def read_json_pin(pin):
    return parse(captured(pin))


def private_path(path, *, exists=False):
    path = Path(path)
    base = ROOT / 'artifacts'
    require(path.is_absolute() and path != base and path.is_relative_to(base), 'explicit private artifacts path required')
    require(path.resolve(strict=exists) == path, 'private path aliases refused')
    for item in (path, *path.parents):
        if item.exists():
            require(not item.is_symlink() and not getattr(item.lstat(), 'st_file_attributes', 0) & 0x400,
                    'private path reparse point refused')
    return path


def remaining_ms(deadline, now):
    require(type(deadline) in (int, float) and type(now) in (int, float)
            and math.isfinite(deadline) and math.isfinite(now), 'finite original clock required')
    value = math.floor((deadline - now) * 1000)
    if value < 1:
        raise TimeoutError('original allowance exhausted before dispatch')
    return value


def validate_policy(policy, delivery):
    # Admission to this current campaign, not an alternate native serializer.
    require(policy.get('schema') == 'airk.arbiter.resource-policy/2'
            and type(policy.get('deadline_ms')) is int and policy['deadline_ms'] == 120000
            and type(policy.get('shared_input_tokens')) is int and policy['shared_input_tokens'] == 8192
            and type(policy.get('focus_pair_input_tokens')) is int and policy['focus_pair_input_tokens'] == 512,
            'current native policy required')
    require(delivery.get('schema') == 'airk.arbiter.budget-cases/1'
            and delivery.get('operation_deadline_ms') == policy['deadline_ms']
            and delivery.get('default_budget', {}).get('input_tokens') == policy['shared_input_tokens']
            and type(delivery.get('transport_deadline_ms')) is int
            and delivery['transport_deadline_ms'] == 125000, 'native transport/policy binding differs')


def validate_shape(plan, packet, policy, delivery):
    require(plan.get('schema') == 'airk.private.composed-execution-plan/1', 'private orchestration schema differs')
    require(plan.get('adapter') == 'airk.maintained.composed-service/1', 'fresh maintained adapter epoch required')
    limit = plan.get('whole_phase_seconds')
    require(type(limit) in (int, float) and math.isfinite(limit) and 0 < limit <= 1800, 'whole-phase bound differs')
    steps = plan.get('steps')
    require(isinstance(steps, list) and 1 <= len(steps) <= 200, 'bounded original step roster required')
    require(steps[-1].get('kind') == 'controller' and steps[-1].get('request', {}).get('kind') == 'finish',
            'explicit final original-session closure required')
    validate_policy(policy, delivery)
    require(packet.get('adapter') == 'airk.maintained.composed-service/1', 'maintained controller epoch required')
    require(packet.get('whole_phase_seconds') == limit
            and packet.get('resource_policy_input') == plan['resource_policy_input']
            and packet.get('native_delivery_input') == plan['native_delivery_input'],
            'controller/orchestration native input binding differs')
    for step in steps:
        require(step.get('kind') in ('controller', 'native_command', 'os_sample', 'await_ready'), 'unreviewed step kind')
        if step['kind'] == 'controller':
            request = step['request']
            require(request.get('kind') in ('open', 'start', 'await', 'abort', 'close', 'watch', 'finish'), 'unknown controller action')
            if request.get('kind') == 'watch':
                require(type(request.get('timeout_ms')) is int and 1 <= request['timeout_ms'] <= limit * 1000, 'bounded event wait required')
            if request.get('kind') == 'start':
                require(type(request.get('deadline_ms')) is int and 1 <= request['deadline_ms'] <= delivery['transport_deadline_ms'],
                        'original transport deadline exceeds native fixture')
            if request.get('kind') == 'open':
                config = request['config']
                require(config.get('resource_policy_input') == plan['resource_policy_input']
                        and config.get('native_delivery_input') == plan['native_delivery_input'], 'session native policy binding differs')
                require(Path(config.get('repository_root', '')) == ROOT, 'session repository differs')
        elif step['kind'] in ('native_command', 'await_ready'):
            require(step.get('argv') and step['argv'][0] in plan['allowed_native_images'], 'unsealed native executable')
            duration = step.get('deadline_seconds')
            require(type(duration) in (int, float) and math.isfinite(duration) and 0 < duration <= limit,
                    'native command deadline exceeds original phase')


def validate_plan(plan, *, reader=read_json_pin, hasher=digest):
    pins = plan.get('pins')
    require(isinstance(pins, dict) and 1 <= len(pins) <= 16380, 'complete bounded input seal required')
    for path, expected in pins.items():
        require(isinstance(expected, str) and len(expected) == 64 and hasher(path) == expected, 'input seal drift: ' + path)
    expected_sources = {str(HERE / name) for name in SOURCE_NAMES} | {str(OBSERVER)}
    require(set(plan.get('source_seals', {})) == expected_sources, 'exact maintained helper closure required')
    require(all(pins.get(path) == expected for path, expected in plan['source_seals'].items()), 'source closure omitted from complete seals')
    require(pins[str(OBSERVER)] == OBSERVER_SHA, 'single mapped OS observer identity differs')
    require(Path(plan['controller']) == HERE / 'session_controller.cjs', 'historical controller execution refused')
    private_path(plan['output_root'])
    private_path(plan['controller_packet'], exists=True)
    packet = reader({'path': plan['controller_packet'], 'sha256': pins[plan['controller_packet']]})
    policy = reader(plan['resource_policy_input'])['policy']
    delivery = reader(plan['native_delivery_input'])
    for item in (plan['resource_policy_input'], plan['native_delivery_input']):
        require(pins.get(item['path']) == item['sha256'], 'native policy/fixture omitted from complete seals')
    require(Path(packet['session_helper']) == HERE / 'native_eval_session.cjs', 'unreviewed session helper')
    require(Path(packet['session_library']) == ROOT / 'packages/vscode/out/airkctl/session.js', 'actual compiled SDK required')
    required = {packet['session_helper'], packet['session_library'], *packet['client_dependencies'],
                str(HERE / 'native_policy.cjs'), str(HERE / 'native_delivery.cjs'), plan['controller']}
    require(all(packet['pins'].get(path) == pins.get(path) and path in pins for path in required), 'SDK/controller source omitted or changed')
    require(all(pins.get(path) == sha for path, sha in packet['pins'].items()), 'controller-only unreviewed input')
    require(str(ROOT / 'packages/vscode/out/generated/arbiterBudget.js') in required, 'current generated SDK budget dependency absent')
    for name in ('runner.js', 'adminChunks.js'):
        require(str(ROOT / 'packages/vscode/out/airkctl' / name) in required, 'compiled SDK dependency absent')
    import re
    sdk_root = ROOT / 'packages/vscode/out'
    for sdk in {packet['session_library'], *packet['client_dependencies']}:
        file = regular(sdk)
        require(file.is_relative_to(sdk_root), 'SDK dependency outside maintained compiled client')
        raw = captured({'path': str(file), 'sha256': pins[str(file)]})
        for relative in re.findall(rb'require\([\"\'](\.[^\"\']+)[\"\']\)', raw):
            dependency = (file.parent / relative.decode()).resolve()
            if dependency.suffix != '.js':
                dependency = dependency.with_suffix('.js')
            require(str(dependency) in required, 'transitive SDK dependency absent from closure')
    for executable in [plan['node'], *plan['allowed_native_images']]:
        require(executable in pins, 'actual executable omitted from complete seals')
    for step in plan['steps']:
        if step.get('kind') == 'controller':
            request = step['request']
            if request.get('kind') == 'open':
                config = request['config']
                private_path(config['transcript_dir'])
                require(Path(config['transcript_dir']).is_relative_to(Path(plan['output_root'])), 'transcript outside private run')
                for path in [config['fixture_ready'], config['index_bindings'],
                             str(Path(config['runtime_pair']) / 'airk-runtime.json'),
                             str(Path(config['runtime_pair']) / 'airkctl.exe')]:
                    require(path in pins, 'scope/runtime input omitted from seals')
            if request.get('kind') == 'start':
                private_path(request['args_file'], exists=True)
                require(request['args_file'] in pins, 'original request input omitted')
    validate_shape(plan, packet, policy, delivery)
    return packet


def load_os_observer():
    raw = captured({'path': str(OBSERVER), 'sha256': OBSERVER_SHA})
    module = types.ModuleType('airk_composed_os_observer')
    module.__file__ = str(OBSERVER)
    exec(compile(raw, str(OBSERVER), 'exec'), module.__dict__)
    return module
