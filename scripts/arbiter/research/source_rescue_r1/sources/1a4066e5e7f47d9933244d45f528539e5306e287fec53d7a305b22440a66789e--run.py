# SPDX-License-Identifier: Apache-2.0
import pathlib,sys,os,subprocess,json,hashlib,time,zipfile,importlib.util,traceback
root=pathlib.Path(r'D:/code/Github/airk/artifacts/v190/final-plan/candidate-worktree-r2');out=pathlib.Path(__file__).resolve().parent
node=pathlib.Path(r'D:/code/Github/airk/artifacts/tools/node-v24.15.0-win-x64/node.exe');destination=pathlib.Path(r'D:/code/Github/airk/artifacts/arbiter-1.8.207/windows-candidate-r2')
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
assert not destination.exists();assert not (root/'artifacts/windows-candidate-r2').exists()
assert subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip()=='6c2410791999fa7691b9be97441f371d4048d870'
assert subprocess.check_output(['git','status','--porcelain'],cwd=root,text=True)==''
files=subprocess.check_output(['git','ls-files','-z'],cwd=root).decode('utf-8').split('\0')[:-1]
source={n:sha(root/n) for n in files};compiled={p.relative_to(root).as_posix():sha(p) for p in (root/'packages/vscode/out').rglob('*') if p.is_file()}
assert len(source)==3504 and len(compiled)==223
argv=[sys.executable,'-B',str(root/'scripts/package-extension.py'),'--channel','stable','--output',str(root/'artifacts/windows-candidate-r2')]
env=os.environ.copy();env['PATH']=str(node.parent)+os.pathsep+env['PATH'];env['PYTHONDONTWRITEBYTECODE']='1';env['TEMP']=env['TMP']=str(out/'temp');(out/'temp').mkdir()
result={'schema':'airk.private.ui-package-r2/1','argv':argv,'cwd':str(root),'commit':'6c2410791999fa7691b9be97441f371d4048d870','source_C7':'f4353a13aef8d5d908944fb643ff833f59af0f215cbd73bad79723b063ea8b1f','source_before':source,'compiled_before':compiled,'passed':False};start=time.monotonic()
try:
 with (out/'run.log').open('x',encoding='utf-8') as log:r=subprocess.run(argv,cwd=root,env=env,stdout=log,stderr=subprocess.STDOUT)
 result['exit_code']=r.returncode
 assert r.returncode==0,'Canonical packaging failed; inspect run.log'
 publish=root/'artifacts/windows-candidate-r2/publish';manifest=json.loads((publish/'manifest.json').read_bytes())
 assert manifest['commit']==result['commit'] and manifest['version']=='1.8.207' and manifest['node']=='v24.15.0'
 assert manifest['tree']=='0fca7d9f61123920362e42a03681058caa5f1570'
 assert set(p.name for p in publish.iterdir())=={'airk-1.8.207.vsix','manifest.json','SHA256SUMS'}
 archive=publish/manifest['archive'];assert sha(archive)==manifest['archive_sha256']
 with zipfile.ZipFile(archive) as z:
  result['archive_census']=[{'name':i.filename,'bytes':i.file_size,'sha256':hashlib.sha256(z.read(i)).hexdigest()} for i in z.infolist() if not i.is_dir()]
  for n,h in {'out/workbenchWebview.js':'878b5d0dcdb45078c6c602c9d6e781794192f48b7077a440c147f0b913c15b49','out/workbenchWebview.css':'1106188be0bc909d7adae8774b0f403ee729d6a748232bb0224b0d8647eb37d4','out/explorerWebview.js':'c18fb1ae95917fa7babb852f8cf9d2930662e05abbb11b32b3baf8888a72aaa6'}.items():assert hashlib.sha256(z.read('extension/'+n)).hexdigest()==h
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
