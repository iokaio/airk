# SPDX-License-Identifier: Apache-2.0
"""Future opt-in adapter around unchanged Root-owned composed controller.

No process launched by preparation/tests. Root must mount/build/review journal
first, then pin this source/helper/driver plus actual Ready in the new plan.
"""
import hashlib,importlib.util,json,sys,time
from pathlib import Path
from wait_warm import WarmWait
ROOT=Path(__file__).resolve().parents[3]
DRIVER=ROOT/'artifacts/advanced-split/actual-composed-service-preparation-r1/run_composed.py'
HELPER=ROOT/'artifacts/advanced-split/stage-service-preparation-r1/cal-execution-r102/registry_events.py'
def load(name,path):
    spec=importlib.util.spec_from_file_location(name,path);module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module);return module
def main(plan_path,output):
    plan=json.loads(Path(plan_path).read_text(encoding='utf-8'));watchers={};events=[]
    sys.path.insert(0,str(DRIVER.parent));driver=load('original_composed',DRIVER);native_events=load('original_directory_events',HELPER)
    required=[Path(__file__),Path(__file__).with_name('wait_warm.py'),DRIVER,HELPER]
    for path in required:
        if plan['pins'].get(str(path))!=hashlib.sha256(path.read_bytes()).hexdigest():raise ValueError('adapter source not bound in original plan')
    original=driver.await_ready
    try:
        for number,step in enumerate(plan['steps']):
            if step['kind']!='await_ready':continue
            if not 0<step['deadline_seconds']<=30:raise ValueError('original 30s warm ceiling required')
            ready_ref=step['warm_fixture_ready'];ready_path=Path(ready_ref['path'])
            if plan['pins'].get(str(ready_path))!=ready_ref['sha256'] or hashlib.sha256(ready_path.read_bytes()).hexdigest()!=ready_ref['sha256']:raise ValueError('actual Ready identity not pinned')
            ready=json.loads(ready_path.read_bytes());event=native_events.RegistryEvents(ready_path.parent);events.append(event)
            waiter=WarmWait(ready_path.parent,ready,event);arm=waiter.arm();watchers[number]=(waiter,arm)
        def observed_ready(step,rows,starts,directory,number,allowed,remaining):
            waiter,arm=watchers[number];configure=step['configuration_observation_step']
            raw=directory/f'{configure:03d}-stdout.bin'
            if hashlib.sha256(raw.read_bytes()).hexdigest()!=rows[configure]['result']['stdout_sha256']:raise ValueError('original Configure changed')
            configuration=json.loads(raw.read_bytes())['configuration']
            deadline=int((starts[configure]+step['deadline_seconds'])*1e9);result=None
            def status_once(budget):
                nonlocal result
                subprocess=driver.subprocess;calls=0
                class Once:
                    def __getattr__(self,key):return getattr(subprocess,key)
                    def run(self,*args,**kwargs):
                        nonlocal calls
                        calls+=1
                        if calls!=1:raise ValueError('matching completion not corroborated by first native status; no retry')
                        kwargs['timeout']=min(kwargs['timeout'],budget);return subprocess.run(*args,**kwargs)
                driver.subprocess=Once()
                try:result=original(step,rows,starts,directory,number,allowed,remaining)
                finally:driver.subprocess=subprocess
                return json.loads(Path(result['ready_observation']['path']).read_bytes())
            try:
                waiter.wait_ready(configuration,step['stage'],step['profile_manifest'],deadline,remaining,status_once)
                return dict(result,native_warm_journal=waiter.trace[-1],preconfigure_arm=arm)
            finally:
                (directory/f'{number:03d}-native-warm-wakeup.json').write_text(json.dumps({'schema':'airk.private.warm-wakeup/1','preconfigure_arm':arm,'deadline_monotonic_ns':deadline,'observations':waiter.trace,'release_qualified':False},indent=2)+'\n',encoding='utf-8')
        driver.await_ready=observed_ready;driver.run(plan_path,output)
    finally:
        for event in events:event.close()
if __name__=='__main__':main(*sys.argv[1:])
