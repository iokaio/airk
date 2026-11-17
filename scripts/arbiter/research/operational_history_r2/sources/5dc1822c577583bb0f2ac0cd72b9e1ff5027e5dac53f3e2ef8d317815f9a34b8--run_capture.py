# SPDX-License-Identifier: Apache-2.0
"""Root-reviewed CAL owner: unchanged collector, exact before/after closure."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import time
from prepare_fixture import pin, ROOT
from run_fixture import verify

def require_absent(path):
    candidate = Path(path)
    if (os.path.lexists(candidate) or candidate.is_symlink()
            or getattr(candidate, 'is_junction', lambda: False)()):
        raise ValueError('recorded absent build input exists or is an alias: ' + str(path))

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--packet', required=True)
    parser.add_argument('--packet-sha256', required=True)
    args = parser.parse_args()
    started = time.time(); started_ns = time.monotonic_ns()
    if pin(args.packet)['sha256'] != args.packet_sha256:
        raise ValueError('exact reviewed capture packet changed')
    packet = json.loads(Path(args.packet).read_text(encoding='utf-8'))
    for name in ('release', 'capture_driver', 'runtime_python', 'owner'):
        verify(packet[name])
    release = json.loads(Path(packet['release']['path']).read_text(encoding='utf-8'))
    if (release.get('approved') is not True or release.get('Context_task_forward') is not False
            or release.get('maximum_questions') != 100 or release.get('maximum_probes') != 500):
        raise ValueError('exact Root-approved original CAL100 release required')
    for path, expected in release.get('absent_build_inputs', {}).items():
        if expected != 'missing':
            raise ValueError('unknown absent-input sentinel')
        require_absent(path)
    output = Path(packet['owner_output']); output.mkdir(exist_ok=False)
    capture = Path(packet['capture_output'])
    if capture.exists():
        raise ValueError('capture output already exists')
    receipt = {'schema': 'airk.private-selected-cal-capture-owned-execution/1',
               'complete': False, 'qualification_granted': False, 'packet': pin(args.packet),
               'started_unix': started, 'original_owner_seconds': packet['owner_seconds']}
    (output / 'started.json').write_text(json.dumps(receipt, indent=2) + '\n', encoding='utf-8')
    spec = importlib.util.spec_from_file_location('existing_owner', packet['owner']['path'])
    owner = importlib.util.module_from_spec(spec); spec.loader.exec_module(owner)
    argv = [packet['runtime_python']['path'], '-B', packet['capture_driver']['path']]
    for name, path in release['paths'].items():
        argv.extend(['--' + name.replace('_', '-'), path])
    argv.extend(['--release', packet['release']['path'], '--release-sha256', packet['release']['sha256'],
                 '--output', str(capture)])
    receipt['argv'] = argv
    try:
        remaining = packet['owner_seconds'] - (time.monotonic_ns() - started_ns) / 1e9
        if remaining <= 0:
            raise TimeoutError('original capture owner deadline expired before launch')
        result = owner.run_owned(argv, ROOT, output / 'native-capture.log', remaining)
        receipt['owned'] = result
        if not owner.successful(result):
            raise ValueError('actual CAL collector failed/unsettled; no retry')
        report_path = capture / 'capture.private.json'
        report = json.loads(report_path.read_text(encoding='utf-8'))
        if (report.get('complete') is not True or len(report['rows']) != 100
                or len({row['id'] for row in report['rows']}) != 100
                or report.get('Context_task_NN_invocations') != 0
                or report.get('release_sha256') != packet['release']['sha256']):
            raise ValueError('actual complete CAL100/no-Context-dispatch closure absent')
        for phase in ('stage_before', 'stage_after'):
            actual = report[phase]['full_native_response']['configuration']
            if actual != release['frozen_configuration']:
                raise ValueError('actual CAL configuration differs from exact frozen native consent')
        # The unchanged collector already hashes all input_pins BEFORE dispatch.
        # Reconcile the entire same closure AFTER normal SDK/session completion.
        drift = []
        for path, expected in release['input_pins'].items():
            if pin(path)['sha256'] != expected:
                drift.append(path)
        for path in release.get('absent_build_inputs', {}):
            require_absent(path)
        for name in ('release', 'capture_driver', 'runtime_python', 'owner'):
            verify(packet[name])
        receipt['post_input_drift'] = drift
        if drift:
            raise ValueError('actual capture input closure drifted')
        if (time.monotonic_ns() - started_ns) / 1e9 >= packet['owner_seconds']:
            raise TimeoutError('original capture owner deadline expired during postchecks')
        receipt['capture'] = pin(report_path)
        receipt['complete'] = True
    finally:
        receipt['finished_unix'] = time.time()
        receipt['elapsed_seconds'] = (time.monotonic_ns() - started_ns) / 1e9
        (output / 'execution.json').write_text(json.dumps(receipt, indent=2) + '\n', encoding='utf-8')

if __name__ == '__main__':
    main()
