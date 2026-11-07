// SPDX-License-Identifier: Apache-2.0
import {strict as assert} from 'node:assert';
import {test} from 'node:test';
import {probeMcp} from '../native';
import {record} from '../serviceClient';
import {NativeServiceFixture} from './nativeServiceFixture';
import {LaunchSpec,UNIFIED_TOOLS} from '../model';
import * as fs from 'node:fs/promises';
import * as path from 'node:path';
import {McpStdioSession} from '../mcpStdioSession';
import {assertQuickStatusTimeout,observeQuickStatus,requestWithReadRetry,readRetryReceipt,ReadRetryReceipt,QuickStatusObservation,QuickStatusObserver} from './nativeReadRetry';

async function quickStatus(session:McpStdioSession,observer?:QuickStatusObserver,expectedScope?:LaunchSpec['catalogSession']):Promise<any>{
    return observeQuickStatus(async deadline=>{
        // native read: observeQuickStatus applies the typed retry contract to this reply.
        const result=await session.request('tools/call',{name:'status',arguments:{}},Math.max(1,deadline-performance.now()));
        return result.isError===true?result:JSON.parse(result.content[0].text);
    },undefined,observer,expectedScope);
}

function assertSelectedScope(status:any,spec:LaunchSpec):void{
    const selected=spec.catalogSession!;
    assert.equal(status.project,undefined,'scoped status carries its complete native scope');
    assert.equal(status.scope_observation.qualifier,'selected_subset');
    assert.equal(status.scope_observation.provenance,'user_reviewed_project_grant');
    assert.deepEqual(status.scope_observation.grant,{id:selected.grant_id,revision:selected.expected_grant_revision});
    assert.deepEqual(status.scope_observation.members.map((member:any)=>member.index_id),[selected.index_id]);
    assert.deepEqual(status.scope_observation.members.flatMap((member:any)=>member.memberships.map((item:any)=>item.project_id)).sort(),selected.project_ids);
}
function assertEmptySetupStatus(status:any):void{
    // Reviewed setup queues explicit indexing; its empty publication may already
    // have completed before this metadata read. Neither result proves inference.
    assert.ok(['available','unavailable'].includes(status.index_state),JSON.stringify(status));
    if(status.index_state==='available'){assert.equal(status.files,0);assert.ok(status.publication);}
    else{assert.equal(status.files,null);assert.equal(status.publication,null);}
}

test('one root-scoped MCP session survives late registration and rejects ambiguous indexes',{skip:!process.env.AIRK_TEST_BINARY,timeout:240000},async()=>{
    const fixture=await NativeServiceFixture.create(process.env.AIRK_TEST_BINARY!);let session:McpStdioSession|undefined;
    try{
        const root=path.join(fixture.owner,'late-source');await fs.mkdir(root);
        session=(await McpStdioSession.open({command:fixture.launcher,args:['runtime','launch','--install-dir',fixture.install,'--','--root',root,'mcp'],cwd:fixture.owner,clientName:'root-recovery-test',clientVersion:'1.6.1'},60000)).session;
        assert.equal((await session.request('tools/list',{},10000)).tools.length,UNIFIED_TOOLS.length);
        const body=(result:any)=>JSON.parse(result.content[0].text);
        // Repeated quick reads must complete cold verification without a diagnostics warm-up.
        let status=await quickStatus(session);
        assert.equal(status.registration_state,'missing');
        // native read: projects is answered by the frontend scope table without a native query.
        const scopes=body(await session.request('tools/call',{name:'projects',arguments:{}},10000));
        const spec=await fixture.register('new-index',root);
        status=await quickStatus(session);
        assert.equal(status.project,scopes.default_project);assert.equal(status.registration_state,undefined);
        assert.equal(status.index_state,'unavailable','registration alone does not imply a publication');
        await fixture.register('another-index',root);
        status=await quickStatus(session);
        assert.equal(status.registration_state,'ambiguous');
        // native read: a deliberate negative test must receive exactly PROJECT_AMBIGUOUS once.
        const search=await session.request('tools/call',{name:'search',arguments:{query:'fixture'}},10000);
        assert.equal(search.isError,true);assert.equal(body(search).error.code,'PROJECT_AMBIGUOUS');
        assert.equal((await fixture.embedding()).loads,0,'metadata and rejected reads do not start inference');
        assert.ok(spec.service!.project);
    }catch(error){await fixture.retainFailure(error,session?[session]:[]);throw error;}finally{session?.close();await fixture.dispose();}
});

test('reviewed native setup verifies without starting inference', {skip:!process.env.AIRK_TEST_BINARY,timeout:240000}, async()=>{
    const fixture=await NativeServiceFixture.create(process.env.AIRK_TEST_BINARY!);
    try{
        const spec=await fixture.register('unindexed');
        const result=await probeMcp(spec);assert.equal(result.tools.length,UNIFIED_TOOLS.length);
        const quick=JSON.parse((record(result.status).content as {text:string}[])[0].text);
        if(quick.index_state==='unknown'){assertQuickStatusTimeout(quick);assert.equal(record(result.status).state,'index_unknown');}
        else {assertEmptySetupStatus(quick);assertSelectedScope(quick,spec);assert.equal(record(result.status).state,quick.index_state==='available'?undefined:'index_unavailable');}
        const status=await fixture.tool(spec,'airk_diagnostics');
        assertEmptySetupStatus(status);assertSelectedScope(status,spec);
        const embedding=await fixture.embedding();
        assert.equal(embedding.loads,0);assert.equal(embedding.sessions,0);
    }catch(error){await fixture.retainFailure(error);throw error;}finally{await fixture.dispose();}
});

test('pinned MCP quick status completes cold verification without diagnostics or inference',{skip:!process.env.AIRK_TEST_BINARY,timeout:240000},async t=>{
    const fixture=await NativeServiceFixture.create(process.env.AIRK_TEST_BINARY!);let session:McpStdioSession|undefined;
    const attempts:QuickStatusObservation[]=[];let confirmed:QuickStatusObservation|undefined;let dropped=0;
    const observe:QuickStatusObserver=value=>{
        if(value.kind==='confirmed')confirmed=value;
        else if(attempts.length<64)attempts.push(value);
        else dropped++;
    };
    try{
        const spec=await fixture.register('pinned-status');const {launchArgs}=await import('../model');
        session=(await McpStdioSession.open({command:spec.command,args:launchArgs(spec,'mcp',true),cwd:fixture.owner,clientName:'pinned-status-test',clientVersion:'1.6.4'},60000)).session;
        const status=await quickStatus(session,observe,spec.catalogSession);assertSelectedScope(status,spec);
        assertEmptySetupStatus(status);
        const embedding=await fixture.embedding();assert.equal(embedding.loads,0);assert.equal(embedding.sessions,0);
    }catch(error){await fixture.retainFailure(error,session?[session]:[]);throw error;}finally{
        t.diagnostic(JSON.stringify({quickStatus:{attempts,confirmed:confirmed??null,dropped,nativeStageCounts:'unmeasured',hashCounts:'unmeasured'}}));
        session?.close();await fixture.dispose();
    }
});

test('scoped status revalidates authority and keeps complete provenance across aliases and detail modes',{skip:!process.env.AIRK_TEST_BINARY,timeout:240000},async()=>{
    const fixture=await NativeServiceFixture.create(process.env.AIRK_TEST_BINARY!);let session:McpStdioSession|undefined;
    try{
        const spec=await fixture.register('cached-status');await fs.writeFile(path.join(spec.root,'memo.txt'),'Original status cache fixture.');
        await fixture.configure(spec,{operation:'vectors',enabled:false});await fixture.job(spec);
        const {launchArgs}=await import('../model');
        session=(await McpStdioSession.open({command:spec.command,args:launchArgs(spec),cwd:fixture.owner,clientName:'status-cache-test',clientVersion:'1.6.1'},60000)).session;
        const read=async(name:string,args:Record<string,unknown>={})=>{
            const result=await requestWithReadRetry(session!,name,args,name.endsWith('diagnostics')?60000:15000,undefined,spec.catalogSession);
            const value=JSON.parse(result.content[0].text);assert.deepEqual(result.structuredContent,value,JSON.stringify(result));return value;
        };
        const expectedVersion=JSON.parse(await fs.readFile(path.resolve(__dirname,'../../package.json'),'utf8')).version;
        const first=await quickStatus(session,undefined,spec.catalogSession);assert.equal(first.version,expectedVersion);assert.ok(['ok','updating','paused','unknown','needs_attention'].includes(first.health));
        assert.equal(first.files,1);assert.equal(first.cached,false);assert.ok(first.observed_unix_ms>0);
        const primedTerse=await read('status',{response_mode:'terse'});
        assert.equal(primedTerse.generation,undefined);assert.equal(primedTerse.publication.token,first.publication.token);
        const full=await read('status',{response_mode:'full'});
        assert.equal(full.cached,false,'full rendering requires a distinct observation');assert.ok(Number.isInteger(full.generation));
        assert.ok(full.observed_unix_ms>=first.observed_unix_ms);assert.equal(full.publication.token,first.publication.token);
        const cached=await read('status',{response_mode:'full'});assert.equal(cached.cached,false);assert.equal(cached.cache_age_ms,0);
        assert.ok(cached.observed_unix_ms>=full.observed_unix_ms);assert.equal(cached.generation,full.generation);
        const legacy=await read('airk_status',{response_mode:'full'});
        assert.equal(legacy.cached,false);assert.ok(legacy.observed_unix_ms>=full.observed_unix_ms);
        assert.equal(legacy.publication.token,first.publication.token);assert.equal(legacy.project,first.project);
        const terse=await read('status',{response_mode:'terse'});assert.equal(terse.cached,false);assert.equal(terse.generation,undefined,'full cache must not leak generation into terse');
        const included=await read('status',{response_mode:'terse',include_status:true});assert.equal(included.cached,false);assert.equal(included.generation,undefined);assert.ok(included.freshness);
        const includedHit=await read('status',{response_mode:'terse',include_status:true});assert.equal(includedHit.cached,false);assert.ok(includedHit.observed_unix_ms>=included.observed_unix_ms);
        const beforeDetail=await read('status',{response_mode:'terse'});assert.equal(beforeDetail.cached,false);assert.equal((await read('status',{response_mode:'terse'})).cached,false);
        for(const value of [first,primedTerse,full,cached,legacy,terse,included,includedHit,beforeDetail]){assert.equal(value.query_concurrency,16,JSON.stringify(value));assert.equal(value.runtime,undefined);assert.equal(value.work,undefined);assert.equal(value.explorer,undefined);assertSelectedScope(value,spec);const {scope_observation,...scalar}=value;assert.ok(Buffer.byteLength(JSON.stringify(scalar))<1024,'scalar status retains its original byte bound; complete scope is additional');}
        for(const name of ['diagnostics','airk_diagnostics']){const detail=await read(name);assert.equal(detail.query_concurrency,16,JSON.stringify(detail));assert.ok(detail.runtime);assert.ok(detail.work);}
        const after=await read('status',{response_mode:'terse'});assert.equal(after.cached,false);assert.ok(after.observed_unix_ms>=first.observed_unix_ms);
    }catch(error){await fixture.retainFailure(error,session?[session]:[]);throw error;}finally{session?.close();await fixture.dispose();}
});

for(const binding of ['pinned','source'] as const){
    test(`four cold ${binding} clients observe first status without diagnostic warmup`,{skip:!process.env.AIRK_TEST_BINARY,timeout:240000},async()=>{
        const fixture=await NativeServiceFixture.create(process.env.AIRK_TEST_BINARY!);
        const sessions:McpStdioSession[]=[];
        try{
            const spec=await fixture.register(`four-${binding}-status`);
            const {launchArgs}=await import('../model');
            const args=binding==='pinned'?launchArgs(spec,'mcp',true):
                ['runtime','launch','--install-dir',fixture.install,'--','--root',spec.root,'mcp'];
            const opened=await Promise.allSettled(Array.from({length:4},async(_,client)=>{
                const opened=await McpStdioSession.open({command:fixture.launcher,args,cwd:fixture.owner,
                    clientName:`cold-${binding}-${client}`,clientVersion:'1.8.0'},60000);
                sessions.push(opened.session);
            }));
            for(const result of opened)if(result.status==='rejected')throw result.reason;
            const expected=await Promise.all(sessions.map(async session=>{
                // native read: one static configured-scope declaration before this session's first native status.
                const result=await session.request('tools/call',{name:'projects',arguments:{}},10000);
                return JSON.parse(result.content[0].text).default_project;
            }));
            const observations=await Promise.all(sessions.map(session=>quickStatus(session,undefined,binding==='pinned'?spec.catalogSession:undefined)));
            for(const [index,observation] of observations.entries()){
                if(binding==='pinned'){assertSelectedScope(observation,spec);assertEmptySetupStatus(observation);}
                else{assert.equal(observation.project,expected[index]);assert.equal(observation.index_state,'unavailable');assert.equal(observation.files,null);assert.equal(observation.publication,null);}
            }
            assert.equal((await fixture.embedding()).loads,0,'first status does not start inference');
        }catch(error){await fixture.retainFailure(error,sessions);throw error;}finally{for(const session of sessions)session.close();await fixture.dispose();}
    });
}

test('mixed four-call batches honor status snapshots and typed read admission',{skip:!process.env.AIRK_TEST_BINARY,timeout:240000},async()=>{
    const fixture=await NativeServiceFixture.create(process.env.AIRK_TEST_BINARY!);
    const sessions:McpStdioSession[]=[];
    const refusals:Array<{binding:string;round:number;tool:string}&ReadRetryReceipt>=[];
    try{
        const spec=await fixture.register('mixed-status');
        await fs.writeFile(path.join(spec.root,'session.rs'),'pub struct Session;\nimpl Session { pub fn new() -> Self { Session } }\nfn connect() -> Session { Session::new() }\n');
        await fs.writeFile(path.join(spec.root,'session.md'),'# Session\nSession is a fictional local handle.\n');
        await fixture.configure(spec,{operation:'vectors',enabled:false});await fixture.job(spec);
        const {launchArgs}=await import('../model');
        const expectedVersion=JSON.parse(await fs.readFile(path.resolve(__dirname,'../../package.json'),'utf8')).version;
        for(const binding of ['pinned','source'] as const){
            const args=binding==='pinned'?launchArgs(spec,'mcp',true):
                ['runtime','launch','--install-dir',fixture.install,'--','--root',spec.root,'mcp'];
            const session=(await McpStdioSession.open({command:fixture.launcher,args,cwd:fixture.owner,
                clientName:`mixed-${binding}-status`,clientVersion:expectedVersion},60000)).session;
            sessions.push(session);
            const first=await quickStatus(session,undefined,spec.catalogSession);assert.equal(first.version,expectedVersion);
            if(binding==='source'){
                assert.equal(first.index_state,'unavailable');assert.equal(first.publication,null);
                // native read: observe absent imported compatibility authority exactly once; no retry.
                const refused=await session.request('tools/call',{name:'read_file',arguments:{path:'session.rs',start_line:1,end_line:3}},30000);
                assert.equal(refused.isError,true,JSON.stringify(refused));const detail=JSON.parse(refused.content[0].text);
                assert.equal(detail.error.code,'scoped_read_unavailable',JSON.stringify(refused));assert.equal(detail.error.retryable,false,JSON.stringify(refused));
                continue;
            }
            assert.equal(first.index_state,'available');assertSelectedScope(first,spec);
            // Establish an evidence channel. Source-selector observations can
            // still be superseded before submission, even on a warm channel.
            // Measured reads below retain those refusals and use only the shared
            // safe-retry contract; submitted status errors remain fatal.
            const warmup=await requestWithReadRetry(session,'read_file',{path:'session.rs',start_line:1,end_line:3},30000,undefined,spec.catalogSession);
            assert.notEqual(warmup.isError,true,JSON.stringify(warmup));
            assert.equal(JSON.parse(warmup.content[0].text).publication.token,first.publication.token,JSON.stringify(warmup));
            for(let round=0;round<3;round++){
                const workloads:Array<Array<[string,Record<string,unknown>]>>=[
                    [['status',{response_mode:'full'}],...['Session','local','handle'].map(query=>['search',{query,mode:'lexical',path_prefix:'session.md',limit:3}] as [string,Record<string,unknown>])],
                    [['status',{response_mode:'full'}],['symbols',{name:'Session',path_prefix:'session.rs'}],
                        ['references',{name:'Session',path_prefix:'session.rs',order:'path',include_files:true,include_count:true}],
                        ['read_file',{path:'session.rs',start_line:1,end_line:3}]],
                ];
                for(const workload of workloads){
                    const replies=await Promise.allSettled(workload.map(([name,arguments_])=>requestWithReadRetry(session,name,arguments_,30000,reply=>{
                        if(reply.isError!==true)return;
                        refusals.push({binding,round,tool:name,...readRetryReceipt(reply,spec.catalogSession)});
                    },spec.catalogSession)));
                    for(const [index,reply]of replies.entries()){
                        assert.equal(reply.status,'fulfilled',`${binding} round ${round} ${workload[index][0]}: ${reply.status==='rejected'?String(reply.reason):''}`);
                        assert.notEqual(reply.value.isError,true,`${binding} round ${round} ${workload[index][0]}: ${JSON.stringify(reply.value)}`);
                        const value=JSON.parse(reply.value.content[0].text),evidence=record(value.evidence??value);
                        assert.ok(evidence.publication&&typeof evidence.publication==='object'&&!Array.isArray(evidence.publication),`${binding} round ${round} ${workload[index][0]}: ${JSON.stringify(reply.value)}`);
                        assert.equal(record(evidence.publication).token,first.publication.token,JSON.stringify(reply.value));
                        if(index===0){
                            assert.equal(value.version,expectedVersion);assert.equal(value.index_state,'available',JSON.stringify(value));
                            assert.equal(value.query_concurrency,16,JSON.stringify(value));
                            assert.ok(value.observed_unix_ms>=first.observed_unix_ms,JSON.stringify(value));
                            if(value.status_observation!==undefined){
                                assert.equal(value.status_observation.superseded,true,JSON.stringify(value));
                                assert.equal(value.status_observation.reason,'scalar_invalidation',JSON.stringify(value));
                                assert.equal(value.cached,false,JSON.stringify(value));
                            }
                        }
                    }
                }
            }
        }
        process.stderr.write(`[native fixture] mixed batches: ${JSON.stringify({scoped_batches:6,scoped_logical_reads:24,unselected_source_refusals:1,unsubmitted_refusals:refusals})}\n`);
    }catch(error){await fixture.retainFailure(error,sessions);throw error;}finally{for(const session of sessions)session.close();await fixture.dispose();}
});
