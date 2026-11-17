# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import datetime,importlib.util,json,os,subprocess,time,shutil
root=Path.cwd();a=root/'artifacts/optimizations'
spec=importlib.util.spec_from_file_location('f12_builder',root/'scripts/build-service-runtime.py');builder=importlib.util.module_from_spec(spec);spec.loader.exec_module(builder)
env=os.environ.copy();env.update(CARGO_BUILD_JOBS='4',CARGO_NET_OFFLINE='true',AIRK_BUNDLED_MODEL_DIR=str(root/'target/local-vector-model'),AIRK_BUNDLED_RUNTIME_DIR=str(root/'target/local-onnx-runtime'))
expected={'sha256':'92e56df7afd7e952b3769a7768cf640c96c471a9f89cfca30453f0ae3f1dfe24','files':710}
commands=[('fmt',['fmt','--all','--check']),('clippy',['clippy','--workspace','--all-targets','--all-features','--locked','--offline','--','-D','warnings']),('full-n',['test','--workspace','--all-features','--locked','--offline'])]
for kind,args in commands:
 before=builder.source_identity()
 if before!=expected:raise SystemExit('source changed before gate')
 command=['C:/Users/tdotn/.cargo/bin/cargo.exe',*args]
 record={'command':command,'environment':{k:env[k] for k in ['CARGO_BUILD_JOBS','CARGO_NET_OFFLINE','AIRK_BUNDLED_MODEL_DIR','AIRK_BUNDLED_RUNTIME_DIR']},'source_before':before,'gate':kind,'started_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'disk_free_before':{d:shutil.disk_usage(d).free for d in ['C:/','D:/']}}
 receipt=a/f'f12a-corrected-{kind}-execution.json'
 with receipt.open('x') as f:json.dump(record,f,indent=2)
 start=time.monotonic()
 with (a/f'f12a-corrected-{kind}.log').open('x',encoding='utf-8') as output:result=subprocess.run(command,env=env,stdout=output,stderr=subprocess.STDOUT)
 record.update(exit_code=result.returncode,elapsed_seconds=time.monotonic()-start,ended_utc=datetime.datetime.now(datetime.timezone.utc).isoformat(),source_after=builder.source_identity(),disk_free_after={d:shutil.disk_usage(d).free for d in ['C:/','D:/']})
 record['source_unchanged']=record['source_before']==record['source_after'];receipt.write_text(json.dumps(record,indent=2)+'\n')
 print(json.dumps({'gate':kind,'exit_code':result.returncode,'source_unchanged':record['source_unchanged'],'elapsed_seconds':record['elapsed_seconds'],'disk_free':record['disk_free_after']}),flush=True)
 if result.returncode or not record['source_unchanged']:raise SystemExit(result.returncode or 1)
