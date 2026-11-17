# SPDX-License-Identifier: Apache-2.0
"""Freeze continuation metadata only. No native/npm/package/editor execution."""
from pathlib import Path
import ast,hashlib,json
A=Path(__file__).parent;W=A/'p10-worktree';S=A.parent/'tools/vscode-1.145.0-a717-r1'
h=lambda p:hashlib.sha256(Path(p).read_bytes()).hexdigest()
read=lambda p:json.loads(Path(p).read_text(encoding='utf-8'))
owner=A/'p14-package-editor-continuation-r1.py';ast.parse(owner.read_text(encoding='utf-8'))
old_prepared=read(A/'p14-package-editor-owner-r3-prepared.json');old=read(A/'p14-package-editor-owner-r2.json')
assert old['status']=='failed' and len(old['steps'])==4 and old['steps'][3]['exit_code']==1
assert all(row['exit_code']==0 for row in old['steps'][:3])
pins=dict(old_prepared['pins']);assert len(pins)==11
extra=[A/'p14-package-editor-owner-r3.py',A/'p14-package-editor-owner-r3-prepared.json',A/'p14-package-editor-owner-r3-inputs.json',
 A/'p14-package-editor-owner-r2.json',A/'p14-release-runtime-r2.receipt.json',A/'p14-release-runtime-r2.log',A/'p14-runtime-final-r2-seal.json',
 S/'receipt.json',S/'stock-file-seal.json',S/'official-metadata.json',S/'official.zip',
 W/'artifacts/p14-package-editor-r2/native/publish/manifest.json',W/'artifacts/p14-package-editor-r2/extension/publish/manifest.json',Path(old['vsix']['path'])]
native=read(extra[-3]);extra.append(W/'artifacts/p14-package-editor-r2/native/publish'/native['archive'])
for row in old['steps'][:3]:
 receipt=Path(row['receipt']);assert h(receipt)==row['receipt_sha256']
 extra.extend([receipt,receipt.with_name(receipt.name.replace('.receipt.json','.log'))])
for p in extra:pins[str(p.resolve())]=h(p)
assert all(h(Path(p))==expected for p,expected in pins.items())
assert not(A/'p14-package-editor-continuation-r1.json').exists()
assert not(W/'artifacts/p14-legacy-upgrade-editor-owner-r2').exists()
assert all(not(A/('p14-package-continuation-'+name+'-r1.log')).exists() for name in ['five-private-scenarios','legacy-fixture','legacy-private-scenario'])
stock=read(S/'receipt.json');seal=read(S/'stock-file-seal.json')
assert stock['files_verified_against_official_archive']==len(seal)==3280
assert stock['stock_file_seal_sha256']==h(S/'stock-file-seal.json')
assert stock['archive_sha256']==h(S/'official.zip')=='79cf4cc5b17357cc64abdd22a2d7d4884d05f07b3f0defbf5fc0d67fe03d9dc4'
prepared={'status':'STATIC_READY_NOT_EXECUTED','owner_sha256':h(owner),'pins':pins,
 'original11pins_unchanged':old_prepared['pins']=={p:pins[p] for p in old_prepared['pins']},
 'command':['python','-X','utf8',str(owner.resolve())],
 'adopted_passed_stages':old['steps'][:3],
 'original_failed_owner_kept':{'path':str((A/'p14-package-editor-owner-r2.json').resolve()),'sha256':h(A/'p14-package-editor-owner-r2.json'),'status':'failed'},
 'actual_stock_editor':{'receipt':str((S/'receipt.json').resolve()),'sha256':h(S/'receipt.json'),'files':len(seal),'editor':stock['editor'],'cli':stock['cli'],'commit':stock['commit'],'version':stock['version']},
 'new_steps':['canonical five same/different/multi/usage/untrusted scenarios','fresh existing legacyOwnerR2','canonical sixth legacy scenario, same VSIX/native digest'],
 'vsix_sha256':old['vsix']['sha256'],'source_expected':old['native_source'],
 'limits':['No repeat native packaging/compile/VSIX stages','Existing three passes adopted only; original updater-blocked editor failure is never relabeled','OfficialZIP stock files/rootdata absence/producttarget absence checked before/after every remaining stage','No normal Code/updater/install action; only canonical private editor profiles','No actual editor/native/npm/package execution by preparation']}
out=A/'p14-package-editor-continuation-r1-prepared.json'
with out.open('x',encoding='utf-8') as f:json.dump(prepared,f,indent=2);f.write('\n')
print(json.dumps({'owner':h(owner),'prepared':h(out),'pins':len(pins),'stock_receipt':h(S/'receipt.json'),'adopted_passes':3,'stages_executed':0}))
