# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import hashlib, importlib.util,json,zipfile
base=Path('artifacts/v190')
spec=importlib.util.spec_from_file_location('runtime_builder',Path('scripts/build-service-runtime.py'))
b=importlib.util.module_from_spec(spec);spec.loader.exec_module(b)
source=b.source_identity()
for old,new in [('p6d-native-inputs.json','p6d-claim-native-inputs.json'),('p6d-fairness-interval-inputs.json','p6d-claim-inputs.json')]:
 previous=json.loads((base/old).read_text())
 files={name:hashlib.sha256(Path(name).read_bytes()).hexdigest() for name in previous['files']}
 (base/new).write_text(json.dumps({'native_source':source,'files':files},indent=2)+'\n')
old=json.loads((base/'p6d-native-inputs.json').read_text())['files']
new=json.loads((base/'p6d-claim-native-inputs.json').read_text())['files']
changed=[name for name in new if old[name]!=new[name]]
assert changed==['crates/airk-store/src/service_background.rs','crates/airk-store/src/service_jobs.rs','crates/airk-store/src/service_watch.rs'],changed
with zipfile.ZipFile(base/'p6d-claim-delta.zip','w',zipfile.ZIP_DEFLATED) as archive:
 for name in changed:archive.write(name,name)
print(json.dumps({'source':source,'changed':changed}))
