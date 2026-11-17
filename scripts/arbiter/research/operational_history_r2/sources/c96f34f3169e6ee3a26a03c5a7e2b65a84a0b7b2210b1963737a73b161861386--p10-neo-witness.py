# SPDX-License-Identifier: Apache-2.0
"""Prepare or collect the bounded witness correction; SSH is explicit launch only."""
import ast, hashlib, importlib.util, json, subprocess, sys
from pathlib import Path

base = Path(__file__).resolve().parent
label = 'p10-native-20261106-r4'
def read(path):
    return json.loads(path.read_text(encoding='utf-8-sig'))
def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()
def retain(path, value):
    assert not path.exists()
    path.write_text(json.dumps(value, indent=2) + '\n', encoding='utf-8')
action = sys.argv[1]
assert action in ('prepare', 'launch', 'collect')
proof_path = base / (label + '-witness-prepared.json')
if action == 'prepare':
    local = read(base / 'p10-witness-correction-store-postings-r1.receipt.json')
    assert local['exit_code'] == local['actual_exit_code'] == 0 and local['passing_tests'] > 0
    assert local['source_before'] == local['source_after'] and local['source_unchanged']
    source = local['source_after']
    spec = importlib.util.spec_from_file_location('builder', base / 'p10-worktree/scripts/build-service-runtime.py')
    builder = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(builder)
    assert builder.source_identity() == source
    subprocess.run([sys.executable, str(base / 'p10-neo-prepare.py'), label], check=True)
    plan_path = base / (label + '-plan.json')
    plan = read(plan_path)
    plan['steps'] = [
        {'name': 'fmt', 'command': ['cargo', 'fmt', '--all', '--check']},
        {'name': 'store-postings', 'expected_passing_tests': local['passing_tests'],
         'command': ['cargo', 'test', '-p', 'airk-store', '--lib', '--locked', '--offline', '--timings',
                     'current::tests::postings_tests::', '--', '--test-threads=1']},
        {'name': 'native-happy-witness', 'expected_passing_tests': 1,
         'command': ['cargo', 'test', '-p', 'airk-cli', '--lib', '--all-features', '--locked', '--offline', '--timings',
                     'real_empty_native_owner_completes_witness_fence_remainder_membership_and_funding_ack', '--', '--test-threads=1']},
        {'name': 'native-prefix-witness', 'expected_passing_tests': 1,
         'command': ['cargo', 'test', '-p', 'airk-cli', '--lib', '--all-features', '--locked', '--offline', '--timings',
                     'actual_witness_reaches_remainder_after_more_than_256_selected_prefix_files', '--', '--test-threads=1']},
    ]
    plan['correction_source'] = source
    plan['not_run'] = ['Targeted witness correction only; broad R2 and metadata R3 remain distinct historical sources. No full Mac workspace/runtime/performance/editor/signing qualification.']
    plan_path.write_text(json.dumps(plan, indent=2) + '\n', encoding='utf-8')
    runner_path = base / (label + '-runner.py')
    runner = runner_path.read_text(encoding='utf-8')
    anchor = "        steps.append({'actual_exit_code': actual_exit_code, 'passing_tests': passing_tests"
    assert runner.count(anchor) == 1
    guard = "        if result.returncode == 0 and row.get('expected_passing_tests') is not None and passing_tests != row['expected_passing_tests']:\n            validation_error = 'Passing test count differs from frozen plan'\n            result.returncode = 1\n"
    runner = runner.replace(anchor, guard + anchor)
    ast.parse(runner)
    runner_path.write_text(runner, encoding='utf-8')
    snapshot = read(base / (label + '-snapshot.json'))
    assert snapshot['native_source'] == source == builder.source_identity()
    retain(proof_path, {'source': source, 'plan_sha256': digest(plan_path), 'runner_sha256': digest(runner_path),
        'archive_sha256': snapshot['archive_sha256'], 'cleanup': 'Existing unconditional finally cargo clean and target absence verification'})
else:
    proof = read(proof_path)
    for suffix, key in [('-plan.json', 'plan_sha256'), ('-runner.py', 'runner_sha256'), ('.tar.gz', 'archive_sha256')]:
        assert digest(base / (label + suffix)) == proof[key]
    subprocess.run([sys.executable, str(base / 'p10-neo-launch.py'), action, label], check=True)
    if action == 'collect':
        qualification = read(base / (label + '-qualification.json'))
        steps = read(base / (label + '-steps.json'))
        cleanup = read(base / (label + '-cleanup.json'))
        plan = read(base / (label + '-plan.json'))
        assert qualification['status'] == 'passed' and qualification['native_source'] == proof['source']
        assert len(steps) == len(plan['steps']) == 4
        assert all(s['actual_exit_code'] == s['exit_code'] == 0 for s in steps)
        assert [s['passing_tests'] for s in steps] == [None, plan['steps'][1]['expected_passing_tests'], 1, 1]
        assert cleanup['cargo_clean_exit_code'] == 0 and cleanup['target_removed'] and cleanup['source_preserved']
        assert cleanup['timing_copy_error'] is None
        retain(base / (label + '-witness-verification.json'), {'status': 'passed', 'source': proof['source'],
            'passing_tests': sum(s['passing_tests'] or 0 for s in steps), 'cleanup_sha256': digest(base / (label + '-cleanup.json'))})
print(action, label)
