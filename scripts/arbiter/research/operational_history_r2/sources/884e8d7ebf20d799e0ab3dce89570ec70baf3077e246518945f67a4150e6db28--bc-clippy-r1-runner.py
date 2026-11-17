# SPDX-License-Identifier: Apache-2.0
"""One serial qualification command with explicit consumed-input receipts."""
import argparse,hashlib,importlib.util,json,os,re,shutil,subprocess,sys,time
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]; BASE=Path(__file__).resolve().parent
p=argparse.ArgumentParser(); p.add_argument('--groups',default='native,harness,fixtures'); p.add_argument('--static',action='store_true'); p.add_argument('--runtime',type=Path); p.add_argument('--fixture-base',type=Path); p.add_argument('--tier',default='T1'); p.add_argument('--release',action='store_true'); p.add_argument('name'); p.add_argument('command',nargs=argparse.REMAINDER); a=p.parse_args()
assert re.fullmatch('[a-z0-9-]+',a.name) and a.command

def digest(path):
    with path.open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
def files(*scopes):
    raw=subprocess.check_output(['git','-c','safe.directory='+ROOT.as_posix(),'ls-files','-z','--cached','--others','--exclude-standard','--',*scopes],cwd=ROOT)
    return sorted({ROOT/os.fsdecode(n) for n in raw.split(b'\0') if n})
def observe():
    scopes={'native':('Cargo.toml','Cargo.lock','rust-toolchain.toml','.cargo','crates','LICENSE','NOTICE','THIRD_PARTY_NOTICES.md','third_party'), 'extension':('packages/vscode',), 'harness':('scripts','.config/nextest.toml','Dockerfile','.dockerignore'), 'fixtures':('examples','fixtures','tools/parser-train','supply-chain')}
    scopes['native-libraries']=scopes['native']
    scopes['offline-assets']=()
    result={}
    for group in a.groups.split(','):
        selected=files(*scopes[group])
        if group=='offline-assets':
            selected=[ROOT/'docs/cli-reference.md',ROOT/'docs/research/contracts-v190/fixtures.json',ROOT/'artifacts/local/one-airk-1.8.14/static-environment-supplement.json']+sorted((ROOT/'target/local-vector-model').rglob('*'))
            selected=[x for x in selected if x.is_file()]
            assert len(selected)==11 and all(not x.is_symlink() for x in selected),'Offline asset roster changed'
        if group=='native-libraries':selected=[x for x in selected if not x.relative_to(ROOT).as_posix().startswith('crates/airk-cli/') or x.relative_to(ROOT).as_posix()=='crates/airk-cli/Cargo.toml']
        values={x.relative_to(ROOT).as_posix():digest(x) if x.is_file() else 'missing' for x in selected}
        if group=='extension':
            for x in (ROOT/'packages/vscode/out').rglob('*'):
                if x.is_file():values[x.relative_to(ROOT).as_posix()]=digest(x)
        result[group]={'sha256':hashlib.sha256(json.dumps(values,sort_keys=True,separators=(',',':')).encode()).hexdigest(),'files':values}
    return result

node=ROOT/'artifacts/tools/node-v24.15.0-win-x64/node.exe'
assert digest(node)=='3331e1ffe19874215472217c5e94f5a0c6d8e18c4ac7111d3937aa0ad5e9b4a5', 'Pinned Node changed'
env=os.environ.copy(); env['PATH']=str(node.parent)+os.pathsep+str(ROOT/'artifacts/tools/nextest')+os.pathsep+'C:/Users/tdotn/.cargo/bin'+os.pathsep+env['PATH']
env.update(CARGO_TARGET_DIR=str(ROOT/'artifacts/v190/p10-target'),CARGO_INCREMENTAL='1',AIRK_BUNDLED_MODEL_DIR=str(ROOT/'target/local-vector-model'),AIRK_TEST_MODEL_DIR=str(ROOT/'target/local-vector-model/minilm-l6-v2-u8-v1'),GIT_CONFIG_COUNT='1',GIT_CONFIG_KEY_0='safe.directory',GIT_CONFIG_VALUE_0=ROOT.as_posix())
for key in ('AIRK_TEST_BINARY','AIRK_NATIVE_TEST_BINARY','AIRK_TEST_F09_QUALIFICATION'):env.pop(key,None)
if a.static:
    spec=importlib.util.spec_from_file_location('static_owner',ROOT/'scripts/static-runtime-environment.py'); module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
    env.update(module.load_environment_file(ROOT/'artifacts/local/one-airk-1.8.14/static-environment-supplement.json'))
private=(a.fixture_base or BASE/'native-fixtures').resolve(); assert private.parent==BASE.resolve(); private.mkdir(exist_ok=True)
# This is a new owned fixture directory. No existing directory ACL is changed.
env.update(TEMP=str(private),TMP=str(private),AIRK_PRIVATE_CENSUS_BASE=str(private))
def runtime_identity():
    spec=importlib.util.spec_from_file_location('native_package_owner',ROOT/'scripts/package-native.py'); module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
    builder=module.runtime_builder()
    version=builder.tomllib.loads((ROOT/'Cargo.toml').read_text())['workspace']['package']['version']
    values,executables=module.validate_runtime(runtime,version,'x86_64-pc-windows-msvc',builder.source_identity())
    return {'version':version,'files':values,'executables':executables}
runtime_before=None
if a.runtime:
    runtime=a.runtime.resolve(strict=True); runtime_before=runtime_identity()
    env.update(AIRK_TEST_BINARY=str(runtime/'airk.exe'),AIRK_NATIVE_TEST_BINARY=str(runtime/'airk.exe'),AIRK_TEST_F09_QUALIFICATION='1')
if a.release: env.pop('CARGO_INCREMENTAL',None)
cmd=list(a.command); cmd[0]=shutil.which(cmd[0],path=env['PATH']) or cmd[0]
command_inputs={}
for value in cmd:
    path=Path(value)
    if not path.is_absolute():path=ROOT/path
    if path.is_file() and path.resolve().is_relative_to(ROOT):command_inputs[str(path.resolve())]=digest(path)
runner_bytes=Path(__file__).read_bytes(); runner_hash=hashlib.sha256(runner_bytes).hexdigest()
(BASE/(a.name+'-runner.py')).write_bytes(runner_bytes)
before=observe(); start=time.monotonic(); utc=time.time()
with (BASE/(a.name+'.log')).open('x',encoding='utf-8') as log:
    print(json.dumps({'stage':a.name,'tier':a.tier,'command':cmd,'input_groups':{k:v['sha256'] for k,v in before.items()}}),flush=True)
    result=subprocess.run(cmd,cwd=ROOT,env=env,stdout=log,stderr=subprocess.STDOUT)
after=observe(); runtime_after=runtime_identity() if a.runtime else None
receipt={'schema':'airk-readiness-command/1','command':cmd,'tier':a.tier,'exit_code':result.returncode,'elapsed_seconds':time.monotonic()-start,'started_unix':utc,'groups':before,'inputs_unchanged':before==after,'changed_groups':[k for k in before if before[k]!=after[k]],'target_directory':env['CARGO_TARGET_DIR'],'incremental':env.get('CARGO_INCREMENTAL','profile default; environment unset'),'log':a.name+'.log','runner_sha256':runner_hash,'runner_unchanged':digest(Path(__file__))==runner_hash,'node_version':'24.15.0','runtime_seal':runtime_before,'runtime_unchanged':runtime_before==runtime_after}
receipt['command_inputs']=command_inputs
receipt['command_inputs_unchanged']=all(Path(path).is_file() and digest(Path(path))==value for path,value in command_inputs.items())
timing=Path(env['CARGO_TARGET_DIR'])/'cargo-timings/cargo-timing.html'
if timing.exists() and timing.stat().st_mtime>=utc:
    shutil.copy2(timing,BASE/(a.name+'-cargo-timing.html'));receipt['cargo_timings']=a.name+'-cargo-timing.html'
junit=Path(env['CARGO_TARGET_DIR'])/'nextest/ci/junit.xml'
if junit.exists() and junit.stat().st_mtime>=utc:
    shutil.copy2(junit,BASE/(a.name+'-junit.xml'));receipt['junit']={'path':a.name+'-junit.xml','sha256':digest(junit)}
log=(BASE/(a.name+'.log')).read_text(errors='replace');receipt['test_summaries']=re.findall(r'(?:test result:.*|Summary \[.*|# tests \d+|# pass \d+|# fail \d+|Ran \d+ tests? in .*)',log)
(BASE/(a.name+'.json')).write_text(json.dumps(receipt,indent=2)+'\n')
print(json.dumps({k:v for k,v in receipt.items() if k not in ('groups','runtime_seal')}),flush=True)
raise SystemExit(result.returncode or (3 if before!=after or not receipt['runner_unchanged'] or not receipt['runtime_unchanged'] or not receipt['command_inputs_unchanged'] else 0))
