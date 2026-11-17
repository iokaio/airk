# SPDX-License-Identifier: Apache-2.0
"""Prepared remote public-source overlay. Root-only; no execution by preparer."""
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import stat
import subprocess
import sys
import tarfile

root = Path.cwd().resolve()
assert os.path.samefile(root, Path.home() / 'code/airk')
plan_path, source_path, archive_path = map(Path, sys.argv[1:4])
for path, expected in zip((plan_path, source_path, archive_path), sys.argv[4:7]):
    assert hashlib.sha256(path.read_bytes()).hexdigest() == expected
assert len(sys.argv) == 7
plan = json.loads(plan_path.read_bytes())
source_raw = source_path.read_bytes()
source = json.loads(source_raw)
seal = source['source_set_sha256']
assert os.environ.get('AIRK_PUBLIC_SYNC_BATON') == 'mac-current-read-' + seal[:16]
assert plan['workingtree_source_sha256'] == seal
assert plan['status'] == 'FROZEN_PUBLIC_DELTA_ROOT_EXECUTION_REVIEW_REQUIRED'
active = [Path(line.strip()).name for line in subprocess.check_output(['/bin/ps', '-axo', 'comm='], text=True).splitlines()
          if Path(line.strip()).name in ('cargo', 'cargo-nextest', 'rustc', 'clippy-driver', 'rustfmt', 'clang', 'clang++', 'cc', 'c++', 'ld', 'ar')]
assert not active


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def safe(name):
    relative = PurePosixPath(name)
    assert not relative.is_absolute() and '..' not in relative.parts
    assert not set(relative.parts) & {'temp', '.airk', '.git', 'node_modules', 'target', 'artifacts'}
    path = root.joinpath(*relative.parts)
    if path.is_file():
        assert path.stat().st_nlink == 1
    for part in (path, *path.parents):
        assert not part.is_symlink()
        if part == root:
            return path
    raise ValueError('public source outside approved checkout')


base = root / 'artifacts/advanced-split/receipts/mac-current-read-source-r15.json'
assert digest(base) == plan['previous_applied_R15_manifest_sha256']
previous = json.loads(base.read_bytes())
for name, row in previous['files'].items():
    assert safe(name).is_file() and digest(safe(name)) == row['sha256'], name
for name in ('crates/airk-cli/build.rs',):
    assert not safe(name).exists(), name
for name, row in source['files'].items():
    path = safe(name)
    expected = plan['destination_preimages'][name]
    if expected is None:
        assert not path.exists(), name
    else:
        assert path.is_file() and digest(path) == expected['sha256'], name
actual_assets = {}
for p in (root / 'crates/airk-cli/admin-assets').iterdir():
    name = p.relative_to(root).as_posix()
    assert safe(name).is_file()
    actual_assets[name] = digest(p)
assert actual_assets == {name: row['sha256'] for name, row in plan['remote_admin_asset_roster'].items()}
with tarfile.open(archive_path, 'r:gz') as archive:
    members = archive.getmembers()
    assert {m.name for m in members} == set(plan['changes'])
    assert all(m.isfile() and not m.issym() and not m.islnk() for m in members)
    payload = {}
    for member in members:
        data = archive.extractfile(member).read()
        assert len(data) == source['files'][member.name]['bytes']
        assert hashlib.sha256(data).hexdigest() == source['files'][member.name]['sha256']
        payload[member.name] = data
stage = root / ('artifacts/advanced-split/receipts/mac-current-read-transfer-' + seal[:16])
stage.mkdir(mode=0o700, exist_ok=False)
recovery = {}
for name in [*plan['changes'], *plan['retire_exact_stale_admin_assets']]:
    path = safe(name)
    if path.exists():
        original = path.read_bytes()
        backup = stage / 'before' / name
        backup.parent.mkdir(parents=True, exist_ok=True)
        with backup.open('xb') as stream:
            stream.write(original)
            stream.flush()
            os.fsync(stream.fileno())
        info = path.stat()
        recovery[name] = {'sha256': hashlib.sha256(original).hexdigest(),
                          'mode': stat.S_IMODE(info.st_mode), 'device': info.st_dev, 'inode': info.st_ino,
                          'backup': str(backup.relative_to(stage))}
    else:
        recovery[name] = None
with (stage / 'recovery.json').open('x') as stream:
    json.dump({'source_sha256': seal, 'originals': recovery}, stream, indent=2)
    stream.flush()
    os.fsync(stream.fileno())
for name, data in payload.items():
    path = safe(name)
    original = recovery[name]
    if original is not None:
        assert digest(path) == original['sha256']
        info = path.stat()
        assert (info.st_dev, info.st_ino) == (original['device'], original['inode'])
        with path.open('r+b') as stream:
            stream.write(data)
            stream.truncate()
            stream.flush()
            os.fsync(stream.fileno())
    else:
        path.parent.mkdir(parents=True, exist_ok=True)
        with path.open('xb') as stream:
            stream.write(data)
            stream.flush()
            os.fsync(stream.fileno())
for name, expected in plan['retire_exact_stale_admin_assets'].items():
    path = safe(name)
    assert name.startswith('crates/airk-cli/admin-assets/app.')
    assert digest(path) == expected['sha256'] == recovery[name]['sha256']
    path.unlink()
for name, row in source['files'].items():
    assert safe(name).is_file() and digest(safe(name)) == row['sha256'], name
expected_assets = {name for name in source['files'] if name.startswith('crates/airk-cli/admin-assets/')}
assert {p.relative_to(root).as_posix() for p in (root / 'crates/airk-cli/admin-assets').iterdir()} == expected_assets
manifest = root / 'artifacts/advanced-split/receipts/mac-current-read-source-r16.json'
with manifest.open('xb') as stream:
    stream.write(source_raw)
    stream.flush()
    os.fsync(stream.fileno())
print(json.dumps({'complete': True, 'source_set_sha256': seal, 'changed': len(payload),
                  'retired_stale_assets': len(plan['retire_exact_stale_admin_assets']),
                  'recovery_receipt': str(stage / 'recovery.json'), 'no_Cargo_or_target_cleanup': True}))
