# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import datetime,hashlib,importlib.util,json,os,subprocess,sys,time
root=Path.cwd();a=root/'artifacts/optimizations';location=json.loads((a/'f09-runtime-location.json').read_text())
task,runtime=Path(location['task']),Path(location['runtime']);kind,approved=sys.argv[1:]
selected_kind='host' if kind in ['host-corrected','host-corrected2'] else 'scope' if kind=='scope-corrected' else kind
assert hashlib.sha256((runtime/'airk-install.json').read_bytes()).hexdigest()==approved
for item in location['scripts']:
    assert hashlib.sha256((task/'scripts'/item['name']).read_bytes()).hexdigest()==item['sha256']
spec=importlib.util.spec_from_file_location('f09_native_builder',root/'scripts/build-service-runtime.py')
builder=importlib.util.module_from_spec(spec);spec.loader.exec_module(builder)
source_before=builder.source_identity()
assert source_before==json.loads((runtime/'airk-build.json').read_text())['source']
assert source_before=={'sha256':'568283bade24bee26a5f6fa24bc025e9e75a57e636f32192f6660c9b2e9af64f','files':708}
env=os.environ.copy();env.pop('AIRK_TEST_BINARY',None);env.pop('AIRK_TEST_F09_QUALIFICATION',None);env.pop('PYTHONOPTIMIZE',None)
env.update(TEMP=str(task/'fixtures'),TMP=str(task/'fixtures'))
cwd=root
if selected_kind in ['smoke','assistance','host']:
    env['AIRK_TEST_BINARY']=str(runtime/'airk.exe');env['AIRK_MCP_RESPONSE_MODE']='terse'
    if selected_kind=='host':env['AIRK_TEST_F09_QUALIFICATION']='1'
    selected={'smoke':'nativeSmoke','assistance':'explorerAssistanceNative','host':'f09NativeQualification'}[selected_kind]
    command=['node','--test',f'out/test/{selected}.test.js'];cwd=root/'packages/vscode'
elif kind in ['install','managed','console']:
    selected={'install':'check-service-install.py','managed':'check-managed-runtime.py','console':'check-service-console.py'}[kind]
    command=[sys.executable,str(task/'scripts'/selected),'--runtime',str(runtime)]
elif kind in ['console-client','console-client-corrected']:
    copy=json.loads((a/'f09-console-client-out-copy.json').read_text())
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
    driver=task/'scripts'/('check-service-console-client-f09-corrected.cjs' if kind=='console-client-corrected' else 'check-service-console-client.cjs')
    if kind=='console-client-corrected':assert hashlib.sha256(driver.read_bytes()).hexdigest()=='7071bd4c51d7fee4b7befdfc161eee4f69cc480b1d840273c0de17963cddcad5'
    command=['node',str(driver),'--runtime',str(runtime)]
elif selected_kind=='scope':
    corrected=kind=='scope-corrected'
    driver=task/'scripts'/('f09-source-scope-native-binding.py' if corrected else 'f09-source-scope-port.py')
    if corrected:assert hashlib.sha256(driver.read_bytes()).hexdigest()=='3e36e5d8276eecff669f4b112ace696fe5767a3b418efd8b4f5ee2651cc3fd0f'
    command=[sys.executable,str(driver),'--runtime',str(runtime),'--approved-sha256',approved,
             '--task-root',str(task),'--protected-root',str(root),'--protected-root','D:/code/Github/bookbuilder','--report-tag',
             'source-scope-native-binding-01' if corrected else 'source-scope-port-01']
elif kind=='clients':
    command=[sys.executable,str(task/'scripts/check-service-clients.py'),'--runtime-dir',str(runtime),'--files','4','--model','--report',str(task/'reports/clients-original.json')]
else:raise ValueError('unknown selected native gate')
record={'command':command,'cwd':str(cwd),'approved_install_sha256':approved,'source_before':source_before,
        'environment':{key:env[key] for key in ['TEMP','TMP','AIRK_TEST_BINARY','AIRK_TEST_F09_QUALIFICATION','AIRK_MCP_RESPONSE_MODE'] if key in env},
        'started_utc':datetime.datetime.now(datetime.timezone.utc).isoformat()}
receipt=a/f'f09-native-{kind}-execution.json'
with receipt.open('x',encoding='utf-8') as stream:json.dump(record,stream,indent=2)
start=time.monotonic()
with (a/f'f09-native-{kind}.log').open('x',encoding='utf-8') as output:
    result=subprocess.run(command,cwd=cwd,env=env,stdout=output,stderr=subprocess.STDOUT)
record.update(exit_code=result.returncode,elapsed_seconds=time.monotonic()-start,ended_utc=datetime.datetime.now(datetime.timezone.utc).isoformat(),source_after=builder.source_identity())
record['source_unchanged']=record['source_before']==record['source_after'];receipt.write_text(json.dumps(record,indent=2)+'\n')
print(json.dumps(record),flush=True)
raise SystemExit(result.returncode or (0 if record['source_unchanged'] else 1))
