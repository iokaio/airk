# SPDX-License-Identifier: Apache-2.0
"""Freeze opaque existing Rust metadata requests; never run Cargo/models/native images."""
import argparse,copy,hashlib,json,os,re,stat,subprocess
from pathlib import Path
ROOT=Path('D:/code/Github/airk')
P=ROOT/'artifacts/advanced-split/models/actual-composed-service-r2'
BASE=P/'final-plan-current-policy-preparation-20261116T085632Z/metadata-request.private.json'
GQA=P/'compact-gqa-native-metadata-r1/metadata-request.private.json'
SID='S-1-5-21-3678370870-3858642417-826968105-1002'
MODELS=['arbiter-focus-qwen3-reranker-06b','arbiter-context-qwen3-17b','arbiter-context-qwen3-4b']
GRAPHS=['5072c663bb8fbc587208dea2564c8ad27880177875e62633f12ea64250ec2738','c79c672513b874ab773ee7df8d4518e110d1e6196ae45076ce718f5b5dc6f7b4','f8ba5d3ea8a4269739ec5e517e61e4ec1f2f94560ea60014a4f5a9476c32fd94']
REGEN='arbiter::runtime::qualification::regeneration::emit_current_policy_profiles_from_verified_original_assets'
TEMPLATE='inference_engine::pool::arbiter::qualification::software_v2::emit_software_v2_native_numeric_template'
ORIGINAL='inference_engine::pool::arbiter::qualification::software_v2::original_sources::emit_software_v2_original_reference_assets'
GPU_PREFIX='inference_engine::pool::arbiter::qualification::software_v2::cuda_qualification::'
def digest(p):
 h=hashlib.sha256()
 with Path(p).open('rb') as f:
  for b in iter(lambda:f.read(4*1024*1024),b''):h.update(b)
 return h.hexdigest()
def ordinary(p):
 p=Path(p).absolute();m=p.lstat()
 if not stat.S_ISREG(m.st_mode) or getattr(m,'st_file_attributes',0)&0x400:raise ValueError('ordinary no-reparse file required')
 return p

def pin(p):
 p=ordinary(p);return {'path':str(p),'sha256':digest(p)}
def frozen(p,h):
 p=ordinary(p)
 if not re.fullmatch('[0-9a-f]{64}',h) or digest(p)!=h:raise ValueError('actual pinned input changed')
 return p

def write(p,v):
 with Path(p).open('x',encoding='utf8',newline='\n') as f:json.dump(v,f,indent=2);f.write('\n');f.flush();os.fsync(f.fileno())
 return pin(p)
def identity():
 who=subprocess.run(['C:/Windows/System32/whoami.exe','/user','/fo','csv','/nh'],check=True,capture_output=True,text=True).stdout
 logged=subprocess.run(['C:/Windows/System32/quser.exe'],check=True,capture_output=True,text=True).stdout
 if SID not in who or not any('tdotn' in x.lower() and x.lstrip().startswith('>') for x in logged.splitlines()):raise ValueError('same active human identity required')
 return {'account':'TYAI\\tdotn','sid':SID,'whoami':who,'quser':logged}
def owner(p):
 env=dict(os.environ);env['AIRK_NN0_OWNER_PATH']=str(p)
 script='(Get-Acl -LiteralPath $env:AIRK_NN0_OWNER_PATH).Owner'
 return subprocess.run(['C:/Program Files/PowerShell/7/pwsh.exe','-NoProfile','-Command',script],env=env,check=True,capture_output=True,text=True).stdout.strip().lower()

def new_root(path):
 path=Path(path).absolute()
 if not path.is_relative_to(ROOT/'artifacts') or path.exists() or not path.parent.is_dir():raise ValueError('fresh approved artifact child required')
 if owner(path.parent)!='tyai\\tdotn':raise ValueError('output parent is not current human owned')
 for parent in [path.parent,*path.parent.parents]:
  if getattr(parent.lstat(),'st_file_attributes',0)&0x400:raise ValueError('output ancestor reparse refused')
 path.mkdir();return path

def command(program,selector,env):
 return {'argv':[str(program),'--exact',selector,'--ignored','--nocapture','--test-threads=1'],'env':env,'NN_invocations':0,'execution':'NOT RUN; Root uses maintained same-human owned receipt runner'}
def prepare(a):
 ident=identity();build_path=frozen(a.build_receipt,a.build_receipt_sha256);inventory_path=frozen(a.inventory,a.inventory_sha256)
 build=json.loads(build_path.read_bytes());inventory=json.loads(inventory_path.read_bytes())
 if build.get('schema')!='airk-readiness-command/1' or build.get('exit_code')!=0 or not all(build.get(k) is True for k in ['inputs_unchanged','runner_unchanged','runtime_unchanged','command_inputs_unchanged']) or build.get('changed_groups')!=[]:raise ValueError('genuine successful unchanged native build required')
 if inventory.get('schema')!='airk.preserved-native-test-images/1' or inventory.get('actual_build_receipt')!=pin(build_path) or inventory.get('native_group')!=build['groups']['native']['sha256']:raise ValueError('image inventory/build/source authority mismatch')
 images={}
 for name in ['airk-retrieval','airk-inference']:
  candidates=[r for r in inventory['images'] if r['binary_id']==name]
  if len(candidates)!=1:raise ValueError('exact preserved native lib image required')
  row=candidates[0];frozen(row['path'],row['sha256'])
  if Path(row['path']).stat().st_size!=row['bytes'] or owner(row['path'])!='tyai\\tdotn':raise ValueError('preserved human image identity differs')
  images[name]=row
 old=json.loads(BASE.read_bytes());catalog_pin=old['source_catalog'];frozen(catalog_pin['path'],catalog_pin['sha256']);catalog=json.loads(Path(catalog_pin['path']).read_bytes())
 if [p['model'] for p in catalog['profiles']]!=MODELS:raise ValueError('original three-model source order differs')
 assets=Path(old['asset_root']);audit=[]
 for profile,expected in zip(catalog['profiles'],GRAPHS):
  for asset in profile['assets']:
   file=ordinary(assets/profile['manifest']/asset['name']);m=file.stat()
   if m.st_size!=asset['bytes'] or m.st_nlink!=1:raise ValueError('original retained asset type/length/link differs')
   if asset['role']=='model' and (asset['sha256']!=expected or digest(file)!=expected):raise ValueError('original graph identity differs; no graph override permitted')
  audit.append({'model':profile['model'],'original_manifest':profile['manifest'],'graph_sha256':expected,'assets':len(profile['assets'])})
 for k in ['acquisition_receipt','asset_census']:frozen(old[k]['path'],old[k]['sha256'])
 source_paths=[Path(p['path']) for p in old['source_files']]
 for relative in ['crates/airk-core/src/arbiter/device.rs','crates/airk-core/src/arbiter/device/receipt_runtime.rs','crates/airk-core/src/arbiter/device/state.rs','crates/airk-inference/src/inference_engine/pool/arbiter/qualification/software_v2/original_sources.rs','crates/airk-inference/src/inference_engine/pool/arbiter/qualification/software_v2/cuda_qualification.rs']:
  source_paths.append(ROOT/relative)
 source_paths=sorted(set(source_paths));sources=[]
 for file in source_paths:
  row=pin(file);key=file.relative_to(ROOT).as_posix()
  if build['groups']['native']['files'].get(key)!=row['sha256']:raise ValueError('consumed live source differs from actual native build: '+key)
  sources.append(row)
 if len(sources)>128:raise ValueError('native source count limit')
 head=subprocess.run(['git','rev-parse','HEAD'],cwd=ROOT,check=True,capture_output=True,text=True).stdout.strip()
 dirty=subprocess.run(['git','status','--porcelain','--untracked-files=no'],cwd=ROOT,check=True,capture_output=True,text=True).stdout
 out=new_root(a.output);commands=[];requests=[]
 for kind in ['original3']+(['compact-gqa'] if a.include_compact_gqa else []):
  base=json.loads((BASE if kind=='original3' else GQA).read_bytes())
  base.update(schema='airk.private.context-metadata-regeneration/3' if kind=='original3' else 'airk.private.context-metadata-regeneration/2',repository_root=str(ROOT),native_build_receipt=pin(build_path),source_commit=head,source_files=sources,output=str(out/('metadata-'+kind)))
  if kind=='original3':
   for key in ['compact_graph','large_graph','large_dense_graph']:base.pop(key,None)
  else:
   for row in base['compact_graph'].values():frozen(row['path'],row['sha256'])
  request=write(out/(kind+'-request.private.json'),base);requests.append({'kind':kind,'request':request,'future_output':base['output']})
  commands.append(command(images['airk-retrieval']['path'],REGEN,{'AIRK_CONTEXT_METADATA_REGEN_INPUT':request['path'],'AIRK_CONTEXT_METADATA_REGEN_INPUT_SHA256':request['sha256']}))
 (out/'numeric-templates').mkdir()
 packet={'schema':'airk.private.current-policy-nn0-preparation/1','identity':ident,'source_commit':head,'source_dirty':dirty,'actual_build_receipt':pin(build_path),'actual_image_inventory':pin(inventory_path),'images':images,'original_requests':[pin(BASE),pin(GQA)],'requests':requests,'commands':commands,'required_selectors':{'retrieval':[REGEN],'inference':[TEMPLATE,ORIGINAL,GPU_PREFIX+'emit_cuda_component_probe_request',GPU_PREFIX+'emit_cuda_fp32_probe_metadata']},'original_assets':audit,'all_weight_hashes_will_be_reauthenticated_by_native_emitter':True,'models_executed':False,'metadata_emitted':False,'scope':'Only freeze opaque existing Rust request and scalar/path/hash bindings; no Profile/CAL/policy/numeric DTO manufactured. /2 optional and separate; old verdicts/producers immutable.'}
 write(out/'commands.json',commands);prep=write(out/'preparation.json',packet);print(json.dumps(prep))
def bind(a):
 identity();prep_path=frozen(a.bind_emitted,a.preparation_sha256);prep=json.loads(prep_path.read_bytes());out=prep_path.parent
 commands=[];reports=[]
 for row in prep['requests']:
  if row['kind']!='original3':continue
  report=Path(row['future_output'])/'metadata-regeneration-report.private.json';v=json.loads(report.read_bytes())
  if v.get('complete') is not True or v.get('error') is not None or v.get('NN_invocations')!=0 or v.get('input')!=row['request'] or len(v.get('profiles',[]))!=3:raise ValueError('actual native original3 metadata emission incomplete')
  for model in MODELS:
   actual=[r for r in v['profiles'] if r['model']==model]
   if len(actual)!=1:raise ValueError('native emitted model roster differs')
   resource=actual[0]['resource_run_input'];frozen(resource['path'],resource['sha256'])
   output=out/'numeric-templates'/(model+'.json')
   if output.exists():raise ValueError('native template output already exists; preserve original')
   commands.append(command(prep['images']['airk-inference']['path'],TEMPLATE,{'AIRK_SOFTWARE_V2_NATIVE_INPUT':resource['path'],'AIRK_SOFTWARE_V2_NATIVE_INPUT_SHA256':resource['sha256'],'AIRK_SOFTWARE_V2_NATIVE_OUTPUT':str(output)}))
  reports.append(pin(report))
 write(out/'native-template-commands.json',{'schema':'airk.private.current-native-template-commands/1','preparation':pin(prep_path),'actual_native_reports':reports,'commands':commands,'models_executed':False,'reference_reuse':'Only after native template exact compiler/resource/row input equality against original independent fixtures; no old verdict/producer changes.'})
 print('Wrote create-new native-template-commands.json; no native/model execution occurred.')
def main():
 p=argparse.ArgumentParser();p.add_argument('--build-receipt');p.add_argument('--build-receipt-sha256');p.add_argument('--inventory');p.add_argument('--inventory-sha256');p.add_argument('--output');p.add_argument('--include-compact-gqa',action='store_true');p.add_argument('--bind-emitted');p.add_argument('--preparation-sha256');a=p.parse_args()
 if a.bind_emitted:bind(a)
 else:
  if not all([a.build_receipt,a.build_receipt_sha256,a.inventory,a.inventory_sha256,a.output]):p.error('all genuine build/inventory/output arguments required')
  prepare(a)
if __name__=='__main__':main()
