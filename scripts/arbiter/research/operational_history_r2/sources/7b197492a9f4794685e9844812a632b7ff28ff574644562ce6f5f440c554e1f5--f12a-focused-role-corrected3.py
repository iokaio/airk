# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import datetime,importlib.util,json,os,subprocess,time
root=Path.cwd();a=root/'artifacts/optimizations';env=os.environ.copy()
env.update(CARGO_BUILD_JOBS='4',CARGO_NET_OFFLINE='true',AIRK_BUNDLED_MODEL_DIR=str(root/'target/local-vector-model'),AIRK_BUNDLED_RUNTIME_DIR=str(root/'target/local-onnx-runtime'))
spec=importlib.util.spec_from_file_location('f12a_focused_builder',root/'scripts/build-service-runtime.py');builder=importlib.util.module_from_spec(spec);spec.loader.exec_module(builder)
base=['C:/Users/tdotn/.cargo/bin/cargo.exe','test','-p','airk-cli','--all-features','--locked','--offline']
commands=[('derived-role-corrected3',base+['--test','helper_authority_roles','--','--nocapture']),
          ('worker-control',base+['--test','service_runtime','private_worker_channel_renews_only_the_live_scoped_generation','--','--nocapture'])]
for kind,command in commands:
    record={'command':command,'source_before':builder.source_identity(),'started_utc':datetime.datetime.now(datetime.timezone.utc).isoformat()};start=time.monotonic()
    with (a/f'f12a-focused-{kind}.log').open('x',encoding='utf-8') as output:
        result=subprocess.run(command,env=env,stdout=output,stderr=subprocess.STDOUT)
    record.update(exit_code=result.returncode,elapsed_seconds=time.monotonic()-start,ended_utc=datetime.datetime.now(datetime.timezone.utc).isoformat(),source_after=builder.source_identity())
    record['source_unchanged']=record['source_before']==record['source_after'];(a/f'f12a-focused-{kind}-execution.json').write_text(json.dumps(record,indent=2)+'\n')
    print(json.dumps({'gate':kind,'exit':result.returncode,'elapsed':record['elapsed_seconds'],'source_unchanged':record['source_unchanged']}),flush=True)
    if result.returncode or not record['source_unchanged']:raise SystemExit(result.returncode or 1)
