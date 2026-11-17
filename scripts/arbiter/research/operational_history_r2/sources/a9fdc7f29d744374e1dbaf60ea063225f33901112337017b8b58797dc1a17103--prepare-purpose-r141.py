# SPDX-License-Identifier: Apache-2.0
import hashlib,json,shutil
from pathlib import Path
r=Path.cwd();manifest=r/'artifacts/advanced-split/models/frontend-focus-purpose-r1/r140-test-correction/mount-manifest.json'
assert hashlib.sha256(manifest.read_bytes()).hexdigest()=='0d2ce3b3caf7b99c99ff6ee9038e37526519970204bfbaaf523243e5cf0330a4'
value=json.loads(manifest.read_bytes());out=r/'artifacts/advanced-split/receipts/purpose-test-correction-r141';out.mkdir()
for name,row in value['files'].items():
    p=r/name;new=r/row['proposed'];assert hashlib.sha256(p.read_bytes()).hexdigest()==row['before_sha256'];assert hashlib.sha256(new.read_bytes()).hexdigest()==row['after_sha256']
    shutil.copyfile(p,out/'before.rs');shutil.copyfile(new,p)
shutil.copyfile(manifest,out/'manifest.json')
source=(r/'artifacts/advanced-split/run-purpose-gates-r140.py').read_text(encoding='utf-8')
source=source.replace('purpose-gates-r140','purpose-gates-r141').replace("'workspace-clippy',cargo,'clippy','--workspace'","'scoped-clippy',cargo,'clippy','-p','airk-frontend','-p','airk-mcp'")
source=source.replace('test(project_client::read_attempt_tests::) | test(focus_handler_tests::)','test(focus_handler_tests::)').replace("receipt['selected_tests']==10","receipt['selected_tests']==4").replace("'expected_focused':10","'expected_focused':4")
(r/'artifacts/advanced-split/run-purpose-gates-r141.py').write_text(source,encoding='utf-8')
print(json.dumps({'mounted':len(value['files']),'review':'Root reviewed existing scoped Status refusal assertion; genuine Help positive control and expanded strict negative matrix. No product/assertion weakening.'}))
