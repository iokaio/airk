# SPDX-License-Identifier: Apache-2.0
import importlib.util
import json
import os
from pathlib import Path
import tempfile
import time

def load(name, filename):
    spec = importlib.util.spec_from_file_location(name, Path('scripts') / filename)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module

clients = load('probe_clients', 'check-service-clients.py')
bench = load('probe_bench', 'benchmark-mcp-investigation.py')
rows = []
for candidate in ('p2b-qualified-runtime', 'p4-qualified-runtime'):
    runtime = Path('artifacts/v190', candidate).resolve()
    with tempfile.TemporaryDirectory(prefix='airk-caps-probe-', dir=os.environ['AIRK_PRIVATE_CENSUS_BASE']) as temporary:
        owner = Path(temporary)
        root, state = owner/'source', owner/'state'
        root.mkdir(); state.mkdir()
        (root/'sample.md').write_text('# Example\nSaved bytes.\n', encoding='utf-8')
        with clients.Fixture(runtime, owner) as service:
            service.register('probe', root, state)
            service.configure('probe', {'operation':'vectors','enabled':False})
            service.job('probe','index')
            service.published_status('probe', time.monotonic()+90)
            config={'command':str(service.binary),'args':[*service.arguments('probe'),'mcp'],'expected_version':'1.9.0'}
            with bench.Session(config,30) as session:
                for rich in (False, True):
                    reply,_ = session.request('tools/call',{'name':'capabilities','arguments':{'rich':rich,'response_mode':'full'}})
                    body = bench.body(reply)
                    rows.append({'runtime':candidate,'rich':rich,'is_error':reply.get('isError'),
                                 'reply_bytes':len(bench.canonical(reply)), 'body_keys':list(body),
                                 'error_content':reply.get('content') if reply.get('isError') else None})
                    print(json.dumps(rows[-1]),flush=True)
            service.log.flush()
            Path('artifacts/v190',candidate+'-caps-service.log').write_bytes((owner/'service.stderr').read_bytes())
Path('artifacts/v190/capability-probe.json').write_text(json.dumps(rows,indent=2),encoding='utf-8')
