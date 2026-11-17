# SPDX-License-Identifier: Apache-2.0
"""Run declared fixture/documentation corrections after the full campaign ends."""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tarfile

run = Path(__file__).resolve().parent
assert (run / 'phase.txt').read_text().strip() == 'complete'
snapshot = json.loads((run / 'snapshot.json').read_bytes())
patches = json.loads((run / 'native-correction.json').read_bytes())
assert hashlib.sha256((run / 'snapshot.json').read_bytes()).hexdigest() == patches['base_snapshot_sha256']
root = run / 'native-correction-source'
root.mkdir(exist_ok=False)
assert hashlib.sha256((run / 'source.tar.gz').read_bytes()).hexdigest() == snapshot['archive_sha256']
with tarfile.open(run / 'source.tar.gz') as archive:
    archive.extractall(root, filter='data')
subprocess.run(['git', 'read-tree', 'HEAD'], cwd=root, check=True)
for name, entry in patches['changes'].items():
    body = (run / entry['input']).read_bytes()
    assert hashlib.sha256(body).hexdigest() == entry['sha256']
    (root / name).write_bytes(body)

def verify():
    for name, entry in snapshot['public_files'].items():
        expected = patches['changes'].get(name, entry)['sha256']
        assert hashlib.sha256((root / name).read_bytes()).hexdigest() == expected, name
    spec = importlib.util.spec_from_file_location('builder', root / 'scripts/build-service-runtime.py')
    builder = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(builder)
    identity = builder.source_identity()
    assert identity == patches['corrected_native_source'], identity
    return identity

identity = verify()
environment = dict(os.environ)
results = {}
commands = [
    ('cli-library', ['cargo', 'test', '-p', 'airk-cli', '--lib', '--all-features', '--locked', '--offline', 'project_worker::admin::tests::', '--', '--test-threads=1']),
    ('clippy', ['cargo', 'clippy', '-p', 'airk-cli', '--all-targets', '--all-features', '--locked', '--offline', '--', '-D', 'warnings']),
    ('capacity', ['cargo', 'test', '-p', 'airk-cli', '--test', 'service_runtime', '--all-features', '--locked', '--offline', 'configured_query_capacity_updates_live_worker_and_reads_through_matching_channels', '--', '--exact', '--test-threads=1']),
    ('vector-components', ['cargo', 'test', '-p', 'airk-kernel', '--test', 'vector_performance', '--features', 'local-inference', '--locked', '--offline', '--', '--ignored', '--test-threads=1']),
    ('python-tests', ['python', '-m', 'unittest', 'discover', '-s', 'scripts/tests', '-v']),
    ('repository', ['pwsh', '-NoProfile', '-File', 'scripts/check-repo.ps1']),
]
for name, command in commands:
    with (run / ('native-correction-' + name + '.log')).open('w') as output:
        result = subprocess.run(command, cwd=root, env=environment, stdout=output, stderr=subprocess.STDOUT)
    results[name] = result.returncode
verify()
summary = {'passed': all(code == 0 for code in results.values()), 'steps': results,
           'source_unchanged_except_declared_corrections': True, 'native_source': identity,
           'changed_source_files': sorted(patches['changes']),
           'descriptor_soft_limit': 4096, 'original_full_run_relabelled': False}
(run / 'native-correction-result.json').write_bytes(json.dumps(summary, indent=2).encode('utf-8'))
print(json.dumps(summary), flush=True)
raise SystemExit(0 if summary['passed'] else 1)
