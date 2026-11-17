# SPDX-License-Identifier: Apache-2.0
import hashlib
import importlib.util
import json
from pathlib import Path
import zipfile

root = Path.cwd()
spec = importlib.util.spec_from_file_location('builder', root / 'scripts/build-service-runtime.py')
builder = importlib.util.module_from_spec(spec)
spec.loader.exec_module(builder)
prior = json.loads((root / 'artifacts/v190/p6c-native-inputs.json').read_text(encoding='utf-8'))
current = {'native_source': builder.source_identity(), 'files': {}}
for name in sorted(prior['files']):
    current['files'][name] = hashlib.sha256((root / name).read_bytes()).hexdigest()
assert current['native_source']['files'] == prior['native_source']['files'] == 789
changes = [name for name, digest in current['files'].items() if prior['files'].get(name) != digest]
(root / 'artifacts/v190/p6d-native-inputs.json').write_text(json.dumps(current, indent=2) + '\n', encoding='utf-8')
with zipfile.ZipFile(root / 'artifacts/v190/p6d-native-delta.zip', 'w', zipfile.ZIP_DEFLATED) as archive:
    for name in changes:
        archive.write(root / name, name)
print(json.dumps({'native_source': current['native_source'], 'declared_inputs': len(current['files']), 'changed_paths': changes}))
