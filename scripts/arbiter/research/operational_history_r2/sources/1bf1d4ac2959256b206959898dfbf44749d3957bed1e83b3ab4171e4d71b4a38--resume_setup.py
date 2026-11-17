# SPDX-License-Identifier: Apache-2.0
"""Finish only unexecuted commands; retain the failed original wrapper intact."""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import time
from prepare_fixture import ROOT, pin
from run_fixture import verify
from run_setup import native_job
from registry_events import RegistryEvents

def main():
    parser = argparse.ArgumentParser()
    for name in ('packet', 'packet-sha256', 'decision', 'decision-sha256', 'output'):
        parser.add_argument('--' + name, required=True)
    args = parser.parse_args()
    if pin(args.packet)['sha256'] != args.packet_sha256 or pin(args.decision)['sha256'] != args.decision_sha256:
        raise ValueError('exact continuation packet/decision changed')
    packet = json.loads(Path(args.packet).read_text(encoding='utf-8'))
    decision = json.loads(Path(args.decision).read_text(encoding='utf-8'))
    if decision.get('approved_remaining_setup_packet_sha256') != args.packet_sha256:
        raise ValueError('Root must approve exact remaining commands and original deadline')
    for name in ('original_packet', 'original_execution', 'successful_original_job', 'successful_original_job_execution'):
        verify(packet[name])
    setup = json.loads(Path(packet['original_packet']['path']).read_text(encoding='utf-8'))
    original = json.loads(Path(packet['original_execution']['path']).read_text(encoding='utf-8'))
    if original['complete'] is not False or original['packet_sha256'] != packet['original_packet']['sha256']:
        raise ValueError('original failed wrapper must be retained, never relabeled')
    if packet['remaining_command_indexes'] != [2, 3, 4, 5]:
        raise ValueError('only four originally unexecuted commands admitted')
    completed = native_job(packet['successful_original_job']['path'])
    submitted = next(row for row in original['commands'] if row['label'] == 'configure-minilm-install.json')
    verify(submitted['log'])
    old = native_job(submitted['log']['path'])
    if (not completed or completed['id'] != old['id'] or completed['state'] != 'succeeded'
            or completed.get('outcome', {}).get('state') != 'succeeded'):
        raise ValueError('same original Configure must actually have succeeded without retry')
    diagnostic = json.loads(Path(packet['successful_original_job_execution']['path']).read_text(encoding='utf-8'))
    observed = diagnostic['owned']
    if (observed['exit_code'] != 0 or observed['timed_out'] or observed['log_oversized']
            or not observed['owned_descendants_settled'] or observed.get('owned_job_forced_termination', False)
            or 'cleanup_refusal' in observed or diagnostic['argv'][-3:] != ['jobs', 'get', completed['id']]):
        raise ValueError('actual same-job observation ownership or command differs')
    fixture = json.loads(Path(setup['fixture_packet']['path']).read_text(encoding='utf-8'))
    refs = [setup['fixture_packet'], setup['fixture_ready'], setup['data_bindings'], setup['model_definition'],
            setup['semantic_plan'], fixture['image'], fixture['owner'], *setup['runtime_images'], *setup['runtime_metadata']]
    deadline = original['original_deadline_monotonic_ns']
    def remaining():
        value = (deadline - time.monotonic_ns()) / 1_000_000_000
        if value <= 0:
            raise TimeoutError('unchanged original setup deadline exhausted')
        return value
    for row in refs:
        verify(row); remaining()
    output = Path(args.output)
    output.mkdir(exist_ok=False)
    spec = importlib.util.spec_from_file_location('existing_owner', fixture['owner']['path'])
    owner = importlib.util.module_from_spec(spec); spec.loader.exec_module(owner)
    control = Path(fixture['work']) / 'control'
    rows = list(original['commands'])
    rows.append({'label': 'original-configure-completed-after-wrapper-cap',
                 'log': packet['successful_original_job'], 'owned': observed, 'argv': diagnostic['argv']})
    receipt = {'schema': 'airk.private-selected-cal-setup-execution/1', 'complete': False,
               'qualification_granted': False, 'phase': 'index',
               'packet_sha256': packet['original_packet']['sha256'],
               'original_failed_wrapper': packet['original_execution'],
               'continuation_packet': pin(args.packet), 'continuation_decision': pin(args.decision),
               'original_deadline_monotonic_ns': deadline, 'commands': rows}
    def run(argv, label, environment, maximum=30):
        log = output / (label + '.log')
        result = owner.run_owned(argv, ROOT, log, min(maximum, remaining()), environment)
        rows.append({'label': label, 'argv': argv, 'owned': result, 'log': pin(log)})
        if not owner.successful(result):
            raise ValueError('actual command failed/unsettled; no retry')
        remaining()
        return log
    try:
        for number in packet['remaining_command_indexes']:
            step = setup['commands'][number]
            if any(row['label'] == step['label'] for row in original['commands']):
                raise ValueError('would repeat an already executed original command')
            events = RegistryEvents(control)
            try:
                environment = dict(os.environ); environment.update(step.get('environment', {}))
                log = run(step['argv'], step['label'], environment, step['deadline_seconds'])
                job = native_job(log)
                if job is None:
                    raise ValueError('expected original native submitted job')
                job_id = job['id']
                for observation in range(1024):
                    if job.get('outcome') is not None:
                        if job['state'] != 'succeeded' or job['outcome'].get('state') != 'succeeded':
                            raise ValueError('original native job failed/canceled; no retry')
                        break
                    if job['state'] not in ('pending', 'running'):
                        raise ValueError('unknown original job state')
                    events.wait(deadline)
                    argv = [step['argv'][0], '--service-runtime-dir', str(control), 'jobs', 'get', job_id]
                    log = run(argv, f'{number}-observe-original-job-{observation:04d}', environment)
                    job = native_job(log)
                    if job is None or job['id'] != job_id:
                        raise ValueError('native original job identity changed')
                else:
                    raise ValueError('bounded event-driven native observations exhausted')
            finally:
                events.close()
        for row in refs:
            verify(row); remaining()
        receipt['complete'] = True
    finally:
        receipt['remaining_original_seconds'] = max(0, (deadline - time.monotonic_ns()) / 1_000_000_000)
        (output / 'execution.json').write_text(json.dumps(receipt, indent=2) + '\n', encoding='utf-8')

if __name__ == '__main__':
    main()
