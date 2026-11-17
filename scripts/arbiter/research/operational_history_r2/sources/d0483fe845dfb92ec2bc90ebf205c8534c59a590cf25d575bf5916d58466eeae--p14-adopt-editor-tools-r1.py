# SPDX-License-Identifier: Apache-2.0
import hashlib
import importlib.util
import json
from pathlib import Path
import subprocess

base = Path('D:/code/Github/airk/artifacts/v190')
work = base / 'p10-worktree'
proposal = base / 'p14-editor-ui-contract-proposed-r1'
manifest = json.loads((proposal / 'manifest.json').read_text())
sha = lambda path: hashlib.sha256(path.read_bytes()).hexdigest() if path.exists() else None
patch = proposal / 'editor-ui.patch'
assert sha(patch) == manifest['patch_sha256'] == '32c0b51334ff803cd752654d795f5571346e900c2c27fec64698a5f5a843bc30'
for row in manifest['files']:
    assert sha(work / row['path']) == row['base_sha256']
spec = importlib.util.spec_from_file_location('builder', work / 'scripts/build-service-runtime.py')
builder = importlib.util.module_from_spec(spec)
spec.loader.exec_module(builder)
before = builder.source_identity()
assert before == {'sha256': '5d8912b117b410306ac9a3b44af6d96d70bb018006c2d49a1efa365597fe3e11', 'files': 960}
git = ['git', '-c', 'safe.directory=' + work.as_posix(), '-C', str(work), 'apply']
subprocess.run([*git, '--check', str(patch)], check=True)
subprocess.run([*git, str(patch)], check=True)
for row in manifest['files']:
    assert sha(work / row['path']) == row['proposed_sha256']
assert builder.source_identity() == before
with (base / 'p14-editor-tools-live-adoption-r1.json').open('x') as output:
    json.dump({'status': 'reviewed test tools adopted, execution pending',
               'native_source_unchanged': before, 'patch_sha256': sha(patch),
               'files': manifest['files'], 'existing_vsix_unchanged': '49170b51765048e438dfd9516a35e4884d7667efe5ad2652d02303caf22f4c90',
               'original_unknown_dispatch': 'failed, unchanged, no replay'}, output, indent=2)
print('Three reviewed test-tool files applied; native source unchanged.')
