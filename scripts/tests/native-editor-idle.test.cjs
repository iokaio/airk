// SPDX-License-Identifier: Apache-2.0
const {test}=require('node:test'),assert=require('node:assert/strict');
const {verifyHealth,progressIdle,waitForIndexIdle,idleEvidencePrefix}=require('../native-editor-idle.cjs');
test('repeated idle observations retain distinct evidence across requests, indexes and windows',()=>{
    const first='idx_'+'a'.repeat(32),second='idx_'+'b'.repeat(32);
    const names=[idleEvidencePrefix('a',1,first),idleEvidencePrefix('a',2,first),idleEvidencePrefix('a',1,second),idleEvidencePrefix('b',1,first)];
    assert.equal(new Set(names).size,4);
    for(const input of [['../',1,first],['a',0,first],['a',257,first],['a',1,'../foreign']])assert.throws(()=>idleEvidencePrefix(...input));
});
// Shape retained from native-ui-same1-complete/native-ui-failure-settlement.json
// health-exact-owned-control; identities and build text redacted, no synthetic grant_policy.
function actualHealth(){return {status:'health',observation:{catalog_clock:{namespace:'a'.repeat(32),revision:4},storage_formats:{index:1,service_state:1,logs:1,query_projection_records:4},build:{source_sha256:'b'.repeat(64),source_files:978,profile:'release',rustc:'redacted'},build_error:null,logs:{available:true,dropped_records:0,write_failures:0},version:'1.8.12',protocol:1,instance:'c'.repeat(32),process_id:24909,executable_sha256:'d'.repeat(64),runtime_sha256:'e'.repeat(64),user:'redacted',uptime_ms:200487,policy:{service:'c'.repeat(32),accepting:true,projects:1,draining_projects:1,pending_connections:0,allocated:{concurrent_queries:0,response_bytes:0},active_queries:0,reserved_response_bytes:0}}};}
test('real public Health DTO qualifies identity without inventing mutation admission',()=>{
    const value=actualHealth(),expected={version:'1.8.12',namespace:'a'.repeat(32),executableSha256:'d'.repeat(64),runtimeSha256:'e'.repeat(64)};
    assert.equal(Object.hasOwn(value.observation,'grant_policy'),false);
    assert.equal(verifyHealth(value,expected),value.observation);
    value.observation.policy.accepting=false;
    assert.throws(()=>verifyHealth(value,expected));
    for(const change of [v=>{v.status='failure';},v=>{v.observation.version='1.8.7';},v=>{v.observation.protocol=2;},v=>{v.observation.runtime_sha256='f'.repeat(64);},v=>{v.observation.executable_sha256='f'.repeat(64);},v=>{v.observation.catalog_clock.namespace='f'.repeat(32);},v=>{delete v.observation.catalog_clock;},v=>{v.observation.catalog_clock.revision=-1;},v=>{v.observation.instance='bad';}]){const crossed=actualHealth();change(crossed);assert.throws(()=>verifyHealth(crossed,expected));}
});
const expected={project:'a'.repeat(64),service:'b'.repeat(32)};
function idle(){const id='c'.repeat(64);return {status:'progress',project:expected.project,queued_jobs:[],queue_truncated:false,observed_unix_ms:10,project_paused:false,pending_paused:null,pending_job:null,observation:{worker:{service:expected.service,worker:'d'.repeat(32),project:expected.project,fencing_generation:2},current_fencing_generation:2,progress:{job:id},job:{id,project:expected.project,kind:'index',state:'succeeded',fencing_generation:2}}};}
test('idle requires explicit complete queue and a succeeded current native work observation',()=>{
    assert.equal(progressIdle(idle(),expected),true);
    for(const change of [v=>{v.observation=null;},v=>{v.queue_truncated=true;},v=>{v.project_paused=true;},v=>{v.observation.job.state='running';},v=>{v.observation.job.kind='inspect';},v=>{v.observation.current_fencing_generation=3;},v=>{v.pending_job={id:'e'.repeat(64),project:expected.project,state:'pending'};v.pending_paused=false;},v=>{v.queued_jobs=[{id:'e'.repeat(64),project:expected.project,state:'pending'}];}]){const value=idle();change(value);assert.equal(progressIdle(value,expected),false);}
});
test('crossed worker/backing, absent completeness, and failed/canceled work never qualify',()=>{
    for(const change of [v=>{v.project='e'.repeat(64);},v=>{v.observation.worker.service='e'.repeat(32);},v=>{v.observation.job.project='e'.repeat(64);},v=>{delete v.queued_jobs;},v=>{delete v.queue_truncated;},v=>{v.observation.job.state='failed';},v=>{v.observation.job.state='canceled';},v=>{v.observation.progress.job='e'.repeat(64);}]){const value=idle();change(value);assert.throws(()=>progressIdle(value,expected));assert.throws(()=>progressIdle(value,expected,{requireIndexWork:false}));}
});

test('later idle may opt into canonical maintenance kinds without qualifying initial indexing',()=>{
    for(const kind of ['index','reconcile','vectors','model_setup','verify','inspect','recover','maintain']){
        const value=idle();value.observation.job.kind=kind;
        assert.equal(progressIdle(value,expected),['index','reconcile','vectors','model_setup'].includes(kind));
        assert.equal(progressIdle(value,expected,{requireIndexWork:false}),true);
    }
    for(const change of [v=>{v.observation=null;},v=>{v.queue_truncated=true;},v=>{v.project_paused=true;},v=>{v.observation.job.state='running';},v=>{v.observation.current_fencing_generation=3;},v=>{v.observation.job.kind='invented';},v=>{v.pending_job={id:'e'.repeat(64),project:expected.project,state:'pending'};},v=>{v.queued_jobs=[{id:'e'.repeat(64),project:expected.project,state:'pending'}];}]){
        const value=idle();value.observation.job.kind='verify';change(value);
        assert.equal(progressIdle(value,expected,{requireIndexWork:false}),false);
    }
    assert.throws(()=>progressIdle(idle(),expected,{requireIndexWork:'false'}),/boolean/);
});

test('later idle mode is explicit in the waiter and retains its actual maintenance observation',async()=>{
    const value=idle();value.observation.job.kind='verify';let reads=0,time=0;const retained=[];
    const result=await waitForIndexIdle(async()=>{reads++;return value;},expected,{deadline:1000,requireIndexWork:false,now:()=>time,record:async sample=>retained.push(structuredClone(sample))});
    assert.equal(reads,1);assert.deepEqual(result.progress,value);assert.deepEqual(retained,[value]);
    reads=0;
    await assert.rejects(waitForIndexIdle(async()=>{reads++;return value;},expected,{deadline:10000,maxObservations:2,now:()=>time,pause:async ms=>{time+=ms;}}),/request budget/);
    assert.equal(reads,2,'Default initial indexing qualification remains unchanged');
});
test('precondition polls only read observations, retains every complete sample, then permits one caller review',async()=>{
    const pending=idle();pending.pending_job={id:'e'.repeat(64),project:expected.project,state:'pending'};
    const running=idle();running.observation.job.state='running';const values=[pending,running,idle()],retained=[];let time=0,reads=0,reviews=0;
    const result=await waitForIndexIdle(async()=>values[reads++],expected,{deadline:5000,now:()=>time,pause:async ms=>{time+=ms;},record:async value=>retained.push(structuredClone(value))});reviews++;
    assert.equal(reads,3);assert.equal(reviews,1);assert.deepEqual(retained,values);assert.equal(result.qualification,'dated_read_only_precondition_not_mutation_admission');
});
test('paused/pending work exhausts bounded reads without a policy retry or cancellation',async()=>{
    const value=idle();value.project_paused=true;let reads=0,time=0,mutations=0;
    await assert.rejects(waitForIndexIdle(async()=>{reads++;return value;},expected,{deadline:10000,maxObservations:2,now:()=>time,pause:async ms=>{time+=ms;}}),/request budget/);
    assert.equal(reads,2);assert.equal(mutations,0);
    await assert.rejects(waitForIndexIdle(async()=>value,expected,{deadline:0,now:()=>0}),/deadline/);
});
test('initial index-work proof observed earlier in the same wait survives automatic maintenance replacing the latest observation',async()=>{
    // Sequence retained from the installed different-window run: vectors succeeded with verify pending, then verify succeeded and idle.
    const first=idle();first.observation.job.kind='vectors';first.pending_job={id:'e'.repeat(64),project:expected.project,state:'pending'};first.pending_paused=false;
    const later=idle();later.observation.job.kind='verify';later.observation.job.id='e'.repeat(64);later.observation.progress.job='e'.repeat(64);
    let reads=0,time=0;const values=[first,later];
    const result=await waitForIndexIdle(async()=>values[reads++],expected,{deadline:5000,now:()=>time,pause:async ms=>{time+=ms;}});
    assert.equal(reads,2);assert.deepEqual(result.progress,later);assert.equal(result.initial_index_work.observation,1);assert.equal(result.initial_index_work.job.kind,'vectors');
    // A new fencing generation discards the earlier proof.
    const refenced=structuredClone(later);refenced.observation.current_fencing_generation=3;refenced.observation.worker.fencing_generation=3;refenced.observation.job.fencing_generation=3;
    reads=0;time=0;const crossed=[first,refenced,refenced];
    await assert.rejects(waitForIndexIdle(async()=>crossed[Math.min(reads++,2)],expected,{deadline:10000,maxObservations:3,now:()=>time,pause:async ms=>{time+=ms;}}),/request budget/);
    // Failed initial work still refuses.
    const failed=idle();failed.observation.job.state='failed';
    await assert.rejects(waitForIndexIdle(async()=>failed,expected,{deadline:5000,now:()=>0}),/did not succeed/);
});
