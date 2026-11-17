# SPDX-License-Identifier: Apache-2.0
import importlib.util,json,os,re,shutil,subprocess,sys,time
from pathlib import Path
sys.stdout.reconfigure(encoding='utf-8',errors='replace')
root=Path.cwd()
base=root/'artifacts/local/1.8.11-indexing-20261107'
spec=importlib.util.spec_from_file_location('runtime_builder',root/'scripts/build-service-runtime.py')
builder=importlib.util.module_from_spec(spec);spec.loader.exec_module(builder)
name=sys.argv[1];command=sys.argv[2:]
env=os.environ.copy()
env['PATH']=str(root/'artifacts/tools/nextest')+os.pathsep+r'C:\Users\tdotn\.cargo\bin'+os.pathsep+env['PATH']
env['GIT_CONFIG_COUNT']='1'
env['GIT_CONFIG_KEY_0']='safe.directory'
env['GIT_CONFIG_VALUE_0']=root.as_posix()
env['CARGO_TARGET_DIR']=str(root/'artifacts/v190/p10-target')
if name=='runtime-release':
    env.pop('CARGO_INCREMENTAL',None)
else:
    env['CARGO_INCREMENTAL']='1'
env['AIRK_CATALOG_CANDIDATE_BINARY']=str(base/'runtime-debug/airk.exe')
env['AIRK_PRIVATE_CENSUS_BASE']=(base/'private-fixture-base.txt').read_text().strip()
env['XDG_CONFIG_HOME']=str(base/'isolated-test-config')
env['AIRK_TEST_MODEL_DIR']=str(root/'target/local-vector-model/minilm-l6-v2-u8-v1')
command[0]=shutil.which(command[0],path=env['PATH']) or command[0]
source_before=builder.source_identity();started=time.perf_counter()
with (base/(name+'.log')).open('w',encoding='utf-8') as stream:
    result=subprocess.run(command,cwd=root,env=env,stdout=stream,stderr=subprocess.STDOUT)
elapsed=time.perf_counter()-started
source_after=builder.source_identity()
text=(base/(name+'.log')).read_text(encoding='utf-8')
timings=[]
for namepath in re.findall(r'Timing report saved to (.+\.html)',text):
    path=Path(namepath.strip());target=base/'cargo-timings'/path.name
    shutil.copyfile(path,target);timings.append(str(target))
receipt={'command':command,'target_dir':env['CARGO_TARGET_DIR'],'incremental':env.get('CARGO_INCREMENTAL','unset; Cargo profile default'),'elapsed_seconds':elapsed,'exit_code':result.returncode,'source_before':source_before,'source_after':source_after,'source_unchanged':source_before==source_after,'timings':timings,'private_census_base':env['AIRK_PRIVATE_CENSUS_BASE'],'xdg_config_home':env['XDG_CONFIG_HOME']}
(base/(name+'-result.json')).write_text(json.dumps(receipt,indent=2)+'\n')
print(json.dumps(receipt));print('\n'.join(text.splitlines()[-15:]))
sys.exit(result.returncode)
