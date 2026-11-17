# SPDX-License-Identifier: Apache-2.0
import hashlib,importlib.util,json,sys
from pathlib import Path
root=Path('/Users/tylerjensen/airk-builds/p6a-20261103.jBR4Jh/source-r2')
expected=json.loads((root.parent/'p6d-claim-inputs.json').read_text())
changed=[name for name,digest in expected['files'].items() if not (root/name).is_file() or hashlib.sha256((root/name).read_bytes()).hexdigest()!=digest]
if len(sys.argv)>1 and sys.argv[1]=='compare':
 print(json.dumps({'changed':changed},indent=2));sys.exit()
assert not changed,changed
spec=importlib.util.spec_from_file_location('builder',root/'scripts/build-service-runtime.py'); b=importlib.util.module_from_spec(spec);spec.loader.exec_module(b)
assert b.source_identity()==expected['native_source'],(b.source_identity(),expected['native_source'])
print(json.dumps({'native_source':expected['native_source'],'verified_public_inputs':len(expected['files'])}))
