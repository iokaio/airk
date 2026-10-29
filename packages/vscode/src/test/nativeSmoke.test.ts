// SPDX-License-Identifier: Apache-2.0
import {strict as assert} from 'node:assert';
import {test} from 'node:test';
import {probeMcp} from '../native';
import {record} from '../serviceClient';
import {NativeServiceFixture} from './nativeServiceFixture';
import * as fs from 'node:fs/promises';
import * as path from 'node:path';
import {McpStdioSession} from '../mcpStdioSession';
import {assertQuickStatusTimeout,observeQuickStatus} from './nativeReadRetry';

async function quickStatus(session:McpStdioSession):Promise<any>{
    return observeQuickStatus(async deadline=>{
        const result=await session.request('tools/call',{name:'status',arguments:{}},Math.max(1,deadline-performance.now()));
        assert.notEqual(result.isError,true);return JSON.parse(result.content[0].text);
    });
}

test('one root-scoped MCP session survives late registration and rejects ambiguous indexes',{skip:!process.env.AIRK_TEST_BINARY,timeout:240000},async()=>{
    const fixture=await NativeServiceFixture.create(process.env.AIRK_TEST_BINARY!);let session:McpStdioSession|undefined;
    try{
        const root=path.join(fixture.owner,'late-source');await fs.mkdir(root);
        session=(await McpStdioSession.open({command:fixture.launcher,args:['runtime','launch','--install-dir',fixture.install,'--','--root',root,'mcp'],cwd:fixture.owner,clientName:'root-recovery-test',clientVersion:'1.6.1'},60000)).session;
        assert.equal((await session.request('tools/list',{},10000)).tools.length,27);
        const body=(result:any)=>JSON.parse(result.content[0].text);
        // Repeated quick reads must complete cold verification without a diagnostics warm-up.
        let status=await quickStatus(session);
        assert.equal(status.registration_state,'missing');
        const scopes=body(await session.request('tools/call',{name:'projects',arguments:{}},10000));
        const spec=await fixture.register('new-index',root);
        status=await quickStatus(session);
        assert.equal(status.project,scopes.default_project);assert.equal(status.registration_state,undefined);
        assert.equal(status.index_state,'unavailable','registration alone does not imply a publication');
        await fixture.register('another-index',root);
        status=await quickStatus(session);
        assert.equal(status.registration_state,'ambiguous');
        const search=await session.request('tools/call',{name:'search',arguments:{query:'fixture'}},10000);
        assert.equal(search.isError,true);assert.equal(body(search).error.code,'PROJECT_AMBIGUOUS');
        assert.equal((await fixture.embedding()).loads,0,'metadata and rejected reads do not start inference');
        assert.ok(spec.service!.project);
    }finally{session?.close();await fixture.dispose();}
});

test('registered native MCP verifies before the first publication without starting inference', {skip:!process.env.AIRK_TEST_BINARY,timeout:240000}, async()=>{
    const fixture=await NativeServiceFixture.create(process.env.AIRK_TEST_BINARY!);
    try{
        const spec=await fixture.register('unindexed');
        const result=await probeMcp(spec);assert.equal(result.tools.length,27);
        const quick=JSON.parse((record(result.status).content as {text:string}[])[0].text);
        if(quick.index_state==='unknown'){assertQuickStatusTimeout(quick);assert.equal(record(result.status).state,'index_unknown');}
        else {assert.equal(quick.index_state,'unavailable');assert.equal(quick.files,null);assert.equal(record(result.status).state,'index_unavailable');}
        const status=await fixture.tool(spec,'airk_diagnostics');
        assert.equal(status.index_state,'unavailable');assert.equal(status.files,null);
        const embedding=await fixture.embedding();
        assert.equal(embedding.loads,0);assert.equal(embedding.sessions,0);
    }finally{await fixture.dispose();}
});

test('pinned MCP quick status completes cold verification without diagnostics or inference',{skip:!process.env.AIRK_TEST_BINARY,timeout:240000},async()=>{
    const fixture=await NativeServiceFixture.create(process.env.AIRK_TEST_BINARY!);let session:McpStdioSession|undefined;
    try{
        const spec=await fixture.register('pinned-status');const {launchArgs}=await import('../model');
        session=(await McpStdioSession.open({command:spec.command,args:launchArgs(spec,'mcp',true),cwd:fixture.owner,clientName:'pinned-status-test',clientVersion:'1.6.4'},60000)).session;
        const status=await quickStatus(session);assert.equal(status.project,spec.service!.project);
        assert.equal(status.index_state,'unavailable');assert.equal(status.files,null);assert.equal(status.publication,null);
        const embedding=await fixture.embedding();assert.equal(embedding.loads,0);assert.equal(embedding.sessions,0);
    }finally{session?.close();await fixture.dispose();}
});

test('basic status includes version and health, caches snapshots and diagnostics forces a new observation',{skip:!process.env.AIRK_TEST_BINARY,timeout:240000},async()=>{
    const fixture=await NativeServiceFixture.create(process.env.AIRK_TEST_BINARY!);let session:McpStdioSession|undefined;
    try{
        const spec=await fixture.register('cached-status');await fs.writeFile(path.join(spec.root,'memo.txt'),'Original status cache fixture.');
        await fixture.configure(spec,{operation:'vectors',enabled:false});await fixture.job(spec);
        const {launchArgs}=await import('../model');
        session=(await McpStdioSession.open({command:spec.command,args:launchArgs(spec),cwd:fixture.owner,clientName:'status-cache-test',clientVersion:'1.6.1'},60000)).session;
        const read=async(name:string,args:Record<string,unknown>={})=>{
            const result=await session!.request('tools/call',{name,arguments:args},name==='diagnostics'?60000:10000);assert.notEqual(result.isError,true);return JSON.parse(result.content[0].text);
        };
        const expectedVersion=JSON.parse(await fs.readFile(path.resolve(__dirname,'../../package.json'),'utf8')).version;
        const first=await quickStatus(session);assert.equal(first.version,expectedVersion);assert.ok(['ok','updating','paused','unknown','needs_attention'].includes(first.health));
        assert.equal(first.files,1);assert.equal(first.cached,false);assert.ok(first.observed_unix_ms>0);
        const cached=await read('status',{response_mode:'full'});assert.equal(cached.cached,true);assert.ok(cached.cache_age_ms<5000);
        assert.equal(cached.observed_unix_ms,first.observed_unix_ms);assert.equal(cached.publication.token,first.publication.token);
        for(const value of [first,cached]){assert.equal(value.runtime,undefined);assert.equal(value.work,undefined);assert.equal(value.explorer,undefined);assert.ok(Buffer.byteLength(JSON.stringify(value))<1024);}
        const detail=await read('diagnostics');assert.ok(detail.runtime);assert.ok(detail.work);
        const after=await read('status');assert.equal(after.cached,false);assert.ok(after.observed_unix_ms>=first.observed_unix_ms);
    }finally{session?.close();await fixture.dispose();}
});
