// SPDX-License-Identifier: Apache-2.0
import {strict as assert} from 'node:assert';
import {test} from 'node:test';
import childProcess = require('node:child_process');
import {assertQuickStatusTimeout,boundedReply,observeQuickStatus,readNative,readRetryDelay,readRetryReceipt,requestWithReadRetry,QuickStatusObservation} from './nativeReadRetry';
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
    assert.equal(readRetryDelay({...wrap(hint),structuredContent:hint}),25);
    assert.equal(readRetryDelay({...wrap(hint),structuredContent:{index_state:'available'}}),undefined);
    for(const changed of [{code:'unknown_busy'},{retryable:false},{action:'refresh_revision'},{retry_after_ms:0},{retry_after_ms:1001}]){
        assert.equal(readRetryDelay(wrap({...hint,...changed})),undefined);
    }
    assert.equal(readRetryDelay({...wrap(hint),isError:false}),undefined);
    for(const code of ['direct_query_admission','direct_query_setup']){
        const error={code,reason:'busy',query_submitted:false,retryable:true,retry_after_ms:100,fresh_brokerage_required:code==='direct_query_setup'};
        assert.equal(readRetryDelay(wrap({error})),100);
        assert.equal(readRetryDelay({...wrap({error}),structuredContent:{error}}),100);
        assert.equal(readRetryDelay({...wrap({error}),structuredContent:{hits:[]}}),undefined);
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

test('MCP fixture reads refuse a successful reply observed after their original deadline',async t=>{
    let clock=1000,calls=0;const now=t.mock.method(performance,'now',()=>clock);
    const session={request:async(_method:string,_params:unknown,timeout:number)=>{
        calls++;assert.equal(timeout,100);clock=1100;return {content:[{type:'text',text:'{"hits":[]}'}]};
    }} as unknown as McpStdioSession;
    try{await assert.rejects(requestWithReadRetry(session,'search',{},100),/observation budget/);assert.equal(calls,1);}finally{now.mock.restore();}
});

test('CLI fixture reads refuse late successful and typed-refusal callbacks with their complete replies',async t=>{
    const replies=[{content:[{type:'text',text:'{"files":1}'}]},
        {isError:true,content:[{type:'text',text:JSON.stringify({status:'retry',code:'selection_busy',retryable:true,action:'repeat_same_request',retry_after_ms:1})}]}];
    for(const reply of replies){
        let clock=1000,calls=0;const now=t.mock.method(performance,'now',()=>clock),stdout=JSON.stringify(reply),stderr='exact callback stderr';
        const execute=t.mock.method(childProcess,'execFile',((...args:unknown[])=>{
            calls++;assert.equal(args[0],'owned-native');assert.deepEqual(args[1],['tools','call','status']);
            assert.equal(Reflect.get(args[2] as object,'timeout'),100);
            clock=1100;
            const callback=args[3] as (error:Error|null,stdout:string,stderr:string)=>void;
            callback('isError' in reply?Object.assign(new Error('exit one'),{code:1}):null,stdout,stderr);
            return {} as childProcess.ChildProcess;
        }) as unknown as typeof childProcess.execFile);
        try{
            await assert.rejects(readNative('owned-native',['tools','call','status'],process.cwd(),1100),error=>{
                assert.match(String(error),/original observation budget/);
                assert.deepEqual(Reflect.get(error as object,'nativeResponse'),reply);
                assert.equal(Reflect.get(error as object,'nativeStdout'),stdout);assert.equal(Reflect.get(error as object,'nativeStderr'),stderr);return true;
            });
            assert.equal(calls,1,'A late refusal cannot admit another process');
        }finally{execute.mock.restore();now.mock.restore();}
    }
});

test('MCP fixture reads retain their last typed refusal when a retry timer overshoots the deadline',async t=>{
    let clock=1000,calls=0;const now=t.mock.method(performance,'now',()=>clock);
    t.mock.timers.enable({apis:['setTimeout']});
    const refusal={isError:true,content:[{type:'text',text:JSON.stringify({status:'retry',code:'selection_busy',retryable:true,action:'repeat_same_request',retry_after_ms:1})}]};
    const session={request:async()=>{calls++;return refusal;}} as unknown as McpStdioSession;
    try{
        const rejected=assert.rejects(requestWithReadRetry(session,'status',{},100),error=>{
            assert.match(String(error),/status failed: .*selection_busy/);assert.equal(Reflect.get(error as object,'nativeResponse'),refusal);return true;
        });
        await Promise.resolve();clock=1100;t.mock.timers.tick(1);await rejected;
        assert.equal(calls,1,'An expired retry delay cannot admit another MCP request');
    }finally{t.mock.timers.reset();now.mock.restore();}
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


const preparedSelection={schema:1 as const,context_key:'06'.repeat(32),namespace:'07'.repeat(16),expected_catalog_revision:7,
    grant_id:'grt_'+'03'.repeat(16),expected_grant_revision:1,index_id:'idx_'+'01'.repeat(16),project_ids:['prj_'+'02'.repeat(16)]};
function scopedSetupReceipt(code='scoped_status_prepare_timeout',nativeCode?:string):any{
    const body={error:{code,retryable:true,query_submitted:false,retry_after_ms:1,fresh_brokerage_required:true,
        ...(nativeCode?{native_control_response:{status:'failure',code:nativeCode}}:{})},requested_scope:{confirmed:false,selection:preparedSelection}};
    return {isError:true,content:[{type:'text',text:JSON.stringify(body)}],structuredContent:body};
}
test('scoped preparation retry keeps original authority across a monotonic metadata-clock refresh',async()=>{
    const timeout=scopedSetupReceipt();
    assert.equal(readRetryDelay(timeout,preparedSelection),1);
    assert.equal(readRetryDelay(timeout),undefined);
    for(const initial of [timeout,scopedSetupReceipt('scoped_connection_unavailable','project_capacity_busy')]){
        const advanced=structuredClone(initial);
        advanced.structuredContent.requested_scope.selection.expected_catalog_revision=12;
        advanced.content[0].text=JSON.stringify(advanced.structuredContent);
        const before=structuredClone(advanced),original=structuredClone(preparedSelection);
        assert.equal(readRetryDelay(advanced,preparedSelection),1,'native clock 7 to 12 retains the original reviewed authority');
        assert.deepEqual(advanced,before,'retain the complete native receipt');
        assert.deepEqual(preparedSelection,original,'never rewrite the caller selection');
    }
    for(const code of ['catalog_connection_busy','project_capacity_busy','project_restart_backoff','previous_workers_draining'])
        assert.equal(readRetryDelay(scopedSetupReceipt('scoped_connection_unavailable',code),preparedSelection),1);
    for(const code of ['scoped_connection_unavailable','scoped_read_unavailable','catalog_connection_outcome_unknown','native catalog exchange still pending'])
        assert.equal(readRetryDelay(scopedSetupReceipt('scoped_connection_unavailable',code),preparedSelection),undefined);
    for(const mutation of [
        (body:any)=>{body.error.query_submitted=true;},(body:any)=>{body.error.query_submitted=null;},
        (body:any)=>{body.error.retryable=false;},(body:any)=>{body.error.retry_after_ms=1001;},
        (body:any)=>{delete body.error.fresh_brokerage_required;},(body:any)=>{body.requested_scope.confirmed=true;},
        (body:any)=>{body.requested_scope.selection.grant_id='grt_'+'09'.repeat(16);},
        (body:any)=>{body.requested_scope.selection.expected_catalog_revision=6;},(body:any)=>{body.hits=[];},
        (body:any)=>{body.requested_scope.selection.expected_grant_revision=2;},
        (body:any)=>{body.requested_scope.selection.namespace='09'.repeat(16);},
        (body:any)=>{body.requested_scope.selection.context_key='09'.repeat(32);},
        (body:any)=>{body.requested_scope.selection.index_id='idx_'+'09'.repeat(16);},
        (body:any)=>{body.requested_scope.selection.project_ids=['prj_'+'09'.repeat(16)];},
        (body:any)=>{body.error.query_replay_safe=false;},
        (body:any)=>{body.status='retry';body.code='publication_busy';body.retryable=true;},
        (body:any)=>{body.error.native_control_response={status:'failure',code:'broker_busy'};},
    ]){
        const reply=structuredClone(timeout);mutation(reply.structuredContent);reply.content[0].text=JSON.stringify(reply.structuredContent);
        assert.equal(readRetryDelay(reply,preparedSelection),undefined,mutation.toString());
    }
    const advanced=structuredClone(timeout);advanced.structuredContent.requested_scope.selection.expected_catalog_revision=12;
    advanced.content[0].text=JSON.stringify(advanced.structuredContent);
    const replies=[advanced,{index_state:'available'}];let calls=0;const deadlines:number[]=[];
    assert.deepEqual(await observeQuickStatus(async deadline=>{deadlines.push(deadline);return replies[calls++];},undefined,undefined,preparedSelection),{index_state:'available'});
    assert.equal(calls,2);assert.equal(new Set(deadlines).size,1);
});

test('connection probe follows only scope-bound preparation hints and retains a refused native result',async t=>{
    const {probeMcp}=await import('../native'),{UNIFIED_TOOLS}=await import('../model');
    const spec={command:process.execPath,root:process.cwd(),dataDir:process.cwd(),collection:'fixture',name:'fixture',catalogSession:preparedSelection,
        service:{installDir:process.cwd(),project:'01'.repeat(32)}};
    const setup=scopedSetupReceipt();
    let calls=0,closed=0;
    const requests:unknown[]=[];
    let replies:any[]=[setup,{content:[{type:'text',text:'{"index_state":"available"}'}]}];
    const session={request:async(method:string,args:unknown)=>{
        if(method==='tools/list')return {tools:UNIFIED_TOOLS.map(name=>({name}))};
        requests.push(args);return replies[calls++];
    },close:()=>{closed++;}};
    t.mock.method(McpStdioSession,'open',async()=>({session} as any));
    await probeMcp(spec);assert.equal(calls,2);assert.equal(closed,1);assert.deepEqual(requests[0],requests[1]);
    const refused=structuredClone(setup);refused.structuredContent.requested_scope.selection.expected_grant_revision=2;
    refused.content[0].text=JSON.stringify(refused.structuredContent);replies=[refused];calls=0;
    await assert.rejects(probeMcp(spec),error=>{
        assert.deepEqual((error as Error&{nativeResponse:unknown}).nativeResponse,refused);return true;
    });
    assert.equal(calls,1);assert.equal(closed,2);
});


function singleCombinedRetry(hint:any={status:'retry',code:'selection_busy',retryable:true,action:'repeat_same_request',retry_after_ms:25}):any{
    const index='idx_'+'01'.repeat(16),identity='04'.repeat(32);
    const result={isError:true,content:[{type:'text',text:JSON.stringify(hint)}],structuredContent:hint};
    const body={schema:'airk.combined-read/1',selection:{project_ids:['prj_'+'02'.repeat(16)],index_ids:[index],grant_id:'grt_'+'03'.repeat(16),grant_revision:1,member_availability:'require',identity},
        publication_vector:{authority:'05'.repeat(32),observed_from_unix_ms:1,observed_to_unix_ms:2,members:[{index_id:index,publication:null}]},
        members:[{index_id:index,is_error:true,outcome:'failed',allocation:{index_id:index,limit:1,max_bytes:4096,deadline_ms:2000,verification_max_sources:1,verification_max_bytes:4096,verification_deadline_ms:2000},result}],
        ranking:[],partial:true,truncated:false,error:{code:'member_unavailable',retryable:false,failed_members:[index]},
        read_retry:{schema:1,scope:'same_single_required_member',index_id:index,selection_identity:identity,action:'repeat_same_request',query_replay_safe:true,
            result_pointer:'/members/0/result/structuredContent',code:hint.code??hint.error?.code,retry_after_ms:hint.retry_after_ms??hint.error?.retry_after_ms,
            fresh_brokerage_required:hint.error?.code==='direct_query_setup'}};
    return body;
}
const combinedEnvelope=(body:any)=>({isError:true,content:[{type:'text',text:JSON.stringify(body)}],structuredContent:body});

test('combined retry preserves complete qualified framing and rejects unknown evidence or crossed scope',()=>{
    const original=singleCombinedRetry();
    const scope={schema:1,qualifier:'selected_subset',fingerprint:'08'.repeat(32),provenance:'user_reviewed_project_grant',
        grant:{id:original.selection.grant_id,revision:1},members:[{index_id:original.selection.index_ids[0],
            memberships:[{project_id:original.selection.project_ids[0],membership_revision:1}],source_root_ids:['src_'+'07'.repeat(16)],source_revision:1,policy_epoch:0,policy_digest:null}]};
    const hint={...original.members[0].result.structuredContent,scope_observation:scope,request_id:'bounded-request'};
    assert.equal(readRetryDelay(combinedEnvelope(singleCombinedRetry(hint))),25);
    for(const provenance of ['imported_legacy_snapshot','imported_standalone_adoption']){
        const adopted={...hint,scope_observation:{...scope,provenance}};
        assert.equal(readRetryDelay(combinedEnvelope(singleCombinedRetry(adopted))),25,'Native imported lineage retains the same exact safe-admission retry');
        assert.equal(readRetryDelay(combinedEnvelope(singleCombinedRetry({...adopted,scope_observation:{...adopted.scope_observation,grant:{...scope.grant,revision:2}}}))),undefined,'New provenance never waives revision binding');
    }
    assert.equal(readRetryDelay(combinedEnvelope(singleCombinedRetry({...hint,scope_observation:{...scope,provenance:'future_unrecognized'}}))),undefined);
    for(const key of ['unknown_future','index_state','chunks','symbols','metadata','hits']){
        assert.equal(readRetryDelay(combinedEnvelope(singleCombinedRetry({...hint,[key]:[]}))),undefined,key);
        const error={code:'direct_query_admission',reason:'busy',query_submitted:false,retryable:true,fresh_brokerage_required:false,retry_after_ms:25,[key]:[]};
        assert.equal(readRetryDelay(combinedEnvelope(singleCombinedRetry({error}))),undefined,'error.'+key);
    }
    for(const change of [
        (value:any)=>{value.grant.revision=2;},(value:any)=>{value.members[0].index_id='idx_'+'09'.repeat(16);},
        (value:any)=>{value.members[0].memberships[0].project_id='prj_'+'09'.repeat(16);},
        (value:any)=>{value.members[0].policy_epoch=1;},(value:any)=>{value.members[0].memberships=[];},
        (value:any)=>{value.unrecognized=true;},
    ]){
        const invalid=structuredClone(scope);change(invalid);
        assert.equal(readRetryDelay(combinedEnvelope(singleCombinedRetry({...hint,scope_observation:invalid}))),undefined,change.toString());
    }
    assert.equal(readRetryDelay(combinedEnvelope(singleCombinedRetry({...hint,request_id:'é'.repeat(65)}))),undefined);
});

test('combined retry requires declared exact single-member admission without partial evidence or widening',()=>{
    const original=singleCombinedRetry();
    assert.equal(readRetryDelay(combinedEnvelope(original)),25);
    for(const code of ['publication_busy','selection_busy']){
        assert.equal(readRetryDelay(combinedEnvelope(singleCombinedRetry({status:'retry',code,retryable:true,action:'repeat_same_request',retry_after_ms:1}))),1);
    }
    for(const code of ['direct_query_admission','direct_query_setup']){
        const body=singleCombinedRetry({error:{code,reason:'busy',query_submitted:false,retryable:true,fresh_brokerage_required:code==='direct_query_setup',retry_after_ms:50}});
        assert.equal(readRetryDelay(combinedEnvelope(body)),50);
    }
    const mutations:Array<(body:any)=>void>=[
        body=>{delete body.read_retry;},body=>{body.selection.member_availability='report';},body=>{body.selection.identity='09'.repeat(32);},
        body=>{body.members.push(structuredClone(body.members[0]));},body=>{body.members[0].outcome='complete';},
        body=>{body.members[0].is_error=false;},body=>{body.members[0].cutoff='budget';},body=>{body.members[0].allocation.limit=0;},
        body=>{body.members[0].allocation.verification_max_sources=257;},body=>{body.publication_vector.members[0].publication={token:'actual-result'};},
        body=>{body.ranking.push({result_pointer:'/members/0/result/hits/0'});},body=>{body.partial=false;},body=>{body.truncated=true;},
        body=>{body.error.retryable=true;},body=>{body.read_retry.query_replay_safe=false;},body=>{body.read_retry.result_pointer='/members/0/result/error';},
        body=>{body.read_retry.retry_after_ms=1;},body=>{body.members[0].result.content[0].text='different';},
        body=>{const hint=body.members[0].result.structuredContent;hint.hits=[];body.members[0].result.content[0].text=JSON.stringify(hint);},
        body=>{const hint=body.members[0].result.structuredContent;hint.query_replay_safe=false;body.members[0].result.content[0].text=JSON.stringify(hint);},
    ];
    for(const mutate of mutations){const body=structuredClone(original);mutate(body);assert.equal(readRetryDelay(combinedEnvelope(body)),undefined,mutate.toString());}
    for(const hint of [
        {status:'retry',code:'unknown_busy',retryable:true,action:'repeat_same_request',retry_after_ms:25},
        {status:'retry',code:'selection_busy',retryable:true,action:'repeat_same_request',retry_after_ms:1001},
        {error:{code:'unknown',retryable:true,nested:original.members[0].result.structuredContent}},
        {error:{code:'direct_query_setup',reason:'busy',query_submitted:true,retryable:true,fresh_brokerage_required:true,retry_after_ms:25}},
    ])assert.equal(readRetryDelay(combinedEnvelope(singleCombinedRetry(hint))),undefined);
    const mismatched=combinedEnvelope(original);mismatched.structuredContent=singleCombinedRetry({status:'retry',code:'publication_busy',retryable:true,action:'repeat_same_request',retry_after_ms:25});
    assert.equal(readRetryDelay(mismatched),undefined);
});
