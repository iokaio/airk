# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import datetime,json,os,subprocess,time
root=Path.cwd();a=root/'artifacts/optimizations';env=os.environ.copy()
env.update(CARGO_BUILD_JOBS='4',CARGO_NET_OFFLINE='true',AIRK_BUNDLED_MODEL_DIR=str(root/'target/local-vector-model'),AIRK_BUNDLED_RUNTIME_DIR=str(root/'target/local-onnx-runtime'))
command=['C:/Users/tdotn/.cargo/bin/cargo.exe','test','-p','airk-cli','--lib','--all-features','--locked','--offline','embedding_pool_client','--','--nocapture']
record={'command':command,'started_utc':datetime.datetime.now(datetime.timezone.utc).isoformat()};start=time.monotonic()
with (a/'f12a-focused-embedding-initial.log').open('x',encoding='utf-8') as output:
    result=subprocess.run(command,env=env,stdout=output,stderr=subprocess.STDOUT)
record.update(exit_code=result.returncode,elapsed_seconds=time.monotonic()-start,ended_utc=datetime.datetime.now(datetime.timezone.utc).isoformat())
(a/'f12a-focused-embedding-initial-execution.json').write_text(json.dumps(record,indent=2)+'\n');print(json.dumps(record),flush=True)
raise SystemExit(result.returncode)
