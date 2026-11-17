# SPDX-License-Identifier: Apache-2.0
import hashlib
import importlib.util
import json
from pathlib import Path
import subprocess

base = Path('D:/code/Github/airk/artifacts/v190')
work = base / 'p10-worktree'
proposal = base / 'p14-installed-editor-contract-proposed-r2'
sha = lambda path: hashlib.sha256(path.read_bytes()).hexdigest() if path.exists() else None
manifest = json.loads((proposal / 'manifest.json').read_text())
patch = proposal / 'installed-editor.patch'
assert sha(patch) == manifest['patch_sha256'] == 'a314e06f621bcd967d555abd90ec1c3cfb32d8fb46d86cd6fd32e298705715bc'
for row in manifest['files']:
    assert sha(work / row['path']) == row['base_sha256'], row['path']
spec = importlib.util.spec_from_file_location('builder', work / 'scripts/build-service-runtime.py')
builder = importlib.util.module_from_spec(spec)
spec.loader.exec_module(builder)
before = builder.source_identity()
assert before == {'sha256': '5d8912b117b410306ac9a3b44af6d96d70bb018006c2d49a1efa365597fe3e11', 'files': 960}
git = ['git', '-c', 'safe.directory=' + work.as_posix(), '-C', str(work), 'apply']
subprocess.run([*git, '--check', str(patch)], check=True)
subprocess.run([*git, str(patch)], check=True)
assert all(sha(work / row['path']) == row['proposed_sha256'] for row in manifest['files'])
assert builder.source_identity() == before
with (base / 'p14-installed-editor-live-adoption-r2.json').open('x') as output:
    json.dump({'status': 'reviewed external test correction applied, compilation pending',
               'patch_sha256': sha(patch), 'manifest_sha256': sha(proposal / 'manifest.json'),
               'files': manifest['files'], 'native_source_unchanged': before,
               'original_unknown_and_selector_failures': 'failed, preserved, not replayed'}, output, indent=2)
print('Four reviewed external-test files applied; native source unchanged.')
