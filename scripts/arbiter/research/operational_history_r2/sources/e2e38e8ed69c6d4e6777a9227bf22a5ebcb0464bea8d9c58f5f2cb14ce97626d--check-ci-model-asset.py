# SPDX-License-Identifier: Apache-2.0
import json,os,subprocess
base={k:v for k,v in os.environ.items() if not k.upper().startswith(('GIT_','GH_','GITHUB_','GCM_TRACE'))};base.update(GIT_TERMINAL_PROMPT='0',GCM_INTERACTIVE='never',GH_HOST='github.com')
r=subprocess.run(['git','credential','fill'],input='protocol=https\nhost=github.com\npath=iokaio/airk.git\n\n',text=True,capture_output=True,env=base,timeout=30);creds=dict(line.split('=',1) for line in r.stdout.splitlines() if '=' in line);assert r.returncode==0 and creds.get('password');base['GH_TOKEN']=creds['password'];creds.clear();r=None
r=subprocess.run(['gh','api','repos/iokaio/airk/actions/variables/AIRK_MODEL_BUILD_INPUT_ASSET_ID'],capture_output=True,text=True,env=base,timeout=30)
if r.returncode:print('official_model_asset_variable_unavailable');raise SystemExit(1)
v=json.loads(r.stdout);identifier=v.get('value','');assert identifier.isdigit();print(json.dumps({'official_model_asset_id':identifier}))
r=subprocess.run(['gh','api','repos/iokaio/airk/releases/assets/'+identifier,'--jq','{id,name,state,size,digest}'],capture_output=True,text=True,env=base,timeout=30);print(r.stdout.strip() if r.returncode==0 else 'official_model_asset_metadata_unavailable')
