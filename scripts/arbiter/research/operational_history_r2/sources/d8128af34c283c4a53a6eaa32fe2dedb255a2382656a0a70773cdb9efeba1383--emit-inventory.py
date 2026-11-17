# SPDX-License-Identifier: Apache-2.0
import hashlib,importlib.util,json,pathlib,shutil,subprocess,time
root=pathlib.Path.cwd().resolve()
out=root/'artifacts/v190/final-plan/ui-admin-inventory-r15'
snapshot=root/'artifacts/v190/final-plan/ui-build-snapshot-r1/source'
def digest(path):return hashlib.sha256(path.read_bytes()).hexdigest()
def load(path):return json.loads(path.read_text(encoding='utf-8'))
def module(path,name):
    spec=importlib.util.spec_from_file_location(name,path);result=importlib.util.module_from_spec(spec);spec.loader.exec_module(result);return result
start=time.monotonic()
owner=module(root/'scripts/build-admin-assets.py','airk_admin_inventory_owner')
node=owner.capture(['node','--version']);npm=owner.capture(['npm','--version'])
base=root/'artifacts/v190/final-plan/ui-build-snapshot-r1/receipts'
inputs=load(base/'behavior-inputs-r11.json')
rows=[]
paths={row['path'] for row in inputs['files'] if row['path'].startswith('packages/')}
paths.update(['fixtures/split/i3/admin_operations.json','scripts/build-admin-assets.py','scripts/split_receipts.py'])
for name in sorted(paths):
    original=root/name;copy=snapshot/name
    assert digest(original)==digest(copy),f'Live/snapshot frontend source differs: {name}'
    rows.append({'path':name,'sha256':digest(original),'bytes':original.stat().st_size})
lock='packages/admin/package-lock.json'
assert digest(root/lock)==digest(snapshot/lock)
assets=snapshot/'crates/airk-cli/admin-assets'
actual=owner.inventory_document(node,npm)
tested=owner.inventory_document(node,npm,assets)
assert actual==tested,'Root asset bytes/inventory differ from tested snapshot'
assert owner.digests()==owner.digests(assets)
block=owner.block_from_inventory(actual)
assert block['locks'][lock]==digest(snapshot/lock)
for receipt in ['admin-build-results-r3.json','final-generation-results-r8.json']:
    result=load(base/receipt);assert all(row['exit_code']==0 for row in result)
manifest=load(base/'final-artifact-handoff-r14.json')
assert sorted(actual['files'],key=lambda row:row['name'])==sorted([{'name':row['path'],'sha256':row['sha256'],'bytes':row['bytes'],'content_type':next(entry['content_type'] for entry in actual['files'] if entry['name']==row['path'])} for row in manifest['admin_assets'] if row['path']!='manifest.json'],key=lambda row:row['name'])
node_path=pathlib.Path(shutil.which('node'))
npm_path=pathlib.Path(shutil.which('npm'))
receipt={'schema':'airk.ui-admin-inventory-handoff/1','account':'TYAI\\tdotn','sid':'S-1-5-21-3678370870-3858642417-826968105-1002','operation':'existing build-admin-assets.inventory_document; block_from_inventory exact current root rebind','toolchain':{'node':node,'npm':npm,'node_path':str(node_path),'node_sha256':digest(node_path),'npm_path':str(npm_path),'npm_launcher_sha256':digest(npm_path)},'admin_build_receipts':['ui-build-snapshot-r1/receipts/admin-build-results-r3.json','ui-build-snapshot-r1/receipts/final-generation-results-r8.json'],'source_input_manifest_sha256':digest(base/'behavior-inputs-r11.json'),'root_snapshot_frontend_inputs':rows,'source_lock':{lock:digest(root/lock)},'root_snapshot_assets_equal':True,'root_snapshot_inventory_equal':True,'build_block':block,'elapsed_seconds':time.monotonic()-start,'limitations':['Reuses the unchanged passed noEmit Admin typecheck and passed exact-byte asset check; neither compilation nor npm ci repeated','Inventory authenticates sealed UI assets and source-lock identity; no live native/model or installed product qualification claim']}
for name,value in [('admin-assets.json',actual),('receipt.json',receipt)]:
    with (out/name).open('x',encoding='utf-8',newline='\n') as stream:stream.write(json.dumps(value,indent=2,ensure_ascii=True)+'\n')
print(json.dumps({'inventory':str(out/'admin-assets.json'),'files':len(actual['files']),'toolchain':actual['toolchain'],'digests':owner.digests(),'block':block,'input_count':len(rows),'elapsed_seconds':receipt['elapsed_seconds']},ensure_ascii=True))