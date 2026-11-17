# SPDX-License-Identifier: Apache-2.0
import importlib.util, json, os, subprocess, sys
from pathlib import Path

base = Path('D:/code/Github/airk/artifacts/v190')
work = base / 'p10-worktree'
spec = importlib.util.spec_from_file_location('builder', work / 'scripts/build-service-runtime.py')
builder = importlib.util.module_from_spec(spec)
spec.loader.exec_module(builder)
source = builder.source_identity()
assert source != json.loads((base / 'p10-final-qualification-r6.json').read_text())['source']
env = os.environ.copy()
env.update(PYTHONUTF8='1', AIRK_PRIVATE_CENSUS_BASE=json.loads((base / 'p10-private-base-r5.json').read_text())['path'])
cargo = 'C:/Users/tdotn/.cargo/bin/cargo.exe'
steps = [
    ('fmt', [cargo, 'fmt', '--all', '--check']),
    ('store-postings', [cargo, 'test', '-p', 'airk-store', '--lib', '--locked', '--offline', '--timings', 'current::tests::postings_tests::', '--', '--test-threads=4']),
    ('kernel-policy', [cargo, 'test', '-p', 'airk-kernel', '--test', 'source_policy', '--all-features', '--locked', '--offline', '--timings', '--', '--test-threads=4']),
    ('native-happy-witness', [cargo, 'test', '-p', 'airk-cli', '--lib', '--all-features', '--locked', '--offline', '--timings', 'real_empty_native_owner_completes_witness_fence_remainder_membership_and_funding_ack', '--', '--test-threads=1']),
    ('native-prefix-witness', [cargo, 'test', '-p', 'airk-cli', '--lib', '--all-features', '--locked', '--offline', '--timings', 'actual_witness_reaches_remainder_after_more_than_256_selected_prefix_files', '--', '--test-threads=1']),
    ('minimal-clippy', [cargo, 'clippy', '-p', 'airk-store', '-p', 'airk-kernel', '--all-targets', '--no-default-features', '--locked', '--offline', '--timings', '--', '-D', 'warnings']),
    ('workspace-clippy', [cargo, 'clippy', '--workspace', '--all-targets', '--all-features', '--locked', '--offline', '--timings', '--', '-D', 'warnings']),
]
output = base / 'p10-witness-correction-validation-r1.json'
assert not output.exists()
record = {'status': 'running', 'source': source, 'historical_full_qualification': 'p10-final-qualification-r6.json', 'steps': []}
def save():
    output.write_text(json.dumps(record, indent=2) + '\n')
save()
for name, command in steps:
    assert builder.source_identity() == source
    log = base / ('p10-witness-correction-' + name + '-r1.log')
    result = subprocess.run([sys.executable, str(base / 'p10-local-command.py'), str(log), *command], env=env)
    record['steps'].append({'name': name, 'exit_code': result.returncode, 'receipt': str(log.with_suffix('.receipt.json'))})
    assert builder.source_identity() == source
    if result.returncode:
        record['status'] = 'failed'
        save()
        sys.exit(result.returncode)
    save()
record['status'] = 'passed'
save()
print(json.dumps(record))
