// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import { ConsoleHost, ServiceConsoleController } from '../serviceConsoleController';
import { consoleAction, diagnosticPreview, ServiceConsoleState } from '../serviceConsoleView';
import { managementView, panelDetails } from '../explorerManagement';

/** What Explorer Management shows for a console state (the Service Console view was removed in 1.5.0). */
const shown = (state: ServiceConsoleState): string => JSON.stringify(managementView(state));
import { record, RecordValue, ServiceClient, ServiceError } from '../serviceClient';
import { IntentJournal } from '../serviceIntents';
import { splitFixture, splitAtStage, splitFrozenFixture } from './fixtures/split';
import { splitExport } from '../indexingSplit';
const project='12'.repeat(32),namespace='34'.repeat(16),instance='56'.repeat(16),job='78'.repeat(32);

test('lost split acceptance survives restart, missing/expired evidence and later terminal native status without replay',async()=>{
    const f=fixture(),base=f.host.runtime.client({} as any),split=splitFixture();let writes=0,lookup:'missing'|'expired'|'found'='missing',current=split.snapshot;
    f.host.runtime.client=()=>new ServiceClient({dispose(){},async request(input){
        const name=typeof input==='string'?input:Object.keys(record(input))[0];
        if(name==='catalog_split_draft'){const selection=record(record(input).catalog_split_draft).selection,key=record(selection).key;record(split.draft.request).key=structuredClone(key);record(split.snapshot.request).key=structuredClone(key);return {status:'catalog_split_draft',draft:split.draft,future_draft_envelope:'retained'};}
        if(name==='catalog_split_apply'){writes++;const saved=[...f.values.values()].map(record).filter(value=>record(value.intent).catalog_split_apply);assert.equal(saved.length,1);assert.equal(saved[0].reservedObservationBytes,192*1024);current=splitAtStage(split.snapshot,'building');throw new ServiceError('console_transport_closed',true);}
        if(name==='catalog_split_receipt'){const request=record(record(input).catalog_split_receipt);assert.deepEqual(request.key,record(split.draft.request).key);return {status:name,...request,snapshot:lookup==='found'?current:null,expired:lookup==='expired',future_receipt_envelope:'retained'};}
        return base.transport.request(input);
    }});
    let reopened:ServiceConsoleController|undefined;
    try{
        await f.controller.refresh();const review=await f.controller.catalogSplitDraft(split.index,split.choice,1);
        await assert.rejects(f.controller.catalogSplitApply(review,()=>true),/console_transport_closed/);assert.equal(writes,1);
        f.controller.dispose();reopened=new ServiceConsoleController(f.host);await reopened.refresh();assert.equal(reopened.catalogPending().length,1);
        await reopened.catalogReconcile();lookup='expired';await reopened.catalogReconcile();assert.equal(reopened.catalogPending().length,1,'absence or expiry never establishes no effect');
        lookup='found';await reopened.catalogReconcile();assert.deepEqual(reopened.catalogPending(),[],'accepted active split is tracked separately from uncertain mutation');
        const tracked=reopened.catalogPendingDetails();assert.equal(tracked.length,1);assert.equal(record(tracked[0].native_observation).future_receipt_envelope,'retained');assert.equal(record(record(tracked[0].native_observation).snapshot).future_native_snapshot!==undefined,true);
        lookup='expired';await reopened.catalogReconcile();const retained=reopened.catalogPendingDetails()[0];assert.equal(record(retained.native_observation).snapshot,null);assert.equal(record(record(retained.last_known_split_snapshot).receipt).stage,'building');assert.equal(record(retained.last_known_split_snapshot).future_native_snapshot!==undefined,true,'null current lookup cannot erase full previously accepted proof');assert.equal(reopened.catalogPendingDetails().length,1);
        lookup='found';
        current=splitAtStage(current,'complete',3);current.receipt_digest='a4'.repeat(32);const terminal=await reopened.catalogReconcile();assert.equal(record(record(terminal[0].snapshot).receipt).stage,'complete');assert.deepEqual(reopened.catalogPendingDetails(),[]);assert.equal(writes,1);
    }finally{f.controller.dispose();reopened?.dispose();}
});

test('catalog receipt batches rotate past active split records and a failed lookup without mutation replay',async()=>{
    const f=fixture(),base=f.host.runtime.client({} as any),split=splitFixture(),journal=new IntentJournal(f.host.storage,'/control',namespace),seen:string[]=[];let fail=true;
    f.host.runtime.client=()=>new ServiceClient({dispose(){},async request(input){
        if(typeof input!=='string'&&'catalog_split_receipt'in record(input)){const query=record(record(input).catalog_split_receipt),nonce=String(record(query.key).nonce);seen.push(nonce);if(fail){fail=false;throw new ServiceError('fixture_lookup_failed');}return {status:'catalog_split_receipt',...query,snapshot:null,expired:false};}
        if(typeof input!=='string'&&'catalog_split_apply'in record(input))throw new Error('must never replay');
        return base.transport.request(input);
    }});
    try{
        for(let at=0;at<10;at++){const draft=structuredClone(split.draft);record(draft.request).key={namespace,epoch:7,nonce:at.toString(16).padStart(32,'0')};await journal.prepare('Tracked split',{catalog_split_apply:{draft}});}
        await f.controller.refresh();await assert.rejects(f.controller.catalogReconcile(),/fixture_lookup_failed/);
        await f.controller.catalogReconcile();await f.controller.catalogReconcile();
        assert.equal(new Set(seen).size,10);assert.equal(f.controller.catalogPendingDetails().length,10);
    }finally{f.controller.dispose();}
});

test('lost split recovery keeps immutable accepted acknowledgment independent of newer complete state',async()=>{
    const f=fixture(),base=f.host.runtime.client({} as any),split=splitFixture(),reviewed=splitAtStage(split.snapshot,'original_fenced');record(reviewed.receipt).state={state:'needs_attention',code:'fixture_failure'};
    let writes=0,ack:RecordValue|undefined,current=reviewed,crossed=false;
    f.host.runtime.client=()=>new ServiceClient({dispose(){},async request(input){
        const name=typeof input==='string'?input:Object.keys(record(input))[0];
        if(name==='catalog_split_status')return {status:name,operation_id:split.operation,snapshot:current,future_status:'retained'};
        if(name==='catalog_split_recover'){
            writes++;assert.equal([...f.values.values()].map(record).filter(value=>record(value.intent).catalog_split_recover).length,1);
            const request=record(record(record(input).catalog_split_recover).request),next=structuredClone(reviewed),receipt=record(next.receipt),accepted='a5'.repeat(32);
            receipt.revision=3;receipt.previous_receipt_digest=reviewed.receipt_digest;receipt.state={state:'pending'};receipt.jobs=[{purpose:'original_removal',caller:request.caller,key:request.key,backing:project,job:accepted,engine_identity:'a6'.repeat(32),configuration_digest:'a7'.repeat(32)}];next.receipt_digest='a8'.repeat(32);
            ack={request:structuredClone(request),request_digest:'a9'.repeat(32),outcome:{outcome:'resume_accepted',accepted_job:accepted,receipt_digest:next.receipt_digest,resulting_receipt:structuredClone(receipt)},future_ack:'retained'};
            current=splitAtStage(next,'complete',4);current.receipt_digest='aa'.repeat(32);throw new ServiceError('console_transport_closed',true);
        }
        if(name==='catalog_split_recovery_receipt'){const request=record(record(input).catalog_split_recovery_receipt),receipt=structuredClone(ack!);if(crossed)record(receipt.request).caller='ff'.repeat(16);return {status:name,...request,receipt,snapshot:current,expired:false,future_receipt:'retained'};}
        return base.transport.request(input);
    }});
    let reopened:ServiceConsoleController|undefined;
    try{
        await f.controller.refresh();const observed=await f.controller.catalogSplitStatus(split.operation);
        await assert.rejects(f.controller.catalogSplitRecover(observed,'resume',()=>true),/console_transport_closed/);assert.equal(writes,1);
        f.controller.dispose();reopened=new ServiceConsoleController(f.host);await reopened.refresh();crossed=true;await assert.rejects(reopened.catalogReconcile(),/crossed_split_recovery_receipt/);assert.equal(reopened.catalogPending().length,1);
        crossed=false;const responses=await reopened.catalogReconcile(),response=responses[0];assert.equal(record(record(response.receipt).outcome).outcome,'resume_accepted');assert.equal(record(response.receipt).future_ack,'retained');assert.equal(record(record(response.snapshot).receipt).stage,'complete');assert.equal(response.future_receipt,'retained');assert.deepEqual(reopened.catalogPendingDetails(),[]);assert.equal(writes,1);
    }finally{f.controller.dispose();reopened?.dispose();}
});

test('generic split recovery failure retains exact frozen request through restart and later finalization attention',async()=>{
    const f=fixture(),base=f.host.runtime.client({} as any),split=splitFrozenFixture();let writes=0,current=structuredClone(split.reviewed),ack:RecordValue|undefined;
    f.host.runtime.client=()=>new ServiceClient({dispose(){},async request(input){
        const name=typeof input==='string'?input:Object.keys(record(input))[0];
        if(name==='catalog_split_status')return {status:name,operation_id:split.operation,snapshot:current};
        if(name==='catalog_split_recover'){writes++;const request=record(record(record(input).catalog_split_recover).request);assert.equal(request.expected_finalization_revision,1);ack={...structuredClone(split.acknowledgment),request:structuredClone(request)};record(current.finalization).revision=3;record(current.finalization).attention='later_publish_failure';return {status:'failure',code:'invalid_registry_request'};}
        if(name==='catalog_split_recovery_receipt')return {status:name,...record(record(input)[name]),receipt:ack,snapshot:current,expired:false};
        return base.transport.request(input);
    }});
    let reopened:ServiceConsoleController|undefined;
    try{
        await f.controller.refresh();const observed=await f.controller.catalogSplitStatus(split.operation);await assert.rejects(f.controller.catalogSplitRecover(observed,'resume',()=>true),(error:unknown)=>error instanceof ServiceError&&error.outcomeUnknown);assert.equal(writes,1);assert.equal(f.controller.catalogPending().length,1);
        f.controller.dispose();reopened=new ServiceConsoleController(f.host);await reopened.refresh();await reopened.catalogReconcile();
        let tracked=reopened.catalogPendingDetails()[0];assert.equal(record(record(record(tracked.historical_split_recovery_receipt).outcome).terminal_receipt).stage,'complete');assert.equal(record(record(tracked.last_known_split_snapshot).finalization).attention,'later_publish_failure');assert.equal(record(record(tracked.last_known_split_snapshot).finalization).revision,3);assert.equal(reopened.catalogPendingDetails().length,1);
        current=structuredClone(split.terminal);current.finalization={...record(split.reviewed.finalization),revision:4,progress:'acknowledging',attention:'release_ack_failure'};await reopened.catalogReconcile();assert.equal(reopened.catalogPendingDetails().length,1,'logical completion cannot erase pending funding acknowledgment');
        record(current.finalization).revision=5;record(current.finalization).progress='complete';record(current.finalization).attention=null;record(current.finalization).funding_acknowledged=Object.values(record(record(current.receipt).funding_release_receipts));await reopened.catalogReconcile();assert.equal(reopened.catalogPendingDetails().length,0);assert.equal(writes,1);
    }finally{f.controller.dispose();reopened?.dispose();}
});

test('split confirmation rechecks trust and stale review before any durable mutation dispatch',async()=>{
    for(const reason of ['trust','stale'] as const){
        const f=fixture(),base=f.host.runtime.client({} as any),split=splitFixture();let writes=0;
        f.host.runtime.client=()=>new ServiceClient({dispose(){},async request(input){const name=typeof input==='string'?input:Object.keys(record(input))[0];if(name==='catalog_split_draft'){const key=record(record(record(input).catalog_split_draft).selection).key;record(split.draft.request).key=key;return {status:name,draft:split.draft};}if(name==='catalog_split_apply'){writes++;throw new Error('must not dispatch');}return base.transport.request(input);}});
        try{await f.controller.refresh();const review=await f.controller.catalogSplitDraft(split.index,split.choice,1);f.host.confirm=async()=>{if(reason==='trust')f.setTrusted(false);return true;};await assert.rejects(f.controller.catalogSplitApply(review,()=>reason!=='stale'));assert.equal(writes,0);assert.equal([...f.values.keys()].filter(key=>key.startsWith('airk.service.intent.')).length,0);}finally{f.controller.dispose();}
    }
});

function nativePolicyStatus():RecordValue{
    const index='idx_'+'91'.repeat(16),root='src_'+'92'.repeat(16);
    return {status:'catalog_policy_status',index_id:index,snapshot:{schema:1,operation_id:instance,job,application:null,application_job:null,last_error:'disk_full',policy:{schema:'airk.source-policy/1',index_id:index,owner_binding:project,namespace:'collections-1.4',collection:'fixture',epoch:1,
        roots:[{id:root,canonical_path:'/source',aliases:[],owner:true}],positive_includes:null,denies:[{source_root_id:root,kind:'subtree',path:'private',rule_id:'deny_'+'93'.repeat(16),reason:'trim',created_epoch:1}]}},
        job:{id:job,project,kind:'configure',state:'failed',fencing_generation:1,attempts:1,outcome:{state:'failed',code:'disk_full'},future_job:'retained'},future_policy_status:'retained'};
}

test('policy cleanup resume keeps the exact durable key through uncertainty and requires receipt plus matching native status',async()=>{
    const f=fixture(),base=f.host.runtime.client({} as any),status=nativePolicyStatus(),resumedJob='94'.repeat(32);let dispatches=0,receiptState='missing',sentRequest:RecordValue|undefined;
    f.host.runtime.client=()=>new ServiceClient({dispose(){},async request(method){
        const name=typeof method==='string'?method:Object.keys(record(method))[0];
        if(name==='catalog_policy_status')return structuredClone(status);
        if(name==='catalog_policy_resume'){
            dispatches++;sentRequest=record(record(method).catalog_policy_resume);assert.equal([...f.values.keys()].filter(key=>key.startsWith('airk.service.intent.')).length,1);
            record(status.snapshot).job=resumedJob;status.job={...record(status.job),id:resumedJob,state:'pending',outcome:null};throw new ServiceError('console_transport_closed',true);
        }
        if(name==='catalog_policy_resume_receipt'){
            const request=record(record(method).catalog_policy_resume_receipt);assert.equal(request.caller,sentRequest!.caller);assert.deepEqual(request.key,sentRequest!.key);
            return {status:'catalog_policy_resume_receipt',...request,receipt:receiptState==='missing'?null:{source_job:job,job:{...record(status.job),id:resumedJob,project:receiptState==='crossed'?'95'.repeat(32):project},configuration:{engine:'98'.repeat(32),request:{incarnation:namespace,settings:project,prepare:false,configuration:{operation:'apply_source_policy',operation_id:instance,policy:record(status.snapshot).policy}}},result:null},expired:false,future_receipt:'retained'};
        }
        return base.transport.request(method);
    }});
    let reopened:ServiceConsoleController|undefined;
    try{
        await f.controller.refresh();const observed=await f.controller.catalogPolicyStatus(String(status.index_id));assert.equal(dispatches,0);
        await assert.rejects(f.controller.catalogPolicyResume(observed,()=>true),/console_transport_closed/);assert.equal(dispatches,1);assert.equal(f.controller.catalogPending().length,1);
        f.controller.dispose();reopened=new ServiceConsoleController(f.host);await reopened.refresh();
        assert.equal(reopened.catalogPending().length,1);await reopened.catalogReconcile();assert.equal(reopened.catalogPending().length,1,'a missing receipt never establishes another resume');
        receiptState='crossed';await assert.rejects(reopened.catalogReconcile(),/crossed_policy_resume_receipt/);assert.equal(reopened.catalogPending().length,1);
        record(status.snapshot).job='96'.repeat(32);record(status.job).id='96'.repeat(32);
        receiptState='matched';const outcomes=await reopened.catalogReconcile();assert.equal(outcomes[0].accepted_job_id,resumedJob);assert.equal(record(outcomes[0].job).id,'96'.repeat(32));assert.equal(record(outcomes[0].resume_receipt).future_receipt,'retained');assert.equal(record(outcomes[0].job).future_job,'retained');assert.deepEqual(reopened.catalogPending(),[]);assert.equal(dispatches,1);
    }finally{f.controller.dispose();reopened?.dispose();}
});

test('lost resume replies reconcile exact old configuration after another client replaces policy',async()=>{
    for(const terminal of ['failed','canceled','succeeded'] as const){
        const f=fixture(),base=f.host.runtime.client({} as any),status=nativePolicyStatus(),oldPolicy=structuredClone(record(status.snapshot).policy),resumedJob='94'.repeat(32);let sends=0;
        f.host.runtime.client=()=>new ServiceClient({dispose(){},async request(method){
            const name=typeof method==='string'?method:Object.keys(record(method))[0];
            if(name==='catalog_policy_status')return structuredClone(status);
            if(name==='catalog_policy_resume'){sends++;throw new ServiceError('console_transport_closed',true);}
            if(name==='catalog_policy_resume_receipt')return {status:name,...record(record(method)[name]),receipt:{source_job:job,job:{...record(status.job),id:resumedJob,state:terminal,outcome:{state:terminal},future_accepted_job:true},configuration:{engine:'98'.repeat(32),request:{incarnation:namespace,settings:project,configuration:{operation:'apply_source_policy',operation_id:instance,policy:oldPolicy}}},result:null,future_proof:true},expired:false,future_receipt:true};
            return base.transport.request(method);
        }});
        let reopened:ServiceConsoleController|undefined;
        try{
            await f.controller.refresh();const observed=await f.controller.catalogPolicyStatus(String(status.index_id));
            await assert.rejects(f.controller.catalogPolicyResume(observed,()=>true),/console_transport_closed/);
            record(status.snapshot).operation_id='99'.repeat(16);record(status.snapshot).job='96'.repeat(32);status.job={...record(status.job),id:'96'.repeat(32),state:'pending',outcome:null};record(record(status.snapshot).policy).epoch=2;record(record(status.snapshot).policy).denies=[];
            f.controller.dispose();reopened=new ServiceConsoleController(f.host);await reopened.refresh();
            const results=await reopened.catalogReconcile();assert.equal(record(results[0].snapshot).operation_id,'99'.repeat(16));
            const proof=record(record(results[0].resume_receipt).receipt);assert.equal(record(proof.job).state,terminal);assert.equal(record(record(record(proof.configuration).request).configuration).operation_id,instance);assert.equal(proof.future_proof,true);assert.equal(record(proof.job).future_accepted_job,true);assert.deepEqual(reopened.catalogPending(),[]);assert.equal(sends,1);
        }finally{f.controller.dispose();reopened?.dispose();}
    }
});

test('exact resume receipt refuses crossed configuration and preserves absent or expired evidence',async()=>{
    const status=nativePolicyStatus(),intent={catalog_policy_resume:{index_id:String(status.index_id),operation_id:instance,caller:instance,key:{namespace,epoch:1,nonce:instance}},project};
    for(const situation of ['missing','expired','operation','index','configuration','caller','key'] as const){
        const response:RecordValue={status:'catalog_policy_resume_receipt',...intent.catalog_policy_resume,expired:situation==='expired',receipt:null};
        if(situation!=='missing'&&situation!=='expired'){
            const policy=structuredClone(record(status.snapshot).policy);
            response.receipt={source_job:job,job:status.job,configuration:{engine:'98'.repeat(32),request:{incarnation:namespace,settings:project,configuration:{operation:'apply_source_policy',operation_id:situation==='operation'?'99'.repeat(16):instance,policy}}},result:null};
            if(situation==='index')record(policy).index_id='idx_'+'99'.repeat(16);
            if(situation==='configuration')record(record(record(response.receipt).configuration).request).configuration={operation:'vectors',enabled:false};
            if(situation==='caller')response.caller='99'.repeat(16);
            if(situation==='key')response.key={...intent.catalog_policy_resume.key,nonce:'99'.repeat(16)};
        }
        const client=new ServiceClient({dispose(){},async request(){return structuredClone(response);}});
        if(situation==='missing'||situation==='expired'){const observed=await client.catalogPolicyResumeReceipt(intent);assert.equal(observed.receipt,null);assert.equal(observed.expired,situation==='expired');}
        else await assert.rejects(client.catalogPolicyResumeReceipt(intent));
    }
});

test('policy cleanup resume consumes a native failed-job observation and rechecks trust after confirmation',async()=>{
    for(const situation of ['changed','running','trust'] as const){
        const f=fixture(),base=f.host.runtime.client({} as any),status=nativePolicyStatus();let dispatches=0;
        if(situation==='running')record(status.job).state='running';
        f.host.runtime.client=()=>new ServiceClient({dispose(){},async request(method){const name=typeof method==='string'?method:Object.keys(record(method))[0];if(name==='catalog_policy_status')return structuredClone(status);if(name==='catalog_policy_resume'){dispatches++;return {...status,status:'catalog_policy_resumed',accepted_job_id:job,status_basis:'current_policy'};}return base.transport.request(method);}});
        try{
            await f.controller.refresh();const observation=await f.controller.catalogPolicyStatus(String(status.index_id));
            if(situation==='changed')record(observation.snapshot).operation_id='96'.repeat(16);
            if(situation==='trust')f.host.confirm=async()=>{f.setTrusted(false);return true;};
            await assert.rejects(f.controller.catalogPolicyResume(observation,()=>true));assert.equal(dispatches,0);assert.deepEqual(f.controller.catalogPending(),[]);
        }finally{f.controller.dispose();}
    }
});

test('explicit physical cleanup continuation preserves the completed removal receipt and never reapplies policy',async()=>{
    for(const pending of [true,false]){
        const f=fixture(),base=f.host.runtime.client({} as any),status=nativePolicyStatus(),report={policy_digest:'97'.repeat(32),operation_id:instance,observed_unix_ms:1793858000000,removal_publication:`airk-r1:${instance}:2:${'98'.repeat(32)}`,no_index:false,removed_artifacts:2,physical_cleanup_pending:pending,reclaimed_files:1,reclaimed_bytes:null,retired_components:['analysis']};let resumes=0;
        record(status.snapshot).application=report;record(status.snapshot).application_job=job;record(status.snapshot).last_error=null;status.job={...record(status.job),state:'succeeded',outcome:{state:'succeeded',revision:report.removal_publication}};
        f.host.runtime.client=()=>new ServiceClient({dispose(){},async request(method){
            const name=typeof method==='string'?method:Object.keys(record(method))[0];
            if(name==='catalog_policy_status')return structuredClone(status);
            if(name==='catalog_policy_resume'){
                resumes++;assert.equal([...f.values.keys()].filter(key=>key.startsWith('airk.service.intent.')).length,1);assert.equal(record(record(method).catalog_policy_resume).operation_id,instance);
                return {...status,status:'catalog_policy_resumed',accepted_job_id:'99'.repeat(32),status_basis:'current_policy',snapshot:{...record(status.snapshot),job:'99'.repeat(32)},job:{...record(status.job),id:'99'.repeat(32),state:'pending',outcome:null}};
            }
            assert.notEqual(name,'catalog_apply');return base.transport.request(method);
        }});
        try{
            await f.controller.refresh();const observed=await f.controller.catalogPolicyStatus(String(status.index_id));
            if(pending){const resumed=await f.controller.catalogPolicyResume(observed,()=>true);assert.deepEqual(record(resumed.snapshot).application,report);assert.equal(resumes,1);assert.deepEqual(f.controller.catalogPending(),[]);}
            else {await assert.rejects(f.controller.catalogPolicyResume(observed,()=>true),/native_failed_or_pending_policy_cleanup_required/);assert.equal(resumes,0);}
        }finally{f.controller.dispose();}
    }
});
test('actual lifecycle callback retains complete historical request/result and pre-health for explicit JSON exports only',async()=>{
    const f=fixture(),opened:string[]=[],saved:string[]=[],responses:RecordValue[]=[];let lifecycleCalls=0;
    f.host.openReport=async text=>{opened.push(text);};f.host.saveReport=async text=>{saved.push(text);};
    f.host.lifecycle=async(binding,intent,force)=>{
        lifecycleCalls++;assert.equal([...f.values.keys()].filter(key=>key.startsWith('airk.service.intent.')).length,1,'exact request is durable before native dispatch');assert.equal(force,false);
        const response={status:'lifecycle',record:{id:'ab'.repeat(32),phase:'complete',request:{...structuredClone(intent.lifecycle),manager:false},future_native_record:{text:'private-native-lifecycle-field'}},future_native_response:{machine_path:'/private/native/path'}};responses.push(response);return response;
    };
    try{
        await f.controller.refresh();const before=structuredClone(f.controller.state.health.value);await f.action('stop');
        const capture=f.controller.state.capturedLifecycle;assert.ok(capture);assert.equal(capture.qualification,'historical_native_lifecycle_response');assert.equal(capture.force,false);assert.equal(capture.namespace,namespace);assert.deepEqual(capture.runtime,{installDir:'/install',controlDir:'/control',runtimeSha256:'90'.repeat(32)});assert.deepEqual(capture.before_health,before);assert.equal(record(record(capture.response.record).request).expected_instance,record(capture.before_health!.observation).instance);assert.deepEqual(capture.response,responses[0]);assert.equal(f.controller.state.pending.length,0);assert.equal(f.controller.state.connected,false);
        record(responses[0].future_native_response).machine_path='changed-after-return';record(f.controller.state.health.value!.observation).user='changed-after-capture';assert.equal(record(capture.response.future_native_response).machine_path,'/private/native/path');assert.equal(record(capture.before_health!.observation).user,'secret-user');
        const full=JSON.stringify(capture,null,2),calls=f.sent.length;await f.action('copyLifecycle');await f.action('openLifecycle');await f.action('saveLifecycle');assert.equal(f.copied.at(-1),full);assert.deepEqual(opened,[full]);assert.deepEqual(saved,[full]);assert.equal(f.sent.length,calls,'captured JSON actions issue no native RPC');assert.equal(lifecycleCalls,1);
        assert.doesNotMatch(diagnosticPreview(f.controller.state,false),/private-native-lifecycle-field|\/private\/native\/path|secret-user/);assert.doesNotMatch(diagnosticPreview(f.controller.state,true),/private-native-lifecycle-field|\/private\/native\/path/);
        const view=managementView(f.controller.state);assert.deepEqual(view.capturedLifecycle,capture);assert.match(panelDetails(view,'service','1.4.0'),/historical; not current health or authority/);assert.equal(consoleAction({type:'service',revision:f.controller.state.revision,action:'copyLifecycle',id:'ab'.repeat(16)},f.controller.state),undefined);
        await f.action('connect');assert.deepEqual(f.controller.state.capturedLifecycle,capture,'own Stop reconnect preserves the original capture as historical');await f.action('connect');assert.equal(f.controller.state.capturedLifecycle,undefined,'unrelated later client replacement withdraws the old capture');
        await f.action('restart');assert.equal(lifecycleCalls,2);
        const restartedCapture=((state:ServiceConsoleState)=>state.capturedLifecycle)(f.controller.state);
        assert.ok(restartedCapture,'the actual Restart callback captures a new result after the prior client withdrew it');assert.equal(record(record(restartedCapture.response.record).request).action,'restart');assert.equal(restartedCapture.qualification,'historical_native_lifecycle_response');
        f.setTrusted(false);await f.controller.refresh();assert.equal(f.controller.state.capturedLifecycle,undefined);assert.equal(managementView(f.controller.state).capturedLifecycle,undefined);const copied=f.copied.length;await f.action('copyLifecycle');assert.equal(f.copied.length,copied);
    }finally{f.controller.dispose();assert.equal(f.controller.state.capturedLifecycle,undefined);}
});

test('late or crossed lifecycle results never escape current runtime/trust fences; oversize capture retains the request',async()=>{
    for(const change of ['trust','dispose','runtime','namespace','oversize'] as const){
        const f=fixture();let entered!:()=>void,release!:()=>void;const started=new Promise<void>(resolve=>{entered=resolve;}),held=new Promise<void>(resolve=>{release=resolve;});
        f.host.lifecycle=async(binding,intent)=>{entered();await held;if(change==='runtime')binding.runtimeSha256='ab'.repeat(32);return {status:'lifecycle',record:{id:'cd'.repeat(32),phase:'complete',request:{...intent.lifecycle,key:change==='namespace'?{...intent.lifecycle.key,namespace:'ee'.repeat(16)}:intent.lifecycle.key,manager:false}},...(change==='oversize'?{future_native_field:'x'.repeat(65536)}:{})};};
        try{
            await f.controller.refresh();const action=f.action('stop');await started;if(change==='trust')f.setTrusted(false);if(change==='dispose')f.controller.dispose();release();await action;
            assert.equal(f.controller.state.capturedLifecycle,undefined,change);assert.equal([...f.values.keys()].filter(key=>key.startsWith('airk.service.intent.')).length,1,`${change}: original request stays tracked`);
            if(change==='oversize'){assert.equal(f.controller.state.capturedLifecycleUnavailable,'native_lifecycle_capture_unavailable_budget');assert.match(f.controller.state.message!,/complete capture exceeds/);assert.equal(record(f.controller.state.bootstrap.value!.lifecycle).explicitly_stopped,true,'accepted native Stop remains accepted despite unavailable capture');}
        }finally{release();f.controller.dispose();}
    }
});

test('same-bound lifecycle reconnection cannot carry historical capture into another runtime or native namespace',async()=>{
    for(const change of ['runtime','namespace'] as const){
        const f=fixture();f.host.lifecycle=async(_binding,intent)=>({status:'lifecycle',record:{id:'ab'.repeat(32),phase:'complete',request:{...intent.lifecycle,manager:false}}});
        try{
            await f.controller.refresh();await f.action('stop');assert.ok(f.controller.state.capturedLifecycle,change);
            if(change==='runtime'){const binding=f.host.runtime.binding;f.host.runtime.binding=(...args)=>({...binding(...args),runtimeSha256:'aa'.repeat(32)});}
            else{const client=f.host.runtime.client({} as any);f.host.runtime.client=()=>new ServiceClient({dispose(){},async request(method){const response=await client.transport.request(method);if(typeof method==='object'&&method!==null&&'events' in method)return {...response,page:{...record(response.page),namespace:'bb'.repeat(16)}};return response;}});}
            await f.action('connect');assert.equal(f.controller.state.capturedLifecycle,undefined,change);assert.equal(managementView(f.controller.state).capturedLifecycle,undefined,change);
        }finally{f.controller.dispose();}
    }
});

test('explicit native lifecycle recovery captures actual force and current pre-health without rewriting its historical target',async()=>{
    const f=fixture(),request={action:'stop' as const,caller:'ab'.repeat(16),expected_instance:'ac'.repeat(16),runtime:'90'.repeat(32),key:{namespace,epoch:7,nonce:'ad'.repeat(16)}},journal=new IntentJournal(f.host.storage,'/control',namespace);
    await journal.prepare('Historical lifecycle',{lifecycle:request});const observation=await f.host.runtime.discover();observation.inspection={control_dir:'/control',lifecycle:{pending:{id:'ae'.repeat(32),phase:'draining',request:{...request,manager:true}}}};f.host.runtime.discover=async()=>observation;
    f.host.lifecycle=async(_binding,intent,force)=>{assert.equal(force,true);assert.deepEqual(intent.lifecycle,request);return {status:'lifecycle',record:{id:'ae'.repeat(32),phase:'complete',request:{...intent.lifecycle,manager:true,future_request_field:'retained'}}};};
    try{await f.controller.refresh();await f.action('forceIntent',{id:request.key.nonce});const captured=f.controller.state.capturedLifecycle;assert.ok(captured);assert.equal(captured.force,true);assert.equal(record(captured.before_health!.observation).instance,instance);assert.equal(record(record(captured.response.record).request).expected_instance,request.expected_instance);assert.notEqual(request.expected_instance,instance);assert.equal(record(record(captured.response.record).request).future_request_field,'retained');assert.equal(captured.qualification,'historical_native_lifecycle_response');assert.deepEqual(f.controller.state.pending,[]);}finally{f.controller.dispose();}
});

test('oversize own Restart keeps its unavailable notice only after exact runtime/namespace reconnect with its original journal',async()=>{
    for(const change of ['same','runtime','namespace'] as const){
        const f=fixture(),baseClient=f.host.runtime.client({} as any),binding=f.host.runtime.binding;let reconnect=false,request:RecordValue|undefined;
        f.host.runtime.binding=(...args)=>({...binding(...args),...(reconnect&&change==='runtime'?{runtimeSha256:'aa'.repeat(32)}:{})});
        f.host.runtime.client=()=>new ServiceClient({dispose(){},async request(method){const response=await baseClient.transport.request(method);if(reconnect&&change==='namespace'&&typeof method==='object'&&method!==null&&'events' in method)return {...response,page:{...record(response.page),namespace:'bb'.repeat(16)}};return response;}});
        f.host.lifecycle=async(_binding,intent)=>{request=structuredClone(intent.lifecycle);reconnect=true;return {status:'lifecycle',record:{id:'cd'.repeat(32),phase:'complete',request:{...intent.lifecycle,manager:false}},future_native_field:'x'.repeat(65536)};};
        try{
            await f.controller.refresh();await f.action('restart');assert.ok(request);assert.equal(f.controller.state.capturedLifecycle,undefined,change);
            assert.equal(f.controller.state.capturedLifecycleUnavailable,change==='same'?'native_lifecycle_capture_unavailable_budget':undefined,change);
            const retained=new IntentJournal(f.host.storage,'/control',namespace).list();assert.equal(retained.length,1,`${change}: accepted native work keeps the original exact request`);assert.deepEqual(record(retained[0].value.intent).lifecycle,request);
            if(change==='same'){assert.equal(f.controller.state.connected,true);await f.action('connect');assert.equal(f.controller.state.capturedLifecycleUnavailable,undefined,'later unrelated replacement cannot retain the one-shot notice');}
        }finally{f.controller.dispose();}
    }
});
test('restart retirement recovery pins full proof and stops only matching local tracking without a native mutation',async()=>{
    for(const kind of ['setup','source'] as const){
        const f=fixture(),base=f.host.runtime.client({} as any),journal=new IntentJournal(f.host.storage,'/control',namespace),index='idx_'+'31'.repeat(16),source='src_'+'41'.repeat(16),digest='43'.repeat(32),key={namespace,epoch:1,nonce:'44'.repeat(16)},nativeCaller='9a'.repeat(16),calls:unknown[]=[];
        const registration={binding:project,root:'/native/source',data_root:'/native/data',collection:'fixture'},setupRequest={caller:nativeCaller,key,expected_catalog_revision:1,name:'Partial',registration,sources:[{canonical_path:registration.root,aliases:[],owner:true}],setup_watch:false};
        const sourceRequest={caller:nativeCaller,key,expected_catalog_revision:1,operation:{operation:'begin_index_source_change',id:index,expected_source_revision:1,plan:{registration,engine_identity:digest,job,configuration:{incarnation:'6a'.repeat(16),settings:digest,configuration:{operation:'add_root',alias:'extra',path:'/extra'}},before:{primary_root:registration.root,aliases:{}},after:{primary_root:registration.root,aliases:{extra:'/extra'}},sources:[{id:source,canonical_path:registration.root,aliases:[],owner:true},{id:'src_'+'42'.repeat(16),canonical_path:'/extra',aliases:['extra'],owner:false}]}}};
        const begin={request:sourceRequest,metadata:{schema:1,operation_id:'90'.repeat(16),caller:nativeCaller,key,request_digest:digest,catalog_revision:2,authority_fence_required:true},preview_digest:digest,prepared_digest:digest,proof:{plan_digest:digest,service:instance,gate_generation:1,confirmations:[]}};
        const originalSource={status:'catalog_source_change_observation',observation:{begin,job:{id:job,project,kind:'configure',state:'failed',fencing_generation:1,attempts:2,outcome:{state:'failed'}},configuration_result:{prior_native_result:'retained'},completion:null,future_begin_result:'retained'}};
        if(kind==='source'){
            assert.doesNotThrow(()=>base.validateSourceObservation(originalSource,begin.metadata.operation_id,namespace),'source fixture must contain a complete valid native Begin before eligibility is tested');
            assert.notEqual(sourceRequest.operation.plan.configuration.incarnation,namespace);assert.notEqual(sourceRequest.operation.plan.configuration.settings,registration.binding);
            const crossed=structuredClone(originalSource);crossed.observation.begin.request.operation.plan.configuration.settings='45'.repeat(32);assert.throws(()=>base.validateSourceObservation(crossed,begin.metadata.operation_id,namespace,originalSource),/crossed_source_observation_continuity/);
        }
        const id=kind==='setup'?await journal.prepare('Partial setup',{catalog_index_setup_apply:{draft:{schema:1,request:setupRequest,storage_action:'create_missing_leaf',review_digest:digest}},request_digest:digest}):await journal.prepare('Uncertain resolve',{catalog_source_change_resolve:{begin_operation:begin.metadata.operation_id},namespace},originalSource);
        if(kind==='setup')await journal.observeCatalog(id,{status:'catalog_index_setup_receipt',receipt:{schema:1,operation_id:'91'.repeat(16),request:setupRequest,request_digest:digest,index_id:index,source_ids:[source],stage:'reserved',catalog_revision:2,source_revision:1,preparation:null,completion:null}});
        // Begin advances the source revision before its configuration job runs. Partial Setup remains at revision one.
        const retireSourceRevision=kind==='source'?2:1;
        const request={caller:'aa'.repeat(16),key:{namespace,epoch:2,nonce:'bb'.repeat(16)},expected_catalog_revision:2,operation:{operation:'retire_index',id:index,expected_definition_revision:1,expected_source_revision:retireSourceRevision,reason:{reason:'confirmed_missing_source',registration_epoch:0,configuration_epoch:1}}};
        const retirement={request,metadata:{schema:1,operation_id:'cc'.repeat(16),caller:request.caller,key:request.key,request_digest:digest,catalog_revision:3,authority_fence_required:true},preview_digest:digest,prepared_digest:digest,proof:{plan_digest:digest,service:instance,gate_generation:2,confirmations:[],worker_obligation:{index_id:index,backing:project,workers:[],providers:[]},worker_confirmations:[]},future_full_proof:'retained'};
        const full:RecordValue={status:'catalog_index_retirement_evidence',index_id:index,evidence:{catalog_clock:{namespace,revision:4},index:{id:index,name:'Retired',definition_revision:2,source_revision:retireSourceRevision+1,backing:registration,tombstone:true,source_import:'pending_metadata'},retirement,future_native_evidence:'retained'}};let response:RecordValue={...full,evidence:null};
        let drafts=0,delayEvidence=false,releaseEvidence:((value:RecordValue)=>void)|undefined;
        f.host.runtime.client=()=>new ServiceClient({dispose(){},async request(method){
            if(typeof method==='object'&&method!==null&&'catalog_index_retirement_evidence' in method){calls.push(structuredClone(method));if(delayEvidence)return new Promise<RecordValue>(resolve=>{releaseEvidence=resolve;});return structuredClone(response);}
            if(typeof method==='object'&&method!==null&&'catalog_source_mutation_draft' in method){drafts++;const selection=record(record(method).catalog_source_mutation_draft).selection,input=record(selection),nativeRequest={...request,caller:nativeCaller,key:input.key,expected_catalog_revision:input.expected_catalog_revision,operation:{...request.operation,reason:{reason:'explicit'}}};return {status:'catalog_source_mutation_draft',draft:{request:nativeRequest,preview:{request_digest:digest,catalog_revision:input.expected_catalog_revision,next_revision:Number(input.expected_catalog_revision)+1,operation:nativeRequest.operation,before:[],after:[],authority_fence_required:true,file_data_changes:false},preview_digest:digest}};}
            return base.transport.request(method);
        }});
        const reopened=new ServiceConsoleController(f.host);
        try{
            await reopened.refresh();assert.equal(reopened.catalogPending().length,1,`${kind}: original request survives restart`);assert.equal(reopened.catalogCanRetire(index,project,namespace),true,`${kind}: complete native original qualifies only this index`);assert.equal(reopened.catalogCanRetire('idx_'+'99'.repeat(16),project,namespace),false,`${kind}: another index cannot supersede tracking`);
            await assert.rejects(reopened.catalogSourceDraft(index,{operation:'add_root',alias:'extra',path:'/extra'},2),/unconfirmed/);await reopened.catalogSourceDraft(index,{operation:'retire_index'},2);assert.equal(drafts,1);assert.equal(reopened.catalogPending().length,1,'distinct retirement review never drops the original');
            delayEvidence=true;const late=reopened.catalogRetirementReview(id);assert.ok(releaseEvidence);f.setTrusted(false);releaseEvidence(structuredClone(full));await assert.rejects(late,/no_longer_authorized/);f.setTrusted(true);delayEvidence=false;assert.equal(reopened.catalogPending().length,1,'late read after trust loss cannot settle tracking');
            let review=await reopened.catalogRetirementReview(id);await assert.rejects(reopened.catalogStopTracking(review,()=>true),/evidence_unavailable/);assert.equal(reopened.catalogPending().length,1);
            response=structuredClone(full);record(record(response.evidence).retirement).proof={...retirement.proof,worker_obligation:undefined};await assert.rejects(reopened.catalogRetirementReview(id));assert.equal(reopened.catalogPending().length,1);
            response=structuredClone(full);record(record(response.evidence).index).backing={...registration,root:'/crossed/source'};await assert.rejects(reopened.catalogRetirementReview(id),/crossed_retirement_backing/);assert.equal(reopened.catalogPending().length,1);
            response={...full,evidence:{...record(full.evidence),oversized_native_field:'x'.repeat(32768)}};await assert.rejects(reopened.catalogRetirementReview(id),/oversize/);assert.equal(reopened.catalogPending().length,1);
            response=full;review=await reopened.catalogRetirementReview(id);assert.equal(record(record(review.response.evidence).retirement).future_full_proof,'retained');
            review.response.future='changed';await assert.rejects(reopened.catalogStopTracking(review,()=>true),/read_current/);assert.equal(reopened.catalogPending().length,1);
            review=await reopened.catalogRetirementReview(id);await assert.rejects(reopened.catalogStopTracking(review,()=>false),/read_current/);assert.equal(reopened.catalogPending().length,1);
            await journal.observeCatalog(id,{status:'failure',code:'retained_new_native_observation'});await assert.rejects(reopened.catalogStopTracking(review,()=>true),/read_current/);assert.equal(reopened.catalogPending().length,1);review=await reopened.catalogRetirementReview(id);
            const replacement=new ServiceConsoleController(f.host);try{await replacement.refresh();await assert.rejects(replacement.catalogStopTracking(review,()=>true),/read_current/);assert.equal(replacement.catalogPending().length,1);}finally{replacement.dispose();}
            f.setTrusted(false);await assert.rejects(reopened.catalogStopTracking(review,()=>true),/no_longer_authorized/);f.setTrusted(true);await assert.rejects(reopened.catalogStopTracking(review,()=>true),/read_current/);review=await reopened.catalogRetirementReview(id);
            const outcome=await reopened.catalogStopTracking(review,()=>true);assert.equal(outcome.local_tracking,'stopped_after_native_retirement');assert.equal(record(outcome.native_retirement_evidence).status,'catalog_index_retirement_evidence');assert.deepEqual(reopened.catalogPending(),[]);
            assert.equal(calls.length,9);assert.ok(calls.every(value=>Object.keys(record(value)).join()==='catalog_index_retirement_evidence'));assert.equal(f.sent.some(value=>typeof value==='object'&&value!==null&&['catalog_apply','catalog_source_change_resolve','catalog_index_setup_apply'].some(name=>name in value)),false);
            const again=new ServiceConsoleController(f.host);try{await again.refresh();assert.deepEqual(again.catalogPending(),[]);}finally{again.dispose();}
            await journal.prepare('Uncertain retirement',{catalog_apply:{request:{...request,operation:{...request.operation,reason:{reason:'explicit'}}},preview_digest:digest},request_digest:digest});assert.equal(reopened.catalogCanRetire(index,project,namespace),false);await assert.rejects(reopened.catalogSourceDraft(index,{operation:'retire_index'},3),/unconfirmed/);assert.equal(drafts,1,'an unknown Retire cannot obtain a replacement key or review');
        }finally{reopened.dispose();f.controller.dispose();}
    }
});
test('only exact native unprepared reconciliation retires a local unknown without replay or a new key',async()=>{
    const f=fixture(),base=f.host.runtime.client({} as any),digest='43'.repeat(32),operation={operation:'rename_project',id:'prj_'+'21'.repeat(16),expected_definition_revision:1,name:'Reviewed'};let request:RecordValue|undefined,phase='missing',applies=0,keys=0;
    f.host.runtime.client=()=>new ServiceClient({dispose(){},async request(method){const name=typeof method==='string'?method:Object.keys(method as object)[0];
        if(name==='request_epoch')keys++;
        if(name==='catalog_preview')return {status:'catalog_preview',preview_digest:digest,preview:{request_digest:digest,catalog_revision:1,next_revision:2,operation,before:[],after:[],authority_fence_required:false,file_data_changes:false}};
        if(name==='catalog_apply'){applies++;request=structuredClone(record(record(method).catalog_apply).request as RecordValue);throw new ServiceError('console_transport_closed',true);}
        if(name==='catalog_receipt'){
            assert.deepEqual(record(method).catalog_receipt,{caller:request!.caller,key:request!.key});if(phase==='missing')return {status:'catalog_receipt',receipt:null};
            return {status:'catalog_receipt_unprepared',request:phase==='caller'?{...request,caller:'9a'.repeat(16)}:phase==='body'?{...request,operation:{...operation,name:'Crossed'}}:request,request_digest:phase==='digest'?'44'.repeat(32):digest,catalog_clock:{namespace:phase==='namespace'?'45'.repeat(16):namespace,revision:phase==='revision'?0:2},future_native_absence:{retained:true}};
        }return base.transport.request(method);
    }});
    let reopened:ServiceConsoleController|undefined;
    try{
        await f.controller.refresh();const review=await f.controller.catalogPrepare(operation,1);await assert.rejects(f.controller.catalogApply(review,()=>true),/transport_closed/);assert.equal(f.controller.catalogPending().length,1);await f.controller.catalogReconcile();assert.equal(f.controller.catalogPending().length,1,'null is not native absence proof');f.controller.dispose();
        reopened=new ServiceConsoleController(f.host);await reopened.refresh();for(const bad of ['caller','body','digest','namespace','revision']){phase=bad;await assert.rejects(reopened.catalogReconcile());assert.equal(reopened.catalogPending().length,1,bad);}
        phase='unprepared';const results=await reopened.catalogReconcile();assert.equal(results[0].status,'catalog_receipt_unprepared');assert.equal(results[0].receipt,undefined);assert.deepEqual(results[0].future_native_absence,{retained:true});assert.deepEqual(results[0].request,request);assert.deepEqual(reopened.catalogPending(),[]);assert.equal(applies,1);assert.equal(keys,1,'known non-preparation cannot create or dispatch a replacement intent');
    }finally{f.controller.dispose();reopened?.dispose();}
});
test('source draft/apply and failed-resolution use one journal; unknown restart reads never replay and preserve failed history',async()=>{
    const f=fixture(),base=f.host.runtime.client({} as any),index='idx_'+'31'.repeat(16),source='src_'+'41'.repeat(16),requestDigest='44'.repeat(32),previewDigest='43'.repeat(32),beginId='90'.repeat(16);
    let applies=0,resolves=0,busy=true,complete=false,crossed=false,savedRequest:RecordValue|undefined,begin:RecordValue|undefined;
    const observe=()=>({status:'catalog_source_change_observation',observation:{begin,job:{id:job,project,kind:'configure',state:'failed',fencing_generation:1,attempts:2,outcome:{state:'failed',code:'native_source_failed'},future_native_job:'retained'},configuration_result:{prior_result:'retained',...(complete?{catalog_native_desired_resolution:{native_proof:'retained'}}:{})},completion:complete?{schema:1,operation_id:'91'.repeat(16),caller:savedRequest!.caller,key:{...record(savedRequest!.key),nonce:'92'.repeat(16)},request_digest:'93'.repeat(32),catalog_revision:3,authority_fence_required:false}:null,future_native_source:'retained'}});
    f.host.runtime.client=()=>new ServiceClient({dispose(){},async request(method){
        const name=typeof method==='string'?method:Object.keys(method as object)[0];
        if(name==='catalog_source_mutation_draft'){
            const selection=record(record(method).catalog_source_mutation_draft).selection,input=record(selection);
            const operation={operation:'begin_index_source_change',id:index,expected_source_revision:1,plan:{registration:{binding:project,root:'/source',data_root:'/data',collection:'fixture'},engine_identity:'51'.repeat(32),job,configuration:{incarnation:'6b'.repeat(16),settings:'6c'.repeat(32),configuration:{operation:'add_root',alias:'extra',path:'/extra'}},before:{primary_root:'/source',aliases:{}},after:{primary_root:'/source',aliases:{extra:'/extra'}},sources:[{id:source,canonical_path:'/source',aliases:[],owner:true},{id:'src_'+'42'.repeat(16),canonical_path:'/extra',aliases:['extra'],owner:false}]}};
            return {status:'catalog_source_mutation_draft',draft:{request:{caller:'9a'.repeat(16),key:input.key,expected_catalog_revision:1,operation},preview:{catalog_revision:1,next_revision:2,operation,request_digest:requestDigest,authority_fence_required:true,file_data_changes:false,before:[],after:[],future_preview:'retained'},preview_digest:previewDigest},future_native_draft:'retained'};
        }
        if(name==='catalog_apply'){
            applies++;const request=record(record(method).catalog_apply).request as RecordValue;
            if(busy)return {status:'source_change_busy',code:'source_change_jobs_pending',busy:{index_id:index,pending_jobs:2,running_jobs:1},future_busy:'retained'};
            savedRequest=structuredClone(request);const metadata={schema:1,operation_id:beginId,caller:request.caller,key:request.key,request_digest:requestDigest,catalog_revision:2,authority_fence_required:true};
            begin={request:savedRequest,metadata,preview_digest:previewDigest,prepared_digest:'52'.repeat(32),proof:{plan_digest:'53'.repeat(32),service:instance,gate_generation:1,confirmations:[]},future_begin:'retained'};
            return {status:'catalog_applied',receipt:metadata,fenced_receipt:begin};
        }
        if(name==='catalog_source_change_observation'){
            assert.deepEqual(record(method).catalog_source_change_observation,{begin_operation:beginId});const response=structuredClone(observe());if(crossed)record(response.observation.begin!.request).caller='8a'.repeat(16);return response;
        }
        if(name==='catalog_pending_source_change'){
            assert.deepEqual(method,{catalog_pending_source_change:{index_id:index}});return {status:'catalog_pending_source_change',index_id:index,observation:structuredClone(observe().observation),future_lookup:'retained'};
        }
        if(name==='catalog_source_change_resolve'){
            resolves++;assert.deepEqual(method,{catalog_source_change_resolve:{begin_operation:beginId}});assert.ok([...f.values.values()].some(value=>value&&typeof value==='object'&&'intent' in value&&'catalog_source_change_resolve' in record(record(value).intent)),'full native begin proof is durable before Resolve');
            return {status:'source_change_resolution_outcome_unknown',code:'source_change_resolution_outcome_unknown',begin_operation:beginId,observation:null,future_unknown:'retained'};
        }
        return base.transport.request(method);
    }});
    let restarted:ServiceConsoleController|undefined,afterBegin:ServiceConsoleController|undefined;
    try{
        await f.controller.refresh();const change={operation:'add_root',alias:'extra',path:'/user/extra'};
        for(const field of ['incarnation','settings'] as const){const changedReview=await f.controller.catalogSourceDraft(index,change,1),configuration=record(record(changedReview.request.operation.plan).configuration);configuration[field]=field==='incarnation'?'6d'.repeat(16):'6e'.repeat(32);await assert.rejects(f.controller.catalogApply(changedReview,()=>true),/stale_or_changed_native_source_review/);assert.equal(applies,0,'frontend modification cannot replace the reviewed concrete native collection identity');}
        const busyReview=await f.controller.catalogSourceDraft(index,change,1);await assert.rejects(f.controller.catalogApply(busyReview,()=>true),(error:unknown)=>{assert.ok(error instanceof ServiceError);assert.equal(error.code,'source_change_jobs_pending');assert.equal(error.outcomeUnknown,false);assert.equal(error.nativeResponse!.future_busy,'retained');return true;});assert.deepEqual(f.controller.catalogPending(),[]);busy=false;
        const localReview=await f.controller.catalogSourceDraft(index,change,1),update=f.host.storage.update;let valid=true;f.host.storage.update=async(key,value)=>{await update(key,value);if(value!==undefined&&key.startsWith('airk.service.intent.'))valid=false;};
        await assert.rejects(f.controller.catalogApply(localReview,()=>valid),/stale_catalog_review/);assert.equal(applies,1);assert.deepEqual(f.controller.catalogPending(),[]);f.host.storage.update=update;
        const review=await f.controller.catalogSourceDraft(index,change,1);assert.equal(review.request.caller,'9a'.repeat(16));assert.equal(resolves,0);const concrete=record(record(review.request.operation.plan).configuration);assert.notEqual(concrete.incarnation,namespace);assert.notEqual(concrete.settings,project);await f.controller.catalogApply(review,()=>true);assert.equal(applies,2);assert.deepEqual(record(record(savedRequest!.operation).plan).configuration,concrete,'canonical Apply retains the exact concrete native configuration rather than registry templates');
        assert.deepEqual(f.controller.catalogPending(),[]);f.controller.dispose();afterBegin=new ServiceConsoleController(f.host);await afterBegin.refresh();
        const read=await afterBegin.catalogPendingSource(index,namespace);assert.equal(read.future_lookup,'retained');assert.equal(resolves,0);record(record(read.observation).job).future_native_job='changed';await assert.rejects(afterBegin.catalogSourceResolve(read,()=>true),/read_current_native_source_observation/);assert.equal(resolves,0);
        const current=await afterBegin.catalogPendingSource(index,namespace);await assert.rejects(afterBegin.catalogSourceResolve(current,()=>true),/source_change_resolution_outcome_unknown/);assert.equal(resolves,1);assert.deepEqual(afterBegin.catalogPending(),[beginId]);
        await afterBegin.action({type:'service',revision:afterBegin.state.revision,action:'forgetIntent',id:beginId});assert.deepEqual(afterBegin.catalogPending(),[beginId],'generic console forgetting cannot discard an accepted uncertain catalog resolution');
        const pending=afterBegin.catalogPendingDetails()[0];assert.equal(record(pending.native_observation).future_unknown,'retained');assert.equal(record(record(pending.native_source_observation).observation).future_native_source,'retained');afterBegin.dispose();
        restarted=new ServiceConsoleController(f.host);await restarted.refresh();assert.equal(resolves,1);await restarted.catalogReconcile();assert.equal(restarted.catalogPending().length,1);
        crossed=true;await assert.rejects(restarted.catalogReconcile(),/crossed/);assert.equal(restarted.catalogPending().length,1);crossed=false;
        const reread=await restarted.catalogSourceObservation(beginId,namespace);await assert.rejects(restarted.catalogSourceResolve(reread,()=>true),/source_resolution_requires_observation/);assert.equal(resolves,1);
        complete=true;const results=await restarted.catalogReconcile(),observed=record(results[0].observation);assert.equal(record(observed.job).state,'failed');assert.equal(record(record(observed.job).outcome).code,'native_source_failed');assert.ok(record(observed.configuration_result).catalog_native_desired_resolution);assert.deepEqual(restarted.catalogPending(),[]);assert.equal(resolves,1);assert.equal(applies,2);
    }finally{f.controller.dispose();afterBegin?.dispose();restarted?.dispose();}
});

test('setup Resume requires a fresh matching receipt and reuses the exact original draft after each explicit recovery',async()=>{
    const f=fixture(),base=f.host.runtime.client({} as any),digest='43'.repeat(32),requests:unknown[]=[];let nativeRequest:RecordValue|undefined,phase='reserved',lose=true;
    const receipt=()=>({schema:1,operation_id:'90'.repeat(16),request:nativeRequest,request_digest:digest,index_id:'idx_'+'31'.repeat(16),source_ids:['src_'+'41'.repeat(16)],stage:phase,catalog_revision:phase==='ready'?3:2,source_revision:phase==='ready'?2:1,preparation:phase==='reserved'?null:{digest:'51'.repeat(32),storage_identity:'52'.repeat(32),source_configuration_digest:'53'.repeat(32),review:{expected_catalog_revision:2,expected_source_revision:1}},completion:phase==='ready'?{prepared_digest:'51'.repeat(32),expected_catalog_revision:2,expected_source_revision:1}:null});
    f.host.runtime.client=()=>new ServiceClient({dispose(){},async request(method){const name=typeof method==='string'?method:Object.keys(method as object)[0];
        if(name==='catalog_index_setup_draft'){const selection=record(record(method).catalog_index_setup_draft).selection,input=record(selection);return {status:'catalog_index_setup_draft',request_digest:digest,draft:{schema:1,request:{caller:'9a'.repeat(16),key:input.key,expected_catalog_revision:1,name:input.name,registration:{binding:project,root:'/native/source',data_root:'/native/data',collection:'fixture'},sources:[{canonical_path:'/native/source',aliases:[],owner:true}],setup_watch:false},storage_action:'create_missing_leaf',review_digest:digest}};}
        if(name==='catalog_index_setup_apply'){requests.push(structuredClone(method));nativeRequest=record(record(record(method).catalog_index_setup_apply).draft).request as RecordValue;if(requests.length>1&&lose)throw new ServiceError('console_transport_closed',true);if(requests.length>1){phase='ready';return {status:'catalog_index_setup_applied',receipt:receipt()};}return {status:'catalog_outcome_unknown',code:'catalog_index_setup_outcome_unknown',caller:nativeRequest.caller,key:nativeRequest.key,request_digest:digest,observation:{kind:'index_setup',receipt:receipt()}};}
        if(name==='catalog_index_setup_receipt')return {status:'catalog_index_setup_receipt',receipt:phase==='missing'?null:receipt()};return base.transport.request(method);
    }});
    let restarted:ServiceConsoleController|undefined;
    try{
        await f.controller.refresh();const review=await f.controller.catalogIndexSetupDraft({name:'Explicit setup',primary_root:'/chosen/source',data_root:'/chosen/data',collection:'fixture',additional_roots:[],setup_watch:false},1);await assert.rejects(f.controller.catalogIndexSetupApply(review,()=>true),/outcome_unknown/);const id=f.controller.catalogPending()[0];
        await assert.rejects(f.controller.catalogSetupResume(id,()=>true),/read_matching/);assert.equal(requests.length,1);f.controller.dispose();restarted=new ServiceConsoleController(f.host);await restarted.refresh();await assert.rejects(restarted.catalogSetupResume(id,()=>true),/read_matching/);
        await restarted.catalogReconcile();assert.equal(restarted.catalogPendingDetails()[0].can_resume_setup,true);await assert.rejects(restarted.catalogSetupResume(id,()=>true),/console_transport_closed/);assert.equal(requests.length,2);assert.equal(restarted.catalogPendingDetails()[0].can_resume_setup,false);await assert.rejects(restarted.catalogSetupResume(id,()=>true),/read_matching/);assert.equal(requests.length,2);
        phase='missing';await restarted.catalogReconcile();assert.equal(restarted.catalogPendingDetails()[0].can_resume_setup,false);phase='prepared';await restarted.catalogReconcile();lose=false;await restarted.catalogSetupResume(id,()=>true);assert.equal(requests.length,3);assert.deepEqual(requests[0],requests[1]);assert.deepEqual(requests[1],requests[2]);assert.deepEqual(restarted.catalogPending(),[]);
    }finally{f.controller.dispose();restarted?.dispose();}
});
test('explicit setup journals native caller/draft and reconciles reserved then ready after restart without another apply',async()=>{
    const f=fixture(),base=f.host.runtime.client({} as any),nativeCaller='9a'.repeat(16),digest='43'.repeat(32);let applies=0,phase='reserved',savedRequest:RecordValue|undefined;
    const lookups:unknown[]=[];
    const receipt=()=>({schema:1,operation_id:'90'.repeat(16),request:savedRequest,request_digest:digest,index_id:'idx_'+'31'.repeat(16),source_ids:['src_'+'41'.repeat(16)],stage:phase,catalog_revision:phase==='ready'?3:2,source_revision:phase==='ready'?2:1,
        preparation:phase==='reserved'?null:{digest:'51'.repeat(32),storage_identity:'52'.repeat(32),source_configuration_digest:'53'.repeat(32),review:{expected_catalog_revision:2,expected_source_revision:1}},
        completion:phase==='ready'?{prepared_digest:'51'.repeat(32),expected_catalog_revision:2,expected_source_revision:1}:null,future_setup_receipt:'retained'});
    f.host.runtime.client=()=>new ServiceClient({dispose(){},async request(method){
        const name=typeof method==='string'?method:Object.keys(method as object)[0];
        if(name==='catalog_index_setup_draft'){
            const selection=record(record(method).catalog_index_setup_draft).selection,input=record(selection);
            const request={caller:nativeCaller,key:input.key,expected_catalog_revision:input.expected_catalog_revision,name:input.name,registration:{binding:project,root:'/native/source',data_root:'/native/storage',collection:input.collection},sources:[{canonical_path:'/native/source',aliases:[],owner:true}],setup_watch:input.setup_watch};
            return {status:'catalog_index_setup_draft',draft:{schema:1,request,review_digest:digest,storage_action:'create_missing_leaf',future_native_draft:'retained'},request_digest:digest,future_setup_response:'retained'};
        }
        if(name==='catalog_index_setup_apply'){
            applies++;savedRequest=record(record(record(method).catalog_index_setup_apply).draft).request as RecordValue;
            return {status:'catalog_outcome_unknown',code:'catalog_index_setup_outcome_unknown',caller:nativeCaller,key:savedRequest.key,request_digest:digest,observation:{kind:'index_setup',receipt:receipt()},future_unknown:'retained'};
        }
        if(name==='catalog_index_setup_receipt'){lookups.push(method);return {status:'catalog_index_setup_receipt',receipt:phase==='missing'?null:receipt(),future_receipt_response:'retained'};}
        return base.transport.request(method);
    }});
    const selection={name:'Explicit setup',primary_root:'/chosen/source',data_root:'/chosen/storage',collection:'fixture',additional_roots:[],setup_watch:false};let restarted:ServiceConsoleController|undefined;
    try{
        await f.controller.refresh();const changed=await f.controller.catalogIndexSetupDraft(selection,1);record(changed.draft.request).name='Changed';
        await assert.rejects(f.controller.catalogIndexSetupApply(changed,()=>true),/stale_or_changed/);assert.equal(applies,0);
        const beforeDispatch=await f.controller.catalogIndexSetupDraft(selection,1),originalUpdate=f.host.storage.update;let valid=true;
        f.host.storage.update=async(key,value)=>{await originalUpdate(key,value);if(value!==undefined&&key.startsWith('airk.service.intent.'))valid=false;};
        await assert.rejects(f.controller.catalogIndexSetupApply(beforeDispatch,()=>valid),/stale_setup_review/);assert.equal(applies,0);assert.deepEqual(f.controller.catalogPending(),[],'a proven local predispatch cancellation does not strand an unknown native request');f.host.storage.update=originalUpdate;
        const reviewed=await f.controller.catalogIndexSetupDraft(selection,1);assert.equal(record(reviewed.draft.request).caller,nativeCaller);assert.equal(applies,0);
        await assert.rejects(f.controller.catalogIndexSetupApply(reviewed,()=>true),/outcome_unknown/);assert.equal(applies,1);assert.equal(f.controller.catalogPending().length,1);assert.equal(record(f.controller.catalogPendingDetails()[0].native_observation).future_unknown,'retained');
        f.controller.dispose();restarted=new ServiceConsoleController(f.host);await restarted.refresh();
        await restarted.catalogReconcile();assert.equal(restarted.catalogPending().length,1);
        phase='missing';await restarted.catalogReconcile();assert.equal(restarted.catalogPending().length,1);
        phase='ready';const results=await restarted.catalogReconcile();assert.equal(record(results[0].receipt).future_setup_receipt,'retained');assert.deepEqual(restarted.catalogPending(),[]);assert.equal(applies,1);
        for(const lookup of lookups)assert.deepEqual(lookup,{catalog_index_setup_receipt:{caller:nativeCaller,key:savedRequest!.key}});
    }finally{f.controller.dispose();restarted?.dispose();}
});
test('automatic polls reuse only recent successful optional observations with their original dates',async context=>{
    context.mock.timers.enable({apis:['Date']});
    const f=fixture(),base=f.host.runtime.client({} as any),calls:string[]=[];
    let currentInstance=instance,resourceFailure=false;
    f.host.runtime.client=()=>new ServiceClient({dispose(){},async request(method){
        const name=typeof method==='string'?method:Object.keys(method as object)[0];calls.push(name);
        if(name==='health')return {status:'health',observation:{instance:currentInstance}};
        if(name==='logs')return {status:'logs',page:{schema:1,records:[]}};
        if(name==='resources'&&resourceFailure)throw new ServiceError('resource_fixture_busy');
        return base.transport.request(method);
    }});
    const optional=['resources','embedding_status','source_status','logs'];
    const counts=()=>optional.map(name=>calls.filter(value=>value===name).length);
    try{
        await f.controller.refresh();const checkedAt=f.controller.state.resources.checkedAt;
        assert.deepEqual(counts(),[1,1,1,1]);
        context.mock.timers.tick(2000);await f.controller.refresh(true);
        assert.deepEqual(counts(),[1,1,1,1]);assert.equal(f.controller.state.resources.state,'cached');
        assert.equal(f.controller.state.resources.checkedAt,checkedAt);
        assert.equal(calls.filter(name=>name==='health').length,2);
        assert.equal(calls.filter(name=>name==='events').length,2);
        assert.equal(calls.filter(name=>name==='console').length,2);
        context.mock.timers.tick(3000);await f.controller.refresh(true);
        assert.deepEqual(counts(),[2,2,2,2]);
        await f.controller.refresh();assert.deepEqual(counts(),[3,3,3,3],'explicit refresh samples immediately');
        resourceFailure=true;await f.controller.refresh();assert.equal(f.controller.state.resources.state,'failed');
        resourceFailure=false;await f.controller.refresh(true);
        assert.deepEqual(counts(),[5,4,4,4],'a failed observation retries independently');
        currentInstance='bc'.repeat(16);await f.controller.refresh(true);
        assert.deepEqual(counts(),[6,5,5,5],'a service replacement cannot reuse old observations');
        f.controller.state.resources.checkedAt=Date.now()+1000;await f.controller.refresh(true);
        assert.deepEqual(counts(),[7,5,5,5],'a backward clock observation requires a fresh sample');
    }finally{f.controller.dispose();}
});

test('routine console observations obtain namespace from Events and reserve request epochs for actions',async()=>{
    const f=fixture();
    try{
        await f.controller.refresh();await f.controller.refresh();
        assert.equal(f.sent.filter(method=>method==='request_epoch').length,0);
        const methods=f.sent.map(method=>typeof method==='string'?method:Object.keys(method as object)[0]);
        assert.ok(methods.indexOf('events')<methods.indexOf('console'));
        await f.action('index',{project});
        assert.ok(f.sent.includes('request_epoch'),'explicit actions still acquire durable request keys');
    }finally{f.controller.dispose();}
});

test('registry namespace replacement discards old event cursors before publishing new project rows',async()=>{
    const f=fixture(),base=f.host.runtime.client({} as any),replacement='ab'.repeat(16);
    let changed=false,refused=false,newPage=false;
    f.host.runtime.client=()=>new ServiceClient({dispose(){},async request(method){
        if(changed&&typeof method==='object'&&method!==null&&'events' in method){
            const args=(method as {events:{cursor:unknown}}).events;
            if(args.cursor!==null){refused=true;throw new ServiceError('event_scope_changed');}
            return {status:'events',page:{namespace:replacement,events:[{sequence:1,kind:'new_registry'}],retained_prefix_lost:false,next:null,tail:{namespace:replacement,project:null,after:1,through:null}}};
        }
        if(changed&&typeof method==='object'&&method!==null&&'console' in method){newPage=true;}
        return base.transport.request(method);
    }});
    try{
        await f.controller.refresh();f.controller.state.events=[{sequence:9,kind:'old_registry'}];
        changed=true;await f.controller.refresh();
        assert.equal(refused,true);assert.equal(newPage,false);
        assert.equal(f.controller.state.page,undefined);assert.deepEqual(f.controller.state.events,[]);
        await f.controller.refresh();
        assert.equal(newPage,true);assert.deepEqual(f.controller.state.events,[{sequence:1,kind:'new_registry'}]);
        assert.equal(f.sent.includes('request_epoch'),false);
    }finally{f.controller.dispose();}
});

test('raw log actions use the trusted local host and never enter summary reports',async()=>{
    const f=fixture(),calls:boolean[]=[];
    f.host.rawLogs=async save=>{calls.push(save);};
    await f.controller.refresh();
    await f.action('openRawLogs');await f.action('saveRawLogs');
    assert.deepEqual(calls,[false,true]);
    await f.action('openRawLogs',{path:'/forged-log'});
    assert.deepEqual(calls,[false,true]);
    f.setTrusted(false);await f.action('openRawLogs');
    assert.deepEqual(calls,[false,true]);
    f.controller.dispose();
});
test('a stuck status poll expires, permits local logs meanwhile, rejects late results and reconnects',async context=>{
    context.mock.timers.enable({apis:['setTimeout','Date']});
    const f=fixture(),original=f.host.runtime.client;
    const base=original({} as any);let release!:()=>void,waiting=false,disposed=0;
    const blocked=new Promise<void>(resolve=>{release=resolve;});
    f.host.runtime.client=()=>new ServiceClient({dispose(){disposed++;},async request(method){
        if(method==='health'){waiting=true;await blocked;}
        return base.transport.request(method);
    }});
    let logs=0;f.host.rawLogs=async()=>{logs++;return 'opened';};
    const poll=f.controller.refresh();
    try{
        for(let i=0;i<30&&!waiting;i++)await Promise.resolve();assert.ok(waiting);
        await f.action('openRawLogs');assert.equal(logs,1);assert.equal(f.controller.state.busy,true);
        context.mock.timers.tick(15001);await poll;
        assert.equal(f.controller.state.busy,false);assert.equal(f.controller.state.connected,false);
        assert.equal(f.controller.state.health.code,'status_poll_timed_out');assert.ok(disposed>0);
        assert.match(f.controller.state.message!,/Reconnecting automatically/);
        release();for(let i=0;i<8;i++)await Promise.resolve();
        assert.equal(f.controller.state.connected,false,'a late reply cannot replace the timed-out observation');
        f.host.runtime.client=original;await f.controller.refresh();assert.equal(f.controller.state.connected,true);
    }finally{release();await poll;f.controller.dispose();}
});
test('console workspace registration uses a trusted host action and highlights only native project bindings',async()=>{
    const f=fixture();let registrations=0;
    f.host.registerWorkspace=async()=>{registrations++;};f.host.currentProjects=()=>new Set([project]);
    await f.controller.refresh();const registration=managementView(f.controller.state);
    assert.equal(registration.enabled.registerWorkspace,true,'Management offers workspace registration');
    assert.ok(registration.projects.some(entry=>entry.summary.current),'the open workspace project is marked current');
    const forged={type:'service',revision:f.controller.state.revision,action:'registerWorkspace',root:'/untrusted'};
    await f.controller.action(forged);assert.equal(registrations,0);
    await f.action('registerWorkspace');assert.equal(registrations,1);
    f.setTrusted(false);await f.action('registerWorkspace');assert.equal(registrations,1);f.controller.dispose();
});

test('trusted rebuild commands create a saved guarded preview and never approve or execute it automatically',async()=>{
    const f=fixture(),base=f.host.runtime.client({} as any),calls:unknown[]=[];
    const plan={id:'de'.repeat(32),preview_digest:'ef'.repeat(32),counts:{selected:1},state:'ready'};
    f.host.runtime.client=()=>new ServiceClient({dispose(){},async request(method){
        const name=typeof method==='string'?method:Object.keys(method as object)[0];
        if(name==='plan_create'){
            assert.ok([...f.values.keys()].some(key=>key.startsWith('airk.service.intent.')));calls.push(structuredClone(method));return {status:'plan',plan};
        }
        if(name==='plan')return {status:'plan',plan};
        if(name==='plan_items')return {status:'plan_items',page:{plan,items:[{project}],after:null}};
        if(name==='plan_execute')throw new Error('Preview must never execute without separate approval');
        return base.transport.request(method);
    }});
    await f.controller.projectPlan(project,'rebuild');assert.equal(f.controller.state.message,undefined);
    assert.equal(f.controller.state.plan?.value.id,plan.id);assert.equal(calls.length,1);
    assert.equal((calls[0] as any).plan_create.request.operation,'rebuild');assert.deepEqual((calls[0] as any).plan_create.request.selection,{selected:[project]});
    f.setTrusted(false);await f.controller.projectPlan(project,'rebuild');assert.equal(calls.length,1);f.controller.dispose();
});

test('observation polling refreshes repair rows through queued running and finished without user actions',async()=>{
    const f=fixture(),base=f.host.runtime.client({} as any);
    const id='de'.repeat(32),preview='ef'.repeat(32);
    let phase='ready',itemPhase='ready',reads=0;
    const header=()=>({id,preview_digest:preview,phase,paused:false,counts:{selected:1,ready:phase==='ready'?1:0,running:phase==='running'?1:0,succeeded:phase==='finished'?1:0}});
    f.host.runtime.client=()=>new ServiceClient({dispose(){},async request(method){
        if(method&&typeof method==='object'&&'plan' in method)return {status:'plan',plan:header()};
        if(method&&typeof method==='object'&&'plan_items' in method){reads++;return {status:'plan_items',page:{plan:header(),items:[{project,phase:itemPhase,action:'reconcile'}],after:null}};}
        return base.rpc(method);
    }});
    try{
        await f.controller.refresh();f.controller.state.plan={value:header(),items:[{project,phase:itemPhase,action:'reconcile'}],after:null};
        for(const next of [['running','action_queued'],['running','running'],['finished','succeeded']]){
            [phase,itemPhase]=next;await f.controller.refresh();
            assert.equal(f.controller.state.plan!.items[0].phase,itemPhase);
            assert.equal(f.controller.state.plan!.value.phase,phase);
        }
        assert.equal(reads,3);await f.controller.refresh();assert.equal(reads,3,'a stable finished plan needs no repeated row fetch');
        assert.equal(f.sent.some(value=>typeof value==='object'&&value!==null&&('enqueue' in value||'plan_execute' in value)),false);
    }finally{f.controller.dispose();}
});
test('native host context adapter retains complete replies on the persistent control client and fences host/namespace/revision',async()=>{
    const f=fixture(),base=f.host.runtime.client({} as any),key='33'.repeat(32);let host='22'.repeat(32),observedNamespace=namespace,revision=3,connections=0;
    const calls:unknown[]=[];
    f.host.runtime.client=()=>{connections++;return new ServiceClient({dispose(){},async request(method){
        if(typeof method==='object'&&method!==null&&'catalog_host_context' in method){calls.push(method);return {status:'catalog_host_context',descriptor:{schema:1,namespace:observedNamespace,host,key,catalog_revision:revision,context:null,future_native_field:{preserved:true}},extra_response_field:'complete'};}
        return base.transport.request(method);
    }});};
    try{
        await f.controller.refresh();const before=connections;
        const full=await f.controller.catalogHostContext(key,3);
        assert.equal((full.descriptor as RecordValue).future_native_field!==undefined,true);assert.equal(full.extra_response_field,'complete');assert.equal(connections,before);
        assert.deepEqual(calls,[{catalog_host_context:{key,expected_catalog_revision:3}}]);
        host='44'.repeat(32);await assert.rejects(f.controller.catalogHostContext(key,3),/stale_native_host_context/);host='22'.repeat(32);
        revision=4;await assert.rejects(f.controller.catalogHostContext(key,3),/crossed_native_host_context/);revision=3;
        observedNamespace='99'.repeat(16);await assert.rejects(f.controller.catalogHostContext(key,3),/namespace_changed/);observedNamespace=namespace;
        await assert.rejects(f.controller.catalogRead({catalog_host_context:{key,host:'claimed-wire-host'}}),/invalid_catalog_host_context_request/);
    }finally{f.controller.dispose();}
});

test('native host context replies cannot survive trust revocation or service identity replacement',async()=>{
    for(const cancel of ['trust','replacement'] as const){
        const f=fixture(),base=f.host.runtime.client({} as any),key='33'.repeat(32);let currentInstance=instance,release!:(value:RecordValue)=>void;
        f.host.runtime.client=()=>new ServiceClient({dispose(){},async request(method){
            if(method==='health')return {status:'health',observation:{instance:currentInstance,version:'1.4.0'}};
            if(typeof method==='object'&&method!==null&&'catalog_host_context' in method)return new Promise<RecordValue>(resolve=>{release=resolve;});
            return base.transport.request(method);
        }});
        try{
            await f.controller.refresh();const pending=f.controller.catalogHostContext(key,3);
            if(cancel==='trust')f.setTrusted(false);else{currentInstance='88'.repeat(16);await f.controller.refresh();}
            release({status:'catalog_host_context',descriptor:{schema:1,namespace,host:'22'.repeat(32),key,catalog_revision:3,context:null}});
            await assert.rejects(pending,cancel==='trust'?/no_longer_authorized/:/stale_catalog_client/);
        }finally{f.controller.dispose();}
    }
});

test('catalog context lookup accepts its canonical catalog_detail response without another connection',async()=>{
    const f=fixture(),base=f.host.runtime.client({} as any),host='22'.repeat(32),key='33'.repeat(32),calls:unknown[]=[];
    const response={status:'catalog_detail',detail:{namespace,revision:1,item:null,native_field:{preserved:true}}};
    f.host.runtime.client=()=>new ServiceClient({dispose(){},async request(method){
        if(typeof method==='object'&&method!==null&&'catalog_context' in method){calls.push(method);return response;}
        return base.transport.request(method);
    }});
    try{await f.controller.refresh();assert.deepEqual(await f.controller.catalogRead({catalog_context:{host,key,expected_catalog_revision:null}}),response);assert.deepEqual(calls,[{catalog_context:{host,key,expected_catalog_revision:null}}]);}
    finally{f.controller.dispose();}
});

test('native grant drafts are immutable and restart receipts retain their original native caller instead of frontend identity',async()=>{
    const f=fixture(),base=f.host.runtime.client({} as any),contextKey='33'.repeat(32),host='22'.repeat(32),projectId='prj_'+'21'.repeat(16),indexId='idx_'+'31'.repeat(16),sourceId='src_'+'41'.repeat(16),previewDigest='43'.repeat(32),requestDigest='44'.repeat(32);
    let client:ServiceClient|undefined,nativeCaller='9a'.repeat(16),applies=0,crossReceipt=true,appliedRequest:RecordValue|undefined;const lookups:unknown[]=[];
    f.host.runtime.client=()=>{client=new ServiceClient({dispose(){},async request(method){
        const name=typeof method==='string'?method:Object.keys(method as object)[0];
        if(name==='catalog_host_context')return {status:'catalog_host_context',descriptor:{schema:1,namespace,host,key:contextKey,catalog_revision:1,context:null}};
        if(name==='catalog_grant_draft'){
            const input=(method as {catalog_grant_draft:{context_key:string;selection:RecordValue}}).catalog_grant_draft;assert.equal(input.context_key,contextKey);assert.deepEqual(Object.keys(input).sort(),['context_key','selection']);
            const operation={operation:'create_grant',grant:{id:'grt_'+'11'.repeat(16),host,revision:1,revoked:false,memberships:[{membership:{project_id:projectId,index_id:indexId,revision:1,scope:[{source_root_id:sourceId,kind:'subtree',path:''}]},source_revision:1,policy_epoch:0,policy_digest:null}]}};
            const request={caller:nativeCaller,key:input.selection.key,expected_catalog_revision:1,operation};
            return {status:'catalog_grant_draft',draft:{request,preview:{request_digest:requestDigest,catalog_revision:1,next_revision:2,operation,before:[],after:[],authority_fence_required:false,file_data_changes:false,extra_native_field:'retained'},preview_digest:previewDigest},future_response_field:'retained'};
        }
        if(name==='catalog_apply'){applies++;appliedRequest=(method as {catalog_apply:{request:RecordValue}}).catalog_apply.request;return {status:'catalog_outcome_unknown',code:'catalog_apply_outcome_unknown',caller:appliedRequest.caller,key:appliedRequest.key,request_digest:requestDigest,future_unknown_field:'retained'};}
        if(name==='catalog_receipt'){
            lookups.push(method);return {status:'catalog_receipt',receipt:{schema:1,operation_id:'90'.repeat(16),caller:crossReceipt?'8a'.repeat(16):appliedRequest!.caller,key:appliedRequest!.key,request_digest:requestDigest,catalog_revision:2,authority_fence_required:false,future_receipt_field:'retained'}};
        }
        return base.transport.request(method);
    }});return client;};
    let restarted:ServiceConsoleController|undefined;
    try{
        await f.controller.refresh();await f.controller.catalogHostContext(contextKey,1);
        const frontend=await client!.prepareCatalog({operation:'rename_project',id:projectId,expected_definition_revision:1,name:'Metadata'},1);
        const chosen={project_ids:[projectId],index_ids:[indexId]};const initial=await f.controller.catalogGrantDraft(contextKey,chosen,1);
        assert.equal(initial.request.caller,nativeCaller);assert.notEqual(frontend.caller,nativeCaller);assert.equal(initial.response.future_response_field,'retained');assert.equal(applies,0);
        nativeCaller='8a'.repeat(16);await assert.rejects(f.controller.catalogGrantDraft(contextKey,chosen,1),/crossed_native_grant_caller/);nativeCaller='9a'.repeat(16);
        for(const changed of ['request','requestDigest','previewDigest','preview','contextKey'] as const){
            const reviewed=await f.controller.catalogGrantDraft(contextKey,chosen,1);
            if(changed==='request')(reviewed.request.operation.grant as RecordValue).host='55'.repeat(32);
            else if(changed==='preview')(record(record(reviewed.response.draft).preview)).extra_native_field='changed';
            else reviewed[changed]='99'.repeat(32);
            await assert.rejects(f.controller.catalogApply(reviewed,()=>true),/stale_or_changed_native_grant_review/);
        }
        assert.equal(applies,0);assert.deepEqual(f.controller.catalogPending(),[]);
        const reviewed=await f.controller.catalogGrantDraft(contextKey,chosen,1);await assert.rejects(f.controller.catalogApply(reviewed,()=>true),/outcome_unknown/);
        assert.equal(f.controller.catalogPending().length,1);assert.equal(applies,1);assert.equal(record(f.controller.catalogPendingDetails()[0].native_observation).future_unknown_field,'retained');f.controller.dispose();
        restarted=new ServiceConsoleController(f.host);await restarted.refresh();
        const newFrontend=await client!.prepareCatalog({operation:'rename_project',id:projectId,expected_definition_revision:1,name:'Metadata'},1);assert.notEqual(newFrontend.caller,nativeCaller);
        await assert.rejects(restarted.catalogReconcile(),/crossed_catalog_receipt/);assert.equal(restarted.catalogPending().length,1);
        crossReceipt=false;const settled=await restarted.catalogReconcile();assert.equal(record(settled[0].receipt).future_receipt_field,'retained');assert.deepEqual(restarted.catalogPending(),[]);
        for(const lookup of lookups)assert.deepEqual(lookup,{catalog_receipt:{caller:'9a'.repeat(16),key:appliedRequest!.key}});
        assert.equal(applies,1,'restoring a native caller never replays its apply');
    }finally{f.controller.dispose();restarted?.dispose();}
});

test('pending native fences survive restart and cannot settle with crossed request or delivery targets',async()=>{
    const f=fixture(),base=f.host.runtime.client({} as any),previewDigest='43'.repeat(32),requestDigest='44'.repeat(32),preparedDigest='45'.repeat(32);
    const operation={operation:'remove_membership',project_id:'prj_'+'21'.repeat(16),index_id:'idx_'+'31'.repeat(16),expected_scope_revision:1,expected_membership_revision:1};
    const target={worker:{service:instance,worker:'51'.repeat(16),project,fencing_generation:2},scope:{schema:1,lease_generation:3,index_id:operation.index_id,grant_id:'grt_'+'11'.repeat(16),grant_revision:1,fingerprint:'52'.repeat(32)},future_target_field:'retained'};
    let request:RecordValue|undefined,phase='pending',applies=0;
    f.host.runtime.client=()=>new ServiceClient({dispose(){},async request(method){
        const name=typeof method==='string'?method:Object.keys(method as object)[0];
        if(name==='catalog_preview')return {status:'catalog_preview',preview_digest:previewDigest,preview:{request_digest:requestDigest,catalog_revision:1,next_revision:2,operation,before:[],after:[],authority_fence_required:true,file_data_changes:false}};
        if(name==='catalog_apply'){applies++;request=(method as {catalog_apply:{request:RecordValue}}).catalog_apply.request;return {status:'failure',code:'invalid_registry_request'};}
        if(name==='catalog_receipt'){
            const metadata={schema:1,operation_id:'90'.repeat(16),caller:request!.caller,key:request!.key,request_digest:requestDigest,catalog_revision:2,authority_fence_required:true};
            if(phase==='unprepared')return {status:'catalog_receipt_unprepared',request,request_digest:requestDigest,catalog_clock:{namespace,revision:2}};
            if(phase==='pending')return {status:'catalog_receipt',receipt:null,fence:{state:'pending',value:{schema:1,operation_id:metadata.operation_id,request,preview_digest:previewDigest,prepared_digest:preparedDigest,plan:{service:instance,gate_generation:7,targets:[target]},stage:'prepared',confirmations:[],future_pending_field:'retained'}}};
            const full={request:phase==='request'?{...request,caller:'91'.repeat(16)}:request,metadata,preview_digest:previewDigest,prepared_digest:preparedDigest,proof:{plan_digest:'53'.repeat(32),service:instance,gate_generation:7,confirmations:[{target:phase==='target'?{...target,scope:{...target.scope,fingerprint:'99'.repeat(32)}}:target,method:'native_scope_ack'}],future_proof_field:'retained'},future_fenced_field:'retained'};
            return {status:'catalog_receipt',receipt:metadata,fence:{state:'applied',value:full},future_response_field:'retained'};
        }
        return base.transport.request(method);
    }});
    let restarted:ServiceConsoleController|undefined;
    try{
        await f.controller.refresh();const reviewed=await f.controller.catalogPrepare(operation,1);await assert.rejects(f.controller.catalogApply(reviewed,()=>true),(error:unknown)=>error instanceof ServiceError&&error.outcomeUnknown&&error.code==='invalid_registry_request');
        const pending=await f.controller.catalogReconcile();assert.equal(pending[0].receipt,null);assert.equal(f.controller.catalogPending().length,1);
        assert.equal(record(record(f.controller.catalogPendingDetails()[0].native_fence_observation).fence).state,'pending');
        f.controller.dispose();restarted=new ServiceConsoleController(f.host);await restarted.refresh();
        phase='unprepared';await assert.rejects(restarted.catalogReconcile(),/catalog_unprepared_conflicts_with_saved_proof/);assert.equal(restarted.catalogPending().length,1);
        for(const crossed of ['request','target']){phase=crossed;await assert.rejects(restarted.catalogReconcile(),/crossed_catalog_fenced_receipt|crossed_catalog_fence_plan/);assert.equal(restarted.catalogPending().length,1);}
        phase='complete';const settled=await restarted.catalogReconcile();assert.equal(settled[0].future_response_field,'retained');
        const proof=record(record(record(settled[0].fence).value).proof);assert.equal(proof.future_proof_field,'retained');assert.deepEqual(restarted.catalogPending(),[]);assert.equal(applies,1);
    }finally{f.controller.dispose();restarted?.dispose();}
});

test('catalog UI uses the established native client and persists exact reviewed apply before dispatch',async()=>{
    const f=fixture(),base=f.host.runtime.client({} as any),id='prj_'+'21'.repeat(16),digest='43'.repeat(32),calls:string[]=[];
    const operation={operation:'rename_project',id,expected_definition_revision:1,name:'Renamed'};
    let applyCount=0;
    f.host.runtime.client=()=>new ServiceClient({dispose(){},async request(method){
        const name=typeof method==='string'?method:Object.keys(method as object)[0];calls.push(name);
        if(name==='catalog_preview')return {status:'catalog_preview',preview_digest:digest,preview:{request_digest:digest,catalog_revision:1,next_revision:2,operation,before:[],after:[],authority_fence_required:false,file_data_changes:false,extra_native_field:'retained'}};
        if(name==='catalog_apply'){
            applyCount++;const saved=[...f.values.values()].find(value=>value&&typeof value==='object'&&'intent' in value) as {intent:unknown};
            assert.deepEqual(saved.intent,{...(method as RecordValue),request_digest:digest},'exact request and native digests are durable before transport dispatch');
            const request=(method as {catalog_apply:{request:{caller:string;key:unknown}}}).catalog_apply.request;
            return {status:'catalog_applied',receipt:{schema:1,operation_id:instance,caller:request.caller,key:request.key,request_digest:digest,catalog_revision:2,authority_fence_required:false,additional_native_field:'kept'}};
        }
        return base.transport.request(method);
    }});
    try{
        await assert.rejects(f.controller.catalogPrepare(operation,1),/connection_unavailable/);
        await f.controller.refresh();const connectionCalls=calls.filter(name=>name==='health').length;
        const preview=await f.controller.catalogPrepare(operation,1);
        assert.equal((preview.response.preview as RecordValue).extra_native_field,'retained');
        const result=await f.controller.catalogApply(preview,()=>true);
        assert.equal((result.receipt as RecordValue).additional_native_field,'kept');assert.equal(applyCount,1);
        assert.equal(calls.filter(name=>name==='health').length,connectionCalls,'catalog actions reuse the control connection without extra status probes');
        assert.deepEqual(f.controller.catalogPending(),[]);
    }finally{f.controller.dispose();}
});

test('catalog uncertain outcomes reconcile only their native caller/key without apply replay',async()=>{
    const f=fixture(),base=f.host.runtime.client({} as any),id='prj_'+'21'.repeat(16),digest='43'.repeat(32),operation={operation:'rename_project',id,expected_definition_revision:1,name:'Renamed'};
    let applies=0,lookup:unknown,appliedRequest:{caller:string;key:unknown}|undefined;
    f.host.runtime.client=()=>new ServiceClient({dispose(){},async request(method){
        const name=typeof method==='string'?method:Object.keys(method as object)[0];
        if(name==='catalog_preview')return {status:'catalog_preview',preview_digest:digest,preview:{request_digest:digest,catalog_revision:1,operation}};
        if(name==='catalog_apply'){applies++;appliedRequest=(method as {catalog_apply:{request:{caller:string;key:unknown}}}).catalog_apply.request;throw new ServiceError('console_transport_closed',true);}
        if(name==='catalog_receipt'){lookup=method;return {status:'catalog_receipt',receipt:null};}
        return base.transport.request(method);
    }});
    try{
        await f.controller.refresh();const preview=await f.controller.catalogPrepare(operation,1);
        await assert.rejects(f.controller.catalogApply(preview,()=>true),/transport_closed/);assert.equal(f.controller.catalogPending().length,1);
        await assert.rejects(f.controller.catalogApply(preview,()=>true),/requires_receipt_reconciliation/);
        const outcomes=await f.controller.catalogReconcile();assert.deepEqual(outcomes,[{status:'catalog_receipt',receipt:null}]);
        assert.deepEqual(lookup,{catalog_receipt:{caller:appliedRequest!.caller,key:appliedRequest!.key}});
        assert.equal(applies,1);assert.equal(f.controller.catalogPending().length,1);
        f.setTrusted(false);await assert.rejects(f.controller.catalogRead({catalog_detail:{}}),/no_longer_authorized/);
    }finally{f.controller.dispose();}
});

test('catalog restart reconciliation validates the durably reviewed native request digest',async()=>{
    const f=fixture(),base=f.host.runtime.client({} as any),id='prj_'+'21'.repeat(16),digest='43'.repeat(32),operation={operation:'rename_project',id,expected_definition_revision:1,name:'Renamed'};
    let requestValue:RecordValue|undefined,matchDigest=false,applies=0;
    f.host.runtime.client=()=>new ServiceClient({dispose(){},async request(method){
        const name=typeof method==='string'?method:Object.keys(method as object)[0];
        if(name==='catalog_preview')return {status:'catalog_preview',preview_digest:digest,preview:{request_digest:digest,catalog_revision:1,operation}};
        if(name==='catalog_apply'){applies++;requestValue=(method as {catalog_apply:{request:RecordValue}}).catalog_apply.request;throw new ServiceError('console_transport_closed',true);}
        if(name==='catalog_receipt')return {status:'catalog_receipt',receipt:{schema:1,operation_id:instance,caller:requestValue!.caller,key:requestValue!.key,request_digest:matchDigest?digest:'99'.repeat(32),catalog_revision:2,authority_fence_required:false,extra_native_field:'preserved'}};
        return base.transport.request(method);
    }});
    let restarted:ServiceConsoleController|undefined;
    try{
        await f.controller.refresh();const preview=await f.controller.catalogPrepare(operation,1);
        await assert.rejects(f.controller.catalogApply(preview,()=>true),/transport_closed/);
        f.controller.dispose();restarted=new ServiceConsoleController(f.host);await restarted.refresh();
        await assert.rejects(restarted.catalogReconcile(),/crossed_catalog_receipt/);assert.equal(restarted.catalogPending().length,1);
        matchDigest=true;const receipts=await restarted.catalogReconcile();assert.equal((receipts[0].receipt as RecordValue).extra_native_field,'preserved');
        assert.deepEqual(restarted.catalogPending(),[]);assert.equal(applies,1,'restarting and checking receipts never replays apply');
    }finally{f.controller.dispose();restarted?.dispose();}
});

test('catalog known authority-fence refusal remains truthful and does not strand a no-write intent',async()=>{
    const f=fixture(),base=f.host.runtime.client({} as any),id='prj_'+'21'.repeat(16),digest='43'.repeat(32),operation={operation:'delete_project',id,expected_definition_revision:1,expected_scope_revision:1,expected_defaults_revision:1};
    f.host.runtime.client=()=>new ServiceClient({dispose(){},async request(method){
        const name=typeof method==='string'?method:Object.keys(method as object)[0];
        if(name==='catalog_preview')return {status:'catalog_preview',preview_digest:digest,preview:{request_digest:digest,catalog_revision:1,operation,authority_fence_required:true}};
        if(name==='catalog_apply')return {status:'failure',code:'catalog_authority_fence_unavailable'};
        return base.transport.request(method);
    }});
    try{await f.controller.refresh();const preview=await f.controller.catalogPrepare(operation,1);await assert.rejects(f.controller.catalogApply(preview,()=>true),/catalog_authority_fence_unavailable/);assert.deepEqual(f.controller.catalogPending(),[]);}
    finally{f.controller.dispose();}
});

function fixture(){
    let trusted=true,fail=false,lastKind='inspect';const sent:unknown[]=[],values=new Map<string,unknown>(),copied:string[]=[];
    const row={project:{registration:{binding:project,root:'/sensitive/<source>',data_root:'/private/data',collection:'<script>name</script>'},paused:false,fencing_generation:1},progress:null,inspection:null};
    const binding={installDir:'/install',controlDir:'/control',executable:'/executable',launcher:null,runtimeSha256:'90'.repeat(32),version:'1.4.0',lifecycle:'on-demand' as const,verificationBasis:'trusted_distribution'};
    const client=new ServiceClient({dispose(){},async request(method){
        sent.push(structuredClone(method));const name=typeof method==='string'?method:Object.keys(method as object)[0];
        switch(name){
            case 'health':return {status:'health',observation:{instance,version:'1.4.0',user:'secret-user'}};
            case 'resources':return {status:'resources',observation:{configuration:{preset:'balanced'}}};
            case 'embedding_status':return {status:'embedding_status',available:false,loads:0};
            case 'source_status':return {status:'source_status',available:false};
            case 'request_epoch':return {status:'request_epoch',namespace,epoch:7};
            case 'console':case 'inactive_console':return {status:'console',page:{revision:1,cached:true,observed_unix_ms:1,projects:[row],after:null}};
            case 'workers':return {status:'workers',observations:[{project,identity:null,restarting:false,automatic_start_blocked:false}],age_ms:1};
            case 'events':assert.ok((method as {events:{limit:number}}).events.limit<=32);return {status:'events',page:{namespace,retained_prefix_lost:false,events:[],next:null,tail:{namespace,project:null,after:0,through:null}}};
            case 'enqueue':case 'configure_project':{
                assert.equal([...values.keys()].filter(key=>key.startsWith('airk.service.intent.')).length,1,'persist before send');
                if(fail)throw new ServiceError('console_transport_closed',true);
                lastKind=name==='enqueue'?String((method as {enqueue:{kind:string}}).enqueue.kind):'configure';
                return {status:'job',job:{id:job,project,kind:lastKind}};
            }
            case 'job':return {status:'job',job:{id:job,project,kind:lastKind,outcome:{state:'succeeded'}}};
            default:return {status:name};
        }
    }});
    const host:ConsoleHost={version:'1.4.0',authority:'local-test',storage:{keys:()=>[...values.keys()],get:<T>(key:string)=>values.get(key) as T|undefined,update:async(key,value)=>{if(value===undefined)values.delete(key);else values.set(key,structuredClone(value));}},
        runtime:{discover:async()=>({bundle:{main:'/main',helper:'/helper',source:'/bundle',digest:binding.runtimeSha256,version:'1.4.0',platform:'test'},installDir:'/install',installed:true,inspection:{control_dir:'/control'},location:{}}),binding:()=>binding,ensure:async()=>binding,client:()=>client,snapshot:async()=>({}),repair:async()=>({}),prepareConfiguration:async()=>({incarnation:namespace,settings:'ab'.repeat(32),configuration:{operation:'vectors',enabled:false}}),
            reviewTransition:async(_binding,action)=>({action,installDir:binding.installDir,controlDir:binding.controlDir,observedActive:binding.runtimeSha256,expectedActive:binding.runtimeSha256,target:'cd'.repeat(32),version:'1.4.0'}),transition:async()=>({})},
        trusted:()=>trusted,changed(){},confirm:async()=>true,copy:async text=>{copied.push(text);},logs:async()=>{},resources:async()=>undefined,lifecycle:async()=>({status:'lifecycle'})};
    const controller=new ServiceConsoleController(host);
    const action=async(name:string,extra:RecordValue={})=>controller.action({type:'service',revision:controller.state.revision,action:name,...extra});
    return {controller,host,sent,values,copied,action,setTrusted:(value:boolean)=>{trusted=value;},setFail:(value:boolean)=>{fail=value;}};
}

test('Management Service concurrency action preserves other service policy and uses native resource configuration',async()=>{
    for(const concurrency of [4,8,16,32,64]){
        const f=fixture(),base=f.host.runtime.client({} as any);
        let submitted:unknown;
        f.host.resources=async(current,queryOnly)=>{
            assert.equal(queryOnly,true);
            assert.equal(current.preset,'balanced');
            return {...current,query_concurrency:concurrency};
        };
        f.host.runtime.client=()=>new ServiceClient({dispose(){},async request(method){
            if(method==='resources'&&submitted)return {status:'resources',observation:{configuration:(submitted as any).configure_resources.configuration}};
            if(method&&typeof method==='object'&&'configure_resources' in method){
                submitted=method;
                return {status:'resources',observation:{configuration:(method as any).configure_resources.configuration}};
            }
            return base.transport.request(method);
        }});
        try{
            await f.controller.refresh();
            assert.equal(managementView(f.controller.state).enabled.queryConcurrency,true);
            await f.action('queryConcurrency');
            assert.deepEqual(submitted,{configure_resources:{configuration:{preset:'balanced',query_concurrency:concurrency}}});
            assert.ok(managementView(f.controller.state).resources.some(row=>row[0]==='Concurrent evidence requests'&&row[1]===String(concurrency)));
        }finally{f.controller.dispose();}
    }
});

test('unconfirmed receipts stay retained and logged without interrupting project observations',async()=>{
    for(const malformed of [false,true]){
        const f=fixture(),base=f.host.runtime.client({} as any),diagnostics:{operation:string;error:unknown}[]=[];
        f.host.diagnosticFailure=async(operation,error)=>{diagnostics.push({operation,error});};
        f.host.runtime.client=()=>new ServiceClient({dispose(){},async request(method){
            if(typeof method==='object'&&method!==null&&'request_receipt' in method){
                return malformed?{status:'failure',code:{unexpected:true}}:{status:'request_receipt',job:null};
            }
            return base.transport.request(method);
        }});
        try{
            await f.controller.refresh();f.setFail(true);
            await f.action('index',{project});
            assert.equal(f.controller.state.message,undefined,'a lost acknowledgement has no warning');
            const saved=[...f.values.entries()].filter(([key])=>key.startsWith('airk.service.intent.'));
            assert.equal(saved.length,1);
            const writes=f.sent.filter(value=>typeof value==='object'&&value!==null&&'enqueue' in value).length;
            await f.controller.refresh();await f.controller.refresh();
            assert.equal(f.controller.state.message,undefined);
            assert.equal(f.controller.state.connected,true);
            assert.equal(f.controller.state.page?.projects.length,1,'receipt failure does not block the project page');
            assert.deepEqual(managementView(f.controller.state).pending,[]);
            assert.deepEqual([...f.values.entries()].filter(([key])=>key.startsWith('airk.service.intent.')),saved,'the exact request is retained');
            assert.equal(f.sent.filter(value=>typeof value==='object'&&value!==null&&'enqueue' in value).length,writes,'lookup never replays a write');
            assert.ok(diagnostics.some(item=>item.operation==='request_receipt'&&String(item.error).includes('request_outcome_unconfirmed')));
            assert.equal(diagnostics.length,malformed?3:2,'identical diagnostics are logged once per controller');
            if(malformed)assert.ok(diagnostics.some(item=>item.error instanceof Error&&item.error.cause instanceof ServiceError
                &&item.error.cause.cause instanceof Error&&item.error.cause.cause.message.includes('unexpected')),'native failure details remain in the diagnostic cause chain');
        }finally{f.controller.dispose();}
    }
});
test('console messages reject stale revisions, injected bodies and out-of-scope identities',async()=>{
    const f=fixture();await f.controller.refresh();const state=f.controller.state;
    for(const message of [
        {type:'service',revision:'old',action:'index',project},
        {type:'service',revision:state.revision,action:'index',project,command:'delete'},
        {type:'service',revision:state.revision,action:'index',project:'ff'.repeat(32)},
        {type:'service',revision:state.revision,action:'rpc',body:{shutdown:{}}},
    ])assert.equal(consoleAction(message,state),undefined);
    assert.ok(consoleAction({type:'service',revision:state.revision,action:'index',project},state));
    f.controller.dispose();
});

test('console binds sampled wait reasons and queued cancellation to observed jobs',async()=>{
    const f=fixture();await f.controller.refresh();const state=f.controller.state,row=state.page!.projects[0],worker=(state.workers!.observations as RecordValue[])[0];
    row.pending_job={id:job,project,kind:'index',state:'pending'};
    worker.waiting={job,reason:'extraction_capacity',age_ms:125};
    assert.match(shown(state),/Waiting for extraction capacity/);
    assert.equal(managementView(state).projects[0].pendingJobId,job,'Management offers cancellation for the observed queued job');
    const action={type:'service',revision:state.revision,action:'cancelJob',id:job};
    assert.ok(consoleAction(action,state));
    assert.equal(consoleAction({...action,id:'ff'.repeat(32)},state),undefined);
    assert.equal(consoleAction({...action,action:'retryJob'},state),undefined);
    worker.waiting={job:'ff'.repeat(32),reason:'reader_lease',age_ms:999};
    assert.doesNotMatch(shown(state),/Waiting for active readers/);
    row.pending_paused=true;
    assert.match(shown(state),/Queued work is paused by project policy/);
    row.pending_paused=false;
    const identity={service:instance,worker:namespace,project,fencing_generation:3};
    worker.identity=identity;
    worker.waiting={job,reason:'reader_lease',age_ms:50};
    row.pending_job=null;row.progress={worker:identity,current_fencing_generation:3,job:{id:job,state:'running'},progress:{phase:'semantic'}};
    assert.match(shown(state),/Waiting for active readers/);
    (row.progress as RecordValue).current_fencing_generation=4;
    assert.doesNotMatch(shown(state),/Waiting for active readers/);
    worker.waiting={job,reason:'<script>bad</script>',age_ms:50};
    assert.doesNotMatch(shown(state),/<script>bad/);
    f.controller.dispose();
});

test('project configuration is journaled before send and uncertain retry preserves the complete guarded request',async()=>{
    const f=fixture();let choices=0;
    f.host.configuration=async()=>{choices++;return {operation:'vectors',enabled:false};};
    await f.controller.refresh();f.controller.state.page!.projects[0].inspection={snapshot:{settings_digest:'ab'.repeat(32)}};f.setFail(true);await f.action('configureProject',{project});
    assert.equal(f.controller.state.pending.length,1);
    const original=f.sent.find(value=>value&&typeof value==='object'&&'configure_project' in value);
    assert.ok(original);f.setFail(false);await f.action('retryIntent',{id:f.controller.state.pending[0].id});
    assert.equal(f.controller.state.pending.length,0);assert.equal(choices,1);
    assert.deepEqual(f.sent.filter(value=>value&&typeof value==='object'&&'configure_project' in value),[original,original]);
    f.controller.dispose();
});

test('trust loss during a project setting dialog prevents native preparation and submission',async()=>{
    const f=fixture();let prepared=false;f.host.configuration=async()=>{f.setTrusted(false);return {operation:'vectors',enabled:false};};
    f.host.runtime.prepareConfiguration=async()=>{prepared=true;throw new Error('Must not prepare');};
    await f.controller.refresh();f.controller.state.page!.projects[0].inspection={snapshot:{settings_digest:'ab'.repeat(32)}};const before=f.sent.length;await f.action('configureProject',{project});
    assert.equal(prepared,false);assert.equal(f.sent.length,before);assert.equal(f.controller.state.pending.length,0);f.controller.dispose();
});

test('scalar preferences are accepted durably without extension inspection or preparation',async()=>{
    const f=fixture();f.host.configuration=async()=>({operation:'vectors',enabled:false});
    f.host.runtime.prepareConfiguration=async()=>{throw new Error('Native owner prepares this preference');};
    await f.controller.refresh();await f.action('configureProject',{project});
    const sent=f.sent.find(value=>value&&typeof value==='object'&&'configure_project' in value) as any;
    assert.deepEqual(sent.configure_project.request,{incarnation:namespace,settings:project,configuration:{operation:'vectors',enabled:false},prepare:true});
    assert.equal(f.sent.some(value=>value&&typeof value==='object'&&'enqueue' in value),false);
    assert.match(f.controller.state.message!,/continues when VS Code closes/);f.controller.dispose();
});

test('semantic changes join pending settings and repeat index clicks follow existing source work',async()=>{
    const f=fixture();await f.controller.refresh();
    const row=f.controller.state.page!.projects[0];row.configuration={id:job,state:'pending'};
    await f.action('enableSemantic',{project});
    assert.match(f.controller.state.message!,/already queued/);
    assert.equal(f.sent.some(value=>value&&typeof value==='object'&&'configure_project' in value),false);
    row.configuration=null;row.progress={job:{id:job,kind:'reconcile',state:'running'}};
    await f.action('index',{project});assert.match(f.controller.state.message!,/Following the existing request/);
    assert.equal(f.sent.some(value=>value&&typeof value==='object'&&'enqueue' in value),false);f.controller.dispose();
});

test('semantic control refuses dispatch if trust is lost while obtaining its durable request key',async()=>{
    for(const trusted of [true,false]){
        const f=fixture(),base=f.host.runtime.client({} as any);let lose=false;
        f.host.runtime.client=()=>new ServiceClient({dispose(){},async request(method){
            const result=await base.transport.request(method);
            if(lose&&method==='request_epoch')f.setTrusted(trusted);
            return result;
        }});
        await f.controller.refresh();lose=true;
        await f.action('enableSemantic',{project});
        assert.equal(f.sent.filter(value=>value&&typeof value==='object'&&'configure_project' in value).length,trusted?1:0);f.controller.dispose();
    }
});

test('direct removal inspects a missing-source project and requires approval before unregistering',async()=>{
    for(const decision of ['approve','decline','lose-trust'] as const){
        const f=fixture();let prepared=false,reviewed='';
        f.host.runtime.prepareConfiguration=async(_binding,selected,draft)=>{
            assert.equal(selected,project);assert.deepEqual(draft,{operation:'unregister'});
            assert.ok(f.sent.some(value=>typeof value==='object'&&value!==null&&'job' in value));
            prepared=true;return {incarnation:'cd'.repeat(16),settings:'ef'.repeat(32),configuration:draft};
        };
        f.host.confirm=async message=>{assert.equal(prepared,true);reviewed=message;if(decision==='lose-trust')f.setTrusted(false);return decision!=='decline';};
        await f.controller.refresh();
        f.controller.state.page!.projects[0].watch={failure:'root_unavailable'};
        assert.equal(f.controller.state.page!.projects[0].inspection,null);
        await f.action('unregisterProject',{project});
        assert.match(reviewed,/source folder has been deleted/);
        assert.match(reviewed,/deletes.*index data.*saved settings/);
        assert.equal(f.sent.filter(value=>value&&typeof value==='object'&&'configure_project' in value).length,decision==='approve'?1:0);
        assert.equal(f.controller.state.inactiveProjects===true,decision==='approve');
        assert.equal(f.controller.state.pending.length,0);
        if(decision==='approve')assert.equal(consoleAction({type:'service',revision:f.controller.state.revision,action:'unregisterProject',project},f.controller.state),undefined);
        f.controller.dispose();
    }
});

test('unregister binds the prepared project and requires approval before durable dispatch',async()=>{
    for(const approved of [false,true]){
        const f=fixture();let reviewed='';
        f.host.configuration=async()=>({operation:'unregister'});
        f.host.runtime.prepareConfiguration=async(_binding,selected,draft)=>{
            assert.equal(selected,project);return {incarnation:'cd'.repeat(16),settings:'ef'.repeat(32),configuration:draft};
        };
        f.host.confirm=async message=>{reviewed=message;return approved;};
        await f.controller.refresh();f.controller.state.page!.projects[0].inspection={snapshot:{settings_digest:'ab'.repeat(32)}};
        await f.action('configureProject',{project});
        assert.match(reviewed,/Unregister/);assert.match(reviewed,/Source: \/sensitive/);assert.match(reviewed,/deletes.*index data.*saved settings/);
        assert.equal(f.sent.filter(value=>value&&typeof value==='object'&&'configure_project' in value).length,approved?1:0);
        assert.equal(f.controller.state.pending.length,0);f.controller.dispose();
    }
    const f=fixture();f.host.configuration=async()=>({operation:'unregister'});
    f.host.runtime.prepareConfiguration=async(_binding,_project,draft)=>({incarnation:'cd'.repeat(16),settings:'ef'.repeat(32),configuration:draft});
    f.host.confirm=async()=>{f.setTrusted(false);return true;};
    await f.controller.refresh();f.controller.state.page!.projects[0].inspection={snapshot:{settings_digest:'ab'.repeat(32)}};
    await f.action('configureProject',{project});
    assert.equal(f.sent.filter(value=>value&&typeof value==='object'&&'configure_project' in value).length,0);
    assert.equal(f.controller.state.pending.length,0);f.controller.dispose();
});

test('legacy startup controls require a separate reviewed scope and approval, preserve trust and resume only listed receipts',async()=>{
    for(const approval of ['decline','approve','lose-trust'] as const){
        const f=fixture();let removed=0,confirmed='';
        const target={root:'/old-source',data_dir:'/old-data',collection:'old-fixture',executable:'/old/airk'},key='ab'.repeat(32);
        const reviewed:any={installDir:'/install',controlDir:'/control',runtimeSha256:'90'.repeat(32),target,approval:key,value:{approval:key,preview:{spec:{id:'airk-fixture',target},observation:{owned:true,definition_matches:true}},workers:{matching_executable_pids:[123],complete:false,unobserved:1}}};
        f.host.legacy={choose:async()=>target,review:async()=>reviewed,load:async(_binding,id)=>{assert.equal(id,key);return {...reviewed,phase:'definition_removed'};},list:async()=>({items:[{approval:key,service_id:'airk-fixture',phase:'definition_removed'}]}),remove:async(_binding,review)=>{assert.equal(review.approval,key);removed++;return {startup_removed:true};}};
        f.host.confirm=async message=>{confirmed=message;if(approval==='lose-trust')f.setTrusted(false);return approval!=='decline';};
        await f.controller.refresh();
        assert.equal(consoleAction({type:'service',revision:f.controller.state.revision,action:'removeLegacyStartup'},f.controller.state),undefined);
        await f.action('reviewLegacyStartup');assert.equal(removed,0);assert.ok(f.controller.state.legacyReview);
        assert.equal(managementView(f.controller.state).legacy?.reviewed,true);
        await f.action('removeLegacyStartup');assert.match(confirmed,/Source: \/old-source/);assert.match(confirmed,/project settings and old indexes stay in place/);
        assert.equal(removed,approval==='approve'?1:0);
        f.setTrusted(true);await f.action('legacyStartupReceipts');
        assert.equal(consoleAction({type:'service',revision:f.controller.state.revision,action:'resumeLegacyStartup',id:'ef'.repeat(32)},f.controller.state),undefined);
        assert.equal(consoleAction({type:'service',revision:f.controller.state.revision,action:'resumeLegacyStartup',id:key,target:{root:'/injected'}},f.controller.state),undefined);
        f.host.confirm=async()=>false;await f.action('resumeLegacyStartup',{id:key});assert.equal(f.controller.state.legacyReview?.phase,'definition_removed');
        assert.equal(removed,approval==='approve'?1:0);f.controller.dispose();
    }
});

test('inactive cleanup requires a completed exact preview and host approval before journaled dispatch',async()=>{
    for(const approval of ['decline','approve','lose-trust'] as const){
        const f=fixture();let reviewed='';
        f.host.runtime.prepareConfiguration=async(_binding,selected,draft)=>{assert.equal(selected,project);return {incarnation:namespace,settings:project,configuration:draft};};
        await f.controller.refresh();
        assert.equal(consoleAction({type:'service',revision:f.controller.state.revision,action:'deleteDerived',project},f.controller.state),undefined);
        await f.action('inactiveProjects');assert.equal(f.controller.state.inactiveProjects,true);
        assert.ok(f.sent.some(value=>value&&typeof value==='object'&&'inactive_console' in value));
        assert.equal(consoleAction({type:'service',revision:f.controller.state.revision,action:'index',project},f.controller.state),undefined);
        assert.equal(consoleAction({type:'service',revision:f.controller.state.revision,action:'repair',scope:'all',operation:'auto'},f.controller.state),undefined);
        await f.action('deleteDerived',{project});assert.equal(f.controller.state.message,'cleanup_preview_required');
        const row=f.controller.state.page!.projects[0];row.configuration={id:job,project,kind:'configure',state:'succeeded'};
        f.controller.state.configurationResults=new Map([[project,{job,jobState:'succeeded',checkedAt:1,value:{active:instance,guard:'ab'.repeat(32),owned_files:9,owned_bytes:123,inventory_complete:false}}]]);
        f.host.confirm=async message=>{reviewed=message;if(approval==='lose-trust')f.setTrusted(false);return approval!=='decline';};
        await f.action('deleteDerived',{project});
        assert.match(reviewed,/Source: \/sensitive/);assert.match(reviewed,/bounded lower bounds/);assert.match(reviewed,/Saved settings and original sources are preserved/);
        const sent=f.sent.filter(value=>value&&typeof value==='object'&&'configure_project' in value) as any[];
        assert.equal(sent.length,approval==='approve'?1:0);
        if(sent.length)assert.deepEqual(sent[0].configure_project.request.configuration,{operation:'delete_derived',preview:'ab'.repeat(32)});
        assert.equal(f.controller.state.pending.length,0);f.controller.dispose();
    }
});

test('usage configuration waits for native completion and preserves a lost acknowledgement without replay',async()=>{
    for(const lost of [false,true]){
        const f=fixture(),base=f.host.runtime.client({} as any),inspect='ab'.repeat(32);let configured=0;
        f.host.runtime.prepareConfiguration=async(_binding,selected,draft)=>{
            assert.equal(selected,project);return {incarnation:'cd'.repeat(16),settings:'ef'.repeat(32),configuration:draft};
        };
        f.host.runtime.client=()=>new ServiceClient({dispose(){},async request(method){
            const name=typeof method==='string'?method:Object.keys(method as object)[0];
            if(name==='enqueue')return {status:'job',job:{id:inspect,project,kind:'inspect'}};
            if(name==='configure_project'){
                assert.ok([...f.values.keys()].some(key=>key.startsWith('airk.service.intent.')));configured++;
                if(lost)throw new ServiceError('console_transport_closed',true);
                return {status:'job',job:{id:job,project,kind:'configure'}};
            }
            if(name==='job'){
                const id=(method as any).job.id;return {status:'job',job:{id,project,kind:id===inspect?'inspect':'configure',outcome:{state:'succeeded'}}};
            }
            if(name==='configuration_result')return {status:'configuration',job:{id:job,project,kind:'configure'},result:{cleared:true}};
            return base.transport.request(method);
        }});
        if(lost)await assert.rejects(f.controller.projectConfiguration(project,{operation:'clear_usage'}),ServiceError);
        else await f.controller.projectConfiguration(project,{operation:'clear_usage'});
        assert.equal(configured,1);
        assert.equal(f.controller.state.pending.length,lost?1:0);
        await f.controller.refresh();assert.equal(configured,1,'read-only refresh must not replay a clear request');
        f.controller.dispose();
    }
});

test('closing or changing the usage scope during inspection prevents configuration dispatch',async()=>{
    const f=fixture(),base=f.host.runtime.client({} as any);let valid=true,prepared=false,configured=false;
    f.host.runtime.prepareConfiguration=async()=>{prepared=true;throw new Error('Stale selection');};
    f.host.runtime.client=()=>new ServiceClient({dispose(){},async request(method){
        const name=typeof method==='string'?method:Object.keys(method as object)[0];
        if(name==='enqueue')return {status:'job',job:{id:job,project,kind:'inspect'}};
        if(name==='job'){valid=false;return {status:'job',job:{id:job,project,kind:'inspect',outcome:{state:'succeeded'}}};}
        if(name==='configure_project'){configured=true;throw new Error('Must not dispatch');}
        return base.transport.request(method);
    }});
    await assert.rejects(f.controller.projectConfiguration(project,{operation:'clear_usage'},()=>valid));
    assert.equal(prepared,false);assert.equal(configured,false);f.controller.dispose();
});

test('disconnected snapshots retain bounded configuration results and Copy uses the exact saved value',async()=>{
    const f=fixture(),observation=await f.host.runtime.discover(),binding=await f.host.runtime.ensure();binding.payloadVerified=false;
    const result={summary:{baseline_tokens:42,returned_tokens:12},notice:'Explicit sample counts'};
    const row={project:{registration:{binding:project,collection:'fixture',root:'/source',data_root:'/data'}},
        configuration:{id:job,project,kind:'configure',state:'succeeded'}};
    observation.inspection={control_dir:'/control',payload_verified:false,registry_namespace:namespace,
        console:{revision:1,cached:true,observed_unix_ms:1,projects:[row],after:null},
        configuration_results:[{project,job,result,unavailable:false}]};
    observation.inspection.configuration_results_omitted=true;
    f.host.runtime.discover=async()=>observation;
    f.host.runtime.client=()=>{throw new Error('No selected executable may run');};
    f.host.runtime.snapshot=async()=>observation.inspection!;
    await f.controller.refresh();assert.equal(f.controller.state.connected,false);
    assert.deepEqual(f.controller.state.configurationResults?.get(project)?.value,result);
    await f.action('copyConfiguration',{project});assert.equal(f.copied[0],JSON.stringify(result,null,2));
    const cached=managementView(f.controller.state);
    assert.equal(cached.projects[0].configurationResult,true,'the cached configuration result stays available');
    assert.equal(cached.configurationResultsOmitted,true);
    const report=JSON.parse(diagnosticPreview(f.controller.state,false));
    assert.equal(report.configurationResultsOmitted,true);
    assert.equal(report.projects[0].configuration.id,job);
    assert.equal(report.projects[0].configurationResult.job,job);
    assert.doesNotMatch(JSON.stringify(report),/baseline_tokens|Explicit sample counts/);
    f.controller.dispose();
});
test('console persists before enqueue and exposes exact-key recovery after an uncertain outcome',async()=>{
    const f=fixture();await f.controller.refresh();f.setFail(true);await f.action('index',{project});
    assert.equal(f.controller.state.pending.length,1);const original=f.sent.find(value=>typeof value==='object'&&value!==null&&'enqueue' in value);
    f.setFail(false);await f.action('retryIntent',{id:f.controller.state.pending[0].id});
    assert.equal(f.controller.state.pending.length,0);
    assert.deepEqual(f.sent.filter(value=>typeof value==='object'&&value!==null&&'enqueue' in value),[original,original]);
    f.controller.dispose();
});
test('trust loss refuses native actions while reports remain previewable and redact source paths',async()=>{
    const f=fixture();await f.controller.refresh();const before=f.sent.length;f.setTrusted(false);
    await f.action('index',{project});assert.equal(f.sent.length,before);
    await f.action('previewReport',{includePaths:false});
    assert.ok(f.controller.state.preview);assert.doesNotMatch(f.controller.state.preview!,/sensitive|secret-user|private\/data|script>name/);
    await f.action('copyReport');assert.equal(f.copied.length,1);assert.equal(f.sent.length,before);
    f.controller.dispose();
});
test('an external service manager disables lifecycle actions and the full report keeps paths only on request',async()=>{
    const f=fixture();await f.controller.refresh();f.controller.state.lifecycle='external';
    const view=managementView(f.controller.state);
    assert.deepEqual([view.enabled.start,view.enabled.stop,view.enabled.restart],[false,false,false]);
    assert.equal(view.lifecycle,'external');
    const text=diagnosticPreview(f.controller.state,true);assert.match(text,/sensitive/);assert.match(text,/secret-user/);
    assert.ok(Buffer.byteLength(text)<=65536);f.controller.dispose();
});
test('bounded diagnostic export records truncation without source, credentials or raw errors',()=>{
    const f=fixture(),state=f.controller.state as ServiceConsoleState;
    state.events=Array.from({length:128},(_,sequence)=>({sequence,kind:'x'.repeat(512),code:'y'.repeat(512),query:'private query',token:'bearer-secret'}));
    const report=diagnosticPreview(state,false);assert.ok(Buffer.byteLength(report)<=256*1024);assert.doesNotMatch(report,/private query|bearer-secret/);
    assert.equal(JSON.parse(report).events.length,128);
    f.controller.dispose();
});
test('background refresh preserves the exact report reviewed before Copy',async()=>{
    const f=fixture();await f.controller.refresh();await f.action('previewReport',{includePaths:false});
    const preview=f.controller.state.preview;assert.ok(preview);
    await f.controller.refresh();assert.equal(f.controller.state.preview,preview);
    await f.action('copyReport');assert.equal(f.copied[0],preview);f.controller.dispose();
});

test('report open and save preserve captured bytes and reject webview supplied destinations',async()=>{
    const f=fixture(),opened:string[]=[],saved:string[]=[];
    f.host.openReport=async text=>{opened.push(text);};f.host.saveReport=async text=>{saved.push(text);};
    await f.controller.refresh();await f.action('previewReport',{includePaths:false});
    const captured=f.controller.state.preview!;
    await f.controller.refresh();await f.action('openReport');await f.action('saveReport');
    assert.deepEqual(opened,[captured]);assert.deepEqual(saved,[captured]);
    await f.action('saveReport',{path:'/forged'});assert.equal(saved.length,1);
    f.controller.dispose();
});

test('diagnostic report retains the failing job outcome and recovery budget without copying raw error fields',async()=>{
    const f=fixture();await f.controller.refresh();
    const row=f.controller.state.page!.projects[0];
    row.progress={job:{id:job,kind:'recover',state:'failed',outcome:{state:'failed',code:'quota_ledger_busy',error:'private raw error'}}};
    row.recovery=[{reason:'quota_ledger_busy',state:'waiting',attempts:1,limit:3,next_check_unix_ms:10,started_unix_ms:1,job}];
    const text=diagnosticPreview(f.controller.state,false),report=JSON.parse(text);
    assert.equal(report.projects[0].job.outcome.code,'quota_ledger_busy');
    assert.equal(report.projects[0].recovery[0].next_check_unix_ms,10);
    assert.doesNotMatch(text,/private raw error/);f.controller.dispose();
});

test('damaged payload uses cached pages and persists an instance-fenced Stop without launching it',async()=>{
    const f=fixture(),binding=await f.host.runtime.ensure();binding.payloadVerified=false;
    const page=(id:string,after:string|null)=>({revision:3,cached:true,observed_unix_ms:10,
        projects:[{project:{registration:{binding:id,collection:'offline',root:'/missing',data_root:'/saved'},paused:false}}],after});
    const second='ab'.repeat(32),requests:unknown[]=[];
    const lifecycle={recorded_instance:instance,namespace,epoch:7,pending:null,explicitly_stopped:false};
    const observation=await f.host.runtime.discover();
    observation.inspection={schema:1,metadata_only:true,payload_verified:false,control_dir:'/control',lifecycle,registry_namespace:namespace,console:page(project,project)};
    f.host.runtime.discover=async()=>observation;
    f.host.runtime.client=()=>{throw new Error('Damaged payload must never execute');};
    f.host.runtime.snapshot=async(_binding,after,revision)=>{
        requests.push({after,revision});return {...observation.inspection,console:after?page(second,null):page(project,project)};
    };
    f.host.lifecycle=async(_binding,intent)=>{
        assert.equal(f.controller.state.pending.length,1,'durably saved before native Stop');
        assert.equal(intent.lifecycle.action,'stop');assert.equal(intent.lifecycle.expected_instance,instance);
        assert.equal(intent.lifecycle.key.namespace,namespace);assert.equal(intent.lifecycle.key.epoch,7);
        return {status:'lifecycle',record:{phase:'complete',request:{...intent.lifecycle,manager:false}}};
    };
    await f.controller.refresh();assert.equal(f.controller.state.connected,false);
    assert.equal(f.controller.state.page?.projects[0].project&&((f.controller.state.page.projects[0].project as RecordValue).registration as RecordValue).binding,project);
    assert.equal(managementView(f.controller.state).enabled.stop,true,'a recorded instance can still be stopped from cached state');
    await f.action('nextPage');assert.deepEqual(requests.at(-1),{after:project,revision:3});
    assert.equal(((f.controller.state.page!.projects[0].project as RecordValue).registration as RecordValue).binding,second);
    await f.action('stop');assert.equal(f.controller.state.pending.length,0);assert.equal(f.controller.state.message,undefined);
    assert.equal((f.controller.state.bootstrap.value?.lifecycle as RecordValue).explicitly_stopped,true);
    assert.equal(f.sent.length,0);f.controller.dispose();
});

test('a failed health check changes previously live resource and work observations to cached',async()=>{
    const f=fixture();await f.controller.refresh();assert.equal(f.controller.state.resources.state,'live');
    f.host.runtime.client=()=>new ServiceClient({dispose(){},request:async()=>{throw new ServiceError('service_down');}});
    await f.action('connect');assert.equal(f.controller.state.connected,false);
    assert.equal(f.controller.state.resources.state,'cached');assert.equal(f.controller.state.model.state,'cached');
    const html=shown(f.controller.state);assert.match(html,/Cached sample/);assert.doesNotMatch(html,/Current generation/);
    f.controller.dispose();
});

test('runtime repair requires Stop and approves only the native reviewed preview',async()=>{
    const f=fixture(),calls:(string|undefined)[]=[],preview='ab'.repeat(32);
    f.host.runtime.repair=async(_binding,token)=>{calls.push(token);return token?{repaired:true}:{preview,preserved_files:5,preserved_bytes:123,preserved_directory:'/preserved'};};
    await f.controller.refresh();await f.action('repairRuntime');
    assert.equal(f.controller.state.message,'stop_service_before_payload_repair');assert.deepEqual(calls,[]);
    const observation=await f.host.runtime.discover();observation.inspection={control_dir:'/control',lifecycle:{explicitly_stopped:true}};
    f.host.runtime.discover=async()=>observation;
    await f.action('connect');f.host.confirm=async()=>false;await f.action('repairRuntime');assert.deepEqual(calls,[undefined]);
    f.host.confirm=async message=>{assert.match(message,/123 bytes/);assert.match(message,new RegExp(preview));return true;};
    await f.action('repairRuntime');assert.deepEqual(calls,[undefined,undefined,preview]);
    assert.match(f.controller.state.message!,/Runtime payload repaired/);
    assert.equal(consoleAction({type:'service',revision:f.controller.state.revision,action:'repairRuntime',preview},f.controller.state),undefined);
    f.controller.dispose();
});

test('runtime activation reviews the exact target and requires host confirmation before applying it',async()=>{
    const f=fixture();let applied=0;
    const review=await f.host.runtime.reviewTransition(await f.host.runtime.ensure(),'upgrade');
    f.host.runtime.reviewTransition=async()=>review;
    f.host.runtime.transition=async(_binding,actual)=>{assert.equal(actual,review);applied++;return {schema:1,active:review.target};};
    await f.controller.refresh();f.host.confirm=async message=>{assert.match(message,new RegExp(review.target));assert.match(message,/changed selection invalidates/);return false;};
    await f.action('upgradeRuntime');assert.equal(applied,0);
    f.host.confirm=async()=>true;await f.action('upgradeRuntime');assert.equal(applied,1);
    assert.equal(consoleAction({type:'service',revision:f.controller.state.revision,action:'upgradeRuntime',target:review.target},f.controller.state),undefined);
    f.controller.dispose();
});

test('a different native version keeps installer observations without executing its console',async()=>{
    const f=fixture(),binding=await f.host.runtime.ensure();binding.version='1.4.1';
    f.host.runtime.client=()=>{throw new Error('Mismatched console must not execute');};
    f.host.runtime.snapshot=async()=>({selection:{active:binding.runtimeSha256},approval:{version:'1.4.1',verification_basis:'trusted_distribution'},payload_verified:false});
    await f.controller.refresh();assert.equal(f.controller.state.connected,false);
    assert.match(f.controller.state.message!,/pin is preserved/);
    assert.equal(f.controller.state.bootstrap.value?.selectedVersion,'1.4.1');
    assert.equal(managementView(f.controller.state).enabled.upgradeRuntime,true,'Management offers the bundled runtime upgrade review');
    assert.equal(f.sent.length,0);f.controller.dispose();
});

test('trust loss during activation confirmation prevents mutation and unknown activation exposes its saved native phase',async()=>{
    const f=fixture();let applied=0;
    f.host.runtime.transition=async()=>{applied++;throw new ServiceError('native_activation_unknown',true);};
    await f.controller.refresh();f.host.confirm=async()=>{f.setTrusted(false);return true;};
    await f.action('rollbackRuntime');assert.equal(applied,0);
    f.setTrusted(true);f.host.confirm=async()=>true;
    const observation=await f.host.runtime.discover();observation.diagnosis={pending_activation:{candidate:'cd'.repeat(32),phase:'probe'}};
    f.host.runtime.discover=async()=>observation;
    await f.action('rollbackRuntime');assert.equal(applied,1);
    assert.equal(f.controller.state.message,'native_activation_unknown');
    assert.equal(managementView(f.controller.state).enabled.resumeActivation,true,'Management offers the saved activation review');
    assert.equal(JSON.parse(diagnosticPreview(f.controller.state,false)).runtimeActivation.phase,'probe');
    f.controller.dispose();
});

test('trust lost while native repair preview loads prevents approval and mutation',async()=>{
    const f=fixture(),observation=await f.host.runtime.discover();observation.inspection={control_dir:'/control',lifecycle:{explicitly_stopped:true}};
    f.host.runtime.discover=async()=>observation;let calls=0,confirmed=false;
    f.host.confirm=async()=>{confirmed=true;return true;};
    f.host.runtime.repair=async()=>{calls++;f.setTrusted(false);return {preview:'ab'.repeat(32)};};
    await f.controller.refresh();await f.action('repairRuntime');
    assert.equal(calls,1);assert.equal(confirmed,false);assert.equal(f.controller.state.message,'runtime_client_no_longer_authorized');f.controller.dispose();
});

test('preparation timeout distinguishes an unsubmitted change from an accepted native change',async context=>{
    let now=Date.now();context.mock.method(Date,'now',()=>now+=100000);
    for(const waitingKind of ['inspect','configure']){
        const f=fixture(),base=f.host.runtime.client({} as any);
        f.host.runtime.client=()=>new ServiceClient({dispose(){},async request(method){
            const response=await base.transport.request(method);
            if(typeof method==='object'&&method!==null&&'job' in method){
                const value=response as any;
                if(value.job.kind===waitingKind){delete value.job.outcome;value.job.state='pending';}
            }
            return response;
        }});
        await assert.rejects(f.controller.projectConfiguration(project,{operation:'usage',mode:'aggregates_only'}),waitingKind==='inspect'?/has not been submitted/:/was accepted/);
        assert.equal(f.sent.filter(value=>value&&typeof value==='object'&&'configure_project' in value).length,waitingKind==='inspect'?0:1);
        assert.equal(f.controller.state.pending.length,0,'acknowledged jobs retain native outcomes, not an unknown request');f.controller.dispose();
    }
});


test('removed-project browsing refreshes active Explorer choices without changing the Management page', async()=>{
    const f=fixture(),base=f.host.runtime.client({} as any);
    let active=true;
    f.host.runtime.client=()=>new ServiceClient({dispose(){},async request(method){
        const name=typeof method==='string'?method:Object.keys(method as object)[0];
        if(name==='inactive_console')return {status:'console',page:{revision:2,cached:true,observed_unix_ms:2,after:null,
            projects:[{project:{registration:{binding:'ab'.repeat(32),root:'/deleted',data_root:'/derived',collection:'removed'},paused:true,fencing_generation:2}}]}};
        if(name==='console'&&!active)return {status:'console',page:{revision:3,cached:true,observed_unix_ms:3,after:null,projects:[]}};
        return base.transport.request(method);
    }});
    try{
        await f.controller.refresh();
        await f.action('inactiveProjects');
        assert.equal(f.controller.state.inactiveProjects,true);
        assert.equal((f.controller.state.page!.projects[0].project as any).registration.root,'/deleted');
        assert.equal((f.controller.state.activePage!.projects[0].project as any).registration.binding,project);
        active=false;await f.controller.refresh();
        assert.deepEqual(f.controller.state.activePage!.projects,[],'actual removal must clear the active choices');
        assert.equal(f.controller.state.inactiveProjects,true);
    }finally{f.controller.dispose();}
});


// Run the actual controller and ServiceClient path; defer only transport replies.
test('verified health is published before every optional console probe finishes',async()=>{
    for(const blockedMethod of ['resources','embedding_status','source_status','logs','console','workers','events']){
        const f=fixture(),base=f.host.runtime.client({} as any),snapshots:ServiceConsoleState[]=[];
        let release!:()=>void,entered!:()=>void;
        const gate=new Promise<void>(resolve=>{release=resolve;});
        const blocked=new Promise<void>(resolve=>{entered=resolve;});
        // Capture values at publication time. Holding mutable state references would
        // incorrectly turn an old disconnected observation into a later healthy one.
        f.host.changed=state=>snapshots.push(structuredClone(state));
        f.host.runtime.client=()=>new ServiceClient({dispose(){},async request(method){
            const name=typeof method==='string'?method:Object.keys(method as object)[0];
            if(name===blockedMethod){entered();await gate;}
            return base.transport.request(method);
        }});
        const poll=f.controller.refresh();
        try{
            await blocked;
            const early=snapshots.find(state=>state.connected&&state.health.state==='live'&&state.busy);
            assert.ok(early,`health must publish before ${blockedMethod}`);
            assert.equal(early!.health.value?.status,'health');
            assert.equal(f.controller.state.busy,true);
            const immutable=structuredClone(early);
            release();await poll;
            assert.equal(f.controller.state.connected,true);
            assert.equal(f.controller.state.busy,false);
            assert.deepEqual(early,immutable,'later optional observations cannot rewrite captured publications');
        }finally{release();await poll;f.controller.dispose();}
    }
});

test('early verified health does not manufacture health after a native refusal',async()=>{
    const f=fixture(),snapshots:ServiceConsoleState[]=[],methods:string[]=[];
    f.host.changed=state=>snapshots.push(structuredClone(state));
    f.host.runtime.client=()=>new ServiceClient({dispose(){},async request(method){
        const name=typeof method==='string'?method:Object.keys(method as object)[0];methods.push(name);
        if(name==='health')throw new ServiceError('service_explicitly_stopped');
        throw new Error('Optional probes must not run after failed health');
    }});
    try{
        await f.controller.refresh();
        assert.deepEqual(methods,['health']);
        assert.ok(!snapshots.some(state=>state.connected));
        assert.equal(f.controller.state.connected,false);
        assert.equal(f.controller.state.health.state,'failed');
        assert.equal(f.controller.state.health.code,'service_explicitly_stopped');
    }finally{f.controller.dispose();}
});

test('trust revocation and disposal reject a delayed health reply before early publication',async()=>{
    for(const cancel of ['trust','dispose'] as const){
        const f=fixture(),base=f.host.runtime.client({} as any),snapshots:ServiceConsoleState[]=[];
        let release!:()=>void,entered!:()=>void;
        const gate=new Promise<void>(resolve=>{release=resolve;});
        const blocked=new Promise<void>(resolve=>{entered=resolve;});
        f.host.changed=state=>snapshots.push(structuredClone(state));
        f.host.runtime.client=()=>new ServiceClient({dispose(){},async request(method){
            if(method==='health'){entered();await gate;}
            return base.transport.request(method);
        }});
        const poll=f.controller.refresh();
        try{
            await blocked;
            if(cancel==='trust')f.setTrusted(false);else f.controller.dispose();
            const before=snapshots.length;release();await poll;
            assert.ok(!snapshots.some(state=>state.connected));
            if(cancel==='dispose')assert.equal(snapshots.length,before,'disposed controller cannot publish late health');
            else assert.equal(snapshots.at(-1)!.trusted,false);
        }finally{release();await poll;f.controller.dispose();}
    }
});

test('optional failure retains genuine early health and records the failed observation',async()=>{
    const f=fixture(),base=f.host.runtime.client({} as any),snapshots:ServiceConsoleState[]=[];
    let release!:()=>void,entered!:()=>void;
    const gate=new Promise<void>(resolve=>{release=resolve;});
    const blocked=new Promise<void>(resolve=>{entered=resolve;});
    f.host.changed=state=>snapshots.push(structuredClone(state));
    f.host.runtime.client=()=>new ServiceClient({dispose(){},async request(method){
        if(method==='resources'){entered();await gate;throw new ServiceError('resource_observation_failed');}
        return base.transport.request(method);
    }});
    const poll=f.controller.refresh();
    try{
        await blocked;const early=snapshots.find(state=>state.connected&&state.busy)!;
        assert.ok(early);assert.equal(early.resources.state,'unavailable');
        release();await poll;
        assert.equal(early.resources.state,'unavailable','captured early snapshot stays immutable');
        assert.equal(f.controller.state.resources.state,'failed');
        assert.equal(f.controller.state.resources.code,'resource_observation_failed');
        assert.equal(f.controller.state.connected,true);
    }finally{release();await poll;f.controller.dispose();}
});

test('split journal reconciles retained witness through observed pending refresh and preserves completed proof in exports',async()=>{
    const f=fixture(),base=f.host.runtime.client({} as any),split=splitFixture(),journal=new IntentJournal(f.host.storage,'/control',namespace);
    const first=splitAtStage(split.snapshot,'destination_validated',4),initial=record(first.receipt),sourceProof=record(record(splitAtStage(split.snapshot,'original_fenced').receipt).remainder_witness);
    const witnessJob=(id:string)=>({purpose:'original_witness',caller:'c1'.repeat(16),key:{namespace,epoch:7,nonce:id.repeat(16)},job:id.repeat(32),backing:record(record(first.request).original_registration).binding,engine_identity:'c2'.repeat(32),configuration_digest:'c3'.repeat(32)});
    initial.jobs=[witnessJob('c4')];initial.remainder_witness={...sourceProof,job:'c4'.repeat(32)};
    const pending=structuredClone(first),pendingReceipt=record(pending.receipt);pendingReceipt.revision=5;pendingReceipt.previous_receipt_digest=first.receipt_digest;pendingReceipt.jobs=[witnessJob('c4'),witnessJob('c5')];pendingReceipt.state={state:'running'};pending.receipt_digest='c6'.repeat(32);
    const completed=structuredClone(pending),completedReceipt=record(completed.receipt);completedReceipt.revision=6;completedReceipt.previous_receipt_digest=pending.receipt_digest;completedReceipt.state={state:'pending'};completedReceipt.remainder_witness={...sourceProof,job:'c5'.repeat(32),expected_hash:'c7'.repeat(32),observed_from_unix_ms:3,observed_to_unix_ms:4,future_verification:'complete_native_proof_retained'};completed.receipt_digest='c8'.repeat(32);
    let current=first,reads=0,writes=0;
    f.host.runtime.client=()=>new ServiceClient({dispose(){},async request(input){
        if(typeof input!=='string'&&'catalog_split_receipt'in record(input)){reads++;return {status:'catalog_split_receipt',...record(record(input).catalog_split_receipt),snapshot:current,expired:false};}
        if(typeof input!=='string'&&('catalog_split_apply'in record(input)||'catalog_split_recover'in record(input))){writes++;throw new Error('read reconciliation must never dispatch a split mutation');}
        return base.transport.request(input);
    }});
    let reopened:ServiceConsoleController|undefined;
    try{
        await journal.prepare('Tracked accepted split',{catalog_split_apply:{draft:split.draft}});await f.controller.refresh();
        await f.controller.catalogReconcile();assert.deepEqual(record(f.controller.catalogPendingDetails()[0].last_known_split_snapshot).receipt,first.receipt);
        current=pending;await f.controller.catalogReconcile();assert.deepEqual(record(f.controller.catalogPendingDetails()[0].last_known_split_snapshot).receipt,pending.receipt);
        current=completed;await f.controller.catalogReconcile();const details=f.controller.catalogPendingDetails();assert.equal(details.length,1);assert.deepEqual(record(details[0].last_known_split_snapshot).receipt,completed.receipt);
        assert.ok(splitExport({details}).includes('complete_native_proof_retained'));assert.ok(splitExport({details},true).includes('complete_native_proof_retained'));
        f.controller.dispose();reopened=new ServiceConsoleController(f.host);await reopened.refresh();assert.deepEqual(record(reopened.catalogPendingDetails()[0].last_known_split_snapshot).receipt,completed.receipt);await reopened.catalogReconcile();assert.equal(reads,4);assert.equal(writes,0);
    }finally{f.controller.dispose();reopened?.dispose();}
});
