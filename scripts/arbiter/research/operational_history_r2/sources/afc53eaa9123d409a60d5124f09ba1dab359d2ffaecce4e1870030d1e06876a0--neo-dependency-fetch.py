# SPDX-License-Identifier: Apache-2.0
"""One primary-approved locked public dependency fetch; no native build or test."""
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import platform
import re
import subprocess
import sys
import tarfile
import time


def sha(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def main():
    if sys.flags.optimize:
        raise RuntimeError('Dependency preparation requires Python assertions enabled')
    os.umask(0o077)
    assert platform.system() == 'Darwin' and platform.machine() == 'arm64'
    run = Path(__file__).resolve().parent
    assert run.parent == Path.home() / 'airk-builds' and run.resolve(strict=True) == run
    assert re.fullmatch(r'readiness-1817-[a-z0-9-]{1,40}', run.name)
    snapshot = json.loads((run / 'snapshot.json').read_text())
    assert snapshot['label'] == run.name
    assert set(snapshot['harness_sha256']) == {'owner.py', 'runner.py', 'dependency-fetch.py'}
    assert snapshot['public_export_preflight'] == {'canonical_native_roster_equal': True, 'tracked_fixture_omissions': []}
    assert all(sha(run / name) == value for name, value in snapshot['harness_sha256'].items())
    assert sha(run / 'source.tar.gz') == snapshot['archive_sha256']
    destination = run / 'dependency-source'
    assert not destination.exists() and not (run / 'dependency-fetch.json').exists(), 'One attempt; preserve prior evidence'
    destination.mkdir(mode=0o700)
    excluded = {'.git', '.codex', '.agents', '.aws', '.mcp.json', '.ssh', 'temp', 'private',
                'index', 'indexes', 'state', 'airk-state', 'artifacts', 'target', 'node_modules', '__pycache__'}
    with tarfile.open(run / 'source.tar.gz') as archive:
        members = archive.getmembers()
        assert len(members) == len(snapshot['files']) and {member.name for member in members} == set(snapshot['files'])
        for member in members:
            relative = PurePosixPath(member.name)
            assert member.isfile() and not relative.is_absolute() and '..' not in relative.parts
            assert not set(relative.parts) & excluded and ':' not in member.name and '\\' not in member.name
            assert member.size == snapshot['files'][member.name]['bytes']
        archive.extractall(destination, filter='data')

    def verify():
        for name, value in snapshot['files'].items():
            path = destination / name
            assert path.is_file() and path.resolve(strict=True) == path and not path.is_symlink()
            assert sha(path) == value['sha256'] and path.stat().st_size == value['bytes'], name

    verify()
    environment = {key: value for key, value in os.environ.items()
                   if not key.startswith(('AIRK_', 'GIT_CONFIG_KEY_', 'GIT_CONFIG_VALUE_'))
                   and key not in {'GIT_DIR', 'GIT_WORK_TREE', 'GIT_INDEX_FILE', 'GIT_COMMON_DIR',
                                   'GIT_OBJECT_DIRECTORY', 'GIT_ALTERNATE_OBJECT_DIRECTORIES',
                                   'GIT_CONFIG', 'GIT_CONFIG_COUNT', 'GIT_CONFIG_PARAMETERS'}}
    environment.update(PATH=str(Path.home() / '.cargo/bin') + ':' + environment.get('PATH', ''),
                       CARGO_NET_OFFLINE='false', CARGO_TARGET_DIR=str(run / 'target'))
    command = ['cargo', 'fetch', '--locked', '--target', 'aarch64-apple-darwin',
               '--manifest-path', str(destination / 'Cargo.toml')]
    started = time.monotonic()
    with (run / 'dependency-fetch.log').open('x', encoding='utf-8') as log:
        result = subprocess.run(command, cwd=destination, env=environment, stdout=log, stderr=subprocess.STDOUT)
    error, unchanged = None, True
    try:
        verify()
    except Exception as failure:
        error, unchanged = repr(failure), False
    no_target = not (run / 'target').exists() and not (destination / 'target').exists()
    passed = result.returncode == 0 and unchanged and no_target
    receipt = {'schema': 'airk.readiness.neo-dependency-fetch/1', 'command': command,
               'status': 'passed' if passed else 'failed', 'exit_code': result.returncode,
               'archive_sha256': snapshot['archive_sha256'], 'native_source': snapshot['native_source'],
               'seconds': round(time.monotonic() - started, 3), 'source_unchanged': unchanged,
               'error': error, 'no_native_build': True, 'no_target_created': no_target,
               'qualification_claim': 'None; locked public registry/git dependency availability only'}
    with (run / 'dependency-fetch.json').open('x', encoding='utf-8') as output:
        json.dump(receipt, output, indent=2)
        output.write('\n')
    print(json.dumps(receipt), flush=True)
    return 0 if passed else 1


if __name__ == '__main__':
    raise SystemExit(main())
