# SPDX-License-Identifier: Apache-2.0
import importlib.util,json,os,subprocess,sys
from pathlib import Path
base=Path(__file__).resolve().parent
env=os.environ.copy()
env.update(PYTHONUTF8='1',AIRK_PRIVATE_CENSUS_BASE=json.loads((base/'p10-private-base-r5.json').read_text())['path'],
    AIRK_CATALOG_CANDIDATE_BINARY=str(base/'p11a-runtime-final-r1/airk.exe'))
spec=importlib.util.spec_from_file_location('builder',base/'p10-worktree/scripts/build-service-runtime.py')
builder=importlib.util.module_from_spec(spec);spec.loader.exec_module(builder)
source=builder.source_identity()
assert source=={'sha256':'4114c78c5404270cd8fe9de6ffebf960fd6ad272b8914c317c66ec1db1dac0d1','files':948}
record={'status':'running','test_source':source,'runtime':'p11a-runtime-final-r1','runtime_source':'33e6df135da90839f409dfbba6dd2ce338af31675875e99f20d0b94af212f3f7','steps':[],
    'scope':'Preflight with corrected external test code; production bytes unchanged. Fresh source-bound runtime/qualification still required.'}
output=base/'p11a-transport-preflight-r3.json';assert not output.exists()
for name,feature in [('minimal','--no-default-features'),('helpers','--all-features')]:
    log=base/f'p11a-flat-parity-preflight-{name}-r3.log'
    cmd=['C:/Users/tdotn/.cargo/bin/cargo.exe','test','-p','airk-cli','--test','combined_reads',feature,'--locked','--offline','--timings',
        'profile_families::opted_file_family_report_keeps_full_native_cli_console_mcp_and_input_catalog_parity','--','--exact','--test-threads=1','--nocapture']
    result=subprocess.run([sys.executable,str(base/'p11-local-command.py'),str(log),*cmd],env=env)
    receipt=json.loads(log.with_suffix('.receipt.json').read_text())
    record['steps'].append({'name':name,'exit_code':result.returncode,'receipt':str(log.with_suffix('.receipt.json'))})
    assert builder.source_identity()==source
    if result.returncode or receipt['passing_tests']!=1:
        record['status']='failed';output.write_text(json.dumps(record,indent=2)+'\n');sys.exit(1)
    output.write_text(json.dumps(record,indent=2)+'\n')
record['status']='passed';output.write_text(json.dumps(record,indent=2)+'\n')
print(json.dumps(record))
