# SPDX-License-Identifier: Apache-2.0
import json,os,subprocess
from pathlib import Path
owner=Path.cwd()/'artifacts/mcp-optimizations-1.8.3'
binary=owner/'baseline-runtime/airk.exe'
base=['--service-install-dir',str(owner/'baseline-install'),'--root',str(owner/'frozen-fixture/source'),
    '--data-dir',str(owner/'baseline-project-data'),'--collection','mcp-optimization-fixture']
env=dict(os.environ,HTTP_PROXY='http://127.0.0.1:1',HTTPS_PROXY='http://127.0.0.1:1',ALL_PROXY='http://127.0.0.1:1')
def call(*args):
    result=subprocess.run([str(binary),*base,*args],cwd=owner,env=env,capture_output=True,encoding='utf-8',timeout=60,creationflags=subprocess.CREATE_NO_WINDOW)
    if result.returncode:raise RuntimeError(result.stderr)
    raw=json.loads(result.stdout)
    return raw.get('structuredContent') or (json.loads(raw['content'][0]['text']) if raw.get('content') else raw)
policy={view:call('settings',view) for view in ['extraction_policy','vector_selection','vector_status','roots']}
(owner/'baseline-policy.json').write_text(json.dumps(policy,indent=2)+'\n')
diagnostics=call('tools','call','airk_diagnostics','--args','{"response_mode":"full"}')
(owner/'baseline-diagnostics.json').write_text(json.dumps(diagnostics,indent=2)+'\n')
runtime=diagnostics.get('runtime',{})
print('policy recorded; runtime evidence',json.dumps({key:runtime.get(key) for key in ['versions_match','payloads_match','builds_match']}))
print('extraction',json.dumps(policy['extraction_policy'])[:2000])
print('vector_selection',json.dumps(policy['vector_selection'])[:2000])
