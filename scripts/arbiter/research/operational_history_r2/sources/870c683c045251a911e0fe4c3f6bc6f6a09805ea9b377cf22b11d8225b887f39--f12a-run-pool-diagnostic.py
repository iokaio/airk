# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import datetime,hashlib,importlib.util,json,os,subprocess,sys,time
root=Path.cwd();a=root/'artifacts/optimizations';loc=json.loads((a/'f12a-runtime-location.json').read_text());task=Path(loc['task']);runtime=Path(loc['runtime']);sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
expected='8b8aa51059934651e10a13aa9005ab91a9bae6750d214719750aadd535132af9';approved='e4f99021403f673e689930fbb9edcec9829924dcab33c0badc7c5799d9fca2a6'
source=a/'f12a-pool-diagnostic.py';assert sha(source)==expected
script=task/'scripts/f12a-pool-diagnostic.py';script.write_bytes(source.read_bytes());assert sha(script)==expected
assert sha(script.with_name('check-service-clients.py'))=='a4588c7202f3ba3aee27057bf0aa9e15dfeb627815ee61e3c8df249491d6984b'
assert sha(runtime/'airk-install.json')==approved
output=task/'reports/pool-diagnostic-01';assert not output.exists();output.mkdir()
for path in [task,task/'scripts',task/'reports',output,script]:
 assert not path.is_symlink() and not getattr(path.lstat(),'st_file_attributes',0)&0x400
assert output.resolve(strict=True).parent.samefile(task/'reports') and output.resolve(strict=True).is_relative_to(task.resolve(strict=True))
spec=importlib.util.spec_from_file_location('f12_diag_builder',root/'scripts/build-service-runtime.py');builder=importlib.util.module_from_spec(spec);spec.loader.exec_module(builder)
before=builder.source_identity();assert before=={'sha256':'92e56df7afd7e952b3769a7768cf640c96c471a9f89cfca30453f0ae3f1dfe24','files':710}
env=os.environ.copy();env.update(TEMP=str(task/'fixtures'),TMP=str(task/'fixtures'))
command=[sys.executable,str(script),'--runtime',str(runtime),'--diagnostic-dir',str(output)]
record={'command':command,'source_before':before,'approved_manifest':approved,'script_sha256':expected,'output':str(output),'diagnostic_only':True,'started_utc':datetime.datetime.now(datetime.timezone.utc).isoformat()}
receipt=a/'f12a-pool-diagnostic-execution.json'
with receipt.open('x') as f:json.dump(record,f,indent=2)
start=time.monotonic()
with (a/'f12a-pool-diagnostic.log').open('x',encoding='utf-8') as out:result=subprocess.run(command,env=env,stdout=out,stderr=subprocess.STDOUT)
record.update(exit_code=result.returncode,elapsed_seconds=time.monotonic()-start,source_after=builder.source_identity(),ended_utc=datetime.datetime.now(datetime.timezone.utc).isoformat(),manifest_after=sha(runtime/'airk-install.json'))
record['source_unchanged']=record['source_before']==record['source_after'];receipt.write_text(json.dumps(record,indent=2)+'\n');print(json.dumps(record),flush=True)
raise SystemExit(result.returncode)
