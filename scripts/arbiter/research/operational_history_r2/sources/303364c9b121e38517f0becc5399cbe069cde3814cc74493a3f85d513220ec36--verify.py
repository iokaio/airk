# SPDX-License-Identifier: Apache-2.0
import hashlib,importlib.util,json,subprocess,sys
from pathlib import Path
run=Path(sys.argv[1]);root=run/'source';record=json.loads((run/'snapshot.json').read_bytes());bad=[]
for name,digest in json.loads((run/'harness.json').read_bytes()).items():assert hashlib.sha256((run/name).read_bytes()).hexdigest()==digest,('qualification harness changed',name)
for name,entry in record['public_files'].items():
 path=root/name
 if not path.is_file() or path.is_symlink() or hashlib.sha256(path.read_bytes()).hexdigest()!=entry['sha256']:bad.append(name)
assert not bad,bad
assert subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip()==record['commit']
assert subprocess.check_output(['git','rev-parse','HEAD^{tree}'],cwd=root,text=True).strip()==record['tree']
assert not subprocess.check_output(['git','status','--porcelain','--untracked-files=no'],cwd=root).strip()
spec=importlib.util.spec_from_file_location('builder',root/'scripts/build-service-runtime.py');builder=importlib.util.module_from_spec(spec);spec.loader.exec_module(builder);assert builder.source_identity()==record['native_source']
print(json.dumps({'verified':True,'commit':record['commit'],'public_files':len(record['public_files']),'native_source':record['native_source']}))
