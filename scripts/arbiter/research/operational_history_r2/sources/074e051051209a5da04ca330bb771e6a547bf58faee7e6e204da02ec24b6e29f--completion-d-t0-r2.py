# SPDX-License-Identifier: Apache-2.0
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import time
import unittest

root = Path(__file__).resolve().parents[3]
os.chdir(root)
base = Path(__file__).resolve().parent
private = json.loads((base / 'user-private-fixture-base.json').read_text())['path']
os.environ.update(TEMP=private, TMP=private, AIRK_PRIVATE_CENSUS_BASE=private)
sys.path.insert(0, str(root / 'scripts/tests'))
modules = ['test_http_fixture', 'test_benchmark_service_http', 'test_benchmark_http_transport', 'test_mcp_investigation_benchmark']
paths = ['scripts/http_fixture.py', 'scripts/benchmark-service.py', 'scripts/benchmark-mcp-investigation.py']
paths.extend('scripts/tests/' + name + '.py' for name in modules)
def identity():
    return {name: hashlib.sha256((root / name).read_bytes()).hexdigest() for name in paths}
before = identity()
started = time.monotonic()
with (base / 'completion-d-t0-r2.log').open('x', encoding='utf-8') as log:
    suite = unittest.defaultTestLoader.loadTestsFromNames(modules)
    selected = suite.countTestCases()
    result = unittest.TextTestRunner(stream=log, verbosity=2).run(suite)
receipt = dict(command=['python', str(Path(__file__).relative_to(root))], scope='D targeted Python T0; no native runtime',
               selected=selected, ran=result.testsRun, failures=len(result.failures), errors=len(result.errors), skipped=len(result.skipped),
               passed=result.wasSuccessful() and selected > 0, elapsed_seconds=time.monotonic()-started,
               execution_identity=subprocess.run(['whoami'], capture_output=True, text=True, check=True).stdout.strip(),
               source_identity=before, scoped_inputs_unchanged=before == identity(), log='completion-d-t0-r2.log')
(base / 'completion-d-t0-r2.json').write_text(json.dumps(receipt, indent=2) + '\n')
print(json.dumps(receipt))
raise SystemExit(0 if receipt['passed'] else 1)
