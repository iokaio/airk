# SPDX-License-Identifier: Apache-2.0
import json, os, subprocess, sys
from pathlib import Path
base = Path('D:/code/Github/airk/artifacts/v190')
env = os.environ.copy()
env.update(PYTHONUTF8='1', AIRK_PRIVATE_CENSUS_BASE=json.loads((base/'p10-private-base-r5.json').read_text())['path'])
cargo = 'C:/Users/tdotn/.cargo/bin/cargo.exe'
tests = [('core-families','airk-core','profile_families',['--lib'],3),
    ('ingest-producer','airk-ingest','metadata_support_requires_complete_producer_without_source_io',['--lib'],1),
    ('kernel-families','airk-kernel','profile_families',['--lib'],2),
    ('kernel-scopes','airk-kernel','scoped_reads::tests',['--lib'],10),
    ('mcp-display','airk-mcp','conditional_file_family_rows_and_unknown_fields_survive_display_modes',['--lib'],1),
    ('cli-info','airk-cli','info_family_flag_forwards_only_explicit_opt_in',['--bin','airk'],1)]
output = base/'p11a-focused-qualification-r1.json'
assert not output.exists()
record = {'status':'running','steps':[]}
for name,package,pattern,target,expected in tests:
    log = base/f'p11a-{name}-r1.log'
    cmd = [cargo,'test','-p',package,*target,'--all-features','--locked','--offline','--timings',pattern,'--','--test-threads=1']
    result = subprocess.run([sys.executable,str(base/'p10-local-command.py'),str(log),*cmd],env=env)
    receipt = json.loads(log.with_suffix('.receipt.json').read_text())
    passed = result.returncode == 0 and receipt['passing_tests'] == expected
    record['steps'].append({'name':name,'receipt':str(log.with_suffix('.receipt.json')),'passed':passed,'expected':expected})
    if not passed:
        record['status']='failed'
        output.write_text(json.dumps(record,indent=2)+'\n')
        sys.exit(1)
    output.write_text(json.dumps(record,indent=2)+'\n')
record['status']='passed'
output.write_text(json.dumps(record,indent=2)+'\n')
print(json.dumps(record))
