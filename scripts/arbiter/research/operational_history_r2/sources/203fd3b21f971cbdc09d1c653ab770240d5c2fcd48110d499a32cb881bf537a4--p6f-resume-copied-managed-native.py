# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import importlib.util,os,time,json,subprocess
root=Path.cwd();s=importlib.util.spec_from_file_location('owner',root/'artifacts/v190/p6f-resume-frozen-pair-builder.py');m=importlib.util.module_from_spec(s);s.loader.exec_module(m)
logs=root/'artifacts/v190/p6f-resume-copied-managed-native';logs.mkdir()
runtime=root/'artifacts/v190/p6f-resume-activation-runtime-r3';binary=root/'target/debug/deps/airk_cli-c80a0f302ae93ae8.exe';env={**os.environ,'AIRK_F03_RUNTIME':str(runtime),'AIRK_F03_APPROVED_SHA256':m.sha(runtime/'airk-install.json')}
cases=['runtime_install::native_connection_tests::actual_copied_first_install_qualifies_genesis_before_first_service','runtime_install::activation::catalog_upgrade::copied_genesis_recovery_tests::actual_fresh_installer_resumes_native_namespace_and_genesis_prefixes','runtime_install::managed::catalog::tests::actual_managed_first_pin_settles_before_connection_and_external_health']
report={'binary_sha256':m.sha(binary),'runtime_manifest_sha256':m.sha(runtime/'airk-runtime.json'),'approval':env['AIRK_F03_APPROVED_SHA256'],'results':[]}
for n,case in enumerate(cases):
 directory=logs/str(n);directory.mkdir();runner=m.OwnedCommands(root,env,directory,time.monotonic()+360,30)
 try:
  listed=subprocess.check_output([str(binary),'--list',case],text=True,env=env,timeout=10);assert listed.count(': test')==1 and case+': test' in listed
  out=runner.run([str(binary),'--exact',case,'--ignored','--nocapture','--test-threads=1']);assert '1 passed; 0 failed' in out.read_text(encoding='utf-8');value={'case':case,'passed':True,'commands':runner.records}
 except Exception as e:value={'case':case,'passed':False,'error':str(e),'commands':runner.records}
 report['results'].append(value);(logs/'report.json').write_text(json.dumps(report,indent=2)+'\n',encoding='utf-8');print(json.dumps(value),flush=True)
raise SystemExit(0 if all(v['passed'] for v in report['results']) else 1)
