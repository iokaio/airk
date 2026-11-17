# SPDX-License-Identifier: Apache-2.0
"""Integrate a reviewed public worktree delta without overwriting unrelated work."""
import argparse
import hashlib
import json
import stat
import subprocess
from pathlib import Path

ROOT = Path('D:/code/Github/airk')
WORK = ROOT / 'artifacts/v190/p10-worktree'
EXPECTED_HEAD = b'f3d981ae675b37c5a3234876785793e63bd94e60'

def git(root, *args):
    return subprocess.check_output(['git', '-c', f'safe.directory={root.as_posix()}',
                                    '-C', str(root), *args], stderr=subprocess.DEVNULL)

def digest(data):
    return hashlib.sha256(data).hexdigest()

def normalized(data):
    return data.replace(b'\r\n', b'\n')

def plain(root, relative):
    candidate = root
    for part in relative.parts:
        candidate = candidate / part
        try:
            info = candidate.lstat()
        except FileNotFoundError:
            continue
        assert not stat.S_ISLNK(info.st_mode)
        assert not getattr(info, 'st_file_attributes', 0) & getattr(stat, 'FILE_ATTRIBUTE_REPARSE_POINT', 1024)
    assert candidate.resolve().is_relative_to(root.resolve())
    return candidate

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--receipt', required=True)
    parser.add_argument('--reviewed-manifest', required=True)
    parser.add_argument('--manifest-sha256', required=True)
    parser.add_argument('--apply', action='store_true')
    args = parser.parse_args()
    assert Path(args.receipt).name == args.receipt and args.receipt not in ('.', '..')
    receipt = plain(ROOT / 'artifacts/v190', Path(args.receipt))
    assert not receipt.exists(), 'Evidence must be create-new'
    manifest_bytes = Path(args.reviewed_manifest).read_bytes()
    assert digest(manifest_bytes) == args.manifest_sha256
    manifest = json.loads(manifest_bytes)
    reviewed = {row['path']: row['source_sha256'] for row in manifest['files']}
    assert len(reviewed) == len(manifest['files'])
    head = git(ROOT, 'rev-parse', 'HEAD').strip()
    assert head == EXPECTED_HEAD == manifest['head'].encode(), 'Unexpected checkpoint'
    assert head == git(WORK, 'rev-parse', 'HEAD').strip(), 'Checkpoint heads differ'
    assert not git(WORK, 'diff', '--cached', '--name-only').strip(), 'Unexpected staged worktree changes'
    changed = git(WORK, 'diff', '--name-only', '-z').split(b'\0')
    new = git(WORK, 'ls-files', '--others', '--exclude-standard', '-z').split(b'\0')
    paths = sorted({p.decode('utf-8') for p in changed + new if p})
    assert paths == sorted(reviewed), 'Public delta differs from reviewed manifest'
    rows = []
    for name in paths:
        relative = Path(name)
        assert not relative.is_absolute() and '..' not in relative.parts
        assert relative.parts[0] not in ('artifacts', 'temp', '.git', '.codex', '.agents', '.aws', '.mcp.json')
        source = plain(WORK, relative)
        destination = plain(ROOT, relative)
        assert source.is_file(), f'Deletion needs separate review: {name}'
        source_bytes = source.read_bytes()
        assert digest(source_bytes) == reviewed[name], f'Unreviewed source bytes: {name}'
        try:
            base = git(WORK, 'show', f'HEAD:{name}')
        except subprocess.CalledProcessError:
            base = None
        before = destination.read_bytes() if destination.exists() else None
        if base is None:
            assert before is None or before == source_bytes, f'Existing new path: {name}'
        else:
            assert before is not None
            assert normalized(before) in (normalized(base), normalized(source_bytes)), f'Unrelated root changes: {name}'
        rows.append({'path': name, 'base_sha256': digest(base) if base is not None else None,
                     'root_before_sha256': digest(before) if before is not None else None,
                     'source_sha256': digest(source_bytes), 'bytes': len(source_bytes)})
    assert paths, 'No public delta selected'
    # Complete preflight precedes every write. Native/test processes must be stopped by owner.
    if args.apply:
        for row in rows:
            source, destination = plain(WORK, Path(row['path'])), plain(ROOT, Path(row['path']))
            data = source.read_bytes()
            assert digest(data) == row['source_sha256'], 'Worktree changed during integration'
            current = destination.read_bytes() if destination.exists() else None
            assert (digest(current) if current is not None else None) == row['root_before_sha256']
            destination.parent.mkdir(parents=True, exist_ok=True)
            destination.write_bytes(data)
        for row in rows:
            assert digest((ROOT / row['path']).read_bytes()) == row['source_sha256']
    result = {'status': 'applied' if args.apply else 'preflight_only', 'head': head.decode(),
              'files': rows, 'count': len(rows), 'write_scope': 'exact reviewed manifest only'}
    with receipt.open('x', encoding='utf-8') as output:
        output.write(json.dumps(result, indent=2) + '\n')
    print(json.dumps({'status': result['status'], 'count': len(rows), 'receipt': str(receipt)}))

if __name__ == '__main__':
    main()
