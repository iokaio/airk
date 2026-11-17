# SPDX-License-Identifier: Apache-2.0
import hashlib,json,os,shutil,subprocess,sys,xml.etree.ElementTree as ET
from pathlib import Path
base=Path(__file__).resolve().parent; root=base.parents[1]; out=base/'windows-r1'
runtime=out/'debug-runtime'
build=json.loads((runtime/'airk-build.json').read_text(encoding='utf-8'))
assert build['source']=={'sha256':'b409ff3d25c46ad22cf4f179edf343492780dcb048d344cfa59a9034920ac209','files':973}
manifest=json.loads((runtime/'airk-install.json').read_text(encoding='utf-8'))
for name,digest in manifest['files'].items():assert hashlib.sha256((runtime/name).read_bytes()).hexdigest()==digest,name
names=['recommendations_keep_native_cli_mcp_parity_and_execute_copied_examples','post_p14_removed_source_ancestor_keeps_healthy_cli_mcp_warm_and_cold','post_p14_source_and_storage_loss_supports_explicit_retirement']
selection=' | '.join('test('+name+')' for name in names)
environment=dict(os.environ,AIRK_CATALOG_CANDIDATE_BINARY=str(runtime/'airk.exe'))
command=['cargo','nextest','run','-p','airk-cli','--test','combined_reads','--all-features','--locked','--offline','--timings','--profile','ci','--ignore-default-filter','--no-fail-fast','--retries','0','--test-threads','1','-E',selection]
print(json.dumps({'command':command,'AIRK_CATALOG_CANDIDATE_BINARY':environment['AIRK_CATALOG_CANDIDATE_BINARY']}),flush=True)
code=subprocess.run(command,cwd=root,env=environment).returncode
report=root/'target/nextest/ci/junit.xml'; saved=out/'preflight-r1.junit.xml'
assert not saved.exists()
if report.exists():shutil.copyfile(report,saved)
if code:sys.exit(code)
cases=list(ET.parse(saved).iter('testcase'));assert len(cases)==3,len(cases)
assert {case.get('name','').rsplit('::',1)[-1] for case in cases}==set(names)
assert all(case.find('failure') is None and case.find('error') is None and case.find('skipped') is None for case in cases)
print(json.dumps({'status':'passed','tests':3,'junit':str(saved)}))
