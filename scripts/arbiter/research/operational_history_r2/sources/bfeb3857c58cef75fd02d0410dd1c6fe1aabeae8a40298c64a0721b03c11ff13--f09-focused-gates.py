# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import datetime, json, os, subprocess, sys, time
root=Path.cwd();a=root/'artifacts/optimizations'
env=os.environ.copy();env.pop('AIRK_TEST_BINARY',None)
env.update(CARGO_BUILD_JOBS='4',CARGO_NET_OFFLINE='true',AIRK_BUNDLED_MODEL_DIR=str(root/'target/local-vector-model'),AIRK_BUNDLED_RUNTIME_DIR=str(root/'target/local-onnx-runtime'))
cargo='C:/Users/tdotn/.cargo/bin/cargo.exe'
base=[cargo,'test','-p','airk-cli','--all-features','--locked','--offline']
commands=[('command-final',base+['--lib','connection_observation_tests','--','--nocapture'],root),
          ('parser',base+['--bin','airk','exact_source_scope_options','--','--nocapture'],root),
          ('scope-final',base+['--lib','exact_scope_metadata_tests','--','--nocapture'],root),
          ('e-compile',['npm.cmd','run','compile'],root/'packages/vscode'),
          ('e-host',['node','--test','out/test/serviceBootstrap.test.js','out/test/serviceWorkspace.test.js',
                     'out/test/serviceExtension.test.js','out/test/explorerAssistanceHost.test.js'],root/'packages/vscode')]
if sys.argv[1:] == ['e-corrected']:
    commands=[(name+'-corrected',command,cwd) for name,command,cwd in commands if name.startswith('e-')]
elif sys.argv[1:] == ['e-final']:
    commands=[(name+'-final',command,cwd) for name,command,cwd in commands if name.startswith('e-')]
elif sys.argv[1:] == ['e-recovery']:
    commands=[(name+'-recovery',command,cwd) for name,command,cwd in commands if name.startswith('e-')]
for name,command,cwd in commands:
    start=time.monotonic();record={'command':command,'cwd':str(cwd),'started_utc':datetime.datetime.now(datetime.timezone.utc).isoformat()}
    with (a/f'f09-focused-{name}.log').open('w',encoding='utf-8') as output:
        result=subprocess.run(command,cwd=cwd,env=env,stdout=output,stderr=subprocess.STDOUT)
    record.update(exit_code=result.returncode,elapsed_seconds=time.monotonic()-start,ended_utc=datetime.datetime.now(datetime.timezone.utc).isoformat())
    (a/f'f09-focused-{name}-execution.json').write_text(json.dumps(record,indent=2)+'\n')
    print(json.dumps(record),flush=True)
    if result.returncode:raise SystemExit(result.returncode)
