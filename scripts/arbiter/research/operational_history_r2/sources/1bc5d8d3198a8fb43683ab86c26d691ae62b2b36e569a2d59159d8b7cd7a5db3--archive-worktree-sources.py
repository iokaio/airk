# SPDX-License-Identifier: Apache-2.0
import json
import hashlib
import zipfile
from pathlib import Path
import subprocess

root = Path.cwd().resolve()
retention = root / 'artifacts/worktree-retention/2026-11-10'
retention.mkdir(parents=True, exist_ok=True)
selected = [
    ('artifacts/v190/p10-worktree', 'f3d981ae'),
    ('artifacts/v190/p6e-worktree', 'd098d2cd'),
    ('artifacts/v190/p6f-worktree', 'd098d2cd'),
    ('artifacts/worktrees/one-process-workers', 'a655891d'),
]
records = []
for relative, expected in selected:
    worktree = (root / relative).resolve(strict=True)
    assert worktree.is_relative_to(root) and worktree != root
    git = ['git', '-c', 'safe.directory=' + worktree.as_posix(), '-C', str(worktree)]
    ref = 'archive/worktrees-20261110/' + worktree.name
    original = subprocess.check_output(git + ['rev-parse', 'HEAD']).decode().strip()
    existing = subprocess.run(git + ['show-ref', '--verify', '--quiet', 'refs/heads/' + ref]).returncode == 0
    if existing:
        assert original == subprocess.check_output(git + ['rev-parse', ref]).decode().strip()
        original = subprocess.check_output(git + ['rev-parse', 'HEAD^']).decode().strip()
    assert original.startswith(expected), (relative, original)
    paths = set(subprocess.check_output(git + ['diff', '--name-only', '-z', original, '--'], stderr=subprocess.DEVNULL).split(b'\0'))
    paths.update(subprocess.check_output(git + ['ls-files', '--others', '--exclude-standard', '-z'], stderr=subprocess.DEVNULL).split(b'\0'))
    paths.discard(b'')
    assert paths
    for raw in paths:
        name = raw.decode()
        assert not name.startswith(('artifacts/', 'target/', 'temp/', '.codex/', '.aws/')), name
    destination = retention / worktree.name
    destination.mkdir(exist_ok=True)
    roster = destination / 'source-paths.nul'
    roster.write_bytes(b'\0'.join(sorted(paths)) + b'\0')
    if not existing:
        subprocess.run(git + ['add', '-A', '--pathspec-from-file=' + str(roster), '--pathspec-file-nul'], check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    message = ('Preserve historical ' + worktree.name + ' source snapshot\n\n'
               'Archive the exact outstanding source changes before removing this linked\n'
               'worktree. This historical implementation is superseded by the current\n'
               'integrated feat/simplify tree and is retained as source history only.\n\n'
               'No tests or builds run. Ignored artifacts, private state and index\n'
               'fixtures are excluded and retained locally where needed.')
    if not existing:
        subprocess.run(git + ['commit', '-s', '-m', message], check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    body = subprocess.check_output(git + ['show', '-s', '--format=%B', 'HEAD']).decode()
    assert 'Signed-off-by: Tyler Jensen <tyler@tsjensen.com>' in body
    commit = subprocess.check_output(git + ['rev-parse', 'HEAD']).decode().strip()
    if not existing:
        subprocess.run(git + ['branch', ref, commit], check=True)
    # Older worktrees have stat-only CRLF changes. Preserve raw source bytes
    # separately, then require Git's actual normalized diff to be empty.
    raw_paths = paths | set(subprocess.check_output(git + ['ls-files', '-m', '-z']).split(b'\0'))
    raw_paths.discard(b'')
    raw_sources = {}
    with zipfile.ZipFile(destination / 'raw-working-source.zip', 'w', zipfile.ZIP_DEFLATED) as archive:
        for raw in sorted(raw_paths):
            name = raw.decode()
            assert not name.startswith(('artifacts/', 'target/', 'temp/', '.codex/', '.aws/')), name
            source = worktree / name
            if source.is_file():
                data = source.read_bytes()
                archive.writestr(name, data)
                raw_sources[name] = hashlib.sha256(data).hexdigest()
    (destination / 'raw-source-digests.json').write_text(json.dumps(raw_sources, indent=2) + '\n')
    assert subprocess.run(git + ['diff', '--quiet', 'HEAD', '--'], stderr=subprocess.DEVNULL).returncode == 0
    assert not subprocess.check_output(git + ['ls-files', '--others', '--exclude-standard', '-z'], stderr=subprocess.DEVNULL).strip(b'\0')
    record = {'worktree': relative, 'original_head': original, 'source_snapshot': commit, 'reference': ref, 'source_paths': len(paths)}
    records.append(record)
    (retention / 'source-checkpoints.json').write_text(json.dumps(records, indent=2) + '\n')
    print(json.dumps(record), flush=True)
