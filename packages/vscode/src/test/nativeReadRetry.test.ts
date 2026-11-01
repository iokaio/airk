// SPDX-License-Identifier: Apache-2.0
import {strict as assert} from 'node:assert';
import {test} from 'node:test';
import {assertQuickStatusTimeout,boundedReply,observeQuickStatus,readRetryDelay,readRetryReceipt,requestWithReadRetry,QuickStatusObservation} from './nativeReadRetry';
import {McpStdioSession} from '../mcpStdioSession';
import {publishedStatusPending} from './nativeServiceFixture';

test('mixed-batch receipt observation accepts both explicit replay contracts and rejects submitted or unknown errors',()=>{
    const wrap=(value:unknown)=>({isError:true,content:[{type:'text',text:JSON.stringify(value)}]});
    for(const code of ['publication_busy','selection_busy']){
        assert.deepEqual(readRetryReceipt(wrap({status:'retry',code,retryable:true,action:'repeat_same_request',retry_after_ms:1})),{code,action:'repeat_same_request'});
    }
    const error={code:'direct_query_setup',reason:'busy',query_submitted:false,retryable:true,retry_after_ms:1,fresh_brokerage_required:true};
    assert.deepEqual(readRetryReceipt(wrap({error})),{code:'direct_query_setup',query_submitted:false});
    assert.throws(()=>readRetryReceipt(wrap({error:{...error,query_submitted:true}})),/Unrecognized native read refusal: .*query_submitted/);
    assert.throws(()=>readRetryReceipt(wrap({status:'retry',code:'unknown_busy',retryable:true,action:'repeat_same_request',retry_after_ms:1})),/Unrecognized native read refusal: .*unknown_busy/);
});

test('quick-status measurements separate first response from confirmed recovery without source content',async()=>{
    const timeout={project:'a'.repeat(64),health:'unknown',index_state:'unknown',publication:null,files:null,chunks:null,index_error:{code:'status_timeout',retryable:true,message:'private sentinel'}};
    const busy={isError:true,content:[{type:'text',text:JSON.stringify({status:'retry',code:'selection_busy',retryable:true,action:'repeat_same_request',retry_after_ms:1})}]};
    for(const state of ['available','unavailable']){
        const confirmed={project:timeout.project,index_state:state,private:'private sentinel'};
        for(const replies of [[confirmed],[timeout,busy,confirmed]]){
            const events:QuickStatusObservation[]=[];const deadlines:number[]=[];let calls=0;
            const result=await observeQuickStatus(async deadline=>{deadlines.push(deadline);return replies[calls++];},undefined,value=>events.push(value));
            assert.equal(result,confirmed);assert.equal(calls,replies.length);assert.equal(new Set(deadlines).size,1);
            const attempts=events.filter(value=>value.kind==='attempt');
            assert.deepEqual(attempts.map(value=>value.outcome),replies.length===1?[state]:['status_timeout','retry',state]);
            for(const value of attempts){assert.ok(Number.isFinite(value.durationMs)&&value.durationMs>=0);assert.ok(value.elapsedMs>=value.durationMs);}
            assert.deepEqual(events.at(-1),{kind:'confirmed',attempts:calls,elapsedMs:events.at(-1)!.elapsedMs,recovered:calls>1});
            assert.ok(!JSON.stringify(events).includes('private sentinel'));assert.ok(!JSON.stringify(events).includes(timeout.project));
        }
    }
});

test('quick-status measurements preserve failure identity and original permissive return behavior',async()=>{
    for(const index_state of [undefined,null,{},'unexpected']){
        const value={index_state};const events:QuickStatusObservation[]=[];let calls=0;
        assert.equal(await observeQuickStatus(async()=>{calls++;return value;},undefined,event=>events.push(event)),value);
        assert.equal(calls,1);assert.equal(events.length,1);assert.equal(events[0].kind,'attempt');
        assert.equal((events[0] as Extract<QuickStatusObservation,{kind:'attempt'}>).outcome,'other');
    }
    const failure=new Error('private transport sentinel');const events:QuickStatusObservation[]=[];
    await assert.rejects(observeQuickStatus(async()=>{throw failure;},undefined,event=>events.push(event)),error=>error===failure);
    assert.equal(events.length,1);assert.equal((events[0] as Extract<QuickStatusObservation,{kind:'attempt'}>).outcome,'read_error');
    assert.ok(!JSON.stringify(events).includes(failure.message));
    const value={index_state:'unavailable'};let calls=0;
    assert.equal(await observeQuickStatus(async()=>{calls++;return value;},undefined,()=>{throw failure;}),value);assert.equal(calls,1);
});

test('quick-status measurements never confirm scope, timeout or exhausted-budget failures',async t=>{
    const timeout={project:'a'.repeat(64),health:'unknown',index_state:'unknown',publication:null,files:null,chunks:null,index_error:{code:'status_timeout',retryable:true}};
    for(const [replies,expected] of [
        [[timeout,{project:'b'.repeat(64),index_state:'available'}],'scope_mismatch'],
        [[{...timeout,files:0}],'invalid_timeout'],
    ] as const){
        const events:QuickStatusObservation[]=[];let calls=0;
        await assert.rejects(observeQuickStatus(async()=>replies[calls++],undefined,event=>events.push(event)));
        assert.equal(events.at(-1)!.kind,'attempt');assert.equal((events.at(-1) as Extract<QuickStatusObservation,{kind:'attempt'}>).outcome,expected);
        assert.ok(events.every(event=>event.kind!=='confirmed'));
    }
    const events:QuickStatusObservation[]=[];let calls=0;
    await assert.rejects(observeQuickStatus(async()=>{calls++;return timeout;},performance.now()-1,event=>events.push(event)),/observation budget/);
    assert.equal(calls,0);assert.deepEqual(events,[]);
    const lateEvents:QuickStatusObservation[]=[];
    let clock=0;const now=t.mock.method(performance,'now',()=>clock);
    try{
        await assert.rejects(observeQuickStatus(async deadline=>{clock=deadline;return {index_state:'available'};},100,event=>lateEvents.push(event)),/observation budget/);
    }finally{now.mock.restore();}
    assert.equal((lateEvents.at(-1) as Extract<QuickStatusObservation,{kind:'attempt'}>).outcome,'late');assert.ok(lateEvents.every(event=>event.kind!=='confirmed'));
});

test('quick-status timeouts disclose unknown readiness without fabricated evidence',()=>{
    const timeout={project:'a'.repeat(64),health:'unknown',index_state:'unknown',publication:null,files:null,chunks:null,index_error:{code:'status_timeout',retryable:true}};
    assert.doesNotThrow(()=>assertQuickStatusTimeout(timeout));
    for(const changed of [{project:'invalid'},{health:'ok'},{index_state:'available'},{files:0},{publication:{token:'invented'}},{index_error:{code:'status_unavailable',retryable:true}},{index_error:{code:'status_timeout',retryable:false}}]){
        assert.throws(()=>assertQuickStatusTimeout({...timeout,...changed}),assert.AssertionError);
    }
});

test('quick-status fixtures require recovery in the same scope within the original budget',async()=>{
    const timeout={project:'a'.repeat(64),health:'unknown',index_state:'unknown',publication:null,files:null,chunks:null,index_error:{code:'status_timeout',retryable:true}};
    const confirmed={project:timeout.project,index_state:'unavailable',publication:null,files:null,chunks:null};
    let calls=0;assert.equal(await observeQuickStatus(async()=>++calls===1?timeout:confirmed),confirmed);assert.equal(calls,2);
    calls=0;await assert.rejects(observeQuickStatus(async()=>++calls===1?timeout:{...confirmed,project:'b'.repeat(64)}),/source scope/);
    await assert.rejects(observeQuickStatus(async()=>timeout,performance.now()+20),/observation budget/);
    await assert.rejects(observeQuickStatus(async()=>({...timeout,index_error:{code:'status_unavailable',retryable:true}})),/status_timeout/);
});

test('native read fixtures require the complete safe replay contract',()=>{
    const wrap=(hint:unknown)=>({isError:true,content:[{type:'text',text:JSON.stringify(hint)}]});
    const hint={status:'retry',code:'publication_busy',retryable:true,action:'repeat_same_request',retry_after_ms:25};
    assert.equal(readRetryDelay(wrap(hint)),25);
    for(const changed of [{code:'unknown_busy'},{retryable:false},{action:'refresh_revision'},{retry_after_ms:0},{retry_after_ms:1001}]){
        assert.equal(readRetryDelay(wrap({...hint,...changed})),undefined);
    }
    assert.equal(readRetryDelay({...wrap(hint),isError:false}),undefined);
    for(const code of ['direct_query_admission','direct_query_setup']){
        const error={code,reason:'busy',query_submitted:false,retryable:true,retry_after_ms:100,fresh_brokerage_required:code==='direct_query_setup'};
        assert.equal(readRetryDelay(wrap({error})),100);
        for(const changed of [{query_submitted:true},{retryable:false},{fresh_brokerage_required:!error.fresh_brokerage_required}]){
            assert.equal(readRetryDelay(wrap({error:{...error,...changed}})),undefined);
        }
    }
});

test('quick-status observation waits for explicit native refusals without treating them as readiness',async()=>{
    const timeout={project:'a'.repeat(64),health:'unknown',index_state:'unknown',publication:null,files:null,chunks:null,index_error:{code:'status_timeout',retryable:true}};
    const confirmed={project:timeout.project,index_state:'unavailable',publication:null,files:null,chunks:null};
    const wrap=(hint:unknown)=>({isError:true,content:[{type:'text',text:JSON.stringify(hint)}]});
    const busy=wrap({error:{code:'direct_query_admission',reason:'busy',query_submitted:false,retryable:true,retry_after_ms:1,fresh_brokerage_required:false}});
    const selection=wrap({status:'retry',code:'selection_busy',retryable:true,action:'repeat_same_request',retry_after_ms:1});
    const replies=[timeout,busy,selection,confirmed];let calls=0;const deadlines:number[]=[];
    assert.equal(await observeQuickStatus(async deadline=>{deadlines.push(deadline);return replies[calls++];}),confirmed);assert.equal(calls,4);
    assert.equal(new Set(deadlines).size,1,'Native refusal retries retain the original deadline');
    calls=0;await assert.rejects(observeQuickStatus(async()=>[timeout,busy,{...confirmed,project:'b'.repeat(64)}][calls++]),/source scope/);
    for(const refused of [wrap({error:{code:'direct_query_admission',reason:'expired',query_submitted:false,retryable:false}}),
        wrap({error:{code:'direct_query_admission',reason:'busy',query_submitted:true,retryable:true,retry_after_ms:1,fresh_brokerage_required:false}}),
        wrap({status:'retry',code:'unknown_busy',retryable:true,action:'repeat_same_request',retry_after_ms:1}),
        {isError:true,content:[{type:'text',text:'permanent fixture failure'}]}]){
        calls=0;await assert.rejects(observeQuickStatus(async()=>{calls++;return refused;}),assert.AssertionError);assert.equal(calls,1);
    }
    calls=0;await assert.rejects(observeQuickStatus(async()=>{calls++;return busy;},performance.now()),/observation budget/);assert.equal(calls,0);
    calls=0;await assert.rejects(observeQuickStatus(async()=>{calls++;return wrap({status:'retry',code:'selection_busy',retryable:true,action:'repeat_same_request',retry_after_ms:1000});},performance.now()+500),/observation budget/);assert.ok(calls<=1,'An oversized retry hint cannot admit another read');
    calls=0;await assert.rejects(observeQuickStatus(async()=>{calls++;throw new Error('transport fixture failure');}),/transport fixture failure/);assert.equal(calls,1);
});

test('MCP fixture reads repeat only typed rejections and report every other failure with its payload',async()=>{
    const wrap=(hint:unknown)=>({isError:true,content:[{type:'text',text:JSON.stringify(hint)}]});
    const busy=wrap({error:{code:'direct_query_admission',reason:'busy',query_submitted:false,retryable:true,retry_after_ms:1,fresh_brokerage_required:false}});
    const ok={content:[{type:'text',text:'{"hits":[]}'}]};
    const session=(replies:unknown[],seen:unknown[]=[])=>({request:async(method:string,params:unknown)=>{seen.push([method,params]);return replies.shift();}}) as unknown as McpStdioSession;
    const seen:unknown[]=[];
    assert.equal(await requestWithReadRetry(session([busy,busy,ok],seen),'search',{query:'q'}),ok);
    assert.equal(seen.length,3);for(const call of seen)assert.deepEqual(call,['tools/call',{name:'search',arguments:{query:'q'}}],'Retries repeat the exact arguments');
    const permanent=wrap({error:{code:'PROJECT_AMBIGUOUS',detail:'x'.repeat(5000)}});
    await assert.rejects(requestWithReadRetry(session([permanent]),'search'),(error:unknown)=>error instanceof assert.AssertionError&&/search failed: .*PROJECT_AMBIGUOUS/.test(error.message)&&error.message.length<2200);
    await assert.rejects(requestWithReadRetry(session([busy,busy]),'search',{},0),/search failed: .*direct_query_admission/,'An exhausted budget reports the last typed rejection');
    assert.equal(boundedReply({a:1}),'{"a":1}');assert.match(boundedReply('y'.repeat(3000),10),/^"y{9}\.\.\. \(3002 bytes\)$/);
});

test('MCP receipt observation retains unsubmitted retries and never replays submitted status errors',async()=>{
    const refusal=(submitted:boolean)=>({isError:true,content:[{type:'text',text:JSON.stringify({error:{
        code:submitted?'status_observation_superseded':'direct_query_setup',reason:'busy',query_submitted:submitted,
        retryable:true,retry_after_ms:1,fresh_brokerage_required:true,
    }})}]});
    const unsubmitted=refusal(false),submitted=refusal(true),ok={content:[{type:'text',text:'{"index_state":"available"}'}]};
    const seen:unknown[]=[],observed:unknown[]=[],replies=[unsubmitted,ok];
    const session={request:async(method:string,params:unknown)=>{seen.push([method,params]);return replies.shift();}} as unknown as McpStdioSession;
    assert.equal(await requestWithReadRetry(session,'status',{response_mode:'full'},1000,value=>observed.push(value)),ok);
    assert.deepEqual(observed,[unsubmitted,ok]);assert.equal(seen.length,2);assert.deepEqual(seen[0],seen[1]);
    seen.length=0;observed.length=0;replies.push(submitted,ok);
    await assert.rejects(requestWithReadRetry(session,'status',{},1000,value=>observed.push(value)),/status failed: .*status_observation_superseded/);
    assert.equal(seen.length,1,'A submitted status failure must remain fatal, even with retryable=true');
    assert.deepEqual(observed,[submitted]);assert.deepEqual(replies,[ok]);
});

test('publication fixtures reject unhealthy, foreign and incomplete worker observations',()=>{
    const value={project:'a',index_state:'unavailable',publication:null,files:null,chunks:null,
        index_error:{code:'index_worker_unavailable',message:'No query submitted'},
        runtime:{service_error:null,service:{responsive:true,accepting:true,instance:'owner'},index_worker:{process:null}},
        work:{basis:'instance_bound_service_metadata',serving_worker_unavailable:true,failed_observations:[],worker:{automatic_start_blocked:false,identity:null}}};
    assert.equal(publishedStatusPending(value,'collection-a','a'),true);
    for(const change of [
        {project:'b'},
        {index_error:{code:'unknown',message:'unknown'}},
        {runtime:{...value.runtime,service:null}},
        {runtime:{...value.runtime,service:{...value.runtime.service,responsive:false}}},
        {work:{...value.work,failed_observations:['worker_sample_unavailable_or_wrong_project']}},
        {work:{...value.work,worker:{automatic_start_blocked:true,identity:null}}},
        {work:{...value.work,worker:{automatic_start_blocked:false,identity:{project:'b',service:'owner'}}}},
        {files:0},
    ])assert.throws(()=>publishedStatusPending({...value,...change},'collection-a','a'));
    const workerSide={...value,collection:'collection-a',index_error:{code:'index_status_unavailable',message:'private-source-message'}};
    assert.equal(publishedStatusPending(workerSide,'collection-a','a'),true);
    assert.throws(()=>publishedStatusPending({...workerSide,work:{...value.work,worker:{automatic_start_blocked:true,identity:null}}},'collection-a','a'),error=>!String(error).includes('private-source-message'));
    assert.throws(()=>publishedStatusPending({...workerSide,runtime:[]},'collection-a','a'));
    const published={collection:'collection-a',index_state:'available',publication:{collection:'collection-a',token:'exact'},files:1};
    assert.equal(publishedStatusPending(published,'collection-a','a'),false);
    assert.throws(()=>publishedStatusPending({...published,collection:'b'},'collection-a','a'));
});
