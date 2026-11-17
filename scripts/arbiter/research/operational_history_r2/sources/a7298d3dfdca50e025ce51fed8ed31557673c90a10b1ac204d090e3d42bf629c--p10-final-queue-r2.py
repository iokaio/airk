# SPDX-License-Identifier: Apache-2.0
"""Execute the reviewed frozen P10 map once; never retry or edit source."""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import atexit

base = Path(__file__).resolve().parent
mapping = base / 'p10-final-qualification-command-map-r2.json'
plan = json.loads(mapping.read_text(encoding='utf-8'))
output = base / 'p10-final-qualification-r2.json'
assert not output.exists(), 'Use a fresh campaign identity after a correction'
owner_lock = base / 'p10-final-qualification-owner.lock'
owner_fd = os.open(owner_lock, os.O_CREAT | os.O_EXCL | os.O_WRONLY)
os.write(owner_fd, str(os.getpid()).encode())
def release_owner():
    os.close(owner_fd)
    owner_lock.unlink()
atexit.register(release_owner)
record = {'status': 'running', 'map_sha256': hashlib.sha256(mapping.read_bytes()).hexdigest(),
          'source': None, 'source_gates_complete': False, 'steps': []}
spec = importlib.util.spec_from_file_location('historical_seal', base / 'p9-integrated-checks.py')
sealer = importlib.util.module_from_spec(spec)
spec.loader.exec_module(sealer)
runtime = Path(plan['runtime'])
seal = None
source_spec = importlib.util.spec_from_file_location('p10_frozen_builder', Path(plan['cwd']) / 'scripts/build-service-runtime.py')
source_builder = importlib.util.module_from_spec(source_spec)
source_spec.loader.exec_module(source_builder)


def save():
    pending = output.with_suffix('.pending')
    pending.write_text(json.dumps(record, indent=2) + '\n', encoding='utf-8')
    pending.replace(output)


save()
try:
    prior = plan['stopped_predecessor']
    prior_path = Path(prior['path'])
    assert hashlib.sha256(prior_path.read_bytes()).hexdigest() == prior['sha256'], 'Stopped r1 record changed'
    assert json.loads(prior_path.read_text())['status'] == prior['status']
    expected = plan['frozen_source_expected']
    assert [gate['name'] for gate in plan['reused_source_gates']] == ['fmt','lightweight-clippy','workspace-clippy','lightweight-check']
    for gate in plan['reused_source_gates']:
        receipt_path, log_path = Path(gate['receipt']), Path(gate['log'])
        assert hashlib.sha256(receipt_path.read_bytes()).hexdigest() == gate['receipt_sha256'], 'Reused receipt drift'
        assert hashlib.sha256(log_path.read_bytes()).hexdigest() == gate['log_sha256'], 'Reused log drift'
        receipt = json.loads(receipt_path.read_text())
        assert receipt['actual_exit_code'] == receipt['exit_code'] == 0 and receipt['validation_error'] is None
        assert receipt['source_unchanged'] and receipt['source_before'] == receipt['source_after'] == expected
        assert receipt['command'] == gate['command'] and receipt['log_sha256'] == gate['log_sha256']
        assert Path(receipt['workdir']) == Path(plan['cwd']) and Path(receipt['target']) == Path(plan['persistent_target'])
        record['steps'].append({'name':gate['name'],'reused':True,'receipt':gate['receipt'],
            'receipt_sha256':gate['receipt_sha256'],'log_sha256':gate['log_sha256'],'exit_code':0})
    assert source_builder.source_identity() == expected, 'Source changed since four completed gates'
    record['source'] = expected
    record['source_gates_complete'] = True
    save()
    for row in plan['commands']:
        if record['source'] is not None:
            assert source_builder.source_identity() == record['source'], 'Source drift before command'
        for proof in plan['driver_parity']:
            root_driver = base.parents[1] / proof['path']
            work_driver = Path(plan['cwd']) / proof['path']
            assert hashlib.sha256(root_driver.read_bytes()).hexdigest() == proof['root_sha256'], 'Root driver drift'
            assert hashlib.sha256(work_driver.read_bytes()).hexdigest() == proof['worktree_sha256'], 'Worktree driver drift'
        for name, digest in plan['unchanged_helper_sha256'].items():
            assert hashlib.sha256((base / name).read_bytes()).hexdigest() == digest, 'Helper drift'
        if seal is not None:
            assert sealer.seal_runtime(runtime, record['source']) == seal, 'Runtime drift'
        environment = os.environ.copy()
        environment['CARGO_NET_OFFLINE'] = 'true'
        environment.pop('AIRK_P10_RESOURCE_OUTPUT_DIR', None)
        environment.pop('AIRK_CATALOG_CANDIDATE_BINARY', None)
        environment.update(row.get('environment', {}))
        if row['name'] == 'release-build':
            assert record['source_gates_complete'], 'Release requires complete source prerequisites'
        if row['name'] == 'split-process-resources':
            Path(environment['AIRK_P10_RESOURCE_OUTPUT_DIR']).mkdir(exist_ok=False)
        if row['name'] == 'workspace-tests':
            environment['AIRK_CATALOG_CANDIDATE_BINARY'] = str(runtime / 'airk.exe')
        command = list(row['command'])
        if 'cargo' in command and '--locked' in command and '--offline' not in command:
            command.insert(command.index('--locked') + 1, '--offline')
        print('START', row['name'], flush=True)
        result = subprocess.run(command, cwd=row['cwd'], env=environment)
        # Verify retained payload even when the command itself failed.
        runtime_after_error = None
        if seal is not None:
            try:
                assert sealer.seal_runtime(runtime, record['source']) == seal, 'Runtime drift after command'
            except BaseException as error:
                runtime_after_error = str(error)
        receipt = json.loads(Path(row['log']).with_suffix('.receipt.json').read_text(encoding='utf-8'))
        record['steps'].append({'name': row['name'], 'exit_code': result.returncode,
                                'runtime_after_error': runtime_after_error,
                                'receipt': str(Path(row['log']).with_suffix('.receipt.json')),
                                'environment_overrides': {key: environment[key] for key in
                                    ['AIRK_DEV_CARGO_INCREMENTAL', 'AIRK_CATALOG_CANDIDATE_BINARY',
                                     'AIRK_P10_RESOURCE_OUTPUT_DIR'] if key in environment}})
        save()
        assert runtime_after_error is None, runtime_after_error
        assert result.returncode == receipt['exit_code'] == receipt['actual_exit_code'] == 0, row['name']
        assert receipt['source_unchanged'] and receipt['source_before'] == receipt['source_after']
        if record['source'] is None:
            record['source'] = receipt['source_after']
        assert receipt['source_after'] == record['source'], 'Source drift'
        if row.get('expected_passing_tests'):
            assert receipt['passing_tests'] == row['expected_passing_tests'], 'Test selection changed'
        if row['name'] == 'lightweight-check':
            record['source_gates_complete'] = True
        if row['name'] == 'release-build':
            assert receipt['cargo_incremental'] == '0'
            seal = sealer.seal_runtime(runtime, record['source'])
            with (base / 'p10-runtime-final-r1-seal.json').open('x', encoding='utf-8') as handle:
                json.dump(seal, handle, indent=2)
                handle.write('\n')
        if seal is not None:
            assert sealer.seal_runtime(runtime, record['source']) == seal, 'Runtime drift'
        save()
    record['status'] = 'passed'
except BaseException as error:
    record['status'] = 'failed'
    record['failure'] = str(error)
    save()
    raise
save()
