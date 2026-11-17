# SPDX-License-Identifier: Apache-2.0
import hashlib, importlib.util, json, pathlib, subprocess, datetime
base=pathlib.Path(__file__).resolve().parent
fixture=base/'user-private-fixtures-6c0f255fb113480ea4938e25f563cdf7'/'airk-service-clients-w464_v5j'
start=json.loads((fixture/'managed-start.json').read_bytes())
request=json.loads((fixture/'managed-stop-request.json').read_bytes())
stop=json.loads((fixture/'managed-stop-result.json').read_bytes())
expected={key:value for key,value in request.items() if key!='force'}
expected['manager']=False
assert stop['status']=='lifecycle' and stop['record']['phase']=='complete' and stop['record']['request']==expected
assert request['expected_instance']==start['health']['instance']
assert request['runtime']==start['health']['runtime_sha256']==start['activation']['active']
image=pathlib.Path(start['inspect']['executable'])
assert image.is_file() and hashlib.sha256(image.read_bytes()).hexdigest()==start['health']['executable_sha256']
health=subprocess.run([str(image),'service','health','--runtime-dir',start['inspect']['control_dir']],capture_output=True,text=True,timeout=15)
spec=importlib.util.spec_from_file_location('owned_census',base/'native_process_image_census.py')
module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
observed=module.census([image])
report={'time_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'fixture':str(fixture),'start':start,'stop_request':request,'stop':stop,'current_image_sha256':hashlib.sha256(image.read_bytes()).hexdigest(),'read_only_health':{'argv':[str(image),'service','health','--runtime-dir',start['inspect']['control_dir']],'exit_code':health.returncode,'stdout':health.stdout,'stderr':health.stderr},'process_census':observed,'fixture_preserved':True}
(base/'windows-release-model-clients-r2-cleanup-audit.json').write_text(json.dumps(report,indent=2)+'\n')
assert health.returncode!=0 and observed['live_count']==0
print(json.dumps({'stop_phase':stop['record']['phase'],'health_exit':health.returncode,'process_census':observed}))
