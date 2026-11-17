# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import datetime,hashlib,importlib.util,json,os,subprocess,sys,time
root=Path.cwd();a=root/'artifacts/optimizations';location=json.loads((a/'f06-authority-runtime-location.json').read_text());runtime=Path(location['runtime']);assert not runtime.exists()
spec=importlib.util.spec_from_file_location('f06_seal_builder',root/'scripts/build-service-runtime.py');builder=importlib.util.module_from_spec(spec);spec.loader.exec_module(builder)
identity=builder.source_identity();assert identity=={'sha256':'5c4333f298c502844abb36a8eedf996ed6aa70fbbc8791876c899845024f5466','files':708}
env=os.environ.copy();env.update(CARGO_BUILD_JOBS='4',CARGO_NET_OFFLINE='true',AIRK_BUNDLED_MODEL_DIR=str(root/'target/local-vector-model'),AIRK_BUNDLED_RUNTIME_DIR=str(root/'target/local-onnx-runtime'))
env['PATH']='C:/Users/tdotn/.cargo/bin'+os.pathsep+env.get('PATH','')
import shutil
assert Path(shutil.which('cargo',path=env['PATH'])).samefile('C:/Users/tdotn/.cargo/bin/cargo.exe')
command=[sys.executable,str(root/'scripts/build-service-runtime.py'),'--destination',str(runtime),'--profile','release']
record={'command':command,'source_before':identity,'builder_sha256':hashlib.sha256((root/'scripts/build-service-runtime.py').read_bytes()).hexdigest(),'environment':{k:env[k] for k in ['CARGO_BUILD_JOBS','CARGO_NET_OFFLINE','AIRK_BUNDLED_MODEL_DIR','AIRK_BUNDLED_RUNTIME_DIR']},'cargo_bin_prefix':'C:/Users/tdotn/.cargo/bin','started_utc':datetime.datetime.now(datetime.timezone.utc).isoformat()}
receipt=a/'f06-authority-runtime-build-execution.json';receipt.write_text(json.dumps(record,indent=2)+'\n');start=time.monotonic()
with (a/'f06-authority-runtime-build.log').open('w',encoding='utf-8') as log:result=subprocess.run(command,env=env,stdout=log,stderr=subprocess.STDOUT)
record.update(exit_code=result.returncode,elapsed_seconds=time.monotonic()-start,ended_utc=datetime.datetime.now(datetime.timezone.utc).isoformat(),source_after=builder.source_identity())
record['source_unchanged']=record['source_before']==record['source_after'];receipt.write_text(json.dumps(record,indent=2)+'\n');print(json.dumps(record),flush=True)
raise SystemExit(result.returncode or (0 if record['source_unchanged'] else 1))
