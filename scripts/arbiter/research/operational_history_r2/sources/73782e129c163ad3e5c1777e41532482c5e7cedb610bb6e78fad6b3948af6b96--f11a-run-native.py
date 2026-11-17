# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import datetime,hashlib,importlib.util,json,os,subprocess,sys,time
root=Path.cwd();a=root/'artifacts/optimizations';loc=json.loads((a/'f11a-runtime-location.json').read_text());task,runtime=Path(loc['task']),Path(loc['runtime']);kind,approved=sys.argv[1:]
assert kind in ['pool','scope','repair','query-coexistence','install','managed','clients']
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
assert sha(runtime/'airk-install.json')==approved
for item in loc['scripts']:assert sha(task/'scripts'/item['name'])==item['sha256']
spec=importlib.util.spec_from_file_location('f11a_native_builder',root/'scripts/build-service-runtime.py');b=importlib.util.module_from_spec(spec);spec.loader.exec_module(b)
before=b.source_identity();assert before=={'sha256':'927128a5eb90a71fb74ea53dac53a56b787ba3f86792d96ca3cfeb5d938344c5','files':718};assert before==json.loads((runtime/'airk-build.json').read_text())['source']
env=os.environ.copy()
for key in ['AIRK_TEST_BINARY','AIRK_TEST_F09_QUALIFICATION','PYTHONOPTIMIZE']:env.pop(key,None)
env.update(TEMP=str(task/'fixtures'),TMP=str(task/'fixtures'))
if kind in ['pool','install','managed']:
 name={'pool':'check-service-pool.py','install':'check-service-install.py','managed':'check-managed-runtime.py'}[kind];command=[sys.executable,str(task/'scripts'/name),'--runtime',str(runtime)]
elif kind=='scope':
 driver=task/'scripts/check-source-scope-current-pool.py'
 command=[sys.executable,str(driver),'--runtime',str(runtime),'--approved-sha256',approved,'--task-root',str(task),'--protected-root',str(root),'--protected-root','D:/code/Github/bookbuilder','--report-tag','source-scope-current-pool-01']
elif kind in ['repair','query-coexistence']:
 qualification=loc['approved_repair_driver' if kind=='repair' else 'query_coexistence_driver']
 driver=Path(qualification['path']);base_path=Path(qualification['base_path'])
 assert sha(driver)==qualification['sha256'] and sha(base_path)==qualification['base_sha256']
 for name,digest in json.loads(base_path.read_text())['files'].items():assert sha(root/name)==digest
 command=[sys.executable,str(driver),'--runtime',str(runtime),'--install-sha256',approved]
else:
 command=[sys.executable,str(task/'scripts/check-service-clients.py'),'--runtime-dir',str(runtime),'--files','4','--model','--report',str(task/'reports/clients-original.json')]
record={'command':command,'approved_install_sha256':approved,'source_before':before,'environment':{k:env[k] for k in ['TEMP','TMP']},'started_utc':datetime.datetime.now(datetime.timezone.utc).isoformat()};receipt=a/f'f11a-native-{kind}-execution.json'
with receipt.open('x') as f:json.dump(record,f,indent=2)
start=time.monotonic()
with (a/f'f11a-native-{kind}.log').open('x') as out:r=subprocess.run(command,env=env,stdout=out,stderr=subprocess.STDOUT)
record.update(exit_code=r.returncode,elapsed_seconds=time.monotonic()-start,source_after=b.source_identity(),ended_utc=datetime.datetime.now(datetime.timezone.utc).isoformat());record['source_unchanged']=record['source_before']==record['source_after'];receipt.write_text(json.dumps(record,indent=2)+'\n');print(json.dumps(record),flush=True);raise SystemExit(r.returncode or (0 if record['source_unchanged'] else 1))
