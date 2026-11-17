# SPDX-License-Identifier: Apache-2.0
import hashlib,importlib.util,json
from pathlib import Path
root=Path('/Users/tylerjensen/airk-builds/p6a-20261103.jBR4Jh/source-r2')
expected=json.loads((root.parent/'p6a-final-native-inputs.json').read_text())
for name,digest in expected['files'].items():
 assert hashlib.sha256((root/name).read_bytes()).hexdigest()==digest,name
spec=importlib.util.spec_from_file_location('builder',root/'scripts/build-service-runtime.py')
b=importlib.util.module_from_spec(spec);spec.loader.exec_module(b)
assert b.source_identity()==expected['native_source']
print(json.dumps(expected['native_source']))
