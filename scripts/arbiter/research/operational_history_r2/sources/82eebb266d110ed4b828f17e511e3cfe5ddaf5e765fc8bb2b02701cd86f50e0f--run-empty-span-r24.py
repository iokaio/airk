# SPDX-License-Identifier: Apache-2.0
import hashlib,json,os,subprocess,sys,time
from pathlib import Path
root=Path.cwd();out=root/'artifacts/advanced-split/receipts/context-empty-span-r24';out.mkdir(exist_ok=False)
roots=['airk-core','airk-process','airk-host','airk-ingest','airk-store','airk-retrieval','airk-inference','airk-kernel']
def snapshot():
 rows={}
 for name in roots:
  for p in (root/'crates'/name).rglob('*'):
   if p.is_file():rows[str(p.relative_to(root))]=hashlib.sha256(p.read_bytes()).hexdigest()
 for name in ['Cargo.toml','Cargo.lock','rust-toolchain.toml']:
  p=root/name;rows[name]=hashlib.sha256(p.read_bytes()).hexdigest()
 return rows
before=snapshot();env={k:v for k,v in os.environ.items() if not k.upper().startswith('GIT_')};env.update(json.loads((root/'artifacts/advanced-split/ort130-x64/cargo-aggregate-env.json').read_text())['environment']);env.update(CARGO_TARGET_DIR=str(root/'artifacts/v190/p10-target'),CARGO_BUILD_JOBS='8',CARGO_INCREMENTAL='1',CARGO_NET_OFFLINE='true',AIRK_BUNDLED_MODEL_DIR=str(root/'artifacts/advanced-split/bundled-model-v2'))
cargo='C:/Users/tdotn/.cargo/bin/cargo.exe';rows=[]
for name,cmd in [('clippy',[cargo,'clippy','-p','airk-kernel','--all-targets','--all-features','--locked','--timings','--','-D','warnings']),('empty-span',[cargo,'nextest','run','--profile','ci','-p','airk-kernel','--lib','--all-features','--locked','--timings','-E','test(context_action_original_empty_spans_are_metadata_never_vacuous_support)'])]:
 started=time.monotonic()
 with (out/(name+'.log')).open('wb') as log:status=subprocess.run(cmd,env=env,stdout=log,stderr=subprocess.STDOUT).returncode
 rows.append({'command':cmd,'exit_code':status,'elapsed_seconds':time.monotonic()-started});(out/'execution.json').write_text(json.dumps({'scope':'Owning Kernel and local dependency source trees; unrelated active frontend/MCP edits excluded, no full workspace claim','source_before':before,'source_after':snapshot(),'commands':rows,'target':env['CARGO_TARGET_DIR'],'incremental':'1'},indent=2)+'\n');print(json.dumps(rows[-1]),flush=True)
 if status:raise SystemExit(status)
assert before==snapshot(),'consumed native dependency source changed'
assert '1 test run: 1 passed' in (out/'empty-span.log').read_text()
