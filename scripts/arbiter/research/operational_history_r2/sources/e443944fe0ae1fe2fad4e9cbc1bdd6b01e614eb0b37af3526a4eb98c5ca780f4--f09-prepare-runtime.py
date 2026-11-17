# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import hashlib,json,shutil,tempfile
root=Path.cwd();a=root/'artifacts/optimizations'
task=Path(tempfile.mkdtemp(prefix='airk-optimizations-f09-'))
assert not task.resolve().is_relative_to(root.resolve())
assert not task.resolve().is_relative_to(Path('D:/code/Github/bookbuilder').resolve())
for name in ['scripts','reports','fixtures','target']:(task/name).mkdir()
copies=[]
for name in ['check-service-clients.py','check-service-pool.py','check-service-install.py','check-client-lifetime.py','check-managed-runtime.py','check-service-console.py','check-service-console-client.cjs']:
    source=root/'scripts'/name;destination=task/'scripts'/name
    shutil.copyfile(source,destination)
    digest=hashlib.sha256(source.read_bytes()).hexdigest()
    assert hashlib.sha256(destination.read_bytes()).hexdigest()==digest
    copies.append({'name':name,'sha256':digest})
source=a/'f09-source-scope-port.py';digest=hashlib.sha256(source.read_bytes()).hexdigest()
assert digest=='9c74cfe2fc291cd2c85146324b7324aea5a43c20244ae99cee09190dd03e4999'
destination=task/'scripts'/source.name;shutil.copyfile(source,destination)
assert hashlib.sha256(destination.read_bytes()).hexdigest()==digest
copies.append({'name':source.name,'sha256':digest})
location={'task':str(task),'runtime':str(task/'runtime'),'scripts':copies,'native_execution':'not approved or executed; fresh build only'}
with (a/'f09-runtime-location.json').open('x',encoding='utf-8') as stream:json.dump(location,stream,indent=2)
print(json.dumps(location,indent=2))
