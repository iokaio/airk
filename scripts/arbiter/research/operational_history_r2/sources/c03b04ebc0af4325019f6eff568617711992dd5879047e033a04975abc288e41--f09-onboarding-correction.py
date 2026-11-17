# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import datetime,json,os,subprocess,sys,time
root=Path.cwd();a=root/'artifacts/optimizations';env=os.environ.copy()
env.pop('AIRK_TEST_BINARY',None);env.pop('AIRK_TEST_F09_QUALIFICATION',None)
prefix='f09-final-ts-focused' if sys.argv[1:]==['final'] else 'f09-final-ts-focused-corrected' if sys.argv[1:]==['final-corrected'] else 'f09-onboarding-correction'
for name,command in [('compile',['npm.cmd','run','compile']),('tests',['node','--test','out/test/serviceOnboarding.test.js','out/test/serviceExtension.test.js','out/test/serviceWorkspace.test.js','out/test/serviceBootstrap.test.js'])]:
    record={'command':command,'started_utc':datetime.datetime.now(datetime.timezone.utc).isoformat()};start=time.monotonic()
    with (a/f'{prefix}-{name}.log').open('x',encoding='utf-8') as output:
        result=subprocess.run(command,cwd=root/'packages/vscode',env=env,stdout=output,stderr=subprocess.STDOUT)
    record.update(exit_code=result.returncode,elapsed_seconds=time.monotonic()-start)
    (a/f'{prefix}-{name}-execution.json').write_text(json.dumps(record,indent=2)+'\n')
    print(json.dumps(record),flush=True)
    if result.returncode:raise SystemExit(result.returncode)
