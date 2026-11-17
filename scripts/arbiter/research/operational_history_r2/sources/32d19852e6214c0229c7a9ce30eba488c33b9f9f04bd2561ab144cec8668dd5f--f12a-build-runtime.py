# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import datetime,hashlib,importlib.util,json,os,shutil,subprocess,sys,time
root=Path.cwd();a=root/'artifacts/optimizations';location=json.loads((a/'f12a-runtime-location.json').read_text());runtime=Path(location['runtime'])
assert not runtime.exists()
spec=importlib.util.spec_from_file_location('f12a_sealed_builder',root/'scripts/build-service-runtime.py')
builder=importlib.util.module_from_spec(spec);spec.loader.exec_module(builder)
identity=builder.source_identity()
assert identity=={'sha256':'92e56df7afd7e952b3769a7768cf640c96c471a9f89cfca30453f0ae3f1dfe24','files':710}
env=os.environ.copy();env.update(CARGO_BUILD_JOBS='4',CARGO_NET_OFFLINE='true',AIRK_BUNDLED_MODEL_DIR=str(root/'target/local-vector-model'),AIRK_BUNDLED_RUNTIME_DIR=str(root/'target/local-onnx-runtime'))
env['PATH']='C:/Users/tdotn/.cargo/bin'+os.pathsep+env.get('PATH','')
assert Path(shutil.which('cargo',path=env['PATH'])).samefile('C:/Users/tdotn/.cargo/bin/cargo.exe')
for item in location['scripts']:
    assert hashlib.sha256((Path(location['task'])/'scripts'/item['name']).read_bytes()).hexdigest()==item['sha256']
command=[sys.executable,str(root/'scripts/build-service-runtime.py'),'--destination',str(runtime),'--profile','release']
record={'command':command,'task':location['task'],'runtime':str(runtime),'scripts':location['scripts'],'source_before':identity,
        'builder_sha256':hashlib.sha256((root/'scripts/build-service-runtime.py').read_bytes()).hexdigest(),
        'environment':{key:env[key] for key in ['CARGO_BUILD_JOBS','CARGO_NET_OFFLINE','AIRK_BUNDLED_MODEL_DIR','AIRK_BUNDLED_RUNTIME_DIR']},
        'cargo_bin_prefix':'C:/Users/tdotn/.cargo/bin','started_utc':datetime.datetime.now(datetime.timezone.utc).isoformat()}
receipt=a/'f12a-runtime-build-execution.json'
with receipt.open('x',encoding='utf-8') as stream:json.dump(record,stream,indent=2)
start=time.monotonic()
with (a/'f12a-runtime-build.log').open('x',encoding='utf-8') as output:result=subprocess.run(command,env=env,stdout=output,stderr=subprocess.STDOUT)
record.update(exit_code=result.returncode,elapsed_seconds=time.monotonic()-start,ended_utc=datetime.datetime.now(datetime.timezone.utc).isoformat(),source_after=builder.source_identity())
record['source_unchanged']=record['source_before']==record['source_after'];receipt.write_text(json.dumps(record,indent=2)+'\n')
print(json.dumps(record),flush=True)
raise SystemExit(result.returncode or (0 if record['source_unchanged'] else 1))
