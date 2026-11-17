# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import hashlib,importlib.util,json,os,shutil,subprocess,sys,time
B=Path(__file__).resolve().parent;W=B/'p10-worktree';R=B/'post-p14-debug-runtime-r1'
EXPECTED={'sha256':'aef56a8252cde5016dde1a69221d5bcc700f77581e0cbe6a904eaac9f882a72a','files':968}
def load(name,p):
 s=importlib.util.spec_from_file_location(name,p);m=importlib.util.module_from_spec(s);s.loader.exec_module(m);return m
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
builder=load('debug_builder',W/'scripts/build-service-runtime.py')
sealer=load('debug_sealer',B/'p9-integrated-checks.py')
producer=json.loads((B/'post-p14-debug-runtime-r1.receipt.json').read_text())
assert producer['exit_code']==producer['actual_exit_code']==0 and producer['source_before']==producer['source_after']==EXPECTED
assert builder.source_identity()==EXPECTED
seal=sealer.seal_runtime(R,EXPECTED)
with (B/'post-p14-debug-runtime-r1-seal.json').open('x') as f:json.dump(seal,f,indent=2)
out=B/'post-p14-debug-process-batch-r1';out.mkdir(exist_ok=False)
record={'status':'running','compiled_native_source':EXPECTED,'runtime_seal_sha256':sha(B/'post-p14-debug-runtime-r1-seal.json'),'steps':[]}
def save(): (out/'summary.json').write_text(json.dumps(record,indent=2),encoding='utf-8')
save()
for scenario in ['source-only','deleted','returned','foreign-return']:
 assert builder.source_identity()==EXPECTED and sealer.seal_runtime(R,EXPECTED)==seal
 result=out/(scenario+'.json')
 cmd=[sys.executable,'-X','utf8','scripts/check-runtime-legacy-retirement.py','--previous-runtime',str(B.parent/'release-1.8.7-review/runtime'),'--runtime',str(R),'--fixture-parent',str(B/'p10-private-fixtures-619c272e8a644988b26bbe4334fc8c02'),'--scenario',scenario,'--output',str(result)]
 if scenario=='deleted':cmd+=['--host-parity-extension',str(W/'artifacts/post-p14-host-parity-extension-r1/stage'),'--host-parity-modules',str(W/'packages/vscode/out'),'--host-parity-node',shutil.which('node')]
 label='post-p14-debug-process-'+scenario+'-r1.log'
 started=time.monotonic()
 code=subprocess.run([sys.executable,'-X','utf8',str(B/'p11-local-command.py'),label,*cmd],cwd=W).returncode
 row={'scenario':scenario,'exit_code':code,'seconds':time.monotonic()-started,'log':label}
 if result.exists():row.update(result=str(result),result_sha256=sha(result),result_status=json.loads(result.read_text())['status'])
 record['steps'].append(row);save()
 assert builder.source_identity()==EXPECTED and sealer.seal_runtime(R,EXPECTED)==seal
record['status']='passed' if all(x['exit_code']==0 and x.get('result_status')=='passed' for x in record['steps']) else 'failed'
save();print(json.dumps(record));sys.exit(0 if record['status']=='passed' else 1)
