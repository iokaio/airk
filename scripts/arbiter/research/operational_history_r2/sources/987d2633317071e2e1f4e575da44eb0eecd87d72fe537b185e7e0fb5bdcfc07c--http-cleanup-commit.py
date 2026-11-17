# SPDX-License-Identifier: Apache-2.0
import hashlib
import os
from pathlib import Path
import subprocess
import uuid

root = Path.cwd()
git = ['git', '-c', 'safe.directory=' + root.as_posix()]
paths = ['crates/airk-cli/src/runtime_install/http_cleanup.rs', 'docs/service-installation.md']
head = subprocess.check_output(git + ['rev-parse', 'HEAD']).decode().strip()
assert head.startswith('a655891d'), 'HEAD changed since review'
prior_staging = subprocess.check_output(git + ['diff', '--cached', '--binary'])
prior_index = hashlib.sha256((root / '.git/index').read_bytes()).hexdigest()
directory = root / 'artifacts/one-process' / ('http-cleanup-index-' + uuid.uuid4().hex)
directory.mkdir()
env = dict(os.environ, GIT_INDEX_FILE=str(directory / 'index'))
def selected(*args):
    return subprocess.check_output(git + list(args), env=env)
selected('read-tree', head)
selected('apply', '--cached', str(root / 'artifacts/one-process/http-cleanup-commit.patch'))
names = selected('diff', '--cached', '--name-only').decode().splitlines()
assert sorted(names) == sorted(paths), names
print(selected('diff', '--cached', '--stat').decode(), flush=True)
message = ('Allow HTTP disposal proof for the selected modern predecessor\n\n'
           'Keep exact predecessor and saved state identity validation while allowing\n'
           'disposal-only lookup for an initialized modern installation.\n\n'
           'Stopped at the user request: no post-fix tests, installer build or PR.\n'
           'Retained snapshots and HTTP ownership checks remain unchanged.')
subprocess.run(git + ['commit', '-s', '-m', message], env=env, check=True)
assert hashlib.sha256((root / '.git/index').read_bytes()).hexdigest() == prior_index
identity = subprocess.check_output(git + ['config', 'user.name']).decode().strip()
email = subprocess.check_output(git + ['config', 'user.email']).decode().strip()
body = subprocess.check_output(git + ['show', '-s', '--format=%B', 'HEAD']).decode()
assert 'Signed-off-by: ' + identity + ' <' + email + '>' in body
subprocess.run(git + ['reset', 'HEAD', '--', *paths], check=True)
assert subprocess.check_output(git + ['diff', '--cached', '--binary']) == prior_staging
print(subprocess.check_output(git + ['log', '-1', '--format=%h %s%n%b']).decode(), flush=True)
