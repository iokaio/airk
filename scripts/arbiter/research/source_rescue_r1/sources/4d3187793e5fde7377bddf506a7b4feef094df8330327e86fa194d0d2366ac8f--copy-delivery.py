# SPDX-License-Identifier: Apache-2.0
import datetime,hashlib,json,pathlib,shutil,subprocess,zipfile
P=pathlib.Path(__file__).resolve().parent
ROOT=pathlib.Path(r'D:\code\Github\airk');D=ROOT/'artifacts/arbiter-1.8.207/windows-candidate-r3'
def read(p):return json.loads(p.read_text(encoding='utf-8-sig'))
def sha(p):
 with p.open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
def row(p):return {'sha256':sha(p),'bytes':p.stat().st_size}
def jsonbytes(value):return (json.dumps(value,indent=2)+'\n').encode()
def new(p,data):
 with p.open('xb') as f:f.write(data)
def copy(src,dest):
 assert src.is_file() and not src.is_symlink()
 with src.open('rb') as fi,dest.open('xb') as fo:shutil.copyfileobj(fi,fo,1024*1024)
 assert row(src)==row(dest)
 return row(dest)
plan=read(P/'plan.json');adopt=read(P/'adoption/plan.json');execution=read(P/'adoption/execution.json');proof=read(P/'phase1-artifact-proof.json');C=pathlib.Path(plan['candidate_root']);R=pathlib.Path(plan['runtime_dir']);runtime=pathlib.Path(adopt['signed_output'])/'runtime';publish=pathlib.Path(adopt['signed_package_output'])/'publish'
w=read(P/'postsign/signed-client-preflight-r3.json');report=read(P/'postsign/native-client-report.json')
for k in ('passed','inputs_unchanged','runner_unchanged','runtime_unchanged','command_inputs_unchanged'):assert w[k] is True,k
assert w['exit_code']==0 and w['runner_exit_code']==0 and not w['evidence_errors'] and not w['interrupted'];assert '--model' in w['command'] and w['command'][w['command'].index('--files')+1]=='16';assert report['managed_stop_complete'] is True and report['source_bytes_preserved'] is True and report['model_checked'] is True
assert report['images']=={'service_sha256':proof['signatures']['airk']['sha256'],'cli_sha256':proof['signatures']['airkctl']['sha256']}
assert report['service_clean_exit'] is None and report['inference_worker_clean_exit'] is None
for key in ('unsigned_manifest','signed_manifest','signing_receipt','archive'):assert sha(pathlib.Path(execution[key]['path']))==execution[key]['sha256'],key
m=read(publish/'manifest.json');assert m['commit']==plan['candidate_head'] and m['source']==plan['C7'] and m['signed_distribution'] is True and m['signing']['provenance']=='local'
rt={p.relative_to(runtime).as_posix():sha(p) for p in runtime.rglob('*') if p.is_file()};assert rt==m['payload'] and rt==w['runtime_seal']['files'] and len(rt)==846
zip_path=publish/m['archive'];assert sha(zip_path)==m['archive_sha256']
with zipfile.ZipFile(zip_path) as z:
 infos=z.infolist();assert len(infos)==len(rt) and {i.filename for i in infos}==set(rt)
 for info in infos:
  with z.open(info) as f:assert hashlib.file_digest(f,'sha256').hexdigest()==rt[info.filename],info.filename
for component in ('airk','airkctl'):
 assert rt[component+'.exe']==proof['signatures'][component]['sha256']
legal={}
for name,entry in proof['research_companions'].items():
 assert rt['notices/'+name]==entry['sha256'];legal['notices/'+name]={'sha256':entry['sha256'],'bytes':entry['bytes']}
assert len(legal)==11
for group,items in read(P/'candidate-input-pins.json').items():
 base=C if group=='sources' else C/'packages/vscode/out'
 for name,digest in items.items():assert sha(base/name)==digest,name
assert subprocess.check_output(['git','status','--porcelain'],cwd=C)==b''
assert subprocess.check_output(['git','rev-parse','HEAD'],cwd=C,text=True).strip()==plan['candidate_head']
original={p.relative_to(R).as_posix():sha(p) for p in R.rglob('*') if p.is_file()};assert original==read(P/'driver/windows-installer-r3.json')['runtime_seal']['files']
vsix={p.name:row(p) for p in (D/'publish').iterdir() if p.is_file()};assert set(vsix)=={'airk-1.8.207.vsix','manifest.json','SHA256SUMS'};assert vsix['airk-1.8.207.vsix']['sha256']=='5053a93364b123b87bee006df7314d4abc3e03b3c2ea8a77d7f33885b46c1c56'
r2=read(P/'preserved-vsix-pins.json')
for entry in r2.values():assert row(pathlib.Path(entry['path']))=={k:entry[k] for k in ('sha256','bytes')}
run=pathlib.Path(proof['local_run']);installer=pathlib.Path(proof['signatures']['installer']['path']);assert row(installer)=={k:proof['signatures']['installer'][k] for k in ('sha256','bytes')}
N=D/'native/publish';I=D/'installer/publish';assert not N.parent.exists() and not I.parent.exists()
N.parent.mkdir();N.mkdir();I.parent.mkdir();I.mkdir()
copied={'native':{},'installer':{}}
assert {p.name for p in publish.iterdir()}=={m['archive'],'manifest.json','SHA256SUMS'}
for src in publish.iterdir():copied['native'][src.name]=copy(src,N/src.name)
copied['installer'][installer.name]=copy(installer,I/installer.name)
copy(run/'signing-installer.json',I/'signing.json')
public_manifest={'schema':'airk-windows-installer-candidate/1','version':'1.8.207','target':m['target'],'commit':plan['candidate_head'],'tree':plan['candidate_tree'],'source':plan['C7'],'profile':'debug','artifact':installer.name,'artifact_sha256':sha(installer),'artifact_bytes':installer.stat().st_size,'signed_distribution':True,'signing_provenance':'local','signing_receipt':'signing.json','signing_receipt_sha256':sha(I/'signing.json'),'payload_inventory_sha256':proof['payload_inventory_sha256'],'signed_native_archive_sha256':m['archive_sha256'],'signed_service_sha256':rt['airk.exe'],'signed_client_sha256':rt['airkctl.exe'],'research_legal_companions':legal,'qualification':{'signed_service_clients':{'files_per_project':16,'model_checked':True,'elapsed_seconds':w['elapsed_seconds'],'managed_stop_complete':True,'source_preserved':True},'installer_execution':False,'installed_acceptance':False,'oldest_os':False,'hosted_publication':False},'evidence_sha256':{'local_driver_receipt':proof['outer_receipt']['sha256'],'local_installer_receipt':proof['local_receipt']['sha256'],'full_signed_preflight':sha(P/'postsign/signed-client-preflight-r3.json'),'full_signed_native_report':sha(P/'postsign/native-client-report.json'),'extracted_payload_census':sha(P/'exe-data/census-proof.json')}}
new(I/'manifest.json',jsonbytes(public_manifest))
checksum_names=sorted(p.name for p in I.iterdir());new(I/'SHA256SUMS',''.join(f'{sha(I/name)}  {name}\n' for name in checksum_names).encode())
copied['installer']={p.name:row(p) for p in I.iterdir()}
for folder in (N,I):
 for line in (folder/'SHA256SUMS').read_text().splitlines():
  digest,name=line.split('  ',1);assert sha(folder/name)==digest,name
assert {p.name:row(p) for p in (D/'publish').iterdir() if p.is_file()}==vsix
for entry in r2.values():assert sha(pathlib.Path(entry['path']))==entry['sha256']
assert subprocess.check_output(['git','status','--porcelain'],cwd=C)==b''
result={'schema':'airk.private.windows-r3-delivery-copy/1','observed_utc':datetime.datetime.now(datetime.UTC).isoformat(),'passed':True,'candidate_root':str(C),'candidate_commit':plan['candidate_head'],'source':plan['C7'],'delivery_root':str(D),'copied':copied,'preserved_r3_vsix':vsix,'preserved_r2_publish':r2,'all846_zip_runtime_entries_exact':True,'all11_legal_companions_exact':legal,'signed_preflight_seconds':w['elapsed_seconds'],'native_stop_evidence':{'managed_stop_complete':True,'source_preserved':True,'service_clean_exit':None,'inference_worker_clean_exit':None,'parent_owned_service_handle_exit_code':'not recorded; not claimed'},'scope':'Verified local Windows debug candidate copies only; no installer execution, installation, publication, Cargo or source changes.'}
new(P/'delivery-copy-proof.json',jsonbytes(result));print(json.dumps({'passed':True,'copied':copied,'proof_sha256':sha(P/'delivery-copy-proof.json')}))
