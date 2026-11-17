# SPDX-License-Identifier: Apache-2.0
import hashlib, importlib.util, json, zipfile
from pathlib import Path

root=Path(__file__).resolve().parents[2]
out=Path(__file__).resolve().parent
sha=lambda data:hashlib.sha256(data).hexdigest()
reuse=json.loads((out/'native-reuse.json').read_text())
runtime=Path(reuse['runtime'])
assert {p.relative_to(runtime).as_posix():sha(p.read_bytes()) for p in runtime.rglob('*') if p.is_file()}==reuse['files']
vsix=out/'extension/publish/airk-1.8.8-win32-x64.vsix'
spec=importlib.util.spec_from_file_location('pack',root/'scripts/package-extension.py')
pack=importlib.util.module_from_spec(spec);spec.loader.exec_module(pack)
with zipfile.ZipFile(vsix) as archive:
    names=archive.namelist()
    assert not any('/out/test/' in n or any(x in Path(n).parts for x in ['.git','.codex','.agents','temp','target']) for n in names)
    assert archive.read('extension/readme.md')==(root/'packages/vscode/README.md').read_bytes()
    actual=json.loads(archive.read('extension/package.json'))
    expected=pack.runtime_package(json.loads((root/'packages/vscode/package.json').read_text(encoding='utf-8')))
    def differences(a,b,p=''):
        if type(a)!=type(b):print(p,repr(a)[:160],repr(b)[:160])
        elif isinstance(a,dict):
            for key in a.keys()|b.keys():
                if key not in a or key not in b:print(p+'/'+key,'missing')
                else:differences(a[key],b[key],p+'/'+key)
        elif isinstance(a,list):
            if len(a)!=len(b):print(p,'length',len(a),len(b))
            for i,(x,y) in enumerate(zip(a,b)):differences(x,y,p+'/'+str(i))
        elif a!=b:print(p,repr(a)[:160],repr(b)[:160])
    differences(actual,expected)
    assert actual==expected
    for name,digest in reuse['files'].items():assert sha(archive.read('extension/native/runtime/'+name))==digest,name
    compiled=[n for n in names if n.startswith('extension/out/') and not n.endswith('/')]
    for name in compiled:assert archive.read(name)==(root/'packages/vscode'/name.removeprefix('extension/')).read_bytes(),name
value={'status':'passed','vsix':str(vsix),'sha256':sha(vsix.read_bytes()),'runtime_files_verified':len(reuse['files']),'compiled_files_verified':len(compiled),'native_reuse':'original compiled identity retained; not a fresh build'}
with (out/'candidate-verification.json').open('x') as stream:json.dump(value,stream,indent=2)
print(json.dumps(value))
