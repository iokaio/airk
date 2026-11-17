# SPDX-License-Identifier: Apache-2.0
"""Inventory/clean only owned Cargo targets in the existing AIRK build workspace."""
from datetime import datetime, timezone
import collections, hashlib, json, os, pathlib, shutil, subprocess, sys, uuid

action = sys.argv[1]
assert action in {'inventory', 'clean', 'profile_probe', 'verified_clean'}
base = pathlib.Path('/Users/tylerjensen/airk-builds').resolve(strict=True)
assert base == pathlib.Path('/Users/tylerjensen/airk-builds') and not base.is_symlink()
pending = collections.deque([(base, 0)])
targets, manifests, examined = [], [], 0
skip = {'.git', '.cargo', '.rustup', 'node_modules', 'artifacts', 'fixtures', 'tools'}
while pending:
    parent, depth = pending.popleft()
    for entry in os.scandir(parent):
        examined += 1
        assert examined <= 50000, 'build workspace inventory exceeded bound'
        path = pathlib.Path(entry.path)
        if entry.is_symlink():
            continue
        if entry.is_dir(follow_symlinks=False):
            if entry.name == 'target':
                marker = path / 'CACHEDIR.TAG'
                cargo_outputs = (path / 'debug/.fingerprint').is_dir() or (path / 'release/.fingerprint').is_dir()
                if cargo_outputs or marker.is_file() and '8a477f597d28d172789f06886806bc55' in marker.read_text(errors='replace')[:512]:
                    assert path.resolve() == path and path.is_relative_to(base) and path.stat().st_uid == os.getuid()
                    targets.append(path)
            elif depth < 4 and entry.name not in skip and not entry.name.startswith('.') and not entry.name.startswith('cargo-clean-'):
                pending.append((path, depth + 1))
        elif entry.name == 'Cargo.toml' and entry.is_file(follow_symlinks=False) and path.stat().st_size <= 65536:
            text = path.read_text()
            if '[workspace]' in text and 'crates/airk-core' in text:
                manifests.append(path)
targets, manifests = sorted(set(targets)), sorted(set(manifests))
assert targets and manifests, 'owned targets or AIRK workspace manifest unavailable'
processes = subprocess.check_output(['/bin/ps', '-axo', 'pid=,comm='], text=True)
active = [line.strip() for line in processes.splitlines() if line.split() and pathlib.Path(line.split(maxsplit=1)[-1]).name in {'cargo', 'rustc', 'clippy-driver'}]
inventory = {'base': str(base), 'targets': [str(path) for path in targets], 'manifests': [str(path) for path in manifests],
             'active_build_processes': active, 'free_bytes': shutil.disk_usage(base).free, 'examined_entries': examined}
print(json.dumps(inventory), flush=True)
if action == 'inventory':
    raise SystemExit(0)
assert not active, 'another Cargo/Rust build is active; no cleanup started'
if action == 'profile_probe':
    target = base / 'p6a-20261103.jBR4Jh/p6f-focused-setup-u1/target'
    assert target in targets and target.resolve() == target and target.stat().st_uid == os.getuid()
    command = ['/Users/tylerjensen/.cargo/bin/cargo', 'clean', '--profile', 'dev', '--locked', '--manifest-path', str(manifests[0]), '--target-dir', str(target)]
    result = subprocess.run(command, cwd=manifests[0].parent, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    print(json.dumps({'command':command,'exit_code':result.returncode,'output':result.stdout,'remaining_target_entries':sorted(path.name for path in target.iterdir())}),flush=True)
    raise SystemExit(result.returncode)
if action == 'verified_clean':
    previous_path = base / 'cargo-clean-69d305e3933f/receipt.json'
    previous = json.loads(previous_path.read_text())
    selected = [pathlib.Path(row['target']) for row in previous['steps'] if row['exit_code'] != 0]
    assert len(selected) == 5 and set(selected) == set(targets)
    proof = []
    for target in selected:
        assert target.resolve() == target and target.is_relative_to(base) and target.name == 'target'
        assert not target.is_symlink() and target.stat().st_uid == os.getuid()
        assert not (target / 'CACHEDIR.TAG').exists(), 'an existing cache marker must not be overwritten'
        paired = [path for path in manifests if path.parent == target.parent or path.parent.parent == target.parent]
        assert paired, 'target has no paired AIRK source workspace'
        fingerprints = []
        for profile in ('debug', 'release'):
            directory = target / profile / '.fingerprint'
            if directory.is_dir():
                assert not directory.is_symlink() and directory.resolve().is_relative_to(target)
                fingerprints.extend(path.name for path in directory.iterdir() if path.name.startswith(('airk-', 'airk_')))
        assert fingerprints and (target / '.rustc_info.json').is_file(), 'AIRK compiler cache proof missing'
        assert not any(path.is_relative_to(target) for path in manifests)
        manifest = paired[0]
        proof.append({'target': str(target), 'device': target.stat().st_dev, 'inode': target.stat().st_ino,
                      'owner_uid': target.stat().st_uid, 'manifest': str(manifest),
                      'manifest_sha256': hashlib.sha256(manifest.read_bytes()).hexdigest(),
                      'airk_fingerprint_count': len(fingerprints), 'airk_fingerprint_examples': sorted(fingerprints)[:4]})
    out = base / ('cargo-clean-verified-' + uuid.uuid4().hex[:12])
    out.mkdir(mode=0o700)
    record = {'started_utc': datetime.now(timezone.utc).isoformat(), 'initial_refusals': str(previous_path),
              'free_bytes_before': shutil.disk_usage(base).free, 'proof': proof, 'steps': [],
              'preserved_inputs': previous['preserved_inputs']}
    (out / 'receipt-before.json').write_text(json.dumps(record, indent=2) + '\n')
    for item in proof:
        target = pathlib.Path(item['target'])
        assert target.stat().st_dev == item['device'] and target.stat().st_ino == item['inode']
        with (target / 'CACHEDIR.TAG').open('x') as marker:
            marker.write('Signature: 8a477f597d28d172789f06886806bc55\n# Verified owned AIRK compiler cache; explicit user-requested cleanup.\n')
        command = ['/Users/tylerjensen/.cargo/bin/cargo', 'clean', '--locked', '--manifest-path', item['manifest'], '--target-dir', str(target)]
        logfile = out / ('cargo-clean-' + str(len(record['steps'])) + '.log')
        with logfile.open('x') as log:
            result = subprocess.run(command, cwd=pathlib.Path(item['manifest']).parent, stdout=log, stderr=subprocess.STDOUT)
        row = {'target': str(target), 'command': command, 'exit_code': result.returncode,
               'cache_tag_created_after_verification': True, 'log': str(logfile), 'output': logfile.read_text(errors='replace')}
        record['steps'].append(row)
        (out / 'receipt.json').write_text(json.dumps(record, indent=2) + '\n')
        print(json.dumps(row), flush=True)
    record.update(completed_utc=datetime.now(timezone.utc).isoformat(), free_bytes_after=shutil.disk_usage(base).free)
    record['free_bytes_delta'] = record['free_bytes_after'] - record['free_bytes_before']
    (out / 'receipt.json').write_text(json.dumps(record, indent=2) + '\n')
    print(json.dumps({'receipt': str(out / 'receipt.json'), 'free_bytes_after': record['free_bytes_after'],
                      'free_bytes_delta': record['free_bytes_delta'], 'preserved_inputs': {name: value['path'] for name, value in record['preserved_inputs'].items()},
                      'all_cleaned': all(row['exit_code'] == 0 for row in record['steps'])}), flush=True)
    raise SystemExit(0 if all(row['exit_code'] == 0 for row in record['steps']) else 1)
out = base / ('cargo-clean-' + uuid.uuid4().hex[:12])
out.mkdir(mode=0o700)
record = {'started_utc': datetime.now(timezone.utc).isoformat(), 'inventory': inventory, 'preserved_inputs': {}, 'steps': []}
seed = base / 'p6a-20261103.jBR4Jh/target'
assert seed in targets, 'saved model/runtime seed target absent from inventory'
for name in ('local-vector-model', 'local-onnx-runtime'):
    source = seed / name
    if not source.exists():
        continue
    destination = out / 'inputs' / name
    expected = {}
    for folder, dirs, files in os.walk(source, followlinks=False):
        for entry in dirs + files:
            path = pathlib.Path(folder) / entry
            assert not path.is_symlink() and path.resolve().is_relative_to(seed)
        for entry in files:
            path = pathlib.Path(folder) / entry
            assert path.stat().st_size <= 256 << 20
            with path.open('rb') as stream:
                expected[str(path.relative_to(source))] = hashlib.file_digest(stream, 'sha256').hexdigest()
    assert len(expected) <= 4096
    shutil.copytree(source, destination)
    for name_in_tree, digest in expected.items():
        with (destination / name_in_tree).open('rb') as stream:
            assert hashlib.file_digest(stream, 'sha256').hexdigest() == digest
    record['preserved_inputs'][name] = {'path': str(destination), 'sha256': expected}
environment = dict(os.environ)
environment['PATH'] = '/Users/tylerjensen/.cargo/bin:' + environment.get('PATH', '')
for offset, target in enumerate(targets):
    assert target.resolve() == target and target.is_relative_to(base) and target.name == 'target'
    assert target.stat().st_uid == os.getuid() and not target.is_symlink()
    assert not any(manifest.is_relative_to(target) for manifest in manifests)
    command = ['/Users/tylerjensen/.cargo/bin/cargo', 'clean', '--locked', '--manifest-path', str(manifests[0]), '--target-dir', str(target)]
    logfile = out / f'cargo-clean-{offset:02d}.log'
    with logfile.open('x') as log:
        result = subprocess.run(command, cwd=manifests[0].parent, env=environment, stdout=log, stderr=subprocess.STDOUT)
    row = {'target': str(target), 'command': command, 'exit_code': result.returncode, 'log': str(logfile), 'output': logfile.read_text(errors='replace')}
    record['steps'].append(row)
    (out / 'receipt.json').write_text(json.dumps(record, indent=2) + '\n')
    print(json.dumps(row), flush=True)
record.update(completed_utc=datetime.now(timezone.utc).isoformat(), free_bytes_after=shutil.disk_usage(base).free)
record['free_bytes_delta'] = record['free_bytes_after'] - inventory['free_bytes']
(out / 'receipt.json').write_text(json.dumps(record, indent=2) + '\n')
print(json.dumps({'receipt': str(out / 'receipt.json'), 'free_bytes_after': record['free_bytes_after'],
                  'free_bytes_delta': record['free_bytes_delta'], 'preserved_inputs': {name: value['path'] for name, value in record['preserved_inputs'].items()},
                  'all_cleaned': all(row['exit_code'] == 0 for row in record['steps'])}), flush=True)
raise SystemExit(0 if all(row['exit_code'] == 0 for row in record['steps']) else 1)
