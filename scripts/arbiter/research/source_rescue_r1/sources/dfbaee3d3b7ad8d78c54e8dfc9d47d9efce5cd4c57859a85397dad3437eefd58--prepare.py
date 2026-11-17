# SPDX-License-Identifier: Apache-2.0
import pathlib,json,hashlib,subprocess,ast,shutil,datetime
root=pathlib.Path.cwd();base=pathlib.Path(r'D:/code/Github/airk/artifacts/v190/final-plan/exec-20261116T052942Z');out=base/'ui-native-r2-cards';runtime=pathlib.Path(r'D:/code/Github/airk/artifacts/arbiter-1.8.207/windows-runtime-debug-r2')
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
assert subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip()=='6c2410791999fa7691b9be97441f371d4048d870';assert subprocess.check_output(['git','status','--porcelain'],text=True)==''
source=subprocess.check_output(['git','ls-files','-z']).decode('utf-8').split('\0')[:-1];assert len(source)==3504
outputs=list(p for p in (root/'packages/vscode/out').rglob('*') if p.is_file());assert len(outputs)==223
files={str(root/n):sha(root/n) for n in source};files.update({str(p):sha(p) for p in outputs});files.update({str(p):sha(p) for p in runtime.rglob('*') if p.is_file()})
for p in [pathlib.Path(r'D:/code/Github/airk/artifacts/tools/node-v24.15.0-win-x64/node.exe'),pathlib.Path(r'C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe'),base/'windows-runtime-debug-r2-build.json',base/'ui-title-build-r1/admin-assets.json']:files[str(p)]=sha(p)
expected={'airk.exe':'60b7f664958208e25336696fc55dfa3c905ee6dcfc9a8eb1ead531bbe0003563','airkctl.exe':'e8d8e32a73700f20b51c29b125dad5dd2ebe8b4aed88e16e38f9f0ca8f72e1c8','airk-build.json':'3c378f3a30940a19c3d6a1e813f26423c2bfd1feb538545c7cf2312dc0443102'}
for n,h in expected.items():assert sha(runtime/n)==h
build=json.loads((runtime/'airk-build.json').read_bytes());assert build['source']=={'sha256':'f4353a13aef8d5d908944fb643ff833f59af0f215cbd73bad79723b063ea8b1f','files':1937}
assert build['admin_assets']['bundle_sha256']=='8e65febbf798774d0adbfe9ce8c757c7615390b9cd14fba8fa7cc1d987a4f7d1'
assert build['admin_assets']['inventory_sha256']=='b21f516d5fe8313e6df13249a1874200e078a4f7f80093b28cbde0401a9985c8'
binding={'schema':'airk.private.native-ui-frozen-binding/1','candidate':str(root),'commit':'6c2410791999fa7691b9be97441f371d4048d870','tree':'0fca7d9f61123920362e42a03681058caa5f1570','source_C7':build['source'],'runtime':str(runtime),'files':files,'source_count':3504,'compiled_count':223,'native_builder_receipt_sha256':sha(base/'windows-runtime-debug-r2-build.json'),'shipped_vsix_sha256':'5de5d43b03642e52f53b5dde3636cb34d8b92095a4e11d788e8d13d15c5b6062','actual_execution':False}
(out/'bindings.json').write_text(json.dumps(binding,indent=2)+'\n',encoding='utf-8')
guard='''import pathlib,json,hashlib,subprocess
BINDING=pathlib.Path(__file__).resolve().parent/'bindings.json'
def verify():
 d=json.loads(BINDING.read_bytes());root=pathlib.Path.cwd()
 assert root.samefile(d['candidate']),'Candidate cwd differs'
 assert subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True,encoding='utf-8').strip()==d['commit'],'Candidate HEAD drift'
 assert subprocess.check_output(['git','rev-parse','HEAD^{tree}'],cwd=root,text=True,encoding='utf-8').strip()==d['tree'],'Candidate tree drift'
 assert subprocess.check_output(['git','status','--porcelain'],cwd=root,text=True,encoding='utf-8')=='','Candidate tracked status changed'
 for name,want in d['files'].items():
  p=pathlib.Path(name)
  assert p.is_file() and hashlib.sha256(p.read_bytes()).hexdigest()==want,'Frozen input differs: '+name
 return {'binding_sha256':hashlib.sha256(BINDING.read_bytes()).hexdigest(),'files_verified':len(d['files']),'source_C7':d['source_C7']}
def settle(result):
 try:result['binding_post']=verify();return True
 except BaseException as error:result.update(passed=False,binding_failure={'type':type(error).__name__,'message':str(error)});return False
'''
admin=out/'admin-r3';work=out/'workbench-r2';admin.mkdir();work.mkdir()
for p in (admin,work):
 (p/'bindings.json').write_bytes((out/'bindings.json').read_bytes());(p/'frozen_binding.py').write_text(guard,encoding='utf-8')
# Preserve all current Admin assertions, including exact12titles, full manual/clipboard/resource details and request terminal counts.
for n in ['admin-live.mjs','browser-harness.mjs']:shutil.copyfile(base/'ui-live-admin-r3'/n,admin/n)
a=(base/'ui-live-admin-r3/run.py').read_text(encoding='utf-8');a=a.replace('root=pathlib.Path.cwd();out=pathlib.Path(__file__).resolve().parent','root=pathlib.Path.cwd();out=pathlib.Path(__file__).resolve().parent\nfrom frozen_binding import verify,settle\nbinding_before=verify()')
a=a.replace("text=True,timeout=240","text=True,encoding='utf-8',timeout=240")
a=a.replace("result.update(elapsed_seconds=time.monotonic()-started,native_processes=witness);", "binding_ok=settle(result);result.update(binding_before=binding_before,elapsed_seconds=time.monotonic()-started,native_processes=witness);")
a+='\nassert binding_ok, "Frozen Admin inputs changed; execution receipt remains nonpassing"\n'
w=(base/'ui-live-workbench-r1/run.py').read_text(encoding='utf-8');w=w.replace('root=pathlib.Path.cwd();out=pathlib.Path(__file__).resolve().parent','root=pathlib.Path.cwd();out=pathlib.Path(__file__).resolve().parent\nfrom frozen_binding import verify,settle\nbinding_before=verify()\nassert not (out/"execution.json").exists(), "Fresh run required"')
w=w.replace('windows-runtime-debug-r1','windows-runtime-debug-r2')
w=w.replace("result.update(elapsed_seconds=time.monotonic()-started,native_processes=witness);", "binding_ok=settle(result);result.update(binding_before=binding_before,elapsed_seconds=time.monotonic()-started,native_processes=witness);")
w+='\nassert binding_ok, "Frozen Workbench inputs changed; execution receipt remains nonpassing"\n'
for p,s in [(admin/'run.py',a),(work/'run.py',w)]:ast.parse(s);p.write_text(s,encoding='utf-8')
# Cheap syntax only: no native/browser launch, no test execution.
ast.parse(guard)
for p in [admin/'admin-live.mjs',admin/'browser-harness.mjs']:
 r=subprocess.run([r'D:/code/Github/airk/artifacts/tools/node-v24.15.0-win-x64/node.exe','--check',str(p)],capture_output=True,text=True,encoding='utf-8');assert r.returncode==0,r.stderr
cards={'schema':'airk.private.native-ui-r2-cards/1','status':'prepared-not-executed-awaiting-root-review','candidate':str(root),'runtime':str(runtime),'binding_sha256':sha(out/'bindings.json'),'private_inputs':{p.relative_to(out).as_posix():sha(p) for p in out.rglob('*') if p.is_file()},'environment':{'AIRK_PRIVATE_CENSUS_BASE':r'C:/Users/tdotn/AppData/Local/airk-final-exec-20261116T052942Z/temp','PYTHONDONTWRITEBYTECODE':'1'},'admin':{'argv':[r'C:/Users/tdotn/AppData/Local/Programs/Python/Python313/python.exe','-B',str(admin/'run.py'),'--runtime-dir',str(runtime),'--expected-head',binding['commit']],'scope':'Actual embedded assets and I3 in private Edge; all24navigation/12titles/12icons/wide+narrow/fullnativeResources+status+3manualWindowsclipboard comparisons; exact issued/completed/failed/aborted/missing-terminal bucket counts, no count target; original native Stop and retained kernel process handle exit proof; clipboard restored in memory only if still owned.'},'workbench':{'argv':[r'C:/Users/tdotn/AppData/Local/Programs/Python/Python313/python.exe','-B',str(work/'run.py')],'scope':'Actual native I2+compiled production Workbench/WindowScope/adapter/session callbacks, injected VSCode host (not installed/DOM). Full-result comparisons, both Answer routes/source opens, source readonly, native Stop and original kernel handle exit proof. Compact actualDOM separately passed65 on same bytes.'},'unchanged_prior_evidence':'Historical r1/r2 receipts remain under original identities. No optional model or installed-native Workbench qualification claimed.'}
(out/'cards.json').write_text(json.dumps(cards,indent=2)+'\n',encoding='utf-8');print(json.dumps({'cards':str(out/'cards.json'),'sha256':sha(out/'cards.json'),'binding_sha256':sha(out/'bindings.json'),'frozen_files':len(files),'executed':False},indent=2))
