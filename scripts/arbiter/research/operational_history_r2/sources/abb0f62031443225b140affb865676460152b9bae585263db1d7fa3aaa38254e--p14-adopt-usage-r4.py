# SPDX-License-Identifier: Apache-2.0
import hashlib
import importlib.util
import json
from pathlib import Path
import subprocess

base = Path('D:/code/Github/airk/artifacts/v190')
work = base / 'p10-worktree'
proposal = base / 'p14-usage-stable-publication-proposed-r4'
sha = lambda path: hashlib.sha256(path.read_bytes()).hexdigest()
manifest = json.loads((proposal / 'manifest.json').read_text())
patch = proposal / 'usage-stable-publication.patch'
assert sha(patch) == manifest['patch_sha256'] == 'e833856663abe79ad020dfcc0a0bdc9b5aac044bc55066df94f1913d3152e8ee'
assert sha(work / 'scripts/check-usage-inputs.py') == manifest['driver_base_sha256']
assert not (work / 'scripts/tests/test_usage_stable_publication.py').exists()
assert sha(work / 'scripts/check-service-clients.py') == manifest['original_helper_sha256']
for name, expected in [('p14-usage-stable-native-r2', 'Usage input CLI smoke passed:'),
                       ('p14-usage-stable-unit-r2', 'Ran 9 tests')]:
    receipt = json.loads((base / (name + '.receipt.json')).read_text())
    assert receipt['exit_code'] == receipt['actual_exit_code'] == 0 and receipt['source_unchanged'] is True
    assert sha(base / (name + '.log')) == receipt['log_sha256']
    assert expected in (base / (name + '.log')).read_text()
spec = importlib.util.spec_from_file_location('builder', work / 'scripts/build-service-runtime.py')
builder = importlib.util.module_from_spec(spec)
spec.loader.exec_module(builder)
before = builder.source_identity()
assert before == {'sha256': '5d8912b117b410306ac9a3b44af6d96d70bb018006c2d49a1efa365597fe3e11', 'files': 960}
git = ['git', '-c', 'safe.directory=' + work.as_posix(), '-C', str(work), 'apply']
subprocess.run([*git, '--check', str(patch)], check=True)
subprocess.run([*git, str(patch)], check=True)
assert sha(work / 'scripts/check-usage-inputs.py') == manifest['driver_proposed_sha256']
assert sha(work / 'scripts/tests/test_usage_stable_publication.py') == manifest['new_test_sha256']
assert sha(work / 'scripts/check-service-clients.py') == manifest['original_helper_sha256']
assert builder.source_identity() == before
result = {'status': 'qualified late tooling adopted', 'native_source_before_after': before,
          'patch_sha256': sha(patch), 'driver_sha256': manifest['driver_proposed_sha256'],
          'test_sha256': manifest['new_test_sha256'], 'helper_unchanged': True,
          'native_receipt': 'p14-usage-stable-native-r2.receipt.json',
          'unit_receipt': 'p14-usage-stable-unit-r2.receipt.json',
          'new_unit_tests': 9, 'full_python_pending': True,
          'history': 'Original failed broader R3 and usage R3 receipts stay failed; final R4 input map unchanged'}
with (base / 'p14-usage-live-adoption-r1.json').open('x') as output:
    json.dump(result, output, indent=2)
print(json.dumps(result))
