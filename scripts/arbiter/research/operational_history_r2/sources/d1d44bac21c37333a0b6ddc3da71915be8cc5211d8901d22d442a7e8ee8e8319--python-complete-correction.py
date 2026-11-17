# SPDX-License-Identifier: Apache-2.0
"""Prepare the locked extension prerequisites and exercise all applicable Python checks."""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess

run = Path(__file__).resolve().parent
root = run / 'native-correction-source'
snapshot = json.loads((run / 'snapshot.json').read_bytes())
patches = json.loads((run / 'native-correction.json').read_bytes())

def verify():
    for name, entry in snapshot['public_files'].items():
        assert hashlib.sha256((root / name).read_bytes()).hexdigest() == patches['changes'].get(name, entry)['sha256'], name
    spec = importlib.util.spec_from_file_location('builder', root / 'scripts/build-service-runtime.py')
    builder = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(builder)
    assert builder.source_identity() == patches['corrected_native_source']

verify()
environment = dict(os.environ)
environment['PATH'] = '/Users/tylerjensen/airk-builds/timing-1.6.4.PutU1P/tools/node-v24.15.0-darwin-arm64/bin:' + environment['PATH']
results = {}
for name, command in [
    ('npm-inputs', ['npm', 'ci', '--offline', '--ignore-scripts', '--no-audit', '--no-fund', '--prefix', 'packages/vscode']),
    ('compile', ['npm', 'run', 'compile', '--prefix', 'packages/vscode']),
    ('tests', ['python', '-m', 'unittest', 'discover', '-s', 'scripts/tests', '-v']),
]:
    with (run / ('python-complete-correction-' + name + '.log')).open('w') as output:
        result = subprocess.run(command, cwd=root, env=environment, stdout=output, stderr=subprocess.STDOUT)
    results[name] = result.returncode
    if result.returncode:
        break
verify()
record = {'passed': len(results) == 3 and all(code == 0 for code in results.values()), 'steps': results,
          'source_unchanged_except_declared_corrections': True, 'native_source': patches['corrected_native_source'],
          'earlier_nine_skip_run_preserved': True}
(run / 'python-complete-correction-result.json').write_bytes(json.dumps(record, indent=2).encode('utf-8'))
print(json.dumps(record), flush=True)
raise SystemExit(0 if record['passed'] else 1)
