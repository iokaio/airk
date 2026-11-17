# SPDX-License-Identifier: Apache-2.0
import hashlib, importlib.util, json, os, subprocess, sys, time
from pathlib import Path
sys.stdout.reconfigure(encoding='utf-8',errors='replace')
sys.stderr.reconfigure(encoding='utf-8',errors='replace')
root=Path(__file__).resolve().parents[2]
log=root/sys.argv[1]
assert log.resolve().is_relative_to(root/'artifacts/v190') and not log.exists()
command=sys.argv[2:]
env=os.environ.copy()
env.update(CARGO_HOME='C:/Users/tdotn/.cargo',RUSTUP_HOME='C:/Users/tdotn/.rustup',CARGO_INCREMENTAL='0',PYTHONUTF8='1')
env['PATH']='C:/Users/tdotn/.cargo/bin;'+env['PATH']
inputs=json.loads((root/'artifacts/v190/p9-clean-inputs.local.json').read_text(encoding='utf-8-sig'))['inputs']
env.update(AIRK_BUNDLED_MODEL_DIR=inputs['local-vector-model'],AIRK_BUNDLED_RUNTIME_DIR=inputs['local-onnx-runtime'],AIRK_TEST_MODEL_DIR=str(Path(inputs['local-vector-model'])/'minilm-l6-v2-u8-v1'),AIRK_PRIVATE_CENSUS_BASE=json.loads((root/'artifacts/v190/p9-private-base.json').read_text(encoding='utf-8-sig'))['path'])
env.pop('AIRK_TEST_BINARY',None)
ignore_base=Path(env['AIRK_PRIVATE_CENSUS_BASE'])/'p9-owned-ignore-controls'
ignore_base.mkdir(exist_ok=True)
ignore_file=ignore_base/'empty.ignore'
ignore_config=ignore_base/'global.gitconfig'
for path,data in [(ignore_file,b''),(ignore_config,('[core]\nexcludesFile = "'+ignore_file.as_posix()+'"\n').encode())]:
    if path.exists():
        assert path.read_bytes()==data, 'owned ignore fixture changed'
    else:
        with path.open('xb') as output: output.write(data)
env['GIT_CONFIG_GLOBAL']=str(ignore_config)
os.environ.update(env)
spec=importlib.util.spec_from_file_location('builder',root/'scripts/build-service-runtime.py')
builder=importlib.util.module_from_spec(spec);spec.loader.exec_module(builder)
before=builder.source_identity()
started=time.monotonic()
with log.open('x',encoding='utf-8') as output:
    result=subprocess.run(command,cwd=root,env=env,stdout=output,stderr=subprocess.STDOUT)
after=builder.source_identity()
receipt={'command':command,'exit_code':result.returncode,'seconds':time.monotonic()-started,'source_before':before,'source_after':after,'source_unchanged':before==after,'log_sha256':hashlib.sha256(log.read_bytes()).hexdigest()}
log.with_suffix('.receipt.json').write_text(json.dumps(receipt,indent=2)+'\n',encoding='utf-8')
print(json.dumps(receipt));print('\n'.join(log.read_text(encoding='utf-8',errors='replace').splitlines()[-40:]))
sys.exit(result.returncode if before==after else 1)
