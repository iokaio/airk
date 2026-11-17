# SPDX-License-Identifier: Apache-2.0
import json,os,subprocess,time,hashlib
from pathlib import Path
r=Path.cwd();out=r/'artifacts/advanced-split/receipts/help-repair-r58';env=dict(os.environ);env.update(json.loads((r/'artifacts/advanced-split/ort130-x64-owned-r5/cargo-aggregate-env.json').read_text())['environment']);env.update(CARGO_BUILD_JOBS='8',CARGO_INCREMENTAL='1',CARGO_NET_OFFLINE='true',CARGO_TARGET_DIR=str(r/'artifacts/v190/p10-target'),AIRK_BUNDLED_MODEL_DIR=str(r/'artifacts/advanced-split/bundled-model-v2'))
rows=[('content','airk-mcp','help_content_manifest_matches_native_owners','AIRK_WRITE_HELP_CONTENT'),('help-native','airk-mcp','ui_pilot_help_native_cases','AIRK_WRITE_UI_PILOT_HELP'),('help-cli','airkctl','ui_pilot_cli_semantic_cases','AIRK_WRITE_UI_PILOT_CLI'),('help-i3','airk-https','ui_pilot_i3_semantic_cases','AIRK_WRITE_UI_PILOT_I3')]
for label,crate,selector,flag in rows:
 cmd=['C:/Users/tdotn/.cargo/bin/cargo.exe','test','-p',crate,'--lib','--all-features',selector,'--locked','--timings'];e=dict(env);e[flag]='1';start=time.monotonic();log=out/(label+'-generation.log');assert not log.exists()
 with log.open('w') as stream:res=subprocess.run(cmd,env=e,stdout=stream,stderr=subprocess.STDOUT)
 receipt={'command':cmd,'generation_variable':flag,'exit_code':res.returncode,'elapsed_seconds':time.monotonic()-start,'target_directory':env['CARGO_TARGET_DIR'],'incremental':'1','scope':'native Help generation only; output identities require following frozen drift controls','log_sha256':hashlib.sha256(log.read_bytes()).hexdigest()};(out/(label+'-generation.json')).write_text(json.dumps(receipt,indent=2)+'\n');print(json.dumps(receipt),flush=True)
 if res.returncode:raise SystemExit(res.returncode)
