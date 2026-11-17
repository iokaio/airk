# SPDX-License-Identifier: Apache-2.0
import hashlib, importlib.util, json, os, subprocess, sys, time
from pathlib import Path
project=Path.cwd(); build=Path(sys.argv[1]).resolve(); binary=build/'native/stage/airk.exe'
base=build/'upgrade-from-tantivy';base.mkdir();source=base/'source';source.mkdir();file=source/'auth.cs';file.write_text('class AuthConfig { public void ConfigureAuthentication() {} }\n',encoding='utf-8');data=base/'state'
old=project/'artifacts/local/20261021-144525-206f925f/native/stage/airk.exe'
args=['--root',str(source),'--data-dir',str(data),'--collection','upgrade-fixture']
def run(exe,args):
 r=subprocess.run([str(exe),*args],capture_output=True,text=True,encoding='utf-8',timeout=120,creationflags=subprocess.CREATE_NO_WINDOW)
 if r.returncode: raise RuntimeError(r.stderr)
 return json.loads(r.stdout)
run(old,[*args,'collections','vectors','set','--enabled','false']);run(old,[*args,'index'])
owner=data/'collections-1.2/upgrade-fixture/current-owner.json';assert json.loads(owner.read_text())['version']==120
before=hashlib.sha256(file.read_bytes()).hexdigest()
run(binary,[*args,'index','--full','--scheduled'])
assert json.loads(owner.read_text())['version']==125
assert run(binary,[*args,'status'])['lexical_backend']=='sqlite-fts5-current'
assert not run(binary,[*args,'collections','vectors','show'])['enabled']
assert run(binary,[*args,'search','ConfigureAuthentication','--mode','lexical'])['hits']
assert hashlib.sha256(file.read_bytes()).hexdigest()==before
result={'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'actual_tantivy_upgrade':True,'saved_settings':True,'unchanged_source':True,'same_collection_id':True,'real_collections':[]}
(build/'fts5-upgrade-check.json').write_text(json.dumps(result,indent=2),encoding='utf-8')
print('Packaged executable: actual Tantivy upgrade passed; waiting for bounded real-repository scans.',flush=True)
report=project/'artifacts/local/20261021-161459-ae6f79cf/fts5-real-repo-tests/results.json'
deadline=time.monotonic()+1800
while True:
 try: rows=json.loads(report.read_text(encoding='utf-8'))['repositories']
 except (FileNotFoundError,json.JSONDecodeError): rows=[]
 if any(row['state']=='failed' for row in rows): raise RuntimeError('Real-repository scan failed; inspect its local report')
 if len(rows)==2 and all(row['state']=='passed_partial_semantic' for row in rows): break
 if time.monotonic()>deadline: raise TimeoutError('Bounded real-repository scans did not finish')
 time.sleep(2)
spec=importlib.util.spec_from_file_location('driver',project/'scripts/benchmark-detail.py');module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
state=project/'artifacts/local/20261021-161459-ae6f79cf/fts5-real-repo-tests/state'
for name in ('iokaio-web','bookbuilder'):
 root=Path('D:/code/Github')/name;collection='local-125-'+name
 command=[str(binary),'--root',str(root),'--data-dir',str(state),'--collection',collection]
 status=run(binary,[*command[1:],'status']);reader=module.Mcp(command,root,collection,base/(name+'-mcp.log'))
 try:
  tools=reader.request('tools/list',{})['result']['tools'];assert len(tools)==15;assert 'airk_history' not in [t['name'] for t in tools]
  queries=[]
  for mode in ('lexical','hybrid','vector'):
   response,elapsed=reader.call('airk_search',{'query':'configuration','mode':mode,'limit':3,'max_bytes':6000})
   assert response['hits'];verified=0
   for hit in response['hits']:
    citation=hit['citation'];path=(root/citation['path']).resolve();assert path.is_relative_to(root.resolve())
    assert hashlib.sha256(path.read_bytes()).hexdigest()==citation['content_hash'];verified+=1
   queries.append({'mode':mode,'seconds':round(elapsed,3),'verified_citations':verified})
  result['real_collections'].append({'name':name,'files':status['files'],'semantic_state':status['semantic_readiness']['state'],'queries':queries})
 finally: reader.close()
(build/'fts5-final-check.json').write_text(json.dumps(result,indent=2),encoding='utf-8');print(json.dumps(result))
