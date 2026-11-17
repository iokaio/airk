# SPDX-License-Identifier: Apache-2.0
import os,subprocess
base={k:v for k,v in os.environ.items() if not k.upper().startswith(('GIT_','GH_','GITHUB_','GCM_TRACE'))};base.update(GIT_TERMINAL_PROMPT='0',GCM_INTERACTIVE='never',GH_HOST='github.com')
result=subprocess.run(['git','credential','fill'],input='protocol=https\nhost=github.com\npath=iokaio/airk.git\n\n',text=True,capture_output=True,env=base,timeout=30)
credential=dict(line.split('=',1) for line in result.stdout.splitlines() if '=' in line)
if result.returncode or not credential.get('password'):
 print('existing_git_credential_unavailable');raise SystemExit(1)
base['GH_TOKEN']=credential['password'];credential.clear();result=None
check=subprocess.run(['gh','api','user','--jq','.login'],text=True,capture_output=True,env=base,timeout=30)
print('github_existing_git_auth='+('ok login='+check.stdout.strip() if check.returncode==0 else 'unavailable'))
if check.returncode:raise SystemExit(1)
workflows=subprocess.run(['gh','api','repos/iokaio/airk/actions/workflows','--jq','.workflows[] | [.id,.name,.state] | @tsv'],text=True,capture_output=True,env=base,timeout=30)
if workflows.returncode==0:print(workflows.stdout.strip())
else:print('workflow_read_unavailable')
