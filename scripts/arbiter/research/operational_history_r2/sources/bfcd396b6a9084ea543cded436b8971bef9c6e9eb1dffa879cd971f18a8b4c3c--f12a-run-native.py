# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import datetime,hashlib,importlib.util,json,os,subprocess,sys,time
root=Path.cwd();a=root/'artifacts/optimizations';location=json.loads((a/'f12a-runtime-location.json').read_text())
task,runtime=Path(location['task']),Path(location['runtime']);kind,approved=sys.argv[1:]
assert kind in ['smoke','assistance','install','managed','console','pool','console-client','scope','semantic','clients','pool-corrected']
assert hashlib.sha256((runtime/'airk-install.json').read_bytes()).hexdigest()==approved
for item in location['scripts']:
    assert hashlib.sha256((task/'scripts'/item['name']).read_bytes()).hexdigest()==item['sha256']
spec=importlib.util.spec_from_file_location('f12a_native_builder',root/'scripts/build-service-runtime.py')
builder=importlib.util.module_from_spec(spec);spec.loader.exec_module(builder)
source_before=builder.source_identity()
assert source_before==json.loads((runtime/'airk-build.json').read_text())['source']
assert source_before=={'sha256':'92e56df7afd7e952b3769a7768cf640c96c471a9f89cfca30453f0ae3f1dfe24','files':710}
env=os.environ.copy();env.pop('AIRK_TEST_BINARY',None);env.pop('AIRK_TEST_F09_QUALIFICATION',None);env.pop('PYTHONOPTIMIZE',None)
env.update(TEMP=str(task/'fixtures'),TMP=str(task/'fixtures'))
cwd=root
if kind in ['smoke','assistance']:
    env['AIRK_TEST_BINARY']=str(runtime/'airk.exe');env['AIRK_MCP_RESPONSE_MODE']='terse'
    selected={'smoke':'nativeSmoke','assistance':'explorerAssistanceNative'}[kind]
    command=['node','--test',f'out/test/{selected}.test.js'];cwd=root/'packages/vscode'
elif kind in ['install','managed','console','pool','pool-corrected']:
    selected={'install':'check-service-install.py','managed':'check-managed-runtime.py','console':'check-service-console.py','pool':'check-service-pool.py','pool-corrected':'check-service-pool-f12a-final.py'}[kind]
    if kind=='pool-corrected':assert hashlib.sha256((task/'scripts'/selected).read_bytes()).hexdigest()=='e4e9034b0d0b5c42ccceaee7e175e2d665f704ccf853a142ac64ab99633a08f5'
    command=[sys.executable,str(task/'scripts'/selected),'--runtime',str(runtime)]
elif kind=='console-client':
    copy=json.loads((a/'f12a-console-client-out-copy.json').read_text())
    output=Path(copy['destination'])
    actual=set()
    for file in output.rglob('*'):
        metadata=file.lstat()
        assert not file.is_symlink() and not getattr(metadata,'st_file_attributes',0)&0x400
        if file.is_file():
            name=file.relative_to(output).as_posix();actual.add(name)
            expected=copy['files_inventory'][name]
            assert metadata.st_size==expected['bytes'] and hashlib.sha256(file.read_bytes()).hexdigest()==expected['sha256']
    assert actual==set(copy['files_inventory'])
    assert all(name.startswith('node:') for name in copy['bare_requires'])
    driver=task/'scripts/check-service-console-client.cjs'
    command=['node',str(driver),'--runtime',str(runtime)]
elif kind in ['scope','semantic']:
    driver=task/'scripts'/('check-source-scope-original-identity.py' if kind=='scope' else 'semantic-restart-qualification.py')
    command=[sys.executable,str(driver),'--runtime',str(runtime),'--approved-sha256',approved,
             '--task-root',str(task),'--protected-root',str(root),'--protected-root','D:/code/Github/bookbuilder','--report-tag',
             'source-scope-original-01' if kind=='scope' else 'semantic-restart-01']
elif kind=='clients':
    command=[sys.executable,str(task/'scripts/check-service-clients.py'),'--runtime-dir',str(runtime),'--files','4','--model','--report',str(task/'reports/clients-original.json')]
else:raise ValueError('unknown selected native gate')
record={'command':command,'cwd':str(cwd),'approved_install_sha256':approved,'source_before':source_before,
        'environment':{key:env[key] for key in ['TEMP','TMP','AIRK_TEST_BINARY','AIRK_TEST_F09_QUALIFICATION','AIRK_MCP_RESPONSE_MODE'] if key in env},
        'started_utc':datetime.datetime.now(datetime.timezone.utc).isoformat()}
receipt=a/f'f12a-native-{kind}-execution.json'
with receipt.open('x',encoding='utf-8') as stream:json.dump(record,stream,indent=2)
start=time.monotonic()
with (a/f'f12a-native-{kind}.log').open('x',encoding='utf-8') as output:
    result=subprocess.run(command,cwd=cwd,env=env,stdout=output,stderr=subprocess.STDOUT)
record.update(exit_code=result.returncode,elapsed_seconds=time.monotonic()-start,ended_utc=datetime.datetime.now(datetime.timezone.utc).isoformat(),source_after=builder.source_identity())
record['source_unchanged']=record['source_before']==record['source_after'];receipt.write_text(json.dumps(record,indent=2)+'\n')
print(json.dumps(record),flush=True)
raise SystemExit(result.returncode or (0 if record['source_unchanged'] else 1))
