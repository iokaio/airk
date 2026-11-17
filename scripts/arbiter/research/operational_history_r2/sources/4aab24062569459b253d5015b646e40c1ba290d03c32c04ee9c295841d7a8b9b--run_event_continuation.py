# SPDX-License-Identifier: Apache-2.0
"""Reuse unchanged driver; adapt only its Ready callback to proven change events."""
import importlib.util
import json
from pathlib import Path
import sys
import time

ROOT=Path(__file__).resolve().parents[4]
DRIVER=ROOT/'artifacts/advanced-split/actual-composed-service-preparation-r1/run_composed.py'
HELPER=ROOT/'artifacts/advanced-split/stage-service-preparation-r1/cal-execution-r102/registry_events.py'

def load(name,path):
    spec=importlib.util.spec_from_file_location(name,path);module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module);return module

def main(plan_path,output):
    sys.path.insert(0,str(DRIVER.parent))
    driver=load('original_composed_driver',DRIVER);events_module=load('original_registry_events',HELPER)
    original=driver.await_ready
    def event_ready(step,rows,starts,directory,number,allowed,remaining):
        # Configure's retained monotonic start belongs to THIS original scenario.
        deadline=int((starts[step['configuration_observation_step']]+step['deadline_seconds'])*1_000_000_000)
        argv=step['argv'];control=argv[argv.index('--service-runtime-dir')+1]
        events=events_module.RegistryEvents(control);subprocess=driver.subprocess;count=0;waits=[]
        class EventCalls:
            def __getattr__(self,key):return getattr(subprocess,key)
            def run(self,*args,**kwargs):
                nonlocal count
                if count:
                    began=time.time_ns()//1_000_000;events.wait(deadline)
                    waits.append({'started_unix_ms':began,'completed_unix_ms':time.time_ns()//1_000_000})
                budget=min(remaining(),(deadline-time.monotonic_ns())/1_000_000_000)
                if budget<=0:raise TimeoutError('original Ready deadline exhausted before status')
                kwargs['timeout']=min(kwargs['timeout'],budget);count+=1
                return subprocess.run(*args,**kwargs)
        driver.subprocess=EventCalls()
        try:return original(step,rows,starts,directory,number,allowed,remaining)
        finally:
            driver.subprocess=subprocess
            try:events.close()
            finally:(directory/f'{number:03d}-ready-event-waits.json').write_text(json.dumps({'helper':str(HELPER),'deadline_monotonic_ns':deadline,'status_calls':count,'waits':waits,'authority':False},indent=2)+'\n',encoding='utf-8')
    driver.await_ready=event_ready
    driver.run(plan_path,output)

if __name__=='__main__':main(*sys.argv[1:])
