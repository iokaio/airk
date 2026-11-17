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
mapping = base / 'p10-final-qualification-command-map-r6.json'
plan = json.loads(mapping.read_text(encoding='utf-8'))
output = base / 'p10-final-qualification-r6.json'
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
    assert source_builder.source_identity() == plan['source_expected'], 'Source changed before fresh r6 gates'
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
        proof = plan['private_base_proof']
        assert hashlib.sha256(Path(proof['receipt']).read_bytes()).hexdigest() == proof['receipt_sha256'], 'Private-base creation receipt drift'
        checked = subprocess.run(['pwsh', '-NoProfile', '-File', proof['verifier'],
                                  '-ExpectedReceiptSha256', proof['receipt_sha256']],
                                 capture_output=True, text=True, check=False)
        assert checked.returncode == 0, 'Private-base ACL proof refused: ' + checked.stdout + checked.stderr
        observation = json.loads(checked.stdout.lstrip('\ufeff'))
        assert all(observation[key] == proof[key] for key in ['path', 'owner_sid', 'sddl']), 'Private base identity drift'
        assert observation['acl_mutated'] is False
        record.setdefault('private_base_observations', []).append({'stage': row['name'], **observation})
        save()
        environment = os.environ.copy()
        environment['CARGO_NET_OFFLINE'] = 'true'
        environment.pop('AIRK_P10_RESOURCE_OUTPUT_DIR', None)
        environment.pop('AIRK_CATALOG_CANDIDATE_BINARY', None)
        environment.update(row.get('environment', {}))
        assert environment['AIRK_PRIVATE_CENSUS_BASE'] == proof['path'], 'Explicit owned fixture root required in every stage'
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
                                     'AIRK_P10_RESOURCE_OUTPUT_DIR', 'CARGO_NET_OFFLINE', 'AIRK_PRIVATE_CENSUS_BASE'] if key in environment}})
        save()
        assert runtime_after_error is None, runtime_after_error
        assert result.returncode == receipt['exit_code'] == receipt['actual_exit_code'] == 0, row['name']
        assert receipt['source_unchanged'] and receipt['source_before'] == receipt['source_after']
        if record['source'] is None:
            record['source'] = receipt['source_after']
        assert receipt['source_after'] == record['source'], 'Source drift'
        assert receipt['source_after'] == plan['source_expected'], 'R6 selected source drift'
        if row['name'] == 'dependency-audit':
            report_path = Path(row['expected_report']['path'])
            report = json.loads(report_path.read_text())
            assert report['status'] == 'passed' and report['all_requested_checks_passed'] is True
            assert report['fresh_full_advisory_check'] is True
            assert report['network_mode'] == 'live advisories, locked vet imports'
            expected_checks = {'source_review', 'cargo_audit', 'npm_audit', 'cargo_deny',
                               'cargo_vet', 'advisory_dispositions', 'input_integrity'}
            assert set(report['checks']) == expected_checks
            assert all(value['status'] == 'passed' for value in report['checks'].values())
            record['steps'][-1]['dependency_report'] = str(report_path)
            record['steps'][-1]['dependency_report_sha256'] = hashlib.sha256(report_path.read_bytes()).hexdigest()

        if row.get('expected_passing_tests'):
            assert receipt['passing_tests'] == row['expected_passing_tests'], 'Test selection changed'
        if row['name'] == 'lightweight-check':
            record['source_gates_complete'] = True
        if row['name'] == 'release-build':
            assert receipt['cargo_incremental'] == '0'
            seal = sealer.seal_runtime(runtime, record['source'])
            with (base / 'p10-runtime-final-r3-seal.json').open('x', encoding='utf-8') as handle:
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
