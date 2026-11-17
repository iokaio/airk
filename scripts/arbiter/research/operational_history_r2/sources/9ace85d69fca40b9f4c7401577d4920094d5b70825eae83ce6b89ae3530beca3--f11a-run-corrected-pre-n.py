# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import datetime,importlib.util,json,os,subprocess,time
root=Path.cwd();a=root/'artifacts/optimizations';env=os.environ.copy();env.update(CARGO_BUILD_JOBS='4',CARGO_NET_OFFLINE='true',AIRK_BUNDLED_MODEL_DIR=str(root/'target/local-vector-model'),AIRK_BUNDLED_RUNTIME_DIR=str(root/'target/local-onnx-runtime'))
spec=importlib.util.spec_from_file_location('f11_pren_builder',root/'scripts/build-service-runtime.py');b=importlib.util.module_from_spec(spec);spec.loader.exec_module(b);seal=b.source_identity();(a/'f11a-corrected-pre-n-source.json').write_text(json.dumps(seal,indent=2)+'\n')
cargo='C:/Users/tdotn/.cargo/bin/cargo.exe';commands=[('fmt',[cargo,'fmt','--all','--check']),('clippy',[cargo,'clippy','--workspace','--all-targets','--all-features','--locked','--offline','--','-D','warnings']),('standalone',[cargo,'check','-p','airk-store','--lib','--no-default-features','--locked','--offline'])]
for kind,command in commands:
 assert b.source_identity()==seal;record={'command':command,'source_before':seal,'started_utc':datetime.datetime.now(datetime.timezone.utc).isoformat()};start=time.monotonic()
 with (a/f'f11a-corrected-pre-n-{kind}.log').open('x') as out:r=subprocess.run(command,env=env,stdout=out,stderr=subprocess.STDOUT)
 record.update(exit_code=r.returncode,elapsed_seconds=time.monotonic()-start,source_after=b.source_identity(),ended_utc=datetime.datetime.now(datetime.timezone.utc).isoformat());record['source_unchanged']=record['source_before']==record['source_after'];(a/f'f11a-corrected-pre-n-{kind}-execution.json').write_text(json.dumps(record,indent=2)+'\n');print(json.dumps({'gate':kind,'exit_code':r.returncode,'elapsed_seconds':record['elapsed_seconds'],'source':seal}),flush=True)
 if r.returncode or not record['source_unchanged']:raise SystemExit(r.returncode or 1)
