# SPDX-License-Identifier: Apache-2.0
import hashlib,json,subprocess,tomllib
from pathlib import Path
root=Path.cwd()
git=['git','-c','safe.directory='+root.as_posix()]
assert subprocess.check_output([*git,'branch','--show-current'],text=True).strip()=='feat/advanced-split'
assert (root/'AGENTS.md').read_bytes()==(root/'CLAUDE.md').read_bytes()
version=tomllib.loads((root/'Cargo.toml').read_text(encoding='utf-8'))['workspace']['package']['version']
assert version=='1.9.0'==json.loads((root/'packages/vscode/package.json').read_bytes())['version']
raw=subprocess.check_output([*git,'status','--porcelain=v1','-z','--untracked-files=all'])
rows=[]
for entry in raw.split(b'\0'):
    if not entry:continue
    status=entry[:2].decode();name=entry[3:].decode('utf-8')
    assert status in (' M',' D','??'),(status,name)
    assert name in {'.dockerignore','AGENTS.md','CLAUDE.md','CONTRIBUTING.md','Dockerfile','README.md'} or name.startswith(('crates/','docs/','fixtures/','packages/','scripts/'))
    assert not any(part in {'artifacts','temp','.airk','node_modules','target','.git'} for part in Path(name).parts)
    p=root/name
    assert not p.is_symlink()
    if p.exists():assert p.is_file() and p.stat().st_size<8*1024*1024
    rows.append({'status':status,'path':name,'sha256':hashlib.sha256(p.read_bytes()).hexdigest() if p.exists() else None})
assert rows
out=root/'artifacts/advanced-split/receipts/stage-milestone-r124';out.mkdir(exist_ok=False)
(out/'roster.json').write_text(json.dumps({'schema':'airk.private-reviewed-stage-milestone-roster/1','branch':'feat/advanced-split','version':version,'files':rows,'scope':'Reviewed advanced-split stage/contracts/model/compiler/runtime/UI/help/tooling implementation and its explicit unfinished qualification ledger. No private artifacts, source corpora, live state or model weights.'},indent=2)+'\n',encoding='utf-8')
(out/'paths.nul').write_bytes(b'\0'.join(row['path'].encode() for row in rows)+b'\0')
print(json.dumps({'count':len(rows),'roster':str(out/'roster.json'),'nul_paths':str(out/'paths.nul')}))
