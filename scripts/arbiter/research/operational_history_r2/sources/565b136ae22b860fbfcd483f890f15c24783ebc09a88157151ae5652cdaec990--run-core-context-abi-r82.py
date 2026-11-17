# SPDX-License-Identifier: Apache-2.0
import hashlib,json,os,subprocess,time
from pathlib import Path
r=Path.cwd();out=r/'artifacts/advanced-split/receipts/core-context-abi-r82';out.mkdir(parents=True,exist_ok=False)
env=dict(os.environ);env.update(json.loads((r/'artifacts/advanced-split/ort130-x64-owned-r5/cargo-aggregate-env.json').read_bytes())['environment']);env.update(CARGO_BUILD_JOBS='8',CARGO_INCREMENTAL='1',CARGO_NET_OFFLINE='true',CARGO_TARGET_DIR=str(r/'artifacts/v190/p10-target'),AIRK_BUNDLED_MODEL_DIR=str(r/'artifacts/advanced-split/bundled-model-v2'),AIRK_WRITE_ARBITER_CONTEXT_ABI_CASES=str(r/'fixtures/split/arbiter/context_qwen_abi_cases.json'))
def snapshot():
 paths=[r/'Cargo.lock',r/'Cargo.toml',r/'rust-toolchain.toml',*(r/'crates/airk-core').rglob('*.rs'),*(r/'crates/airk-core').rglob('Cargo.toml')]
 return {str(p.relative_to(r)):hashlib.sha256(p.read_bytes()).hexdigest() for p in paths}
cargo='C:/Users/tdotn/.cargo/bin/cargo.exe'
rows=[('clippy',[cargo,'clippy','-p','airk-core','--all-targets','--all-features','--locked','--timings','--','-D','warnings']),('focus-only-cases',[cargo,'nextest','run','--profile','ci','-p','airk-core','--all-features','--locked','--timings','--fail-fast','-E','test(context_qwen_six_input_abi_refuses_incomplete_foreign_or_reordered_tensors)'])]
for label,cmd in rows:
 before=snapshot();start=time.monotonic()
 with (out/(label+'.log')).open('wb') as log:result=subprocess.run(cmd,env=env,stdout=log,stderr=subprocess.STDOUT)
 after=snapshot();receipt={'schema':'airk.private-scoped-core-validation/1','scope':'airk-core only; independently edited consumers not consumed','command':cmd,'exit_code':result.returncode,'elapsed_seconds':time.monotonic()-start,'target_directory':env['CARGO_TARGET_DIR'],'incremental':'1','source_before':before,'source_after':after,'consumed_core_inputs_unchanged':before==after}
 (out/(label+'.json')).write_text(json.dumps(receipt,indent=2)+'\n',encoding='utf-8');print(json.dumps({k:v for k,v in receipt.items() if not k.startswith('source_')}),flush=True)
 if result.returncode or before!=after:raise SystemExit(result.returncode or 1)
