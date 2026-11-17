# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import hashlib,json,tempfile
root=Path.cwd();a=root/'artifacts/optimizations';task=Path(tempfile.mkdtemp(prefix='airk-optimizations-f11a-'))
for name in ['scripts','fixtures','reports','target']:(task/name).mkdir()
inputs=[(root/'scripts'/name,name) for name in ['check-service-clients.py','check-service-pool.py','check-service-install.py','check-client-lifetime.py','check-managed-runtime.py']]
inputs.append((a/'f10a-source-scope-current-pool.py','check-source-scope-current-pool.py'))
rows=[]
for source,name in inputs:
 data=source.read_bytes();target=task/'scripts'/name;target.write_bytes(data);digest=hashlib.sha256(data).hexdigest();assert hashlib.sha256(target.read_bytes()).hexdigest()==digest;rows.append({'source':str(source),'name':name,'sha256':digest})
assert rows[1]['sha256']=='e4e9034b0d0b5c42ccceaee7e175e2d665f704ccf853a142ac64ab99633a08f5'
assert rows[-1]['sha256']=='b0f5d8c617204dcce751da09f2537d6b4bbe95e956ffca0b8cf19f74a8f8ec57'
driver=a/'f11-fresh-pair-coverage/qualify-approved-repair.py';base=driver.with_name('BASE.json');assert hashlib.sha256(driver.read_bytes()).hexdigest()=='2943e05a16fef4ff21ea53cac6daef772c599c7dbc0d6918172f42290cc8db58'
record={'task':str(task),'runtime':str(task/'runtime'),'scripts':rows,'approved_repair_driver':{'path':str(driver),'sha256':hashlib.sha256(driver.read_bytes()).hexdigest(),'base_path':str(base),'base_sha256':hashlib.sha256(base.read_bytes()).hexdigest(),'run_from_repo_artifact':True}}
with (a/'f11a-runtime-location.json').open('x') as f:json.dump(record,f,indent=2)
print(json.dumps(record,indent=2))
