# SPDX-License-Identifier: Apache-2.0
import datetime,json,os,subprocess,time
from pathlib import Path
root=Path.cwd(); artifacts=root/'artifacts/optimizations'
env=os.environ.copy();env.update(CARGO_BUILD_JOBS='4',CARGO_NET_OFFLINE='true',AIRK_BUNDLED_MODEL_DIR=str(root/'target/local-vector-model'),AIRK_BUNDLED_RUNTIME_DIR=str(root/'target/local-onnx-runtime'))
command=['C:/Users/tdotn/.cargo/bin/cargo.exe','test','-p','airk-cli','-p','airk-mcp','--all-features','--locked','--offline']
record={'command':command,'environment':{k:env[k] for k in ['CARGO_BUILD_JOBS','CARGO_NET_OFFLINE','AIRK_BUNDLED_MODEL_DIR','AIRK_BUNDLED_RUNTIME_DIR']},'baseline':'f9969f8b64c39cafa707971a6330bba72067ab3d','checkpoint':'7273cf3041bc1076636326b3b5bb5ee90122577e','gate':'N-CLI, not full N','started_utc':datetime.datetime.now(datetime.timezone.utc).isoformat()}
(artifacts/'f06-n-cli-execution.json').write_text(json.dumps(record,indent=2)+'\n')
start=time.monotonic()
with (artifacts/'f06-n-cli.log').open('w',encoding='utf-8') as output:
 result=subprocess.run(command,env=env,stdout=output,stderr=subprocess.STDOUT)
record.update(exit_code=result.returncode,elapsed_seconds=time.monotonic()-start,ended_utc=datetime.datetime.now(datetime.timezone.utc).isoformat())
(artifacts/'f06-n-cli-execution.json').write_text(json.dumps(record,indent=2)+'\n')
print(json.dumps(record));raise SystemExit(result.returncode)
