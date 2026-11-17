# SPDX-License-Identifier: Apache-2.0
"""Exact reviewed setup argv through the existing process owner; no native DTOs."""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import time
from prepare_fixture import ROOT, pin
from run_fixture import verify

def native_job(log):
    jobs = []
    for line in Path(log).read_text(encoding='utf-8').splitlines():
        try:
            row = json.loads(line)
        except ValueError:
            continue
        if isinstance(row, dict) and row.get('status') == 'job':
            jobs.append(row['job'])
    if len(jobs) > 1:
        raise ValueError('ambiguous original native job response')
    return jobs[0] if jobs else None

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--packet', required=True)
    parser.add_argument('--packet-sha256', required=True)
    parser.add_argument('--decision', required=True)
    parser.add_argument('--decision-sha256', required=True)
    parser.add_argument('--phase', choices=('vector', 'index'), required=True)
    parser.add_argument('--output', required=True)
    args = parser.parse_args()
    started = time.monotonic()
    if pin(args.packet)['sha256'] != args.packet_sha256 or pin(args.decision)['sha256'] != args.decision_sha256:
        raise ValueError('reviewed packet/decision changed')
    packet = json.loads(Path(args.packet).read_text(encoding='utf-8'))
    decision = json.loads(Path(args.decision).read_text(encoding='utf-8'))
    if (decision.get('approved_setup_packet_sha256') != args.packet_sha256
            or args.phase not in decision.get('approved_phases', [])):
        raise ValueError('exact packet and phase must be Root approved')
    input_pins = [packet['fixture_packet'], packet['data_bindings'], packet['model_definition'],
                  *packet['runtime_images'], *packet['runtime_metadata']]
    if packet.get('semantic_plan'):
        input_pins.append(packet['semantic_plan'])
    if args.phase == 'index':
        if not packet.get('fixture_ready'):
            raise ValueError('actual native Ready pin required before real setup')
        input_pins.append(packet['fixture_ready'])
    for row in input_pins:
        verify(row)
    fixture = json.loads(Path(packet['fixture_packet']['path']).read_text(encoding='utf-8'))
    verify(fixture['owner'])
    verify(fixture['image'])
    output = Path(args.output)
    output.mkdir(exist_ok=False)
    spec = importlib.util.spec_from_file_location('existing_owner', fixture['owner']['path'])
    owner = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(owner)
    limit = 60 if args.phase == 'vector' else 1800
    commands = [step for step in packet['commands'] if bool(step.get('before_fixture_start')) == (args.phase == 'vector')]
    if not commands:
        raise ValueError('zero selected native commands; completed emitters must not be repeated')
    results = []
    receipt = {'schema': 'airk.private-selected-cal-setup-execution/1', 'complete': False,
               'qualification_granted': False, 'packet_sha256': args.packet_sha256,
               'decision_sha256': args.decision_sha256, 'phase': args.phase, 'commands': results,
               'original_deadline_monotonic_ns': int((started + limit) * 1_000_000_000)}
    try:
        for ordinal, step in enumerate(commands):
            remaining = limit - (time.monotonic() - started)
            if remaining <= 0:
                raise TimeoutError('original setup phase deadline exhausted')
            environment = dict(os.environ)
            environment.update(step.get('environment', {}))
            log = output / f'{ordinal:02d}-{step["label"]}.log'
            result = owner.run_owned(step['argv'], ROOT, log,
                                     min(remaining, step['deadline_seconds']), environment)
            results.append({'label': step['label'], 'argv': step['argv'], 'owned': result, 'log': pin(log)})
            if not owner.successful(result):
                raise ValueError('original command failed/unsettled; no retry: ' + step['label'])
            # Configuration, model setup and index return submitted Job records.
            # Observe that same native ID through successful reads before issuing
            # dependent commands; never resubmit/retry a failed job.
            job = native_job(log)
            if job is not None:
                job_id = job['id']
                for observation in range(1024):
                    if job.get('outcome') is not None:
                        if job['state'] != 'succeeded' or job['outcome'].get('state') != 'succeeded':
                            raise ValueError('original native job failed/canceled; no retry')
                        break
                    if job['state'] not in ('pending', 'running'):
                        raise ValueError('unknown native job state/outcome')
                    remaining = limit - (time.monotonic() - started)
                    if remaining <= 0:
                        raise TimeoutError('original setup deadline expired observing job')
                    argv = [step['argv'][0], '--service-runtime-dir', str(Path(fixture['work']) / 'control'),
                            'jobs', 'get', job_id]
                    observed_log = output / f'{ordinal:02d}-job-{observation:04d}.log'
                    observed = owner.run_owned(argv, ROOT, observed_log, min(30, remaining), environment)
                    results.append({'label': 'observe-original-job', 'argv': argv,
                                    'owned': observed, 'log': pin(observed_log)})
                    if not owner.successful(observed):
                        raise ValueError('actual job observation failed; no retry')
                    job = native_job(observed_log)
                    if job is None or job['id'] != job_id:
                        raise ValueError('original native job identity changed')
                else:
                    raise ValueError('bounded successful native job observations exhausted')
        for row in input_pins:
            verify(row)
        verify(fixture['image'])
        verify(fixture['owner'])
        if time.monotonic() - started >= limit:
            raise TimeoutError('original setup phase deadline exhausted during postchecks')
        receipt['complete'] = True
    finally:
        receipt['elapsed_seconds'] = time.monotonic() - started
        (output / 'execution.json').write_text(json.dumps(receipt, indent=2) + '\n', encoding='utf-8')

if __name__ == '__main__':
    main()
