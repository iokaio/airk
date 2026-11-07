// SPDX-License-Identifier: Apache-2.0
const {test}=require('node:test'),assert=require('node:assert/strict');
const {progressIdle,waitForIndexIdle}=require('../native-editor-idle.cjs');
const expected={project:'a'.repeat(64),service:'b'.repeat(32)};
function idle(){const id='c'.repeat(64);return {status:'progress',project:expected.project,queued_jobs:[],queue_truncated:false,observed_unix_ms:10,project_paused:false,pending_paused:null,pending_job:null,observation:{worker:{service:expected.service,worker:'d'.repeat(32),project:expected.project,fencing_generation:2},current_fencing_generation:2,progress:{job:id},job:{id,project:expected.project,kind:'index',state:'succeeded',fencing_generation:2}}};}
test('idle requires explicit complete queue and a succeeded current native work observation',()=>{
    assert.equal(progressIdle(idle(),expected),true);
    for(const change of [v=>{v.observation=null;},v=>{v.queue_truncated=true;},v=>{v.project_paused=true;},v=>{v.observation.job.state='running';},v=>{v.observation.job.kind='inspect';},v=>{v.observation.current_fencing_generation=3;},v=>{v.pending_job={id:'e'.repeat(64),project:expected.project,state:'pending'};v.pending_paused=false;},v=>{v.queued_jobs=[{id:'e'.repeat(64),project:expected.project,state:'pending'}];}]){const value=idle();change(value);assert.equal(progressIdle(value,expected),false);}
});
test('crossed worker/backing, absent completeness, and failed/canceled work never qualify',()=>{
    for(const change of [v=>{v.project='e'.repeat(64);},v=>{v.observation.worker.service='e'.repeat(32);},v=>{v.observation.job.project='e'.repeat(64);},v=>{delete v.queued_jobs;},v=>{delete v.queue_truncated;},v=>{v.observation.job.state='failed';},v=>{v.observation.job.state='canceled';},v=>{v.observation.progress.job='e'.repeat(64);}]){const value=idle();change(value);assert.throws(()=>progressIdle(value,expected));}
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
