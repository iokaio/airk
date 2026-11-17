# SPDX-License-Identifier: Apache-2.0
import pathlib,sys,os,subprocess,json,hashlib,time,zipfile,importlib.util,traceback
root=pathlib.Path(r'D:/code/Github/airk/artifacts/v190/final-plan/candidate-worktree-r3');out=pathlib.Path(__file__).resolve().parent
node=pathlib.Path(r'D:/code/Github/airk/artifacts/tools/node-v24.15.0-win-x64/node.exe');destination=pathlib.Path(r'D:/code/Github/airk/artifacts/arbiter-1.8.207/windows-candidate-r3')
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
assert not destination.exists();assert not (root/'artifacts/windows-candidate-r3').exists()
assert subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip()=='c547e6547b1312c2e113bd1ddc9ab84969c229c0'
assert subprocess.check_output(['git','status','--porcelain'],cwd=root,text=True)==''
files=subprocess.check_output(['git','ls-files','-z'],cwd=root).decode('utf-8').split('\0')[:-1]
source={n:sha(root/n) for n in files};compiled={p.relative_to(root).as_posix():sha(p) for p in (root/'packages/vscode/out').rglob('*') if p.is_file()}
assert len(source)==3508 and len(compiled)==223
spec=importlib.util.spec_from_file_location('current_C3_native_source_owner',root/'scripts/build-service-runtime.py');source_owner=importlib.util.module_from_spec(spec);spec.loader.exec_module(source_owner)
expected_C7={'sha256':'a8d7983b0f8bfdb701c7659f3b7a424505359a0dbe52446eb10c31de0b1a3dfe','files':1941};assert source_owner.source_identity()==expected_C7
plan=json.loads((out.parent/'candidate-overlay-preparation-r3/postcommit-plan.json').read_bytes());snapshot=pathlib.Path(plan['ui']['snapshot'])
assert all(sha(root/n)==sha(snapshot/n) for n in plan['ui']['reviewed_ui_sources'])
assert all(sha(root/'packages/vscode/out'/r['path'])==r['sha256'] for r in plan['ui']['compiled_outputs'])
argv=[sys.executable,'-B',str(root/'scripts/package-extension.py'),'--channel','stable','--output',str(root/'artifacts/windows-candidate-r3')]
env=os.environ.copy();env['PATH']=str(node.parent)+os.pathsep+env['PATH'];env['PYTHONDONTWRITEBYTECODE']='1';env['TEMP']=env['TMP']=str(out/'temp');(out/'temp').mkdir();(out/'npm-cache').mkdir();env['npm_config_cache']=str(out/'npm-cache');env.pop('NODE_COMPILE_CACHE',None);env['NODE_DISABLE_COMPILE_CACHE']='1'
result={'schema':'airk.private.ui-package-r3/1','argv':argv,'cwd':str(root),'commit':'c547e6547b1312c2e113bd1ddc9ab84969c229c0','source_C7':'a8d7983b0f8bfdb701c7659f3b7a424505359a0dbe52446eb10c31de0b1a3dfe','source_before':source,'compiled_before':compiled,'passed':False};start=time.monotonic()
try:
 with (out/'run.log').open('x',encoding='utf-8') as log:r=subprocess.run(argv,cwd=root,env=env,stdout=log,stderr=subprocess.STDOUT)
 result['exit_code']=r.returncode
 assert r.returncode==0,'Canonical packaging failed; inspect run.log'
 publish=root/'artifacts/windows-candidate-r3/publish';manifest=json.loads((publish/'manifest.json').read_bytes())
 assert manifest['commit']==result['commit'] and manifest['version']=='1.8.207' and manifest['node']=='v24.15.0'
 assert manifest['tree']=='b83e6757e1314ebd1109d37bac9cee8a33fb7ac4'
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
 (destination/'publish').mkdir(parents=True,exist_ok=False)
 for p in publish.iterdir():
  with (destination/'publish'/p.name).open('xb') as f:f.write(p.read_bytes())
  assert sha(p)==sha(destination/'publish'/p.name)
 result.update(passed=True,publish={p.name:{'bytes':p.stat().st_size,'sha256':sha(p)} for p in publish.iterdir()},destination=str(destination/'publish'),source_and_compiled_unchanged=True,qualification='Verified universal candidate; installed Explorer journey separate; no native payload or native qualification')
except BaseException as error:
 result['failure']={'type':type(error).__name__,'message':str(error)};traceback.print_exc();raise
finally:
 result['elapsed_seconds']=time.monotonic()-start;(out/'receipt.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8')
print(json.dumps({k:result[k] for k in ('passed','elapsed_seconds','publish','destination')},indent=2))
