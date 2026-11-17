# SPDX-License-Identifier: Apache-2.0
import json,os,subprocess,sys,time,hashlib,re,importlib.util
from pathlib import Path
base=Path('D:/code/Github/airk'); work=base/'artifacts/v190/p10-worktree'; log=base/'artifacts/v190'/sys.argv[1]
assert not log.exists()
env=os.environ.copy();env.update(CARGO_HOME='C:/Users/tdotn/.cargo',RUSTUP_HOME='C:/Users/tdotn/.rustup',CARGO_INCREMENTAL=os.environ.get('AIRK_DEV_CARGO_INCREMENTAL','1'),PYTHONUTF8='1',CARGO_TARGET_DIR=str(base/'artifacts/v190/p10-target'))
env['PATH']='C:/Users/tdotn/.cargo/bin;'+env['PATH']
inputs=json.loads((base/'artifacts/v190/p9-clean-inputs.local.json').read_text(encoding='utf-8-sig'))['inputs']
env.update(AIRK_BUNDLED_MODEL_DIR=inputs['local-vector-model'],AIRK_BUNDLED_RUNTIME_DIR=inputs['local-onnx-runtime'],AIRK_TEST_MODEL_DIR=str(Path(inputs['local-vector-model'])/'minilm-l6-v2-u8-v1'))
env.pop('AIRK_TEST_BINARY',None)
private_override=env.get('AIRK_PRIVATE_CENSUS_BASE')
if private_override is not None:
    assert private_override.strip(), 'Explicit private fixture base cannot be empty'
    private_base=Path(private_override).resolve(strict=True)
    assert private_base.is_dir(), 'Explicit private fixture base must already be a directory'
else:
    private_base=base/'artifacts/v190/p10-private-fixtures'
    private_base.mkdir(exist_ok=True)
env['AIRK_PRIVATE_CENSUS_BASE']=str(private_base)
ignore_base=base/'artifacts/v190/p10-owned-ignore-controls'
ignore_base.mkdir(exist_ok=True)
ignore_file=ignore_base/'empty.ignore'
ignore_config=ignore_base/'global.gitconfig'
for path,data in [(ignore_file,b''),(ignore_config,('[core]\nexcludesFile = "'+ignore_file.as_posix()+'"\n').encode())]:
    if path.exists(): assert path.read_bytes()==data, 'owned ignore fixture changed'
    else:
        with path.open('xb') as output: output.write(data)
env['GIT_CONFIG_GLOBAL']=str(ignore_config)
git_count=int(env.get('GIT_CONFIG_COUNT','0'))
env[f'GIT_CONFIG_KEY_{git_count}']='safe.directory'
env[f'GIT_CONFIG_VALUE_{git_count}']=work.as_posix()
env['GIT_CONFIG_COUNT']=str(git_count+1)
os.environ.update(env)
spec=importlib.util.spec_from_file_location('builder',work/'scripts/build-service-runtime.py')
builder=importlib.util.module_from_spec(spec);spec.loader.exec_module(builder)
before=builder.source_identity()
started=time.monotonic()
with log.open('x',encoding='utf-8') as output: result=subprocess.run(sys.argv[2:],cwd=work,env=env,stdout=output,stderr=subprocess.STDOUT)
body=log.read_text(encoding='utf-8',errors='replace')
passed=sum(int(value) for value in re.findall(r'test result: (?:ok|FAILED)\. (\d+) passed;',body))
validation_error=None
if len(sys.argv)>3 and Path(sys.argv[2]).stem=='cargo' and sys.argv[3]=='test' and result.returncode==0 and passed==0 and '--doc' not in sys.argv[4:]:
    validation_error='Cargo succeeded but no passing tests were observed; selection is unqualified'
effective_exit=result.returncode or (1 if validation_error else 0)
after=builder.source_identity()
if before!=after:
    validation_error='Native source changed during command'
    effective_exit=effective_exit or 1
receipt={'command':sys.argv[2:],'workdir':str(work),'target':env['CARGO_TARGET_DIR'],'cargo_incremental':env['CARGO_INCREMENTAL'],'private_census_base':env['AIRK_PRIVATE_CENSUS_BASE'],'exit_code':effective_exit,'actual_exit_code':result.returncode,'passing_tests':passed,'validation_error':validation_error,'source_before':before,'source_after':after,'source_unchanged':before==after,'seconds':time.monotonic()-started,'log_sha256':hashlib.sha256(log.read_bytes()).hexdigest()}
log.with_suffix('.receipt.json').write_text(json.dumps(receipt,indent=2)+'\n',encoding='utf-8');print(json.dumps(receipt));print('\n'.join(body.splitlines()[-85:]))
sys.exit(effective_exit)
