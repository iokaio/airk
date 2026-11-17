# SPDX-License-Identifier: Apache-2.0
import collections, datetime, hashlib, importlib.util, json, pathlib, subprocess
P=pathlib.Path(__file__).resolve().parent
def load(p): return json.loads(p.read_text(encoding='utf-8-sig'))
def sha(p):
    with p.open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
def inv(root):
    rows={}
    for p in root.rglob('*'):
        assert not p.is_symlink(),str(p)
        if p.is_file():rows[p.relative_to(root).as_posix()]={'sha256':sha(p),'bytes':p.stat().st_size}
    return rows
proof=load(P/'phase1-artifact-proof.json'); plan=load(P/'plan.json'); C=pathlib.Path(plan['candidate_root']); R=pathlib.Path(plan['runtime_dir']); run=pathlib.Path(proof['local_run']); extract=P/'exe-data/extracted'; app=extract/'{app}'
actual=inv(app); layout=inv(run/'layout'); assert len(actual)==844 and actual==layout
assert sorted(p.name for p in extract.iterdir())==['install_script.iss','{app}']
assert sha(run/'layout/payload-inventory.json')==proof['payload_inventory_sha256']
legal={}
for name,row in proof['research_companions'].items():
    path='notices/dependencies/'+name
    assert actual[path]['sha256']==row['sha256'] and actual[path]['bytes']==row['bytes']
    legal[path]=actual[path]
executables=sorted(n for n in actual if n.lower().endswith('.exe')); assert executables==['bin/airkctl.exe','service/airk.exe']
assert actual['service/airk.exe']['sha256']==proof['signatures']['airk']['sha256']
assert actual['bin/airkctl.exe']['sha256']==proof['signatures']['airkctl']['sha256']
assert all(n in executables or n=='payload-inventory.json' or n.startswith('notices/') for n in actual)
forbidden={'.onnx','.gguf','.db','.sqlite','.pem','.key','.pfx','.jsonl','.zip','.tar'}
assert not [n for n in actual if pathlib.PurePosixPath(n).suffix.lower() in forbidden]
for name in actual:
    assert not ({'temp','.airk','node_modules','fixtures','credentials','training','src'} & set(pathlib.PurePosixPath(name).parts)),name
pins=load(P/'candidate-input-pins.json')
for key,group in pins.items():
    base=C if key=='sources' else C/'packages/vscode/out'
    for name,digest in group.items():assert sha(base/name)==digest,name
assert subprocess.check_output(['git','status','--porcelain'],cwd=C)==b''
for row in load(P/'preserved-vsix-pins.json').values():assert sha(pathlib.Path(row['path']))==row['sha256']
wrapper=load(P/'driver/windows-installer-r3.json')
for name,digest in wrapper['runtime_seal']['files'].items():assert sha(R/name)==digest,name
for component,row in proof['signatures'].items(): assert sha(pathlib.Path(row['path']))==row['sha256'],component
result={'schema':'airk.private.windows-installer-data-extraction-proof/1','observed_utc':datetime.datetime.now(datetime.UTC).isoformat(),'passed':True,'installer_sha256':proof['signatures']['installer']['sha256'],'extracted_app_files':len(actual),'layout_files':843,'exact_complete_layout_including_payload_inventory':True,'payload_inventory_sha256':proof['payload_inventory_sha256'],'research_legal_companions_exact':legal,'executables':executables,'all_files':actual,'extension_census':dict(collections.Counter(pathlib.PurePosixPath(n).suffix.lower() or '<none>' for n in actual)),'source_outputs_original_runtime_and_preserved_vsix_unchanged':True,'candidate_clean':True,'extractor_result':load(P/'exe-data/result.json'),'scope':'Data-only signed EXE extraction; no installer execution, external model/corpus/private database/credential/source payload, adoption/Cargo/ZIP or post-sign full-client gate.'}
with (P/'exe-data/census-proof.json').open('x',encoding='utf-8',newline='\n') as f:json.dump(result,f,indent=2);f.write('\n')
print(json.dumps({'passed':True,'files':len(actual),'research':len(legal),'census_sha256':sha(P/'exe-data/census-proof.json')}))
