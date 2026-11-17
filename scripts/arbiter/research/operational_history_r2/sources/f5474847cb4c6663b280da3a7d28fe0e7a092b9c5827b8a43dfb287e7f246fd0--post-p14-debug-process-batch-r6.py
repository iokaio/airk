# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import hashlib,importlib.util,json,os,shutil,subprocess,sys,time
B=Path(__file__).resolve().parent;W=B/'p10-worktree';R=B/'post-p14-debug-runtime-r2'
EXPECTED={'sha256':'6de490d57d7053cbd63862366e840cc3657df4ef10e8a57c4ca018e6d19a2633','files':968}
def load(name,p):
 s=importlib.util.spec_from_file_location(name,p);m=importlib.util.module_from_spec(s);s.loader.exec_module(m);return m
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
builder=load('debug_builder',W/'scripts/build-service-runtime.py')
sealer=load('debug_sealer',B/'p9-integrated-checks.py')
def debug_seal(runtime,expected):
 files={}
 for path in sorted(runtime.rglob('*')):
  assert not path.is_symlink() and not (hasattr(path,'is_junction') and path.is_junction())
  if path.is_file(): files[path.relative_to(runtime).as_posix()]=sha(path)
 install=json.loads((runtime/'airk-install.json').read_text());metadata=json.loads((runtime/'airk-runtime.json').read_text());build=json.loads((runtime/'airk-build.json').read_text())
 assert set(files)==set(install['files'])|{'airk-install.json'}
 assert all(files[name]==value for name,value in install['files'].items())
 assert install['version']==metadata['version']=='1.8.8'
 assert build['source']==expected and build['profile']=='debug' and build['runtime']==metadata
 assert files['airk.exe']==metadata['airk_sha256'] and files['airk-embed.exe']==metadata['embed_sha256']
 return {'path':str(runtime),'files':files,'runtime':metadata,'build':build,'install_manifest_sha256':files['airk-install.json'],'verified_inventory':True}
sealer.seal_runtime=debug_seal
producer=json.loads((B/'post-p14-debug-runtime-r2.receipt.json').read_text())
assert producer['exit_code']==producer['actual_exit_code']==0 and producer['source_before']==producer['source_after']==EXPECTED
assert builder.source_identity()==EXPECTED
seal=sealer.seal_runtime(R,EXPECTED)
with (B/'post-p14-debug-runtime-r2-seal.json').open('x') as f:json.dump(seal,f,indent=2)
out=B/'post-p14-debug-process-batch-r6';out.mkdir(exist_ok=False)
record={'status':'running','compiled_native_source':EXPECTED,'runtime_is_reused_not_fresh_build':False,'runtime_seal_sha256':sha(B/'post-p14-debug-runtime-r2-seal.json'),'steps':[]}
def save(): (out/'summary.json').write_text(json.dumps(record,indent=2),encoding='utf-8')
save()
for scenario in ['source-only','deleted','returned','foreign-return']:
 assert builder.source_identity()==EXPECTED and sealer.seal_runtime(R,EXPECTED)==seal
 result=out/(scenario+'.json')
 cmd=[sys.executable,'-X','utf8','scripts/check-runtime-legacy-retirement.py','--previous-runtime',str(B.parent/'release-1.8.7-review/runtime'),'--runtime',str(R),'--fixture-parent',str(B/'p10-private-fixtures-619c272e8a644988b26bbe4334fc8c02'),'--scenario',scenario,'--output',str(result)]
 if scenario=='deleted':cmd+=['--host-parity-extension',str(W/'artifacts/post-p14-host-parity-extension-r2/stage'),'--host-parity-modules',str(W/'packages/vscode/out'),'--host-parity-node',shutil.which('node')]
 label='post-p14-debug-process-'+scenario+'-r6.log'
 started=time.monotonic()
 code=subprocess.run([sys.executable,'-X','utf8',str(B/'p11-local-command.py'),label,*cmd],cwd=W).returncode
 row={'scenario':scenario,'exit_code':code,'seconds':time.monotonic()-started,'log':label}
 if result.exists():row.update(result=str(result),result_sha256=sha(result),result_status=json.loads(result.read_text())['status'])
 record['steps'].append(row);save()
 assert builder.source_identity()==EXPECTED and sealer.seal_runtime(R,EXPECTED)==seal
record['status']='passed' if all(x['exit_code']==0 and x.get('result_status')=='passed' for x in record['steps']) else 'failed'
save();print(json.dumps(record));sys.exit(0 if record['status']=='passed' else 1)
