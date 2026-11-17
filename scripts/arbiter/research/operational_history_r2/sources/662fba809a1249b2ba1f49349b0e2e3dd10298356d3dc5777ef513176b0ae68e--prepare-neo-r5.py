# SPDX-License-Identifier: Apache-2.0
import hashlib
import importlib.util
import json
from pathlib import Path
import tarfile

root = Path('/Users/tylerjensen/airk-builds/p6a-20261103.jBR4Jh')
source = root / 'source-r2'
record = json.loads((root / 'p6a-source-r5-snapshot.json').read_text())
archive = root / 'p6a-source-r5.tar.gz'
assert hashlib.sha256(archive.read_bytes()).hexdigest() == record['archive_sha256']
with tarfile.open(archive) as bundle:
    members = bundle.getmembers()
    assert {m.name for m in members} == set(record['files'])
    for member in members:
        assert member.isfile() and not Path(member.name).is_absolute()
        assert (source / member.name).resolve().is_relative_to(source)
    bundle.extractall(source, filter='data')
for name, digest in record['files'].items():
    assert hashlib.sha256((source / name).read_bytes()).hexdigest() == digest, name
# The transferred Windows timestamps can precede cached Cargo artifacts.
# Refresh only CLI compilation inputs, retaining every source byte.
inputs = list((source / 'crates/airk-cli').rglob('*.rs'))
for path in inputs:
    path.touch()
spec = importlib.util.spec_from_file_location('builder', source / 'scripts/build-service-runtime.py')
builder = importlib.util.module_from_spec(spec)
spec.loader.exec_module(builder)
assert builder.source_identity() == record['native_source']
print(json.dumps({'verified_source': record['native_source'], 'cli_inputs_refreshed': len(inputs)}))
