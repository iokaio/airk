# SPDX-License-Identifier: Apache-2.0
import hashlib
import importlib.util
import json
from pathlib import Path
import sys

base=Path('D:/code/Github/airk/artifacts/v190')
work=base/'p10-worktree'
proposal=base/'p14-usage-editor-pick-proposed-r1'
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
assert len(sys.argv)==3,'Root supplies independently reviewed receipt path and exact SHA256'
review=Path(sys.argv[1]).resolve(strict=True)
assert review.is_relative_to(base.resolve())
assert sha(review)==sys.argv[2]
review_value=json.loads(review.read_text(encoding='utf-8'))
manifest_path=proposal/'manifest.json'
assert sha(manifest_path)=='5aaecc17fe9b1fd0d2469687103e5b5f24cb068fd775f3f3438b0debf6898cf2'
manifest=json.loads(manifest_path.read_text())
assert sha(proposal/'editor-usage.patch')==manifest['patch_sha256']=='4df9b6d4dfaf98a232e1edf9d64be419f4c5cae14c931980f84a4bed4af4049f'
expected={'scripts/editor-ui.cjs','scripts/tests/editor-ui.test.cjs','packages/vscode/src/test/installedEditor.ts'}
assert {row['path'] for row in manifest['files']}==expected
for row in manifest['files']:
    assert sha(work/row['path'])==row['base_sha256'],row['path']
    assert sha(proposal/row['path'])==row['proposed_sha256'],row['path']
spec=importlib.util.spec_from_file_location('builder',work/'scripts/build-service-runtime.py')
builder=importlib.util.module_from_spec(spec);spec.loader.exec_module(builder)
native=builder.source_identity()
assert native=={'sha256':'5d8912b117b410306ac9a3b44af6d96d70bb018006c2d49a1efa365597fe3e11','files':960}
vsix=work/'artifacts/p14-package-editor-r2/extension/publish/airk-1.8.8-win32-x64.vsix'
assert sha(vsix)=='49170b51765048e438dfd9516a35e4884d7667efe5ad2652d02303caf22f4c90'
def product_inventory():
    result={}
    for folder in ['packages/vscode/src','packages/vscode/out']:
        root=work/folder
        for p in root.rglob('*'):
            if p.is_file() and 'test' not in p.relative_to(root).parts:
                result[p.relative_to(work).as_posix()]=sha(p)
    return result
before=product_inventory()
receipt_path=base/'p14-usage-editor-pick-live-adoption-r1.json'
receipt={'status':'APPLY_STARTED','manifest_sha256':sha(manifest_path),'patch_sha256':manifest['patch_sha256'],'independent_review':{'path':str(review),'sha256':sha(review)},'files':manifest['files'],'native_source':native,'production_inventory':before,'vsix_sha256':sha(vsix)}
with receipt_path.open('x',encoding='utf-8') as stream:json.dump(receipt,stream,indent=2)
for row in manifest['files']:(work/row['path']).write_bytes((proposal/row['path']).read_bytes())
assert all(sha(work/row['path'])==row['proposed_sha256'] for row in manifest['files'])
assert builder.source_identity()==native
assert product_inventory()==before
assert sha(vsix)==receipt['vsix_sha256']
receipt.update(status='REVIEWED_EXTERNAL_HARNESS_DELTA_APPLIED_VALIDATION_PENDING',native_product_vsix_unchanged=True,old_failed_queue='preserved failed, no mutation replay; first3 exact existing pass adoption separate')
receipt_path.write_text(json.dumps(receipt,indent=2)+'\n',encoding='utf-8')
print(json.dumps({'receipt':str(receipt_path),'sha256':sha(receipt_path),'status':receipt['status']}))
