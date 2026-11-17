# SPDX-License-Identifier: Apache-2.0
import json,os,subprocess
base={k:v for k,v in os.environ.items() if not k.upper().startswith(('GIT_','GH_','GITHUB_','GCM_TRACE'))};base.update(GIT_TERMINAL_PROMPT='0',GCM_INTERACTIVE='never',GH_HOST='github.com')
r=subprocess.run(['git','credential','fill'],input='protocol=https\nhost=github.com\npath=iokaio/airk.git\n\n',text=True,capture_output=True,env=base,timeout=30);creds=dict(line.split('=',1) for line in r.stdout.splitlines() if '=' in line);assert r.returncode==0 and creds.get('password');base['GH_TOKEN']=creds['password'];creds.clear();r=None
for endpoint in ['repos/iokaio/airk','repos/iokaio/airk/actions/variables','repos/iokaio/airk/releases?per_page=5']:
 r=subprocess.run(['gh','api',endpoint],capture_output=True,text=True,env=base,timeout=30)
 if r.returncode:
  print(json.dumps({'endpoint':endpoint,'status':'unavailable','reason':r.stderr.strip()[:400]}));continue
 obj=json.loads(r.stdout)
 if endpoint.endswith('/airk'):out={'private':obj.get('private'),'permissions':obj.get('permissions')}
 elif endpoint.endswith('/variables'):out={'total_count':obj.get('total_count'),'names':[x['name'] for x in obj.get('variables',[])]}
 else:out=[{'id':x['id'],'tag':x['tag_name'],'draft':x['draft'],'assets':[{'id':a['id'],'name':a['name'],'size':a['size']} for a in x['assets']]} for x in obj]
 print(json.dumps({'endpoint':endpoint,'metadata':out}))
