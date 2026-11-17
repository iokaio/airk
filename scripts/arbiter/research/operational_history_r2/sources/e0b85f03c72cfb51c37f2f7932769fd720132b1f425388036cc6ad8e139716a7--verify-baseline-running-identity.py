# SPDX-License-Identifier: Apache-2.0
import importlib.util,json,time
from pathlib import Path
owner=Path.cwd()/'artifacts/mcp-optimizations-1.8.3'
spec=importlib.util.spec_from_file_location('bench',Path.cwd()/'scripts/benchmark-mcp-investigation.py')
bench=importlib.util.module_from_spec(spec);spec.loader.exec_module(bench)
config=json.loads((owner/'baseline-launch.json').read_text())
provenance=json.loads((owner/'baseline-provenance.json').read_text())
with bench.Session(config,30) as session:
    observations=[]
    for attempt in range(6):
        raw,_=session.request('tools/call',{'name':'diagnostics','arguments':{'response_mode':'full'}})
        value=bench.body(raw);observations.append(value)
        runtime=value.get('runtime',{})
        if all(runtime.get(k) is True for k in ['versions_match','payloads_match','builds_match']):
            break
        time.sleep(1)
    (owner/'baseline-running-identity-observations.json').write_text(json.dumps(observations,indent=2)+'\n')
    assert all(runtime.get(k) is True for k in ['versions_match','payloads_match','builds_match'])
    for item in [runtime['mcp_frontend']['process'],runtime['index_worker']['process'],runtime['service']]:
        assert item['version']=='1.8.2'
        assert item['executable_sha256']==provenance['main_sha256']
        assert item['payload_sha256']==provenance['install_manifest_sha256']
        assert item['build']['source_sha256']==provenance['source']['sha256']
    final={k:runtime[k] for k in ['versions_match','payloads_match','builds_match']}
    final.update(version='1.8.2',source=provenance['source'],main_sha256=provenance['main_sha256'],payload_sha256=provenance['install_manifest_sha256'])
assert session.cleanup['graceful_eof']
(owner/'baseline-running-identity.json').write_text(json.dumps(final,indent=2)+'\n')
print(json.dumps(final))
