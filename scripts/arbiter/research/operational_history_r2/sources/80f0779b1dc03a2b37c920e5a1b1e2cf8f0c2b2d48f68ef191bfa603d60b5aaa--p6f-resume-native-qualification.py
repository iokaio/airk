# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import importlib.util,os,json,time,hashlib,subprocess
root=Path.cwd(); p=root/'artifacts/v190/p6f-resume-frozen-pair-builder.py'
s=importlib.util.spec_from_file_location('owned',p); m=importlib.util.module_from_spec(s); s.loader.exec_module(m)
logs=root/'artifacts/v190/p6f-resume-native-qualification'; logs.mkdir()
env=os.environ.copy(); env['AIRK_GUARDED_PAIR_MAIN']=str(root/'artifacts/v190/p6f-resume-guarded-pair-r2/payload/airk.exe'); env['AIRK_TEST_GUARDED_RUNTIME']=str(root/'artifacts/v190/p6f-resume-guarded-pair-r2/payload')
binary=root/'target/debug/deps/airk_cli-c80a0f302ae93ae8.exe'
selectors=['embedding_pool_process::shared::tests::native::guarded_real_pair_starts_and_records_actual_native_completion','embedding_pool_process::shared::tests::native::installer_maintenance_recovers_last_service_helper_without_new_service_identity','project_supervisor::guarded_qualification_tests::ordinary_exit_keeps_real_provider_work_and_sibling_ownership_until_engine_release','project_supervisor::guarded_qualification_tests::retire_requires_actual_engine_release_and_complete_native_worker_obligation','project_supervisor::guarded_qualification_tests::restart::restored_native_worker_obligation']
report={'binary_sha256':m.sha(binary),'pair_receipt_sha256':m.sha(root/'artifacts/v190/p6f-resume-guarded-pair-r2/qualification.json'),'results':[]}
for n,case in enumerate(selectors):
 runner=m.OwnedCommands(root,env,logs/time.strftime('%Y%m%d-%H%M%S'),time.monotonic()+180,30); runner.logs.mkdir()
 try:
  listing=subprocess.check_output([str(binary),'--list',case],env=env,text=True,timeout=10); assert listing.count(': test')==1 and case+': test' in listing
  out=runner.run([str(binary),'--exact',case,'--ignored','--nocapture','--test-threads=1']); text=out.read_text(encoding='utf-8'); assert '1 passed; 0 failed' in text
  report['results'].append({'case':case,'passed':True,'records':runner.records})
 except Exception as e:
  report['results'].append({'case':case,'passed':False,'error':str(e),'records':runner.records})
 (logs/'report.json').write_text(json.dumps(report,indent=2),encoding='utf-8'); print(json.dumps(report['results'][-1]),flush=True)
raise SystemExit(0 if all(r['passed'] for r in report['results']) else 1)
