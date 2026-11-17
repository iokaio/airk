# SPDX-License-Identifier: Apache-2.0
"""Fresh same-human single-child reference custody. No execution without --execute."""
import argparse
import ctypes
import importlib.util
import json
import math
import os
from pathlib import Path
import sys
import time
sys.dont_write_bytecode = True
import reference_worker as worker


def validate_limits(limits):
    expected = {'wall_seconds', 'host_committed_bytes', 'cuda_allocator_bytes', 'minimum_cuda_free_bytes', 'threads'}
    if set(limits) != expected or any(type(value) is not int for value in limits.values()):
        raise ValueError('complete explicit integral reference limits required')
    if not 1 <= limits['wall_seconds'] <= 1200 or not 1 << 30 <= limits['host_committed_bytes'] <= 64 << 30:
        raise ValueError('bounded reference phase wall/host limits required')
    if not 1 << 30 <= limits['cuda_allocator_bytes'] <= 36 << 30 or limits['minimum_cuda_free_bytes'] < limits['cuda_allocator_bytes'] + (2 << 30):
        raise ValueError('bounded CUDA allocator plus actual reserve required')
    if limits['threads'] not in (1, 2):
        raise ValueError('reviewed one/two-thread reference math required')


def preflight(plan):
    worker.require_supported(plan)
    validate_limits(plan['limits'])
    if plan.get('schema') != 'airk.private.independent-torch-plan/1':
        raise ValueError('unknown private experiment plan')
    request, template, profile, original = worker.native_documents(plan)
    if any(row.get('reference_same_artifact') is not None or row.get('reference_original_fp32') is not None for row in template['rows']):
        raise ValueError('producer requires the untouched native blank template, never prior expected scores')
    if any(not row['input'] or not isinstance(row['name'], str) for row in template['rows']):
        raise ValueError('native complete tensor rows required')
    required = [plan['native_input'], plan['native_template'], plan['original_roster'], plan['worker'], plan['reference_module'], plan['ownership_module'], plan['base_python'], plan['runtime_manifest']]
    required += [request['profile'], request['source_manifest'], original['original_source_input'], plan['original_export_graph']]
    if plan['kind'] == 'same_artifact' and plan['upstream'] == 'Qwen/Qwen3-Reranker-0.6B':
        required.append(plan['focus_adapter'])
    for pin in required:
        path = worker.pinned(pin)
        if plan['input_seals'].get(str(path)) != pin['sha256']:
            raise ValueError('exact phase input omitted from original seal: ' + str(path))
    for path in request['files'].values():
        if str(Path(path)) not in plan['input_seals']:
            raise ValueError('candidate asset omitted from phase seal')
    for file in original['original_reference_files'].values():
        if plan['input_seals'].get(file['path']) != file['sha256']:
            raise ValueError('original neural asset omitted from phase seal')
    for file in original['original_source_files'].values():
        if plan['input_seals'].get(file['path']) != file['sha256']:
            raise ValueError('original configuration/index omitted from phase seal')
    if Path(plan['worker']['path']) != Path(worker.__file__).resolve() or plan['input_seals'].get(str(Path(__file__).resolve())) != worker.digest(__file__):
        raise ValueError('actual parent/worker source is not the frozen source pair')
    runtime = json.loads(worker.pinned(plan['runtime_manifest']).read_bytes())
    if any(plan['input_seals'].get(path) != pin for path, pin in runtime['files'].items()):
        raise ValueError('runtime source omitted from actual phase input seals')
    if len(plan['input_seals']) > 16380:
        raise ValueError('phase input closure exceeds native seal census')
    return request, template, profile, original


def complete_arrays(result, template):
    if result.get('complete') is not True or result.get('torch_forward_invocations') != len(template['rows']):
        raise ValueError('actual successful Torch forwards incomplete')
    if len(result['rows']) != len(template['rows']):
        raise ValueError('partial actual score population')
    for observed, expected in zip(result['rows'], template['rows']):
        if observed['name'] != expected['name'] or observed['input'] != expected['input']:
            raise ValueError('actual input changed, cropped or reconstructed')
        if len(observed['scores']) != len(expected['input']['candidate_positions']) or not all(type(value) in (float, int) and math.isfinite(value) for value in observed['scores']):
            raise ValueError('actual score array incomplete/nonfinite')


def settled(owned):
    required = {'exit_code': 0, 'timed_out': False, 'log_oversized': False,
        'owned_descendants_settled': True, 'owned_job_forced_termination': False,
        'owned_job_final_active_processes': 0, 'owned_job_final_member_pids': [],
        'parent_assigned_to_job': False, 'child_assigned_before_resume': True,
        'owned_total_processes': 1, 'unexpected_third_process': False,
        'whole_process_deadline_not_refreshed': True}
    if any(type(owned.get(key)) is not type(value) or owned.get(key) != value for key, value in required.items()):
        return False
    exits = owned.get('retained_member_exits')
    if not isinstance(exits, list) or len(exits) != 1 or exits[0].get('exit_observed') is not True or type(exits[0].get('exit_code')) is not int or exits[0]['exit_code'] != 0:
        return False
    close = owned.get('handle_close_proof', {})
    required = {'forced': False, 'settled': True, 'final_active': 0, 'members_before': [], 'final_member_pids': []}
    return all(type(close.get(key)) is type(value) and close.get(key) == value for key, value in required.items())


def suspended_authentication(api, info, deadline, child_deadline, human_sid, python_pin, image_reader, sid_reader=worker.windows_sid, clock=time.monotonic):
    if child_deadline > deadline or clock() >= deadline:
        raise ValueError('original phase expired or child deadline was extended before identity authentication')
    sid = sid_reader(info.process)
    if sid != human_sid:
        raise ValueError('suspended child identity differs from verified human parent')
    if clock() >= deadline:
        raise ValueError('original phase expired during SID authentication; child cannot resume')
    image = image_reader(api, info.process, python_pin)
    if clock() >= deadline:
        raise ValueError('original phase expired during image authentication; child cannot resume')
    return {'pid': int(info.pid), 'image': image, 'execution_user_sid': sid}


def final_phase_observation(receipt, deadline, started, clock=time.monotonic):
    now = clock()
    receipt['whole_phase_expired'] = now >= deadline
    receipt['elapsed_seconds'] = now - started
    late_failure = receipt['whole_phase_expired'] and receipt['failure'] is None
    if receipt['whole_phase_expired']:
        receipt['complete'] = False
    if late_failure:
        receipt['failure'] = {'type': 'TimeoutError', 'message': 'original phase expired during final observation'}
    return late_failure


def run(plan_path, expected, output):
    plan = json.loads(worker.pinned({'path': str(plan_path), 'sha256': expected}).read_bytes())
    if worker.windows_sid() != plan['human_sid']:
        raise ValueError('parent is not the verified logged-in human SID')
    started = time.monotonic()
    deadline = started + plan['limits']['wall_seconds']
    _, template, profile, original = preflight(plan)
    # The explicit parent deadline includes preflight hashing and never refreshes.
    output = Path(output)
    anchor = Path(__file__).resolve().parent
    if not output.is_absolute() or not output.resolve().is_relative_to(anchor) or output.exists():
        raise ValueError('fresh run directory under this approved private preparation required')
    base_python = worker.pinned(plan['base_python'])
    ownership = worker.load_module(plan['ownership_module'], 'approved_owned_process')
    output.mkdir()
    for name in ('cuda-cache', 'torch-cache', 'hf-cache', 'pycache'):
        (output / name).mkdir()
    raw_path, log_path = output / 'torch-output.json', output / 'stdout-stderr.log'
    command = [str(base_python), '-B', '-X', 'pycache_prefix=' + str(output / 'pycache'), str(worker.pinned(plan['worker'])), '--plan', str(plan_path), '--plan-sha256', expected, '--output', str(raw_path)]
    environment = dict(os.environ)
    environment.update({'PYTHONDONTWRITEBYTECODE': '1', 'PYTHONPATH': plan['site_packages'], 'HF_HUB_OFFLINE': '1',
        'TRANSFORMERS_OFFLINE': '1', 'HF_HUB_DISABLE_TELEMETRY': '1', 'CUBLAS_WORKSPACE_CONFIG': ':4096:8',
        'OMP_NUM_THREADS': str(plan['limits']['threads']), 'MKL_NUM_THREADS': str(plan['limits']['threads']),
        'CUDA_CACHE_PATH': str(output / 'cuda-cache'), 'TORCH_HOME': str(output / 'torch-cache'), 'HF_HOME': str(output / 'hf-cache')})
    receipt = {'schema': 'airk.arbiter.independent-numeric-reference-execution/1',
        'model': template['model'], 'compiler': template['compiler'], 'resource_policy_digest': template['resource_policy'],
        'reference_kind': plan['kind'], 'source_assets': original['original_reference_assets'] if plan['kind'] == 'original_fp32' else [asset for asset in profile['assets'] if asset['role'] in ('model', 'external_weights', 'tokenizer')],
        'input_seals': dict(plan['input_seals'], **{str(plan_path): expected}), 'command': command,
        'execution_user_sid': worker.windows_sid(), 'native_airk_nn_invocations': 0,
        'complete': False, 'failure': None, 'post_input_drift': [], 'rows': [],
        'whole_phase_deadline_not_refreshed': True, 'whole_phase_expired': False,
        'no_automatic_retry': True, 'other_processes_touched': False}
    try:
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            raise ValueError('original phase expired during source authentication')
        def before_resume(api, job, info, child_deadline):
            return suspended_authentication(api, info, deadline, child_deadline, plan['human_sid'], plan['base_python'], ownership.image_proof)
        with (output / 'stdin.txt').open('xb') as initial:
            initial.flush()
        with (output / 'stdin.txt').open('rb') as initial, log_path.open('xb') as log:
            receipt['owned'] = ownership.run_windows_owned_training(command, Path(plan['repository_root']), log,
                remaining, environment, initial, plan['limits']['host_committed_bytes'], before_resume=before_resume, absolute_deadline=deadline)
        if not settled(receipt['owned']):
            raise ValueError('actual original reference process did not naturally settle as exactly one child')
        result = json.loads(worker.regular(raw_path).read_bytes())
        complete_arrays(result, template)
        receipt['rows'] = result['rows']
        receipt['torch_forward_invocations'] = result['torch_forward_invocations']
        receipt['raw_output'] = {'path': str(raw_path), 'bytes': raw_path.stat().st_size, 'sha256': worker.digest(raw_path)}
        for path, pin in receipt['input_seals'].items():
            if worker.digest(path) != pin:
                receipt['post_input_drift'].append(path)
        if receipt['post_input_drift']:
            raise ValueError('actual frozen phase inputs changed')
        receipt['whole_phase_expired'] = time.monotonic() > deadline
        if receipt['whole_phase_expired']:
            raise ValueError('original reference phase deadline expired')
        receipt['complete'] = True
    except BaseException as error:
        receipt['failure'] = {'type': type(error).__name__, 'message': str(error)}
        if hasattr(error, 'owned_cleanup'):
            receipt['actual_cleanup_failure'] = error.owned_cleanup
        raise
    finally:
        if log_path.exists():
            receipt['stdout'] = {'path': str(log_path), 'bytes': log_path.stat().st_size, 'sha256': worker.digest(log_path)}
        # Final observation includes failed/timeout/post-hash paths too. The
        # receipt write follows this observation; it cannot manufacture success.
        late_failure = final_phase_observation(receipt, deadline, started)
        with (output / 'execution.json').open('x', encoding='utf-8') as stream:
            json.dump(receipt, stream, indent=2)
            stream.write('\n')
        if late_failure:
            raise TimeoutError('original reference phase expired during final observation')
    return receipt


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--plan', type=Path, required=True)
    parser.add_argument('--plan-sha256', required=True)
    parser.add_argument('--output', type=Path)
    parser.add_argument('--execute', action='store_true')
    args = parser.parse_args()
    plan = json.loads(worker.pinned({'path': str(args.plan), 'sha256': args.plan_sha256}).read_bytes())
    if not args.execute:
        preflight(plan)
        print(json.dumps({'input_audit_passed': True, 'model_executed': False, 'qualification_granted': False}))
        return
    if args.output is None:
        raise ValueError('explicit fresh output required for reviewed execution')
    run(args.plan, args.plan_sha256, args.output)


if __name__ == '__main__':
    main()
