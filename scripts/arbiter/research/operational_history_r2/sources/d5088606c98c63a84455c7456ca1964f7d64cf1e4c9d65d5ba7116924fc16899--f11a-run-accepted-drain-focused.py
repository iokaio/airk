# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import datetime,importlib.util,json,os,subprocess,time
root=Path.cwd();a=root/'artifacts/optimizations';env=os.environ.copy();env.update(CARGO_BUILD_JOBS='4',CARGO_NET_OFFLINE='true',AIRK_BUNDLED_MODEL_DIR=str(root/'target/local-vector-model'),AIRK_BUNDLED_RUNTIME_DIR=str(root/'target/local-onnx-runtime'),GIT_CONFIG_COUNT='2',GIT_CONFIG_KEY_0='safe.directory',GIT_CONFIG_VALUE_0=root.as_posix(),GIT_CONFIG_KEY_1='safe.directory',GIT_CONFIG_VALUE_1=root.as_posix()+'/*')
spec=importlib.util.spec_from_file_location('f11a_builder',root/'scripts/build-service-runtime.py');builder=importlib.util.module_from_spec(spec);spec.loader.exec_module(builder);seal=builder.source_identity();(a/'f11a-accepted-drain-focused-source.json').write_text(json.dumps(seal,indent=2)+'\n')
cargo='C:/Users/tdotn/.cargo/bin/cargo.exe';base=[cargo,'test','--locked','--offline','--all-features'];store=base+['-p','airk-store','--lib'];cli=base+['-p','airk-cli']
commands=[('native-control',cli+['--test','integrity_restart','native_project_worker_accepted_verify_cancel_and_closed_input_retain_owned_drain','--','--exact','--nocapture'])]
for kind,command in commands:
 assert builder.source_identity()==seal
 record={'command':command,'source_before':seal,'environment':{k:env[k] for k in ['CARGO_BUILD_JOBS','CARGO_NET_OFFLINE','AIRK_BUNDLED_MODEL_DIR','AIRK_BUNDLED_RUNTIME_DIR']},'started_utc':datetime.datetime.now(datetime.timezone.utc).isoformat()};start=time.monotonic()
 with (a/f'f11a-accepted-drain-focused-{kind}.log').open('x') as out:r=subprocess.run(command,env=env,stdout=out,stderr=subprocess.STDOUT)
 record.update(exit_code=r.returncode,elapsed_seconds=time.monotonic()-start,source_after=builder.source_identity(),ended_utc=datetime.datetime.now(datetime.timezone.utc).isoformat());record['source_unchanged']=record['source_before']==record['source_after'];(a/f'f11a-accepted-drain-focused-{kind}-execution.json').write_text(json.dumps(record,indent=2)+'\n');print(json.dumps({'gate':kind,'exit_code':r.returncode,'elapsed_seconds':record['elapsed_seconds'],'source':seal,'source_unchanged':record['source_unchanged']}),flush=True)
 if r.returncode or not record['source_unchanged']:raise SystemExit(r.returncode or 1)
