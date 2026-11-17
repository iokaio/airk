# SPDX-License-Identifier: Apache-2.0
import importlib.util,json
from pathlib import Path
owner=Path.cwd()/'artifacts/mcp-optimizations-1.8.3'
spec=importlib.util.spec_from_file_location('bench',Path.cwd()/'scripts/benchmark-mcp-investigation.py')
bench=importlib.util.module_from_spec(spec);spec.loader.exec_module(bench)
path=owner/'baseline-launch.json'
config=json.loads(path.read_text())
config['env']={key:'http://127.0.0.1:1' for key in ['HTTP_PROXY','HTTPS_PROXY','ALL_PROXY']}
config['env']['AIRK_MCP_RESPONSE_MODE']='terse'
path.write_text(json.dumps(config,indent=2)+'\n')
source,fixture=bench.load_fixture(owner/'frozen-fixture')
config,_=bench.load_configuration(path,source)
with bench.Session(config,30) as session:
    first=session.first_status['observation']
    assert first['is_error'] is False and first['index_state']=='available'
    assert first['discovery_state']=='ready' and first['semantic_state']=='ready'
    result={'status':'private_installed_mcp_ready','version':session.version,'fixture_sha256':fixture['fixture_sha256'],
        'publication_token_sha256':first['publication_token_sha256'],'discovery':first['discovery_state'],'semantic':first['semantic_state']}
result['frontend_cleanup']=session.cleanup
assert session.cleanup['graceful_eof']
(owner/'baseline-launch-smoke.json').write_text(json.dumps(result,indent=2)+'\n')
print(json.dumps(result))
