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
plan_path = base / 'p14-broader-commands-r2.json'
def read(path):
    return json.loads(path.read_text(encoding='utf-8'))
def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()
def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module
assert sha(plan_path) == '5a3b39c3525a943bc241dd67fafa083a93b7bae2c0e14c64f735bf816b443c34'
plan = read(plan_path)
source, runtime = plan['source_expected'], Path(plan['runtime'])
final = read(base / 'p14-final-qualification-r3.json')
assert final['status'] == 'passed' and final['source'] == source
assert len(final['steps']) == 15 and all(r['exit_code'] == 0 for r in final['steps'])
final_map_path = base / 'p14-final-qualification-command-map-r3.json'
assert sha(final_map_path) == final['map_sha256']
final_map = read(final_map_path)
private = final_map['private_base_proof']
os.environ.update(PYTHONUTF8='1', CARGO_NET_OFFLINE='true',
    GIT_CONFIG_GLOBAL=str(base / 'p10-owned-ignore-controls/global.gitconfig'),
    AIRK_PRIVATE_CENSUS_BASE=str(base / 'p10-private-fixtures-619c272e8a644988b26bbe4334fc8c02'))
for key in ['AIRK_P10_RESOURCE_OUTPUT_DIR', 'AIRK_P11B_RESOURCE_OUTPUT_DIR', 'AIRK_CATALOG_CANDIDATE_BINARY', 'AIRK_TEST_BINARY']:
    os.environ.pop(key, None)
assert Path(os.environ['AIRK_PRIVATE_CENSUS_BASE']).resolve() == Path(private['path']).resolve()
builder = load('p14_broader_builder', work / 'scripts/build-service-runtime.py')
sealer = load('p14_broader_sealer', base / 'p9-integrated-checks.py')
seal = read(base / 'p14-runtime-final-r1-seal.json')
out = base / 'p14-broader-remainder-r1.json'
assert not out.exists()
record = {'status': 'running', 'source': source, 'plan_sha256': sha(plan_path), 'steps': [], 'prior_failure': 'p14-broader-qualification-r1.json: scale10000 typed selection_busy; retained unresolved, not adopted as pass',
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
    for name, digest in final_map['pinned_inputs'].items():
        assert sha(Path(name)) == digest, 'Accepted final input changed: ' + name
    for name, digest in plan['driver_pins'].items():
        assert sha(Path(name)) == digest, 'Reviewed driver/helper changed: ' + name
save()
try:
    for row in plan['commands'][2:]:
        pins()
        assert sha(Path(private['receipt'])) == private['receipt_sha256']
        check = subprocess.run(['pwsh', '-NoProfile', '-File', private['verifier'], '-ExpectedReceiptSha256', private['receipt_sha256']], capture_output=True, text=True)
        assert check.returncode == 0, check.stdout + check.stderr
        observed = json.loads(check.stdout.lstrip('\ufeff'))
        assert all(observed[k] == private[k] for k in ['path', 'owner_sid', 'sddl']) and observed['acl_mutated'] is False
        log = Path(row['log'])
        assert not log.exists() and not log.with_suffix('.receipt.json').exists()
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
        pins()
    record['status'] = 'passed'
    save()
except BaseException as error:
    record['status'] = 'failed'
    record['failure'] = str(error)
    save()
    raise
