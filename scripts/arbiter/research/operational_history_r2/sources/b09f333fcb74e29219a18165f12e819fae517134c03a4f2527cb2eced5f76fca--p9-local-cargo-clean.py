# SPDX-License-Identifier: Apache-2.0
"""Clean only this checkout's verified target; retain pinned offline build inputs."""
from datetime import datetime, timezone
import hashlib, json, os, pathlib, shutil, subprocess, uuid

root = pathlib.Path(__file__).resolve().parents[2]
target = root / 'target'
assert root == pathlib.Path('D:/code/Github/airk').resolve()
assert target.is_dir() and not target.is_symlink() and target.resolve() == root / 'target'
assert not target.stat().st_file_attributes & 0x400
out = root / 'artifacts/v190' / ('p9-cargo-clean-local-' + uuid.uuid4().hex[:12])
out.mkdir()
record = {'started_utc': datetime.now(timezone.utc).isoformat(), 'target': str(target),
          'free_bytes_before': shutil.disk_usage(root).free, 'preserved_inputs': {}}
for name in ('local-vector-model', 'local-onnx-runtime'):
    source = target / name
    if not source.exists():
        continue
    destination = out / 'inputs' / name
    expected = {}
    for folder, dirs, files in os.walk(source, followlinks=False):
        for entry in dirs + files:
            path = pathlib.Path(folder) / entry
            assert not path.is_symlink() and not path.stat().st_file_attributes & 0x400
            assert path.resolve().is_relative_to(target)
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
environment = dict(os.environ, CARGO_HOME='C:/Users/tdotn/.cargo', RUSTUP_HOME='C:/Users/tdotn/.rustup')
environment['PATH'] = 'C:/Users/tdotn/.cargo/bin;' + environment['PATH']
command = ['C:/Users/tdotn/.cargo/bin/cargo.exe', 'clean', '--locked', '--manifest-path', str(root / 'Cargo.toml'), '--target-dir', str(target)]
record['command'] = command
(out / 'receipt-before.json').write_text(json.dumps(record, indent=2) + '\n')
print(json.dumps({'started': True, 'output': str(out), 'target': str(target), 'pinned_inputs_preserved': list(record['preserved_inputs'])}), flush=True)
with (out / 'cargo-clean.log').open('x') as log:
    result = subprocess.run(command, cwd=root, env=environment, stdout=log, stderr=subprocess.STDOUT, creationflags=subprocess.CREATE_NO_WINDOW)
record.update(exit_code=result.returncode, completed_utc=datetime.now(timezone.utc).isoformat(), free_bytes_after=shutil.disk_usage(root).free)
record['free_bytes_delta'] = record['free_bytes_after'] - record['free_bytes_before']
(out / 'receipt.json').write_text(json.dumps(record, indent=2) + '\n')
(root / 'artifacts/v190/p9-clean-inputs.local.json').write_text(json.dumps({'receipt': str(out / 'receipt.json'), 'inputs': {name: value['path'] for name, value in record['preserved_inputs'].items()}}, indent=2) + '\n')
print((out / 'cargo-clean.log').read_text(errors='replace'), flush=True)
print(json.dumps({name: value for name, value in record.items() if name != 'preserved_inputs'}), flush=True)
raise SystemExit(result.returncode)
