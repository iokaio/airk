# SPDX-License-Identifier: Apache-2.0
import hashlib, json, subprocess
from pathlib import Path
root = Path('D:/code/Github/airk')
base = root / 'artifacts/v190'
manifest = json.loads((base / 'p10-integration-manifest-r2.json').read_text())
paths = [row['path'] for row in manifest['files']]
assert len(paths) == 125 and manifest['mac_verification_pending'] is False
assert all(hashlib.sha256((root / row['path']).read_bytes()).hexdigest() == row['tested'] for row in manifest['files'])
git = ['git', '-c', 'safe.directory=' + root.as_posix(), '-c', 'core.autocrlf=true', '-C', str(root)]
assert subprocess.check_output([*git, 'rev-parse', 'HEAD']).decode().strip() == manifest['base']
assert not subprocess.check_output([*git, 'diff', '--cached', '--name-only']).strip(), 'Unrelated staged work'
with (base / 'p10-stage-public-r1.log').open('x', encoding='utf-8') as log:
    subprocess.run([*git, 'add', '--', *paths], stdout=log, stderr=log, check=True)
    subprocess.run([*git, 'diff', '--cached', '--check'], stdout=log, stderr=log, check=True)
staged = subprocess.check_output([*git, 'diff', '--cached', '--name-only', '-z']).decode().strip('\0').split('\0')
assert set(staged) == set(paths), staged
assert all(not p.startswith(('artifacts/', 'temp/', '.airk/')) for p in staged)
assert all(hashlib.sha256((root / row['path']).read_bytes()).hexdigest() == row['tested'] for row in manifest['files'])
print(subprocess.check_output([*git, 'diff', '--cached', '--shortstat']).decode().strip())
print('Exactly125 reviewed public paths staged; whitespace passed; working bytes preserved')
