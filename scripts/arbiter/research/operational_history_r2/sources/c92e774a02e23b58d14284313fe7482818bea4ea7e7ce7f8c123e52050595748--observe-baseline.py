# SPDX-License-Identifier: Apache-2.0
import hashlib, json, os, subprocess, time
from pathlib import Path
owner=Path.cwd()/'artifacts/mcp-optimizations-1.8.3'
binary=owner/'baseline-runtime/airk.exe'
install=owner/'baseline-install'
source=owner/'frozen-fixture/source'
data=owner/'baseline-project-data'
base=['--service-install-dir',str(install),'--root',str(source),'--data-dir',str(data),'--collection','mcp-optimization-fixture']
env=dict(os.environ,HTTP_PROXY='http://127.0.0.1:1',HTTPS_PROXY='http://127.0.0.1:1',ALL_PROXY='http://127.0.0.1:1')
observations=[]
deadline=time.monotonic()+180
while True:
    result=subprocess.run([str(binary),*base,'status'],cwd=owner,env=env,capture_output=True,encoding='utf-8',timeout=60,creationflags=subprocess.CREATE_NO_WINDOW)
    if result.returncode:
        observations.append({'error':result.stderr,'exit_code':result.returncode})
        if time.monotonic()>=deadline: raise RuntimeError('baseline status failed')
    else:
        raw=json.loads(result.stdout)
        value=raw.get('structuredContent') or json.loads(raw['content'][0]['text'])
        observations.append(value)
        (owner/'baseline-status-observations.json').write_text(json.dumps(observations,indent=2)+'\n')
        print(json.dumps(value),flush=True)
        semantic=value.get('semantic_readiness',{})
        discovery=value.get('discovery_readiness',{})
        if semantic.get('state')=='ready' and discovery.get('state')=='ready':
            break
        if time.monotonic()>=deadline: raise RuntimeError('baseline semantic readiness deadline')
    time.sleep(3)
with (owner/'baseline-install/airk-launcher.exe').open('rb') as stream:
    launcher_sha256=hashlib.file_digest(stream,'sha256').hexdigest()
config={'command':str(owner/'baseline-install/airk-launcher.exe'),'args':['runtime','launch','--install-dir',str(install),'--',*base[2:],'mcp'],
    'cwd':str(owner),'expected_version':'1.8.2','launcher_sha256':launcher_sha256,'route':'installed-launcher'}
(owner/'baseline-launch.json').write_text(json.dumps(config,indent=2)+'\n')
print('Saved explicit private installed-launcher configuration.',flush=True)
