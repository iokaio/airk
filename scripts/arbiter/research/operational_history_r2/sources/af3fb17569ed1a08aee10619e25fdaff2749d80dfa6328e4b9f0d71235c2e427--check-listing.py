# SPDX-License-Identifier: Apache-2.0
import hashlib, json, zipfile
from pathlib import Path
base=Path(__file__).resolve().parent
root=Path.cwd()
with zipfile.ZipFile(base/'extension/publish/airk-1.8.7-win32-x64.vsix') as z:
    manifest=json.loads(z.read('extension/package.json'))
    source=json.loads((root/'packages/vscode/package.json').read_text(encoding='utf-8'))
    assert manifest['version']=='1.8.7' and manifest['description']==source['description']
    listing=z.read(next(n for n in z.namelist() if n.lower()=='extension/readme.md')).decode('utf-8')
    assert 'workbench-1.8.7-results.png' in listing and 'Complete response' in listing
    name=next(n for n in z.namelist() if n.endswith('/workbench-1.8.7-results.png'))
    assert z.read(name)==(root/'docs/workbench/images/workbench-1.8.7-results.png').read_bytes()
    assert not any('/temp/' in n or '/artifacts/' in n or '/.airk/' in n for n in z.namelist())
result={'version':manifest['version'],'description':manifest['description'],'image':name,'image_sha256':hashlib.sha256((root/'docs/workbench/images/workbench-1.8.7-results.png').read_bytes()).hexdigest(),'passed':True}
(base/'listing-verification.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8')
print(json.dumps(result))
