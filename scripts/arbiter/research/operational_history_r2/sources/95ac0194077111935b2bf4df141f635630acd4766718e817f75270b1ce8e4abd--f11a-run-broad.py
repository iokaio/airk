# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import datetime,importlib.util,json,os,subprocess,sys,time,shutil
root=Path.cwd();a=root/'artifacts/optimizations';env=os.environ.copy();env.update(CARGO_BUILD_JOBS='4',CARGO_NET_OFFLINE='true',AIRK_BUNDLED_MODEL_DIR=str(root/'target/local-vector-model'),AIRK_BUNDLED_RUNTIME_DIR=str(root/'target/local-onnx-runtime'),GIT_CONFIG_COUNT='2',GIT_CONFIG_KEY_0='safe.directory',GIT_CONFIG_VALUE_0=root.as_posix(),GIT_CONFIG_KEY_1='safe.directory',GIT_CONFIG_VALUE_1=root.as_posix()+'/*')
spec=importlib.util.spec_from_file_location('f11_broad_builder',root/'scripts/build-service-runtime.py');b=importlib.util.module_from_spec(spec);spec.loader.exec_module(b);seal={'sha256':'927128a5eb90a71fb74ea53dac53a56b787ba3f86792d96ca3cfeb5d938344c5','files':718}
commands=[('full-n',['C:/Users/tdotn/.cargo/bin/cargo.exe','test','--workspace','--all-features','--locked','--offline']),('full-python',[sys.executable,'-m','unittest','discover','-s','scripts/tests','-v'])]
for kind,command in commands:
 assert b.source_identity()==seal
 record={'command':command,'source_before':seal,'environment':{k:env[k] for k in ['CARGO_BUILD_JOBS','CARGO_NET_OFFLINE','AIRK_BUNDLED_MODEL_DIR','AIRK_BUNDLED_RUNTIME_DIR','GIT_CONFIG_COUNT','GIT_CONFIG_KEY_0','GIT_CONFIG_VALUE_0','GIT_CONFIG_KEY_1','GIT_CONFIG_VALUE_1']},'started_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'disk_before':{d:shutil.disk_usage(d).free for d in ['C:/','D:/']}};start=time.monotonic();receipt=a/f'f11a-{kind}-execution.json'
 with receipt.open('x') as f:json.dump(record,f,indent=2)
 with (a/f'f11a-{kind}.log').open('x') as out:r=subprocess.run(command,env=env,stdout=out,stderr=subprocess.STDOUT)
 record.update(exit_code=r.returncode,elapsed_seconds=time.monotonic()-start,source_after=b.source_identity(),ended_utc=datetime.datetime.now(datetime.timezone.utc).isoformat(),disk_after={d:shutil.disk_usage(d).free for d in ['C:/','D:/']});record['source_unchanged']=record['source_before']==record['source_after'];receipt.write_text(json.dumps(record,indent=2)+'\n');print(json.dumps({'gate':kind,'exit_code':r.returncode,'elapsed_seconds':record['elapsed_seconds'],'source_unchanged':record['source_unchanged']}),flush=True)
 if r.returncode or not record['source_unchanged']:raise SystemExit(r.returncode or 1)
