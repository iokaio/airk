# SPDX-License-Identifier: Apache-2.0
import importlib.util
import json
import os
from pathlib import Path
import statistics
import tempfile
import time

def load(name, file):
    spec=importlib.util.spec_from_file_location(name,Path('scripts')/file)
    module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
    return module

clients=load('timing_clients','check-service-clients.py')
bench=load('timing_bench','benchmark-mcp-investigation.py')
rows=[]
for candidate in ('p2b-qualified-runtime','p4-router-runtime'):
    runtime=Path('artifacts/v190',candidate).resolve()
    with tempfile.TemporaryDirectory(prefix='airk-timing-owned-',dir=os.environ['AIRK_PRIVATE_CENSUS_BASE']) as temporary:
        owner=Path(temporary);fixture=owner/'fixture';bench.create_fixture(fixture)
        root,manifest=bench.load_fixture(fixture);state=owner/'state';state.mkdir()
        with clients.Fixture(runtime,owner) as service:
            service.register('timing',root,state)
            service.configure('timing',{'operation':'vectors','enabled':False})
            service.configure('timing',{'operation':'detail','level':'agent_lean'})
            service.configure('timing',{'operation':'usage','mode':'aggregates_only'})
            service.job('timing','index');service.published_status('timing',time.monotonic()+90)
            config={'command':str(service.binary),'args':[*service.arguments('timing'),'mcp'],'expected_version':'1.9.0'}
            if candidate.startswith('p4'):config['verification_override']='off'
            hashes={row['path']:row['sha256'] for row in manifest['inventory']}
            usage=lambda:json.loads(service.run(*service.arguments('timing'),'usage','report').stdout)['report']['operations']
            for cohort in ('fresh','reused'):
                before=usage();trials=[];session=None
                try:
                    if cohort=='reused':session=bench.Session(config,30).__enter__()
                    for _ in range(6):
                        current=session or bench.Session(config,30).__enter__()
                        try:trials.append(bench.run_trial(current,1,manifest['oracle'],hashes))
                        finally:
                            if session is None:current.__exit__(None,None,None)
                finally:
                    if session:session.__exit__(None,None,None)
                after=usage()
                native={name:{'calls':summary['calls']-before.get(name,{}).get('calls',0),
                              'elapsed_ms':(int(summary['total_elapsed_us'])-int(before.get(name,{}).get('total_elapsed_us',0)))/1000}
                        for name,summary in after.items() if summary['calls']>before.get(name,{}).get('calls',0)}
                row={'runtime':candidate,'cohort':cohort,'batch_ms':bench.distribution([t['task_ms'] for t in trials]),'native':native,
                     'tool_medians':{task:statistics.median(c['elapsed_ms'] for t in trials for c in t['calls'] if c['task_id']==task) for task in bench.TASK_IDS}}
                rows.append(row);print(json.dumps(row),flush=True)
Path('artifacts/v190/p4-native-timing-diagnostic.json').write_text(json.dumps({'diagnostic_only':True,'recording':'aggregates_only','rows':rows},indent=2),encoding='utf-8')
