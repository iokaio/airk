# SPDX-License-Identifier: Apache-2.0
import hashlib,json,os,re,subprocess,time
from pathlib import Path
r=Path.cwd();out=r/'artifacts/advanced-split/receipts/retrieval-focus-r81';out.mkdir(parents=True,exist_ok=False)
static=r/'artifacts/advanced-split/ort130-x64-owned-r5/cargo-aggregate-env.json'
env=dict(os.environ);env.update(json.loads(static.read_bytes())['environment']);env.update(CARGO_BUILD_JOBS='8',CARGO_INCREMENTAL='1',CARGO_NET_OFFLINE='true',CARGO_TARGET_DIR=str(r/'artifacts/v190/p10-target'),AIRK_BUNDLED_MODEL_DIR=str(r/'artifacts/advanced-split/bundled-model-v2'))
prep=r/'artifacts/advanced-split/focus-native-qualification-preparation-r1'
for name in ['compiler-command.json','profile-emitter-command-r2.json']:
 env.update(json.loads((prep/name).read_bytes())['environment'])
def sha(p):
 with p.open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
def snapshot():
 paths=[r/'Cargo.lock',r/'Cargo.toml',r/'rust-toolchain.toml',static,prep/'compiler-command.json',prep/'profile-emitter-command-r2.json',prep/'selected-focus-profile-inputs-r2.json']
 for name in ['airk-core','airk-process','airk-retrieval']:
  paths.extend(p for p in (r/'crates'/name).rglob('*') if p.is_file())
 paths.extend(p for p in (r/'fixtures').rglob('*') if p.is_file())
 paths.extend(p for p in (r/'.cargo').rglob('*') if p.is_file())
 return {str(p.relative_to(r)):sha(p) for p in paths}
cargo='C:/Users/tdotn/.cargo/bin/cargo.exe'
prefix=[cargo,'nextest','run','--profile','ci','-p','airk-retrieval','--all-features','--locked','--timings','--fail-fast']
rows=[('clippy',[cargo,'clippy','-p','airk-retrieval','--all-targets','--all-features','--locked','--timings','--','-D','warnings']),('cheap-model-guards',prefix+['-E','test(arbiter::compiler::qwen::tests::) | test(arbiter::runtime::input::tests::) | test(arbiter::runtime::qualification::tests::) | test(arbiter::publication::tests::publication_requires_)']),('focus-compiler',prefix+['-E','test(arbiter::compiler::focus::tests::)','--run-ignored','all']),('focus-profile',prefix+['-E','test(arbiter::runtime::qualification::tests::emit_selected_experimental_profiles_from_reviewed_assets)','--run-ignored','only'])]
for label,cmd in rows:
 before=snapshot();start=time.monotonic()
 with (out/(label+'.log')).open('wb') as log:result=subprocess.run(cmd,env=env,stdout=log,stderr=subprocess.STDOUT)
 after=snapshot();log=(out/(label+'.log')).read_text(encoding='utf-8',errors='replace');selected=re.search(r'Starting (\d+) tests?',log)
 receipt={'schema':'airk.private-scoped-retrieval-validation/1','scope':'airk-retrieval and actual Core/process source dependencies plus retained fixtures; other edited crates not consumed','command':cmd,'exit_code':result.returncode,'elapsed_seconds':time.monotonic()-start,'target_directory':env['CARGO_TARGET_DIR'],'incremental':'1','source_before':before,'source_after':after,'consumed_inputs_unchanged':before==after,'selected_tests':int(selected.group(1)) if selected else None,'model_forwards':0,'environment':{key:value for key,value in env.items() if key.startswith(('ORT_','LIBONNXRUNTIME','AIRK_FOCUS_','AIRK_SELECTED_','CARGO_'))}}
 (out/(label+'.json')).write_text(json.dumps(receipt,indent=2)+'\n',encoding='utf-8');print(json.dumps({k:v for k,v in receipt.items() if k not in ['source_before','source_after','environment']}),flush=True)
 if result.returncode or before!=after or (label!='clippy' and (not selected or int(selected.group(1))==0)):raise SystemExit(result.returncode or 1)
