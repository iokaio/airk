# SPDX-License-Identifier: Apache-2.0
import pathlib,sys,os,subprocess,json,hashlib,time,zipfile,importlib.util,traceback
root=pathlib.Path(r'D:/code/Github/airk/artifacts/v190/final-plan/candidate-worktree-r4');out=pathlib.Path(__file__).resolve().parent
node=pathlib.Path(r'D:/code/Github/airk/artifacts/tools/node-v24.15.0-win-x64/node.exe');destination=pathlib.Path(r'D:/code/Github/airk/artifacts/arbiter-1.8.207/windows-candidate-r4')
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
# Existing reviewed identity/copy/owner functions; no Root HEAD comparison.
E=out.parent;P=E/'windows-packaging-candidate-r4-plan'
spec=importlib.util.spec_from_file_location('C4_existing_owner_policy',P/'candidate_overlay.proposed.py');policy=importlib.util.module_from_spec(spec);spec.loader.exec_module(policy);actor=policy.identity();policy.human_path(out,E)
assert sha(P/'final-transfer-proof.json')=='a2db79b92d210c383daee229097da437ee8270936c55794a0b295a41e8309043'
workspace=pathlib.Path(r'D:/code/Github/airk');parent=destination.parent
while not parent.exists():parent=parent.parent
policy.human_path(parent,workspace)
assert not os.path.lexists(destination/'publish');assert not os.path.lexists(root/'artifacts/windows-candidate-r4')
assert subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip()=='059a2a893069f89e8ea8aefec7f9bdb8bb6063b6'
assert subprocess.check_output(['git','status','--porcelain'],cwd=root,text=True)==''
files=subprocess.check_output(['git','ls-files','-z'],cwd=root).decode('utf-8').split('\0')[:-1]
source={n:sha(root/n) for n in files};compiled={p.relative_to(root).as_posix():sha(p) for p in (root/'packages/vscode/out').rglob('*') if p.is_file()}
assert len(source)==3511 and len(compiled)==223
spec=importlib.util.spec_from_file_location('current_C4_native_source_owner',root/'scripts/build-service-runtime.py');source_owner=importlib.util.module_from_spec(spec);spec.loader.exec_module(source_owner)
expected_C7={'sha256':'06c740ee904ba9ccc3aec6a8359bff2b6e1f76bd061aa26e41e3c55a2bfd910e','files':1944};assert source_owner.source_identity()==expected_C7
plan=json.loads((out.parent/'windows-packaging-candidate-r4-plan/plan.json').read_bytes());snapshot=pathlib.Path(plan['ui']['snapshot'])
assert sha(out.parent/'windows-packaging-candidate-r4-plan/plan.json')=='84a4d1f71b7b6fd95479d506aa7ca259c41f2061cc93004f631d74b027746b06'
event_path=P/'listing-input-event.json';assert sha(event_path)=='6c25bece67d362f7de2a7161191131283550cb782d7b74ec659ff63eabed99eb';event=json.loads(event_path.read_bytes())
unchanged=0;listing_delta=[]
for name in plan['ui']['reviewed_ui_sources']:
 if sha(root/name)==sha(snapshot/name):unchanged+=1
 else:
  assert name==event['path']=='packages/vscode/README.md' and sha(root/name)==event['current_source_sha256']=='96d130413d4387492ad340ac8eda2663b0c3dfee665e3e9dba3d6a203b763d64' and sha(snapshot/name)==event['original_source_sha256']=='660096057a8a06678c4d1ac3b3a0083896e5de1a609a66f94c3072a46d189689'
  listing_delta.append(name)
assert unchanged==449 and listing_delta==['packages/vscode/README.md']
preact=json.loads((P/'test-preact-supplement-plan.json').read_bytes());expected_deps=[dict(row,path=row['path'].removeprefix('node_modules/')) for row in plan['ui']['dependency_files']]+[{key:row[key] for key in ('path','sha256','bytes')}|{'path':'preact/'+row['path']} for row in preact['files']];expected_deps.sort(key=lambda row:row['path'])
assert len(expected_deps)==8918 and policy.files_below(root/'packages/vscode/node_modules',root)==expected_deps
assert policy.files_below(root/'packages/vscode/out',root)==plan['ui']['compiled_outputs']
assert all(sha(root/'packages/vscode/out'/r['path'])==r['sha256'] for r in plan['ui']['compiled_outputs'])
argv=[sys.executable,'-B',str(root/'scripts/package-extension.py'),'--channel','stable','--output',str(root/'artifacts/windows-candidate-r4')]
env=os.environ.copy();env['PATH']=str(node.parent)+os.pathsep+env['PATH'];env['PYTHONDONTWRITEBYTECODE']='1';env['TEMP']=env['TMP']=str(out/'temp');(out/'temp').mkdir();(out/'npm-cache').mkdir();env['npm_config_cache']=str(out/'npm-cache');env.pop('NODE_COMPILE_CACHE',None);env['NODE_DISABLE_COMPILE_CACHE']='1'
result={'schema':'airk.private.ui-package-r4/1','argv':argv,'cwd':str(root),'commit':'059a2a893069f89e8ea8aefec7f9bdb8bb6063b6','source_C7':'06c740ee904ba9ccc3aec6a8359bff2b6e1f76bd061aa26e41e3c55a2bfd910e','source_before':source,'compiled_before':compiled,'passed':False};start=time.monotonic()
try:
 with (out/'run.log').open('x',encoding='utf-8') as log:r=subprocess.run(argv,cwd=root,env=env,stdout=log,stderr=subprocess.STDOUT)
 result['exit_code']=r.returncode
 assert r.returncode==0,'Canonical packaging failed; inspect run.log'
 publish=root/'artifacts/windows-candidate-r4/publish';manifest=json.loads((publish/'manifest.json').read_bytes())
 assert manifest['commit']==result['commit'] and manifest['version']=='1.8.207' and manifest['node']=='v24.15.0'
 assert manifest['tree']=='92f38579cbd9a4650466cc839e34bde1b8574973'
 assert set(p.name for p in publish.iterdir())=={'airk-1.8.207.vsix','manifest.json','SHA256SUMS'}
 archive=publish/manifest['archive'];assert sha(archive)==manifest['archive_sha256']
 assert len(manifest['payload'])==129
 for name,h in manifest['payload'].items():
  if name.startswith('out/'):
   assert sha(root/'packages/vscode'/name)==h, 'SDK/bundle payload differs: '+name
 with zipfile.ZipFile(archive) as z:
  result['archive_census']=[{'name':i.filename,'bytes':i.file_size,'sha256':hashlib.sha256(z.read(i)).hexdigest()} for i in z.infolist() if not i.is_dir()]
  for n,h in {'out/workbenchWebview.js':'878b5d0dcdb45078c6c602c9d6e781794192f48b7077a440c147f0b913c15b49','out/workbenchWebview.css':'1106188be0bc909d7adae8774b0f403ee729d6a748232bb0224b0d8647eb37d4','out/explorerWebview.js':'c18fb1ae95917fa7babb852f8cf9d2930662e05abbb11b32b3baf8888a72aaa6'}.items():assert hashlib.sha256(z.read('extension/'+n)).hexdigest()==h
 assert source_owner.source_identity()==expected_C7
 assert source=={n:sha(root/n) for n in files};assert compiled=={p.relative_to(root).as_posix():sha(p) for p in (root/'packages/vscode/out').rglob('*') if p.is_file()}
 assert subprocess.check_output(['git','status','--porcelain'],cwd=root,text=True)==''
 assert policy.files_below(root/'packages/vscode/node_modules',root)==expected_deps
 assert policy.files_below(root/'packages/vscode/out',root)==plan['ui']['compiled_outputs']
 (destination/'publish').mkdir(parents=True,exist_ok=False);policy.human_path(destination/'publish',workspace)
 for p in publish.iterdir():
  policy.copy_new(p,destination/'publish'/p.name,{'sha256':sha(p),'bytes':p.stat().st_size},workspace)
  assert sha(p)==sha(destination/'publish'/p.name)
 result.update(actor=actor,reviewed_listing_event_sha256=sha(event_path),unchanged_old_UI_rows=449,compiled_original_handoff_sha256='3411049ba9793cbb6bddf1ed2cdd7362015cde3add94a81266f21cfc955fe8e1',compiled_outputs_reused_under_original_identity=True,new_compilation_performed=False,all8918dependencies_unchanged=True,passed=True,publish={p.name:{'bytes':p.stat().st_size,'sha256':sha(p)} for p in publish.iterdir()},destination=str(destination/'publish'),source_and_compiled_unchanged=True,qualification='Verified universal candidate; installed Explorer journey separate; no native payload or native qualification')
except BaseException as error:
 result['failure']={'type':type(error).__name__,'message':str(error)};traceback.print_exc();raise
finally:
 result['elapsed_seconds']=time.monotonic()-start;(out/'receipt.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8')
print(json.dumps({k:result[k] for k in ('passed','elapsed_seconds','publish','destination')},indent=2))
