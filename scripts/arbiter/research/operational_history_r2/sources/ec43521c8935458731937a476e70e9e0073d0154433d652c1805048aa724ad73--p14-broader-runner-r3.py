# SPDX-License-Identifier: Apache-2.0
"""Execute the reviewed existing P14 campaigns serially; no producer or retries."""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess

base = Path(__file__).resolve().parent
work = base / 'p10-worktree'
plan_path = base / 'p14-broader-commands-r3.json'
def read(path):
    return json.loads(path.read_text(encoding='utf-8'))
def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()
def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module
assert sha(plan_path) == 'aa2bb8260d817ec83abe3ffe46a43dffc7d6b32f5602bd44ff56acb91db55b4c'
plan = read(plan_path)
assert plan.get('ready') is True and not plan.get('pending_inputs'), 'Corrected upgrade owner pin required before launch'
source, runtime = plan['source_expected'], Path(plan['runtime'])
final = read(base / 'p14-final-qualification-r4.json')
assert final['status'] == 'passed' and final['source'] == source
assert len(final['steps']) == 15 and all(r['exit_code'] == 0 and r['seal_or_source_error'] is None for r in final['steps'])
final_map_path = base / 'p14-final-qualification-command-map-r4.json'
assert sha(final_map_path) == final['map_sha256']
final_map = read(final_map_path)
assert [r['name'] for r in final['steps']] == [r['name'] for r in final_map['commands']]
assert final_map['source_expected'] == source and Path(final_map['runtime']).resolve() == runtime.resolve()
private = final_map['private_base_proof']
os.environ.update(PYTHONUTF8='1', CARGO_NET_OFFLINE='true',
    GIT_CONFIG_GLOBAL=str(base / 'p10-owned-ignore-controls/global.gitconfig'),
    AIRK_PRIVATE_CENSUS_BASE=str(base / 'p10-private-fixtures-619c272e8a644988b26bbe4334fc8c02'))
for key in ['AIRK_P10_RESOURCE_OUTPUT_DIR', 'AIRK_P11B_RESOURCE_OUTPUT_DIR', 'AIRK_CATALOG_CANDIDATE_BINARY', 'AIRK_TEST_BINARY']:
    os.environ.pop(key, None)
assert Path(os.environ['AIRK_PRIVATE_CENSUS_BASE']).resolve() == Path(private['path']).resolve()
builder = load('p14_broader_builder', work / 'scripts/build-service-runtime.py')
sealer = load('p14_broader_sealer', base / 'p9-integrated-checks.py')
seal = read(base / 'p14-runtime-final-r2-seal.json')
out = base / 'p14-broader-qualification-r3.json'
assert not out.exists()
assert final['adopted_release']['source'] == source and final['adopted_release']['seal_sha256'] == sha(base / 'p14-runtime-final-r2-seal.json')
record = {'status': 'running', 'source': source, 'plan_sha256': sha(plan_path), 'steps': [], 'historical_stress_scope': plan['historical_stress_adoption']['scope'],
          'limits': 'Selected existing Windows campaigns only; real-corpus and VSIX/legacy qualification remain separate. Counter limitations from the owning drivers apply.'}
def save():
    pending = out.with_suffix('.pending')
    pending.write_text(json.dumps(record, indent=2) + '\n', encoding='utf-8')
    pending.replace(out)
def pins():
    assert builder.source_identity() == source
    assert sealer.seal_runtime(runtime, source) == seal
    assert sha(plan_path) == record['plan_sha256']
    assert sha(final_map_path) == final['map_sha256']
    for name, digest in {**final_map['pinned_inputs'], **final['resolved_qualification_pins']}.items():
        assert sha(Path(name)) == digest, 'Accepted final input changed: ' + name
    for name, digest in {**plan['driver_pins'], **plan['historical_stress_adoption']['pins'], **plan['historical_scale256']['pins']}.items():
        assert sha(Path(name)) == digest, 'Reviewed driver/helper changed: ' + name
def ci_summary(log, repeats):
    import re
    paths = re.findall(r'Logs:\s*([^\r\n]+)', log.read_text(encoding='utf-8'))
    assert len(paths) == 1, 'One actual ci-local output required'
    path = Path(paths[0].strip()) / 'summary.json'
    assert path.resolve().is_relative_to((work / 'artifacts/ci-local').resolve())
    rows = json.loads(path.read_text(encoding='utf-8-sig'))
    assert len(rows) == repeats * 3 and all(row['Passed'] is True for row in rows)
    assert {(row['Iteration'], row['Suite']) for row in rows} == {(n, suite) for n in range(1, repeats + 1) for suite in ['smoke', 'clients', 'install']}
    return {'path': str(path), 'sha256': sha(path), 'passed_suites': len(rows)}

save()
try:
    pins()
    old = plan['historical_stress_adoption']
    prior = read(Path(old['receipt']))
    assert prior['exit_code'] == prior['actual_exit_code'] == 0
    assert prior['source_before'] == prior['source_after'] == old['source'] and prior['source_unchanged']
    assert prior['command'] == old['argv'] and prior['command'][-4:] == ['-Cores', '4', '-Repeat', '5']
    assert sha(Path(old['log'])) == prior['log_sha256']
    record['historical_stress'] = {**ci_summary(Path(old['log']), 5), 'source': old['source'], 'receipt': old['receipt'], 'scope': old['scope']}
    control = read(Path(plan['historical_scale256']['receipt']))
    assert control['exit_code'] == control['actual_exit_code'] == 0 and control['source_before'] == control['source_after'] == old['source'] and control['source_unchanged']
    assert read(Path(plan['historical_scale256']['report']))['passed'] is True
    record['historical_scale256'] = plan['historical_scale256']
    save()
    for row in plan['commands']:
        pins()
        assert sha(Path(private['receipt'])) == private['receipt_sha256']
        check = subprocess.run(['pwsh', '-NoProfile', '-File', private['verifier'], '-ExpectedReceiptSha256', private['receipt_sha256']], capture_output=True, text=True)
        assert check.returncode == 0, check.stdout + check.stderr
        observed = json.loads(check.stdout.lstrip('\ufeff'))
        assert all(observed[k] == private[k] for k in ['path', 'owner_sid', 'sddl']) and observed['acl_mutated'] is False
        log = Path(row['log'])
        assert not log.exists() and not log.with_suffix('.receipt.json').exists()
        for flag in ['--output', '--report']:
            if flag in row['argv']:
                assert not Path(row['argv'][row['argv'].index(flag) + 1]).exists(), 'Fresh owning output required'
        print('START', row['name'], flush=True)
        result = subprocess.run(['python', str(base / 'p11-local-command.py'), str(log), *row['argv']], cwd=work, env=os.environ)
        receipt_path = log.with_suffix('.receipt.json')
        receipt = read(receipt_path)
        record['steps'].append({'name': row['name'], 'exit_code': result.returncode, 'receipt': str(receipt_path)})
        save()
        assert receipt['command'] == row['argv'] and result.returncode == receipt['exit_code'] == receipt['actual_exit_code'] == 0
        assert receipt['source_before'] == receipt['source_after'] == source and receipt['source_unchanged']
        assert sha(log) == receipt['log_sha256']
        if row['name'] == 'ignored-vector-performance':
            assert receipt['passing_tests'] == 3
        if row['name'] == 'scale-10000':
            report_path = Path(row['argv'][row['argv'].index('--output') + 1]) / 'report.json'
            report = read(report_path)
            assert report['passed'] is True and len(report['corpora']) == 1 and report['corpora'][0]['initial_files'] == 10000
            record['steps'][-1]['report'] = {'path': str(report_path), 'sha256': sha(report_path)}
        elif row['name'] == 'service-load':
            report_path = Path(row['argv'][row['argv'].index('--output') + 1])
            report = read(report_path)
            assert report['passed'] is True and {(r['projects'], r['clients']) for r in report['cases']} == {(p, c) for p in [1, 4, 10] for c in [1, 10, 50]}
            assert len(report['cases']) == 9 and all(r['rounds'] == 3 and r['source_bytes_preserved'] is True for r in report['cases'])
            record['steps'][-1]['report'] = {'path': str(report_path), 'sha256': sha(report_path)}
        elif row['name'] == 'installed-native-smokes':
            assert row['argv'][-4:] == ['-Cores', '4', '-Repeat', '1']
            record['steps'][-1]['current_ci_local'] = ci_summary(log, 1)
        save()
        pins()
    record['status'] = 'passed'
    save()
except BaseException as error:
    record['status'] = 'failed'
    record['failure'] = str(error)
    save()
    raise
