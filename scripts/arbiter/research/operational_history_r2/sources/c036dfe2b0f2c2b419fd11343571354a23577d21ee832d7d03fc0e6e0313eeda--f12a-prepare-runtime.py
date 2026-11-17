# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import hashlib,json,tempfile
root=Path.cwd();a=root/'artifacts/optimizations'
task=Path(tempfile.mkdtemp(prefix='airk-optimizations-f12a-'))
for name in ['scripts','fixtures','reports','target']:(task/name).mkdir()
old=json.loads((a/'f06-authority-runtime-location.json').read_text())
inputs=[root/'scripts'/name for name in ['check-service-clients.py','check-service-pool.py','check-service-install.py','check-client-lifetime.py','check-managed-runtime.py','check-service-console.py','check-service-console-client.cjs']]
inputs += [Path(old['task'])/'scripts/check-source-scope-original-identity.py',a/'f12a-draft/semantic-restart-qualification.py']
items=[]
for source in inputs:
 destination=task/'scripts'/source.name
 data=source.read_bytes();destination.write_bytes(data)
 sha=hashlib.sha256(data).hexdigest();assert hashlib.sha256(destination.read_bytes()).hexdigest()==sha
 items.append({'name':destination.name,'sha256':sha,'source':str(source)})
assert items[-1]['sha256']=='9d638350655ea86819eaf322259608b3fec00645767a17f9c2e40c9016231638'
assert items[-2]['sha256']=='02d6c9573972cb1b21aa67813c29d1effde48bd7237c6374b8b36e7a542e82ae'
record={'task':str(task),'runtime':str(task/'runtime'),'scripts':items}
with (a/'f12a-runtime-location.json').open('x') as f:json.dump(record,f,indent=2)
print(json.dumps(record,indent=2))
