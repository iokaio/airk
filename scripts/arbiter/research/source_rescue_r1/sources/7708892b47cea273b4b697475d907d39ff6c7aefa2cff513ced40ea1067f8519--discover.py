# SPDX-License-Identifier: Apache-2.0
"""Read official version inventories only; never publish, tag, bump or open PRs."""
from concurrent.futures import ThreadPoolExecutor
import datetime,hashlib,importlib.util,json,pathlib,re,subprocess,time
ROOT=pathlib.Path.cwd();OUT=ROOT/'artifacts/v190/final-plan/exec-20261116T052942Z/version-discovery-r1'
start=datetime.datetime.now(datetime.timezone.utc).isoformat();tick=time.monotonic()
spec=importlib.util.spec_from_file_location('version_inventory',ROOT/'scripts/release-version.py');version=importlib.util.module_from_spec(spec);spec.loader.exec_module(version)
records=[];data={}
def get(name,call):
 t=time.monotonic();utc=datetime.datetime.now(datetime.timezone.utc).isoformat()
 try:
  result=call();raw=(json.dumps(result,ensure_ascii=False,indent=2)+'\n').encode();(OUT/(name+'.json')).write_bytes(raw)
  return name,result,{'operation':name,'started_at_utc':utc,'elapsed_seconds':time.monotonic()-t,'status':'passed','output_sha256':hashlib.sha256(raw).hexdigest()}
 except Exception as e:return name,None,{'operation':name,'started_at_utc':utc,'elapsed_seconds':time.monotonic()-t,'status':'unavailable','error':str(e)[:1500]}
with ThreadPoolExecutor(max_workers=3)as pool:
 jobs=[pool.submit(get,'github-releases',lambda:version.api('releases?per_page=100',paginate=True)),pool.submit(get,'github-tags',lambda:version.api('tags?per_page=100',paginate=True)),pool.submit(get,'marketplace-versions',lambda:version.marketplace(timeout=45))]
 for job in jobs:
  name,result,record=job.result();records.append(record)
  if result is not None:data[name]=result
source=version.source_version();local_tags=subprocess.check_output(['git','tag','--list'],text=True).splitlines()
used={source};used.update(m[1]for tag in local_tags if(m:=version.TAG.fullmatch(tag)))
remote_releases=[r for page in data.get('github-releases',[])for r in page];remote_tags=[t['name']for page in data.get('github-tags',[])for t in page];market=data.get('marketplace-versions',[])
used.update(x['version']for x in market);used.update(m[1]for tag in remote_tags if(m:=version.TAG.fullmatch(tag)));used.update(m[1]for r in remote_releases if(m:=version.TAG.fullmatch(r['tag_name'])))
local=[];unavailable=[]
for root_name in ['artifacts/local-service-installer','artifacts/local-vsix']:
 root=ROOT/root_name
 if not root.exists():continue
 for folder in sorted(root.iterdir()):
  if not folder.is_dir():continue
  paths=[folder/'receipt.json']
  if root_name.endswith('local-vsix'):paths.extend([folder/'vsix/publish/manifest.json',folder/'vsix/publish/inventory.json',folder/'vsix/inventory.json'])
  for path in paths:
   if not path.exists():continue
   try:
    raw=path.read_bytes();obj=json.loads(raw)
   except (OSError,ValueError)as e:unavailable.append({'path':str(path.relative_to(ROOT)),'error':type(e).__name__});continue
   # Package receipts are metadata, not credentials/source-state roots. Record
   # every literal numeric version and package filename version in these inputs.
   found=sorted(set(re.findall(r'(?<![0-9.])1\.8\.(?:0|[1-9][0-9]*)(?![0-9.])',raw.decode('utf-8'))),key=version.numeric)
   used.update(found);local.append({'path':str(path.relative_to(ROOT)),'sha256':hashlib.sha256(raw).hexdigest(),'bytes':len(raw),'versions':found})
  for path in list(folder.glob('airk-service-1.8.*-*-installer.exe'))+list((folder/'vsix/publish').glob('airk-1.8.*.vsix')):
   found=re.findall(r'(?<![0-9.])1\.8\.(?:0|[1-9][0-9]*)(?![0-9.])',path.name);used.update(found)
   local.append({'path':str(path.relative_to(ROOT)),'metadata':'package filename/stat only; payload not opened or requalified','bytes':path.stat().st_size,'versions':found})
major,minor,patch=version.numeric(source);patch+=1
while f'{major}.{minor}.{patch}'in used:patch+=1
candidate=f'{major}.{minor}.{patch}'
complete=len(data)==3 and not unavailable
summary={'schema':'airk.private-next-local-candidate-version/1','started_at_utc':start,'ended_at_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'elapsed_seconds':time.monotonic()-tick,'execution_identity':{'account':'TYAI\\tdotn','sid':'S-1-5-21-3678370870-3858642417-826968105-1002','verification':'active quser and Explorer session1'},'source_commit':subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),'source_version':source,'script_sha256':hashlib.sha256((ROOT/'scripts/release-version.py').read_bytes()).hexdigest(),'invoked_script_helpers':['source_version','api(releases?per_page=100,paginate=True)','api(tags?per_page=100,paginate=True)','marketplace(timeout=45)'],'public_sources':['https://api.github.com/repos/iokaio/airk/releases?per_page=100','https://api.github.com/repos/iokaio/airk/tags?per_page=100','https://marketplace.visualstudio.com/_apis/public/gallery/extensionquery'],'operations':records,'counts':{'github_releases':len(remote_releases),'github_tags':len(remote_tags),'marketplace_entries':len(market),'local_tags':len(local_tags),'local_receipt_or_package_metadata':len(local)},'local_tags':local_tags,'local_artifact_inventory':local,'unavailable_local_inputs':unavailable,'used_1_8_versions':sorted([v for v in used if re.fullmatch(r'1\.8\.[0-9]+',v)],key=version.numeric),'recommended_version':candidate if complete else None,'tentative_next_unobserved_version':candidate,'inventory_complete_for_examined_sources':complete,'reason':'Smallest patch greater than synchronized live source not present in any complete official release/tag/Marketplace inventory or examined approved local candidate metadata. No version reserved or bumped; recheck immediately before bump if concurrent publication or candidate creation occurs.','actions_performed':{'source_mutations':0,'git_mutations':0,'tags_created':0,'publication':0,'PRs':0}}
(OUT/'summary.json').write_text(json.dumps(summary,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
print(json.dumps({k:summary[k]for k in ['source_version','counts','recommended_version','tentative_next_unobserved_version','inventory_complete_for_examined_sources','unavailable_local_inputs']}));print(json.dumps(records))
