# SPDX-License-Identifier: Apache-2.0
import hashlib,importlib.util,json,subprocess
from pathlib import Path
base=Path('D:/code/Github/airk/artifacts/v190');work=base/'p10-worktree';proposal=base/'p11b-composed-p11a-r3'
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest() if p.exists() else None
manifest=json.loads((proposal/'composed-manifest.json').read_text())
supplementary=json.loads((proposal/'preserved-supplementary-manifest.json').read_text())
deferred=json.loads((proposal/'deferred-docs.json').read_text())['not_in_source_patch']
assert sha(proposal/'p11b-source-composed.patch')=='342be02270c969fa36984f6cfade0a67e88bbcf13dab9f858652d190f77ae939'
assert json.loads((base/'p11a-local-acceptance-commit-r1.json').read_text())['phase_accepted']
paths={p:v for p,v in manifest.items() if p not in deferred}
assert len(paths)==91
for p,row in paths.items():
    assert not p.startswith(('artifacts/','temp/','.git/','.airk/'))
    assert sha(work/p)==row['base_sha256'],p
    assert sha(proposal/'tree'/p)==row['proposed_sha256'],p
for p,value in supplementary.items():assert sha(work/p)==value,p
subprocess.run(['git','-c','safe.directory='+work.as_posix(),'-C',str(work),'apply','--check','--ignore-space-change',str(proposal/'p11b-source-composed.patch')],check=True)
# Copy exact reviewed public bytes after validating every live base; this avoids
# silently varying native fingerprints through Git's checkout EOL conversion.
for p in paths:
    destination=work/p;destination.parent.mkdir(parents=True,exist_ok=True)
    destination.write_bytes((proposal/'tree'/p).read_bytes())
for p,row in paths.items():assert sha(work/p)==row['proposed_sha256']
for p,value in supplementary.items():assert sha(work/p)==value,p
spec=importlib.util.spec_from_file_location('builder',work/'scripts/build-service-runtime.py')
builder=importlib.util.module_from_spec(spec);spec.loader.exec_module(builder)
receipt={'status':'91 reviewed source/test files applied; not compiled or accepted','source':builder.source_identity(),
    'patch_sha256':sha(proposal/'p11b-source-composed.patch'),'files':paths,'preserved':supplementary}
output=base/'p11b-applied-r1.json';assert not output.exists();output.write_text(json.dumps(receipt,indent=2)+'\n')
print(json.dumps({'files':len(paths),'source':receipt['source']}))
