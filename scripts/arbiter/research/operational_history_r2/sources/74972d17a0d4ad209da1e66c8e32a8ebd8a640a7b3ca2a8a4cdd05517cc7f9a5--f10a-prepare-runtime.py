# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import hashlib,json,tempfile
root=Path.cwd();a=root/'artifacts/optimizations';task=Path(tempfile.mkdtemp(prefix='airk-optimizations-f10a-'))
for name in ['scripts','fixtures','reports','target']:(task/name).mkdir()
original=json.loads((a/'f06-authority-runtime-location.json').read_text())
inputs=[root/'scripts'/name for name in ['check-service-clients.py','check-service-pool.py','check-service-install.py','check-client-lifetime.py','check-managed-runtime.py']]
inputs += [Path(original['task'])/'scripts/check-source-scope-original-identity.py']
rows=[]
for source in inputs:
 data=source.read_bytes();target=task/'scripts'/source.name;target.write_bytes(data);sha=hashlib.sha256(data).hexdigest();assert hashlib.sha256(target.read_bytes()).hexdigest()==sha;rows.append({'source':str(source),'name':target.name,'sha256':sha})
assert rows[1]['sha256']=='e4e9034b0d0b5c42ccceaee7e175e2d665f704ccf853a142ac64ab99633a08f5'
assert rows[-1]['sha256']=='02d6c9573972cb1b21aa67813c29d1effde48bd7237c6374b8b36e7a542e82ae'
record={'task':str(task),'runtime':str(task/'runtime'),'scripts':rows}
with (a/'f10a-runtime-location.json').open('x') as f:json.dump(record,f,indent=2)
print(json.dumps(record,indent=2))
