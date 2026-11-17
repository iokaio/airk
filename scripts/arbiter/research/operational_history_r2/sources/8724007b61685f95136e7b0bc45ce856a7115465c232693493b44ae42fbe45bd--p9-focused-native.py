# SPDX-License-Identifier: Apache-2.0
import json, os, subprocess, time
from pathlib import Path

root=Path(__file__).resolve().parents[2]
env=os.environ.copy()
env['CARGO_HOME']='C:/Users/tdotn/.cargo'
env['RUSTUP_HOME']='C:/Users/tdotn/.rustup'
env['PATH']='C:/Users/tdotn/.cargo/bin;'+env['PATH']
env['AIRK_PRIVATE_CENSUS_BASE']=json.loads((root/'artifacts/v190/p9-private-base.json').read_text(encoding='utf-8-sig'))['path']
env['AIRK_TEST_MODEL_DIR']=str(root/'target/local-vector-model/minilm-l6-v2-u8-v1')
groups=[
('kernel-policy-unit',['-p','airk-kernel','--lib','source_policy::tests']),
('kernel-policy-native',['-p','airk-kernel','--test','source_policy']),
('ingest-permanent-policy',['-p','airk-ingest','permanent_policy']),
('kernel-source-watch',['-p','airk-kernel','--lib','source_watch']),
('cli-trim-admission',['-p','airk-cli','--lib','permanent_trim']),
('cli-outer-delivery',['-p','airk-cli','--lib','outer_native_read']),
('cli-watcher-fence',['-p','airk-cli','--lib','policy_fence']),
('cli-policy-draft',['-p','airk-cli','--lib','catalog_source_change_control::tests']),
('cli-policy-authority',['-p','airk-cli','--lib','catalog_authority::tests']),
('mcp-policy-error',['-p','airk-mcp','--lib','permanent_policy_refusal']),
('cli-trim-transport',['-p','airk-cli','--test','combined_reads','permanent_trim']),
]
records=[]
for label,args in groups:
    command=['C:/Users/tdotn/.cargo/bin/cargo.exe','test',*args,'--all-features','--locked','--','--test-threads=1']
    log=root/f'artifacts/v190/p9-{label}-r1.log'
    print('START '+label,flush=True)
    start=time.monotonic()
    with log.open('w',encoding='utf-8') as output:
        result=subprocess.run(command,cwd=root,env=env,stdout=output,stderr=subprocess.STDOUT)
    record={'label':label,'command':command,'exit_code':result.returncode,'elapsed_seconds':round(time.monotonic()-start,3),'log':str(log.relative_to(root))}
    records.append(record)
    (root/'artifacts/v190/p9-focused-native-r1.json').write_text(json.dumps({'groups':records},indent=2),encoding='utf-8')
    tail='\n'.join(log.read_text(encoding='utf-8',errors='replace').splitlines()[-15:])
    print(json.dumps(record)+'\n'+tail,flush=True)
raise SystemExit(0 if all(record['exit_code']==0 for record in records) else 1)
