# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import hashlib,importlib.util,json,zipfile
base=Path('artifacts/v190')
spec=importlib.util.spec_from_file_location('builder',Path('scripts/build-service-runtime.py'))
b=importlib.util.module_from_spec(spec);spec.loader.exec_module(b)
source=b.source_identity()
for old,new in [('p6d-claim-native-inputs.json','p6e-native-inputs.json'),('p6d-claim-inputs.json','p6e-smoke-inputs.json')]:
 previous=json.loads((base/old).read_text())
 files={name:hashlib.sha256(Path(name).read_bytes()).hexdigest() for name in previous['files']}
 (base/new).write_text(json.dumps({'native_source':source,'files':files},indent=2)+'\n')
old=json.loads((base/'p6d-claim-native-inputs.json').read_text())['files'];new=json.loads((base/'p6e-native-inputs.json').read_text())['files']
changed=[name for name in new if old[name]!=new[name]]
assert set(changed)=={'crates/airk-cli/tests/mcp.rs','crates/airk-mcp/src/execution.rs','crates/airk-mcp/src/lib.rs','crates/airk-mcp/src/remote.rs'},changed
with zipfile.ZipFile(base/'p6e-native-delta.zip','w',zipfile.ZIP_DEFLATED) as archive:
 for name in changed:archive.write(name,name)
print(json.dumps({'source':source,'changed':changed}))
