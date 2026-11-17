# SPDX-License-Identifier: Apache-2.0
import hashlib, importlib.util, json, re, subprocess, sys, tarfile
from pathlib import Path
label = sys.argv[1]
assert re.fullmatch(r'[a-z0-9-]+', label)
root = Path.cwd()
base = root / 'artifacts/v190'
spec = importlib.util.spec_from_file_location('builder', root / 'scripts/build-service-runtime.py')
builder = importlib.util.module_from_spec(spec)
spec.loader.exec_module(builder)
before = builder.source_identity()
paths = subprocess.check_output(['git', '-c', 'safe.directory=D:/code/Github/airk', 'ls-files', '-z', '--cached', '--others', '--exclude-standard']).split(b'\0')
excluded = {'.git', '.codex', '.agents', '.aws', '.mcp.json', 'temp', 'target', 'artifacts'}
files = sorted({p.decode() for p in paths if p and p.decode().split('/')[0] not in excluded})
manifest = {}
archive = base / (label + '.tar.gz')
with tarfile.open(archive, 'w:gz') as tar:
    for name in files:
        path = root / name
        assert path.is_file() and not path.is_symlink() and path.resolve().is_relative_to(root), name
        manifest[name] = hashlib.sha256(path.read_bytes()).hexdigest()
        tar.add(path, arcname=name, recursive=False)
assert builder.source_identity() == before
assert all(hashlib.sha256((root / n).read_bytes()).hexdigest() == h for n, h in manifest.items())
record = {'entry_commit': subprocess.check_output(['git', '-c', 'safe.directory=D:/code/Github/airk', 'rev-parse', 'HEAD']).decode().strip(), 'native_source': before, 'files': manifest, 'archive_sha256': hashlib.sha256(archive.read_bytes()).hexdigest()}
(base / (label + '-snapshot.json')).write_text(json.dumps(record, indent=2) + '\n')
print(label, len(files), before, record['archive_sha256'])
