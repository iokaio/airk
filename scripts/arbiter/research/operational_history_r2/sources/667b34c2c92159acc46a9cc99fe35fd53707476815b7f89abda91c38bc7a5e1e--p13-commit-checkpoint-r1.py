# SPDX-License-Identifier: Apache-2.0
import hashlib
import json
import subprocess
from pathlib import Path

root = Path('D:/code/Github/airk')
base = root / 'artifacts/v190'
work = base / 'p10-worktree'
paths = ['docs/research/implementation-v190.md', 'docs/releases/release-1.9.0.md']
def git(where, *args):
    return subprocess.check_output(['git', '-c', 'safe.directory=' + where.as_posix(), '-c', 'core.autocrlf=true', '-c', 'core.safecrlf=false', '-C', str(where), *args], stderr=subprocess.PIPE)
def names(data):
    return set(data.decode().strip('\0').split('\0')) - {''}
assert git(root, 'rev-parse', 'HEAD').decode().strip() == '4cb4312480be4c33fe70dc48f58e4877cce05c31'
assert not git(root, 'diff', '--cached', '--name-only')
assert names(git(root, 'diff', '--name-only', '-z')) == set(paths)
assert not git(root, 'ls-files', '--others', '--exclude-standard')
assert all((root / n).read_bytes() == (work / n).read_bytes() for n in paths)
decision = json.loads((base / 'p13-decision-r1.json').read_text())
assert decision['decision'] == 'retain_current' and decision['production_changed'] is False
git(root, 'add', '--', *paths)
git(root, 'diff', '--cached', '--check')
print(git(root, 'commit', '-s', '-m', 'Record conditional P13 decision to retain current ranking').decode())
commit = git(root, 'rev-parse', 'HEAD').decode().strip()
name = git(root, 'config', 'user.name').decode().strip()
email = git(root, 'config', 'user.email').decode().strip()
assert f'Signed-off-by: {name} <{email}>' in git(root, 'log', '-1', '--format=%B').decode()
assert not git(root, 'status', '--porcelain')
def inventory():
    return {n: hashlib.sha256((work / n).read_bytes()).hexdigest() for n in names(git(work, 'ls-files', '--cached', '--others', '--exclude-standard', '-z'))}
before = inventory()
git(work, 'reset', '--mixed', '--quiet', commit)
assert before == inventory()
git(work, 'diff', '--quiet')
with (base / 'p13-local-checkpoint-r1.json').open('x', encoding='utf-8') as output:
    json.dump({'commit': commit, 'phase': 'P13', 'decision': 'retain_current', 'production_changed': False,
               'dco_verified': True, 'root_clean': True, 'all_public_bytes_preserved': True}, output, indent=2)
print(commit)
