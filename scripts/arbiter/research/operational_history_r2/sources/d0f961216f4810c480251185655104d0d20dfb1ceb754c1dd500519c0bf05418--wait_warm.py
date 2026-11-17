# SPDX-License-Identifier: Apache-2.0
"""Future fixture-only wakeup adapter. No scoring, sleeps or native DTO mirrors.

Call arm() BEFORE the original Configure. Install the directory watcher before
arm, and close it after wait_ready. Native status remains the final authority.
The existing RegistryEvents(work), never RegistryEvents(control), supplies wait.
"""
import hashlib
import json
import time
from pathlib import Path

SCHEMA='airk.private.arbiter-warm-completion/1'
class WarmWait:
    def __init__(self, work, ready, events, clock=time.monotonic_ns):
        self.work=Path(work);self.observer=ready.get('warm_observer_id');self.events=events;self.clock=clock
        if not self.observer or ready.get('evaluation') is not True or ready.get('release_qualified') is not False:
            raise ValueError('actual opt-in fixture Ready required')
        self.floor=None;self.trace=[]
    def records(self):
        raw=(self.work/'warm-completions.jsonl').read_bytes()
        if len(raw)>8*1024*1024:raise ValueError('bounded journal exceeded')
        # A flush can be observed between write and final newline: only complete
        # records are consumed. The next actual directory event wakes a rescan.
        lines=raw.split(b'\n')[:-1]
        if len(lines)>128:raise ValueError('bounded journal roster exceeded')
        result=[];sequence=0
        for line in lines:
            if len(line)>65536:raise ValueError('bounded record exceeded')
            row=json.loads(line)
            if row['schema']!=SCHEMA or row['observer_id']!=self.observer or row['sequence']!=sequence+1:
                raise ValueError('foreign/missing/reordered native journal record')
            sequence=row['sequence'];result.append(row)
        return result
    def arm(self):
        if self.floor is not None:raise ValueError('attempt already armed')
        rows=self.records();self.floor=rows[-1]['sequence'] if rows else 0
        return {'observer_id':self.observer,'sequence_floor':self.floor}
    def wait_ready(self,configuration,stage,manifest,deadline_ns,remaining,invoke_status):
        if self.floor is None:raise ValueError('pre-Configure arm required')
        if stage not in ('focus','context') or configuration[stage]['enabled'] is not True or configuration[stage]['selected_profile']!=manifest:
            raise ValueError('exact native Configure consent required')
        while True:
            budget=min(remaining(),(deadline_ns-self.clock())/1e9)
            if budget<=0:raise TimeoutError('original warm/whole deadline expired')
            for row in self.records():
                if row['sequence']<=self.floor or row['stage']!=stage or row['expected_configuration']!=configuration or row['profile_manifest']!=manifest:continue
                if row['outcome']!='ready':raise ValueError('native warm failed or superseded')
                if (row['activation_path'] not in ('retained_resident','native_warm_admitted') or row['canceled'] or not row['operation_succeeded']
                    or row['phase']!='ready' or row['active_profile']!=manifest
                    or row['native_configuration_revision']!=configuration['revision']
                    or row['lifecycle_serial']!=row['current_lifecycle_serial']
                    or row['accepted_configure_revision']!=row['captured_configure_revision']):
                    raise ValueError('stale native completion cannot grant Ready')
                budget=min(remaining(),(deadline_ns-self.clock())/1e9)
                if budget<=0:raise TimeoutError('original deadline expired before status')
                # ONE canonical status call; callback retains original argv,
                # stdout/stderr/native identity and enforces this timeout.
                status=invoke_status(budget)
                if self.clock()>=deadline_ns or remaining()<=0:raise TimeoutError('Ready observed beyond original deadline')
                observed=status[stage]
                if status.get('schema')!='airk.arbiter.status/2' or status.get('initialization') is not None or status['configuration']!=configuration or observed['availability']!='ready' or observed['selected_profile']!=manifest or observed['active_profile']!=manifest:
                    raise ValueError('canonical status did not corroborate exact current Ready')
                self.trace.append({'matched_native_event':row,'status_calls':1,'journal_sha256_at_observation':hashlib.sha256((self.work/'warm-completions.jsonl').read_bytes()).hexdigest(),'authority':'canonical_status','release_qualified':False})
                return status
            closed=self.work/'warm-completions.closed.json'
            if closed.exists():raise ValueError('fixture journal closed without matching completion')
            # The original parent deadline may expire before warm deadline.
            parent_remaining=remaining()
            effective_deadline=min(deadline_ns,self.clock()+max(0,int(parent_remaining*1e9)))
            if effective_deadline<=self.clock():raise TimeoutError("original whole deadline expired before event wait")
            self.events.wait(effective_deadline)
