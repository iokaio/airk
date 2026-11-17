# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import importlib.util,json,os,subprocess,hashlib,time
root=Path.cwd();out=root/'artifacts/advanced-split/w4-file-info-diagnostic-r1'
spec=importlib.util.spec_from_file_location('source_probe_clients',root/'scripts/check-service-clients.py');clients=importlib.util.module_from_spec(spec);spec.loader.exec_module(clients)
runtime=root/'artifacts/advanced-split/runtime-current-context-r2';node=root/'artifacts/tools/node-v24.15.0-win-x64/node.exe'
sha=lambda p:hashlib.sha256(Path(p).read_bytes()).hexdigest()
before={str(p):sha(p) for p in [out/'probe.cjs',root/'scripts/check-service-clients.py',runtime/'airk.exe',runtime/'airkctl.exe']}
started=time.monotonic();result=None
try:
 with clients.owned_client_fixture() as directory:
  owner=Path(directory).resolve();(owner/'data').mkdir(mode=0o700);sources=[]
  for name in ('a','b'):
   source=owner/('source-'+name);source.mkdir(mode=0o700);(source/'fixture.py').write_bytes(('# amber_marker\ndef workbench_fixture():\n    return "workbenchneedle-'+name+'"\n').encode());sources.append(source)
  with clients.Fixture(runtime,owner) as fixture:
   for name,source in zip(('workbench-a','workbench-b'),sources):fixture.register(name,source,owner/'data');fixture.job(name,'index');fixture.published_status(name)
   request={'runtime':str(runtime),'owner':str(owner),'cli':str(fixture.cli),'cli_sha256':sha(fixture.cli),'version':'1.9.0','control':str(fixture.control.resolve()),'project':fixture.projects['workbench-a'],'output':str(owner/'source-probe.json')}
   request_path=owner/'source-probe-request.json';request_path.write_text(json.dumps(request)+'\n',encoding='utf-8')
   result=subprocess.run([str(node),str(out/'probe.cjs'),str(request_path)],cwd=root,env=fixture.environment,text=True,capture_output=True,timeout=90,creationflags=getattr(subprocess,'CREATE_NO_WINDOW',0))
   for path in owner.glob('source-probe.json*'):(out/path.name).write_bytes(path.read_bytes())
   (out/'stdout.txt').write_text(result.stdout,encoding='utf-8');(out/'stderr.txt').write_text(result.stderr,encoding='utf-8')
   (out/'invocation.json').write_text(json.dumps({'owner':str(owner),'returncode':result.returncode,'request':request},indent=2)+'\n',encoding='utf-8')
   if result.returncode:raise RuntimeError('Bounded source diagnostic retained full original error; inspect source-probe.json.failure.json')
finally:
 assert all(sha(Path(path))==value for path,value in before.items())
 (out/'run-receipt.json').write_text(json.dumps({'elapsed_seconds':time.monotonic()-started,'inputs':before,'unchanged':True,'returncode':None if result is None else result.returncode},indent=2)+'\n',encoding='utf-8')
