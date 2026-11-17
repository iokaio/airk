# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import datetime,importlib.util,json,os,subprocess,time
root=Path.cwd();artifacts=root/'artifacts/optimizations'
spec=importlib.util.spec_from_file_location('f06_final_builder',root/'scripts/build-service-runtime.py');builder=importlib.util.module_from_spec(spec);spec.loader.exec_module(builder)
env=os.environ.copy();env.update(CARGO_BUILD_JOBS='4',CARGO_NET_OFFLINE='true',AIRK_BUNDLED_MODEL_DIR=str(root/'target/local-vector-model'),AIRK_BUNDLED_RUNTIME_DIR=str(root/'target/local-onnx-runtime'))
commands=[('clippy',['clippy','--workspace','--all-targets','--all-features','--locked','--offline','--','-D','warnings']),('n-cli',['test','-p','airk-cli','-p','airk-mcp','--all-features','--locked','--offline'])]
for kind,arguments in commands:
 command=['C:/Users/tdotn/.cargo/bin/cargo.exe',*arguments]
 record={'command':command,'environment':{key:env[key] for key in ['CARGO_BUILD_JOBS','CARGO_NET_OFFLINE','AIRK_BUNDLED_MODEL_DIR','AIRK_BUNDLED_RUNTIME_DIR']},'source_before':builder.source_identity(),'baseline':'f9969f8b64c39cafa707971a6330bba72067ab3d','gate':kind,'started_utc':datetime.datetime.now(datetime.timezone.utc).isoformat()}
 receipt=artifacts/f'f06-{kind}-options-corrected-execution.json';receipt.write_text(json.dumps(record,indent=2)+'\n')
 start=time.monotonic()
 with (artifacts/f'f06-{kind}-options-corrected.log').open('w',encoding='utf-8') as output:result=subprocess.run(command,env=env,stdout=output,stderr=subprocess.STDOUT)
 record.update(exit_code=result.returncode,elapsed_seconds=time.monotonic()-start,ended_utc=datetime.datetime.now(datetime.timezone.utc).isoformat(),source_after=builder.source_identity())
 record['source_unchanged']=record['source_before']==record['source_after'];receipt.write_text(json.dumps(record,indent=2)+'\n')
 print(json.dumps({'gate':kind,'exit_code':result.returncode,'source_unchanged':record['source_unchanged'],'elapsed_seconds':record['elapsed_seconds']}),flush=True)
 if result.returncode or not record['source_unchanged']:raise SystemExit(result.returncode or 1)
