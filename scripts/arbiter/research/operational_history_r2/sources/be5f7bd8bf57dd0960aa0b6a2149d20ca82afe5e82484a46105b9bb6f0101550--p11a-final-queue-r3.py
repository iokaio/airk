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
import time

base = Path(__file__).resolve().parent
mapping = base / 'p11a-final-qualification-command-map-r3.json'
plan = json.loads(mapping.read_text(encoding='utf-8'))
output = base / 'p11a-final-qualification-r3.json'
assert not output.exists(), 'Use a fresh campaign identity after a correction'
owner_lock = base / 'p11a-final-qualification-owner-r3.lock'
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
    release_receipt=base/'p11a-release-runtime-r1.receipt.json'
    record['waiting_for_release_receipt']=str(release_receipt)
    save()
    waiting=time.monotonic()
    while not release_receipt.exists():
        assert time.monotonic()-waiting<7200, 'Release prerequisite did not finish'
        time.sleep(3)
    release=json.loads(release_receipt.read_text())
    assert release['actual_exit_code']==release['exit_code']==0 and release['cargo_incremental']=='0'
    assert release['source_before']==release['source_after']==plan['compiled_runtime_source'] and release['source_unchanged']
    assert hashlib.sha256((base/'p11a-release-runtime-r1.log').read_bytes()).hexdigest()==release['log_sha256']
    record['source']=plan['source_expected']
    record['compiled_runtime_source']=plan['compiled_runtime_source']
    record['runtime_adoption_proof']=plan['production_equivalence_proof']
    record['adopted_release_receipt']=str(release_receipt)
    adoption_path=Path(plan['production_equivalence_proof'])
    assert hashlib.sha256(adoption_path.read_bytes()).hexdigest()==plan['production_equivalence_proof_sha256']
    adoption=json.loads(adoption_path.read_text())
    assert adoption['current_test_source']==plan['source_expected'] and adoption['compiled_runtime_source']==plan['compiled_runtime_source']
    seal=sealer.seal_runtime(runtime,plan['compiled_runtime_source'])
    original_seal_path=base/'p11a-runtime-final-r1-seal.json'
    assert seal==json.loads(original_seal_path.read_text()), 'Adopted runtime differs from original saved R1 pair'
    record['original_runtime_seal_sha256']=hashlib.sha256(original_seal_path.read_bytes()).hexdigest()
    record['production_equivalence_proof_sha256']=plan['production_equivalence_proof_sha256']
    preflight=json.loads((base/'p11a-transport-preflight-r4.json').read_text())
    assert preflight['status']=='passed' and preflight['test_source']==plan['source_expected'] and preflight['runtime_source']==plan['compiled_runtime_source']['sha256']
    record['adopted_checks']=[]
    for adopted in plan['adopted_checks']:
        receipt_path=Path(adopted['receipt'])
        observed=json.loads(receipt_path.read_text())
        log_path=receipt_path.with_name(receipt_path.name.replace('.receipt.json','.log'))
        assert observed['command']==adopted['expected_command']
        assert observed['exit_code']==observed['actual_exit_code']==0 and observed['passing_tests']==1
        assert observed['source_before']==observed['source_after']==plan['source_expected'] and observed['source_unchanged']
        assert hashlib.sha256(log_path.read_bytes()).hexdigest()==observed['log_sha256']
        record['adopted_checks'].append({'name':adopted['name'],'receipt':str(receipt_path),'log_sha256':observed['log_sha256'],'passing_tests':1,'executed_in_this_queue':False})
    with (base/'p11a-runtime-adoption-r3-seal.json').open('x',encoding='utf-8') as handle:
        json.dump(seal,handle,indent=2)
    record['adopted_prerequisites']=[]
    for adopted in plan['adopted_prerequisites']:
        rp=Path(adopted['receipt']); r=json.loads(rp.read_text())
        lp=rp.with_name(rp.name.replace('.receipt.json','.log'))
        assert r['actual_exit_code']==r['exit_code']==0 and r['source_before']==r['source_after']==plan['source_expected'] and r['source_unchanged']
        assert hashlib.sha256(lp.read_bytes()).hexdigest()==r['log_sha256']
        record['adopted_prerequisites'].append({'name':adopted['name'],'receipt':str(rp),'log_sha256':r['log_sha256'],'executed_in_this_queue':False})
    resolution_path=Path(plan['workspace_resolution_proof'])
    assert hashlib.sha256(resolution_path.read_bytes()).hexdigest()==plan['workspace_resolution_proof_sha256']
    resolution=json.loads(resolution_path.read_text())
    assert resolution['source']==plan['source_expected'] and resolution['status']=='sole_documentation_failure_resolved_by_focused_normal_check'
    repair=json.loads(Path(resolution['repair']['receipt']).read_text())
    assert repair['exit_code']==repair['actual_exit_code']==0 and repair['passing_tests']==1
    assert repair['source_before']==repair['source_after']==plan['source_expected']
    record['resolved_workspace']=resolution
    record['source_gates_complete']=True
    assert source_builder.source_identity() == plan['source_expected'], 'Source changed before P11a gates'
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
            assert sealer.seal_runtime(runtime, plan['compiled_runtime_source']) == seal, 'Runtime drift'
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
                assert sealer.seal_runtime(runtime, plan['compiled_runtime_source']) == seal, 'Runtime drift after command'
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
        if row['name']=='workspace-tests':
            import xml.etree.ElementTree as ET
            junit=base/'p10-worktree/target/nextest/ci/junit.xml'
            cases=ET.parse(junit).getroot().findall('.//testcase')
            assert len(cases)>2000 and not any(c.find('failure') is not None or c.find('error') is not None for c in cases), 'Routine workspace results incomplete'
            record['steps'][-1]['nextest_cases']=len(cases)
            record['steps'][-1]['junit_sha256']=hashlib.sha256(junit.read_bytes()).hexdigest()
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
            seal = sealer.seal_runtime(runtime, plan['compiled_runtime_source'])
            with (base / 'p11a-runtime-final-r1-seal.json').open('x', encoding='utf-8') as handle:
                json.dump(seal, handle, indent=2)
                handle.write('\n')
        if seal is not None:
            assert sealer.seal_runtime(runtime, plan['compiled_runtime_source']) == seal, 'Runtime drift'
        save()
    record['status'] = 'passed'
except BaseException as error:
    record['status'] = 'failed'
    record['failure'] = str(error)
    save()
    raise
save()
