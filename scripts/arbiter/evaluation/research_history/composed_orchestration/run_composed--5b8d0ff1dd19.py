# SPDX-License-Identifier: Apache-2.0
"""Private bounded controller; Root-owned launcher must contain all descendants.

The execution plan contains actual Rust-emitted files and reviewed argv, never
handwritten native request DTOs. No scoring, mocking, retry or promotion here.
"""
import hashlib
import json
import math
import os
from pathlib import Path
import subprocess
import sys
import time
from os_measurement import sample as os_memory_sample
from native_policy import NativePolicy

def digest(path):
    result = hashlib.sha256()
    with Path(path).open('rb') as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b''):
            result.update(block)
    return result.hexdigest()

def controller_request(step, number, rows, starts, policy, remaining):
    request = dict(step['request'], control_id=str(number))
    if request.get('kind') == 'watch' and 'warm_configuration_observation_step' in step:
        configure = step['warm_configuration_observation_step']
        if not 0 <= configure < number or rows[configure]['request']['kind'] != 'native_command':
            raise ValueError('warm event wait lacks original Configure observation')
        allowance = step['warm_deadline_seconds'] - (time.monotonic() - starts[configure])
        bounded = int(min(remaining(), allowance) * 1000)
        if bounded < 1:
            raise TimeoutError('original whole warm observation deadline exhausted before watcher')
        request['timeout_ms'] = min(request['timeout_ms'], bounded)
        request['warm_observation'] = True
    return request

def run(plan_path, output):
    started = time.monotonic()
    plan = json.loads(Path(plan_path).read_text(encoding='utf-8'))
    if plan['schema'] != 'airk.private.composed-execution-plan/1':
        raise ValueError('unreviewed plan schema')
    limit = plan['whole_phase_seconds']
    if not 0 < limit <= 1800:
        raise ValueError('invalid reviewed whole-phase limit')
    if not 1 <= len(plan['steps']) <= 200:
        raise ValueError('invalid bounded step roster')
    if plan['steps'][-1].get('request', {}).get('kind') != 'finish':
        raise ValueError('final explicit original-session closure observation required')
    policy = NativePolicy(plan['resource_policy_input'])
    controller_packet = json.loads(Path(plan['controller_packet']).read_text(encoding='utf-8'))
    if controller_packet['whole_phase_seconds'] != limit:
        raise ValueError('controller and frozen plan use different original whole-phase limits')
    if plan['pins'].get(str(Path(plan['controller_packet']))) != digest(plan['controller_packet']):
        raise ValueError('controller packet absent from original plan seals')
    if controller_packet['resource_policy_input'] != policy.pin:
        raise ValueError('controller and frozen plan use different native policy inputs')
    if plan['pins'].get(str(policy.path)) != policy.pin['sha256']:
        raise ValueError('native policy input absent from complete plan seals')
    for step in plan['steps']:
        if step['kind'] == 'controller' and step['request'].get('kind') == 'start':
            if type(step['request']['deadline_ms']) is not int or not 1 <= step['request']['deadline_ms'] <= policy.deadline_ms:
                raise ValueError('original read deadline exceeds native policy input')
    output = Path(output)
    output.mkdir(exist_ok=False)
    def remaining():
        value = limit - (time.monotonic() - started)
        if value <= 0:
            raise TimeoutError('original whole-phase deadline exhausted')
        return value
    def verify(full=True):
        policy.verify()
        for path, expected in (plan['pins'] if full else plan.get('per_step_pins', {})).items():
            if digest(path) != expected:
                raise ValueError('consumed input drift: ' + path)
        remaining()
    verify()
    process = None
    stderr_stream = None
    # Parent's existing owned Job supplies hard wallclock/resource/descendant
    # enforcement, including a blocked stdout read. No new process-owner clone.
    rows = []
    monotonic_starts = {}
    try:
        stderr_stream = (output / 'controller-stderr.bin').open('wb')
        process = subprocess.Popen([plan['node'], plan['controller'], plan['controller_packet']],
                                   stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                   stderr=stderr_stream, text=True,
                                   encoding='utf-8')
        for number, step in enumerate(plan['steps']):
            verify(full=False)
            began = time.time_ns() // 1000000
            monotonic_starts[number] = time.monotonic()
            if step['kind'] == 'controller':
                request = controller_request(step, number, rows, monotonic_starts, policy, remaining)
                process.stdin.write(json.dumps(request) + '\n')
                process.stdin.flush()
                line = process.stdout.readline()
                if not line:
                    raise RuntimeError('original controller exited without response')
                result = json.loads(line)
                if result['id'] != str(number) or result.get('ok') is not True:
                    raise RuntimeError('actual controller refused: ' + line)
            elif step['kind'] == 'os_sample':
                result = os_memory_sample(plan['actual_process_observation'], step['phase'])
            elif step['kind'] == 'await_ready':
                result = await_ready(step, rows, monotonic_starts, output, number,
                                     plan['allowed_native_images'], remaining)
            elif step['kind'] == 'native_command':
                # The original runtime pair and argv are frozen in Root's plan.
                if step['argv'][0] not in plan['allowed_native_images']:
                    raise ValueError('unreviewed native executable')
                deadline = step['deadline_seconds']
                if isinstance(deadline, bool) or not isinstance(deadline, (int, float)) or not math.isfinite(deadline) or not 0 < deadline <= limit:
                    raise ValueError('invalid reviewed original command deadline')
                environment = dict(os.environ)
                environment.update(step.get('environment', {}))
                try:
                    completed = subprocess.run(step['argv'], capture_output=True, check=False,
                                               timeout=min(remaining(), deadline), env=environment)
                except subprocess.TimeoutExpired as error:
                    (output / f'{number:03d}-stdout.bin').write_bytes(error.stdout or b'')
                    (output / f'{number:03d}-stderr.bin').write_bytes(error.stderr or b'')
                    raise
                (output / f'{number:03d}-stdout.bin').write_bytes(completed.stdout)
                (output / f'{number:03d}-stderr.bin').write_bytes(completed.stderr)
                result = {'returncode': completed.returncode, 'argv': step['argv'],
                          'stdout_sha256': hashlib.sha256(completed.stdout).hexdigest(),
                          'stderr_sha256': hashlib.sha256(completed.stderr).hexdigest()}
                if completed.returncode != step.get('expected_returncode', 0):
                    raise RuntimeError('native command failed; batch stopped')
            else:
                raise ValueError('unknown reviewed step kind')
            row = {'schema': 'airk.private.composed-observation/1', 'step': number, 'case': step['case'], 'request': step, 'started_unix_ms': began,
                   'completed_unix_ms': time.time_ns() // 1000000, 'result': result}
            if step['kind'] == 'controller':
                row['submitted_controller_request'] = request
            rows.append(row)
            (output / f'{number:03d}-observation.json').write_text(
                json.dumps(row, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')
        process.stdin.close()
        code = process.wait(timeout=remaining())
        if code:
            raise RuntimeError('actual controller did not close normally')
        verify()
        status = 'observations_complete_not_qualified'
    except BaseException as error:
        status = 'failed_or_incomplete'
        (output / 'failure.json').write_text(json.dumps({'type': type(error).__name__, 'message': str(error),
            'completed_steps': len(rows)}), encoding='utf-8')
        raise
    finally:
        if stderr_stream is not None:
            stderr_stream.flush()
            stderr_stream.close()
        # Never manufacture natural process closure by terminating it here. Root's
        # unchanged Job owner records containment/forced cleanup on failure.
        (output / 'driver-receipt.json').write_text(json.dumps({'schema': 'airk.private.composed-driver/1',
            'status': locals().get('status', 'failed_or_incomplete'), 'plan_sha256': digest(plan_path),
            'elapsed_seconds': time.monotonic()-started, 'observed_steps': len(rows),
            'controller_pid': process.pid if process is not None else None,
            'controller_exitcode': process.poll() if process is not None else None,
            'qualification_granted': False}, indent=2)+'\n', encoding='utf-8')

def await_ready(step, rows, starts, output, number, allowed_images, remaining):
    """Successful native status observations, not scoring probes or failed retries."""
    stage = step['stage']
    if stage not in ('focus', 'context') or step['argv'][0] not in allowed_images or step['argv'][-2:] != ['arbiter', 'status']:
        raise ValueError('unreviewed native readiness command/stage')
    configure = step['configuration_observation_step']
    if not 0 <= configure < number or rows[configure]['request']['kind'] != 'native_command':
        raise ValueError('original native Configure observation required')
    source = output / f'{configure:03d}-stdout.bin'
    if hashlib.sha256(source.read_bytes()).hexdigest() != rows[configure]['result']['stdout_sha256']:
        raise ValueError('original Configure response changed')
    original = json.loads(source.read_bytes())
    idle = step['prior_idle_observation_step']
    if not 0 <= idle < configure or rows[idle]['request']['kind'] != 'native_command':
        raise ValueError('original prior idle native status observation required')
    idle_source = output / f'{idle:03d}-stdout.bin'
    if hashlib.sha256(idle_source.read_bytes()).hexdigest() != rows[idle]['result']['stdout_sha256']:
        raise ValueError('original prior idle status changed')
    prior = json.loads(idle_source.read_bytes())
    if prior.get('schema') != 'airk.arbiter.status/2' or prior[stage]['active_profile'] is not None or prior[stage]['running'] != 0 or prior[stage]['queued'] != 0 or prior[stage]['availability'] in ('failed', 'draining'):
        raise ValueError('stage was not actually idle/drained before original Configure')
    watched = step['completion_watch_observation_step']
    if not configure < watched < number:
        raise ValueError('actual warm completion wakeup must follow original Configure')
    observation = rows[watched]
    event = observation['result']['value']['event']
    if (observation['request']['kind'] != 'controller'
            or observation['request']['request']['kind'] != 'watch'
            or event.get('schema') != 'airk.private.arbiter-warm-completion/1'
            or event.get('observer_id') != step['warm_observer_id']
            or event.get('profile_manifest') != step['profile_manifest']
            or event.get('accepted_configure_revision') != original['configuration']['revision']
            or event.get('outcome') != 'ready'):
        raise ValueError('matching actual successful native warm completion wakeup absent')
    revision = original['configuration']['revision']
    manifest = step['profile_manifest']
    if not original['configuration'][stage]['enabled'] or original['configuration'][stage]['selected_profile'] != manifest:
        raise ValueError('original Configure did not consent to exact selected profile')
    limit = step['deadline_seconds']
    if isinstance(limit, bool) or not isinstance(limit, (int, float)) or not math.isfinite(limit) or not 0 < limit <= 1800:
        raise ValueError('invalid original whole warm deadline')
    observations = []
    summary = {'schema':'airk.private.composed-warm-observation/1', 'stage':stage,
               'profile_manifest':manifest, 'configuration_revision':revision,
               'configuration_observation_step':configure,'original_warm_deadline_seconds':limit,
               'prior_idle_observation_step':idle,
               'observations':observations,'ready_observed':False}
    def retain():
        (output / f'{number:03d}-warm-observations.json').write_text(json.dumps(summary,indent=2)+'\n',encoding='utf-8')
    retain()
    try:
        for observation in range(128):
            budget = min(remaining(), limit - (time.monotonic() - starts[configure]))
            if budget <= 0:
                raise TimeoutError('original whole warm observation deadline exhausted')
            began = time.time_ns() // 1000000
            timed_out = False
            try:
                response = subprocess.run(step['argv'], capture_output=True, check=False, timeout=budget)
                stdout_bytes, stderr_bytes, returncode = response.stdout, response.stderr, response.returncode
            except subprocess.TimeoutExpired as error:
                stdout_bytes, stderr_bytes, returncode = error.stdout or b'', error.stderr or b'', None
                timed_out = True
            stdout, stderr = output / f'{number:03d}-status-{observation:03d}-stdout.bin', output / f'{number:03d}-status-{observation:03d}-stderr.bin'
            stdout.write_bytes(stdout_bytes);stderr.write_bytes(stderr_bytes)
            observations.append({'argv':step['argv'],'started_unix_ms':began,'completed_unix_ms':time.time_ns()//1000000,
                'returncode':returncode,'timed_out':timed_out,'stdout':{'path':str(stdout.resolve()),'sha256':hashlib.sha256(stdout_bytes).hexdigest()},
                'stderr':{'path':str(stderr.resolve()),'sha256':hashlib.sha256(stderr_bytes).hexdigest()}})
            retain()
            if timed_out:
                raise TimeoutError('actual native status exceeded original warm deadline; no retry')
            if returncode != 0:
                raise ValueError('actual readiness status failed; no retry')
            status = json.loads(stdout_bytes)
            configured, observed = status['configuration'][stage], status[stage]
            if status.get('schema') != 'airk.arbiter.status/2' or status.get('initialization') is not None or status['configuration'] != original['configuration']:
                raise ValueError('native readiness configuration/schema changed')
            if configured['enabled'] is not True or configured['selected_profile'] != manifest or observed['selected_profile'] != manifest:
                raise ValueError('native readiness consent/selection changed')
            phase = observed['availability']
            if phase == 'ready':
                if observed['active_profile'] != manifest:
                    raise ValueError('Ready did not activate exact selected profile')
                if time.monotonic() - starts[configure] >= limit:
                    raise TimeoutError('native Ready observed beyond original warm deadline')
                summary.update(ready_observed=True,ready_observation=observations[-1]['stdout'],
                               activation_observed_seconds=time.monotonic()-starts[configure])
                return summary
            if phase not in ('requested', 'verifying', 'warming'):
                raise ValueError('native warm failed/constrained/draining/unknown; no retry: '+phase)
        raise ValueError('bounded successful status observation roster exhausted')
    finally:
        retain()

if __name__ == '__main__':
    run(*sys.argv[1:])
