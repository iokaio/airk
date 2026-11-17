# SPDX-License-Identifier: Apache-2.0
import hashlib,importlib.util,json,sys
from pathlib import Path
run=Path(sys.argv[1]);root=run/'source';record=json.loads((run/'snapshot.json').read_bytes())
changed=[name for name,digest in record['files'].items() if not (root/name).is_file() or hashlib.sha256((root/name).read_bytes()).hexdigest()!=digest]
assert not changed,changed
spec=importlib.util.spec_from_file_location('native_builder',root/'scripts/build-service-runtime.py');builder=importlib.util.module_from_spec(spec);spec.loader.exec_module(builder)
assert builder.source_identity()==record['native_source'],(builder.source_identity(),record['native_source'])
print(json.dumps({'verified':True,'files':len(record['files']),'native_source':record['native_source']}))
