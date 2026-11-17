# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import datetime, importlib.util, json, os, subprocess, sys, time

root=Path.cwd();a=root/'artifacts/optimizations'
prefix='f09-corrected' if sys.argv[1:]==['corrected'] else 'f09'
spec=importlib.util.spec_from_file_location('f09_gate_builder',root/'scripts/build-service-runtime.py')
builder=importlib.util.module_from_spec(spec);spec.loader.exec_module(builder)
env=os.environ.copy();env.pop('AIRK_TEST_BINARY',None);env.pop('AIRK_WRITE_CLI_REFERENCE',None)
env.update(CARGO_BUILD_JOBS='4',CARGO_NET_OFFLINE='true',AIRK_BUNDLED_MODEL_DIR=str(root/'target/local-vector-model'),AIRK_BUNDLED_RUNTIME_DIR=str(root/'target/local-onnx-runtime'))
cargo='C:/Users/tdotn/.cargo/bin/cargo.exe'
commands=[('fmt',[cargo,'fmt','--all','--check']),
          ('clippy',[cargo,'clippy','--workspace','--all-targets','--all-features','--locked','--offline','--','-D','warnings']),
          ('full-n',[cargo,'test','--workspace','--all-features','--locked','--offline']),
          ('full-e',['npm.cmd','test'])]
for kind,command in commands:
    record={'command':command,'environment':{key:env[key] for key in ['CARGO_BUILD_JOBS','CARGO_NET_OFFLINE','AIRK_BUNDLED_MODEL_DIR','AIRK_BUNDLED_RUNTIME_DIR']},
            'AIRK_TEST_BINARY':'unset','AIRK_WRITE_CLI_REFERENCE':'unset','source_before':builder.source_identity(),
            'baseline':'1bafad3aa169f5326c350eb96524f17f8ca6ba58','gate':kind,'started_utc':datetime.datetime.now(datetime.timezone.utc).isoformat()}
    receipt=a/f'{prefix}-{kind}-execution.json'
    with receipt.open('x') as stream:json.dump(record,stream,indent=2)
    start=time.monotonic()
    with (a/f'{prefix}-{kind}.log').open('x',encoding='utf-8') as output:
        result=subprocess.run(command,cwd=root/'packages/vscode' if kind=='full-e' else root,env=env,stdout=output,stderr=subprocess.STDOUT)
    record.update(exit_code=result.returncode,elapsed_seconds=time.monotonic()-start,ended_utc=datetime.datetime.now(datetime.timezone.utc).isoformat(),source_after=builder.source_identity())
    record['source_unchanged']=record['source_before']==record['source_after']
    receipt.write_text(json.dumps(record,indent=2)+'\n')
    print(json.dumps({'gate':kind,'exit_code':result.returncode,'elapsed_seconds':record['elapsed_seconds'],'source_unchanged':record['source_unchanged']}),flush=True)
    if result.returncode or not record['source_unchanged']:raise SystemExit(result.returncode or 1)
