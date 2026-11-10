// SPDX-License-Identifier: Apache-2.0
// Qualification precondition only. A dated observation never grants mutation admission.
function requireValue(value,message){if(!value)throw Error(message);}
const uint=value=>Number.isSafeInteger(value)&&value>=0;
function idleEvidencePrefix(role,sequence,index){
    requireValue(['a','b'].includes(role)&&Number.isSafeInteger(sequence)&&sequence>=1&&sequence<=256&&/^idx_[a-f0-9]{32}$/.test(index),'Invalid exact idle observation identity');
    return `${role}-ui-${sequence}-${index}`;
}
function verifyHealth(value,expected){
    const observed=value?.observation;
    requireValue(value?.status==='health'&&observed?.version===expected.version&&observed.protocol===1
        &&observed.runtime_sha256===expected.runtimeSha256&&observed.executable_sha256===expected.executableSha256
        &&observed.catalog_clock?.namespace===expected.namespace&&uint(observed.catalog_clock.revision)&&observed.policy?.accepting===true
        &&/^[a-f0-9]{32}$/.test(observed.instance),'Index idle service identity crossed its approved runtime/catalog');
    // Broker acceptance is a dated readiness precondition, never source mutation admission.
    return observed;
}
// A later caller may accept maintenance only after separately retaining its
// initial index-work proof for this exact backing and service incarnation.
function progressIdle(value,expected,{requireIndexWork=true}={}){
    requireValue(typeof requireIndexWork==='boolean','Index work requirement must be explicit boolean');
    requireValue(value?.status==='progress'&&value.project===expected.project,'Progress crossed the exact native backing');
    requireValue(Array.isArray(value.queued_jobs)&&typeof value.queue_truncated==='boolean'&&uint(value.observed_unix_ms),'Progress queue completeness or observation time is unavailable');
    requireValue(value.project_paused===true||value.project_paused===false,'Progress project pause state is unavailable');
    requireValue(value.pending_paused===null||typeof value.pending_paused==='boolean','Progress pending pause state is invalid');
    function job(row){requireValue(row&&row.project===expected.project&&/^[a-f0-9]{64}$/.test(row.id)&&['pending','running','succeeded','failed','canceled'].includes(row.state),'Progress job crossed its native binding or state');}
    if(value.pending_job!==null){job(value.pending_job);requireValue(value.pending_job.state==='pending','Pending job does not describe pending work');}
    for(const row of value.queued_jobs){job(row);requireValue(row.state==='pending','Queued job does not describe pending work');}
    const observed=value.observation;
    if(observed!==null){
        job(observed?.job);
        requireValue(observed.worker?.project===expected.project&&observed.worker.service===expected.service&&/^[a-f0-9]{32}$/.test(observed.worker.worker),'Progress worker crossed its service or backing');
        requireValue(uint(observed.current_fencing_generation)&&observed.current_fencing_generation>0&&uint(observed.worker.fencing_generation)&&observed.worker.fencing_generation>0&&observed.progress?.job===observed.job.id,'Progress worker/job fencing is invalid');
        requireValue(uint(observed.job.fencing_generation)&&observed.job.fencing_generation===observed.worker.fencing_generation,'Progress job is not bound to its observed worker');
        if(['failed','canceled'].includes(observed.job.state))throw Error('Initial accepted indexing work did not succeed');
    }
    // A missing/old observation is never evidence that the queue is idle.
    return value.queue_truncated===false&&value.pending_job===null&&value.queued_jobs.length===0&&value.project_paused===false
        &&value.pending_paused!==true&&observed!==null&&observed.job.state==='succeeded'
        &&(requireIndexWork?['model_setup','index','reconcile','vectors']:['model_setup','index','reconcile','vectors','verify','inspect','recover','maintain']).includes(observed.job.kind)
        &&observed.worker.fencing_generation===observed.current_fencing_generation;
}
async function waitForIndexIdle(read,expected,{deadline,maxObservations=128,record=async()=>{},now=Date.now,pause=ms=>new Promise(resolve=>setTimeout(resolve,ms)),requireIndexWork=true}){
    requireValue(typeof requireIndexWork==='boolean','Index work requirement must be explicit boolean');
    // Automatic maintenance (for example verify) can replace the worker's latest
    // observation before the queue drains. Retain the initial index-work proof
    // observed earlier in this same wait for the same service and fencing
    // generation; a wait that never observed it still cannot qualify.
    let indexWork;
    for(let count=1;count<=maxObservations;count++){
        requireValue(now()<deadline,'Index idle observation deadline exhausted');
        const observed=await read();await record(observed,count);
        const latest=observed?.observation;
        if(indexWork&&latest&&(latest.worker?.service!==indexWork.worker.service||latest.current_fencing_generation!==indexWork.worker.fencing_generation))indexWork=undefined;
        if(progressIdle(observed,expected,{requireIndexWork:requireIndexWork&&!indexWork}))return {observations:count,progress:observed,...(requireIndexWork&&indexWork?{initial_index_work:indexWork}:{}),qualification:'dated_read_only_precondition_not_mutation_admission'};
        const seen=observed.observation;
        if(requireIndexWork&&!indexWork&&seen&&seen.job.state==='succeeded'&&['model_setup','index','reconcile','vectors'].includes(seen.job.kind)
            &&seen.worker.fencing_generation===seen.current_fencing_generation)indexWork={observation:count,job:structuredClone(seen.job),worker:structuredClone(seen.worker)};
        requireValue(now()<deadline,'Index idle observation deadline exhausted');await pause(Math.min(750,deadline-now()));
    }
    throw Error('Index idle observation request budget exhausted');
}
module.exports={verifyHealth,progressIdle,waitForIndexIdle,idleEvidencePrefix};
