# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import hashlib,json,re,shutil
root=Path.cwd();a=root/'artifacts/optimizations';location=json.loads((a/'f12a-runtime-location.json').read_text())
source=root/'packages/vscode/out';destination=Path(location['task'])/'packages/vscode/out'
def inventory(directory):
    rows={}
    for path in directory.rglob('*'):
        metadata=path.lstat()
        assert not path.is_symlink() and not getattr(metadata,'st_file_attributes',0)&0x400
        assert path.is_dir() or path.is_file()
        if path.is_file():rows[path.relative_to(directory).as_posix()]={'sha256':hashlib.sha256(path.read_bytes()).hexdigest(),'bytes':metadata.st_size}
    return rows
before=inventory(source);assert before and not destination.exists()
destination.parent.mkdir(parents=True,exist_ok=True);shutil.copytree(source,destination)
assert inventory(source)==before and inventory(destination)==before
pending=[destination/'serviceBootstrap.js',destination/'serviceConsoleController.js'];seen=set();bare=set()
while pending:
    file=pending.pop()
    if file in seen:continue
    seen.add(file)
    for name in re.findall(r'require\([\'"]([^\'"]+)[\'"]\)',file.read_text(encoding='utf-8')):
        if name.startswith('.'):
            target=(file.parent/name).with_suffix('.js')
            assert target.is_file(),str(target)
            pending.append(target)
        else:bare.add(name)
result={'source':str(source),'destination':str(destination),'files':len(before),'bytes':sum(row['bytes'] for row in before.values()),
        'exact_hash_size_file_set_verified':True,'no_reparse':True,'reachable_js_modules':len(seen),'bare_requires':sorted(bare),'files_inventory':before}
with (a/'f12a-console-client-out-copy.json').open('x',encoding='utf-8') as stream:json.dump(result,stream,indent=2)
print(json.dumps({key:value for key,value in result.items() if key!='files_inventory'},indent=2))
