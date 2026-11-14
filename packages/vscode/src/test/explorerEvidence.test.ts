// SPDX-License-Identifier: Apache-2.0
import * as fs from 'node:fs';
import * as path from 'node:path';
import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import { EvidenceClient, EvidenceFailure, EXPLORER_CLIENT_NAME, NativeTool } from '../explorerEvidence';
import { McpSessionError } from '../mcpStdioSession';
import { LaunchSpec } from '../model';
import { sharedCapabilityDigest } from '../sharedCapabilityFields';
import { nativeHeaderTemplateDigest } from '../nativeMetadata';

const project = 'ab'.repeat(32);
const spec: LaunchSpec = { command: '/install/airk-launcher', root: '/source', dataDir: '/data', collection: 'demo', name: 'demo',
    service: { installDir: '/install', project } };
const text = (value: unknown) => ({ content: [{ type: 'text', text: JSON.stringify(value) }] });

type Handler = (tool: string, args: any, timeout: number) => Promise<any>;
function harness(handler: Handler, options: { maxInFlight?: number } = {}) {
    const opened: any[] = [];
    let session: any;
    const client = new EvidenceClient(spec, '1.5.0-test', { ...options, retryBudget: 2000, open: async (launch: any) => {
        opened.push(launch);
        session = { closed: false, close() { this.closed = true; },
            request: (method: string, params: any, timeout: number) => { assert.equal(method, 'tools/call'); return handler(params.name, params.arguments, timeout); } };
        return { session };
    } });
    return { client, opened, session: () => session };
}
const failure = (code: string) => (error: unknown) => error instanceof EvidenceFailure && error.error.code === code;

test('missing reviewed scope has actionable scope guidance even while the service is running',async()=>{
    // A spec naming no project and no reviewed session has nothing to read with.
    const unscoped:LaunchSpec={...spec,service:undefined};
    const client=new EvidenceClient(spec,'1.8.14',{resolveConnection:async()=>unscoped,transport:()=>{throw new Error('a transport must not open without a selection');}});
    try{await assert.rejects(client.call('airk_status',{}),error=>error instanceof EvidenceFailure&&error.error.code==='SCOPE_NOT_GRANTED'&&/scope/i.test(error.error.message));}
    finally{client.dispose();}
});

test('the session identifies as the Explorer and returns parsed payloads with publication tokens', async () => {
    const { client, opened } = harness(async tool => tool === 'airk_status'
        ? text({ index_state: 'available', publication: { token: 'airk-r1:abc', generation: 3 } })
        : text({ hits: [], publication: 'airk-r1:def' }));
    const status = await client.call('airk_status', {});
    assert.equal(status.publication, 'airk-r1:abc');
    assert.equal((await client.call('airk_search', { query: 'x' })).publication, 'airk-r1:def');
    assert.equal(opened.length, 1, 'one persistent session');
    assert.equal(opened[0].clientName, EXPLORER_CLIENT_NAME);
    assert.equal(opened[0].clientName, 'airk-explorer');
    assert.deepEqual(opened[0].args.slice(-2), [project, 'mcp']);
    assert.equal(client.project, project);
});

test('native references retain their nested publication and conflicting identities fail closed', async () => {
    const { client } = harness(async (_tool, args) => text({ evidence: { hits: [], publication: { token: 'airk-r1:references' } },
        ...(args.name === 'conflict' ? { publication: { token: 'airk-r1:other' } } : {}) }));
    try {
        assert.equal((await client.call('airk_references', { name: 'sample' })).publication, 'airk-r1:references');
        await assert.rejects(client.call('airk_references', { name: 'conflict' }), failure('EVIDENCE_MISMATCH'));
    } finally { client.dispose(); }
});

test('the evidence queue refuses overload without growing unbounded or losing accepted calls', async () => {
    let release!: () => void;
    const gate = new Promise<void>(resolve => { release = resolve; });
    const { client } = harness(async () => { await gate; return text({ ok: true }); });
    const pending = Array.from({ length: 67 }, () => client.call('airk_status', {}));
    const done = Promise.all(pending);
    await assert.rejects(client.call('airk_status', {}), failure('RATE_LIMITED'));
    release(); assert.equal((await done).length, 67); client.dispose();
});

test('only explicit native retry hints repeat the same read; tool errors are classified', async () => {
    let attempts = 0;
    const { client } = harness(async tool => {
        if (tool === 'airk_search') {
            attempts++;
            return attempts === 1 ? { isError: true, content: [{ type: 'text', text: JSON.stringify({ status: 'retry', retryable: true, action: 'repeat_same_request', code: 'publication_busy', retry_after_ms: 5 }) }] }
                : text({ hits: [1] });
        }
        if (tool === 'airk_outline') { return { isError: true, content: [{ type: 'text', text: 'workspace is not indexed' }] }; }
        return { isError: true, content: [{ type: 'text', text: JSON.stringify({ error: { code: 'invalid_argument', message: 'path must be relative' } }) }] };
    });
    assert.deepEqual((await client.call('airk_search', { query: 'x' })).payload, { hits: [1] });
    assert.equal(attempts, 2);
    await assert.rejects(client.call('airk_outline', { path: 'a' }), failure('INDEX_UNAVAILABLE'));
    await assert.rejects(client.call('airk_read_file', { path: '/abs' }), (error: unknown) => failure('invalid_argument')(error) && !/path must be relative/.test((error as Error).message));
    await assert.rejects(client.call('shell_exec' as NativeTool, {}), failure('INVALID_MESSAGE'));
});

test('at most three calls are outstanding and newer searches supersede older ones', async () => {
    let active = 0, peak = 0;
    const releases: (() => void)[] = [];
    const { client } = harness((tool, args) => new Promise(resolve => {
        active++; peak = Math.max(peak, active);
        releases.push(() => { active--; resolve(text({ tool, n: args.n })); });
    }));
    const plain = [1, 2, 3, 4].map(n => client.call('airk_file_info', { n }));
    await new Promise(resolve => setImmediate(resolve));
    assert.equal(peak, 3);
    const older = assert.rejects(client.call('airk_search', { n: 'old' }, { supersede: 'search' }), failure('REQUEST_SUPERSEDED'),
        'a queued search is dropped when a newer one arrives');
    const newer = client.call('airk_search', { n: 'new' }, { supersede: 'search' });
    await older;
    while (releases.length) { releases.shift()!(); await new Promise(resolve => setImmediate(resolve)); }
    await Promise.all(plain);
    assert.equal((await newer).payload.n, 'new');
    assert.ok(peak <= 3);

    // A superseded search that already started is discarded when it finishes late.
    const started = assert.rejects(client.call('airk_search', { n: 'started' }, { supersede: 'search' }), failure('REQUEST_SUPERSEDED'));
    await new Promise(resolve => setImmediate(resolve));
    const latest = client.call('airk_search', { n: 'latest' }, { supersede: 'search' });
    await new Promise(resolve => setImmediate(resolve));
    while (releases.length) { releases.shift()!(); await new Promise(resolve => setImmediate(resolve)); }
    await started;
    assert.equal((await latest).payload.n, 'latest');
});

test('timeouts and closed sessions map to Explorer errors, and a closed session is reopened', async () => {
    const { client, opened, session } = harness(async tool => {
        if (tool === 'airk_context') { throw new McpSessionError('timeout', 'slow'); }
        if (tool === 'airk_symbols') { session().closed = true; throw new McpSessionError('exited', 'AIRK MCP exited (1)'); }
        return text({ ok: true });
    });
    await assert.rejects(client.call('airk_context', {}), failure('REQUEST_TIMEOUT'));
    await assert.rejects(client.call('airk_symbols', {}), failure('SERVICE_UNAVAILABLE'));
    assert.deepEqual((await client.call('airk_status', {})).payload, { ok: true });
    assert.equal(opened.length, 2);
    client.dispose();
    await assert.rejects(client.call('airk_status', {}), failure('SERVICE_UNAVAILABLE'));
});

test('a project without a native service binding cannot open an evidence session', () => {
    assert.throws(() => new EvidenceClient({ ...spec, service: undefined }, 'x'), failure('NO_PROJECT'));
});


test('native refusals retain the exact original MCP envelope and transport failures invent none', async () => {
    const native = {isError:true, content:[{type:'text',text:JSON.stringify({error:{code:'scope_revoked',message:'Reviewed access changed'},scope_observation:{schema:1,fingerprint:'captured'},retry:{allowed:false}})}], structuredContent:{original:['all','fields']}};
    const {client}=harness(async()=>native);
    try {
        await assert.rejects(client.call('airk_status',{}),(error:unknown)=>{
            assert.ok(error instanceof EvidenceFailure);assert.deepEqual(error.nativeResponse,native);
            assert.notEqual(error.nativeResponse,native,'capture is detached from transport-owned values');return true;
        });
    } finally {client.dispose();}
    const offline=harness(async()=>{throw new Error('transport closed');});
    try {await assert.rejects(offline.client.call('airk_status',{}),(error:unknown)=>{
        assert.ok(error instanceof EvidenceFailure);assert.equal(error.nativeResponse,undefined);return true;
    });}finally{offline.client.dispose();}
});


test('native ingress materializes metadata once and preserves original encoded receipts for copies', async () => {
    const fields = { adapters: [{ id: 'native' }], workbench: { operations: {} } }, id = sharedCapabilityDigest(fields);
    const raw = { schema: 'airk.combined-read/1', shared_values: { schema: 'airk.shared-capability-fields/1', values: { [id]: fields } },
        members: [{ index_id: 'idx_' + '12'.repeat(16), outcome: 'complete', result: { $airk_shared_capability_fields: id, future_inline: { retained: true } } }],
        delivery_observation: { digest: 'authenticated-original' } };
    const original = structuredClone(raw);
    const { client } = harness(async () => text(raw));
    try {
        const result = await client.call('airk_capabilities', {});
        assert.deepEqual(result.rawPayload, original);
        assert.equal(result.payload.delivery_observation, undefined, 'old digest does not describe the derived projection');
        const member = (result.payload.members as any[])[0].result;
        assert.deepEqual(member.adapters, fields.adapters);
        assert.deepEqual(member.future_inline, { retained: true });
        assert.equal(member.$airk_shared_capability_fields, undefined);
        assert.deepEqual(raw, original);
    } finally { client.dispose(); }
});

test('unresolvable metadata refuses derived evidence and retains the complete original reply without retry', async () => {
    let calls = 0;
    const raw = { schema: 'airk.combined-read/1', members: [{ index_id: 'idx_' + '12'.repeat(16), outcome: 'complete', result: { $airk_shared_capability_fields: 'ab'.repeat(32), hits: [] } }] };
    const wire = text(raw), { client } = harness(async () => { calls++; return wire; });
    try {
        await assert.rejects(client.call('airk_search', {}), error => {
            assert.ok(error instanceof EvidenceFailure);
            assert.deepEqual(error.nativeResponse, wire);
            assert.deepEqual(error.nativePayload, {});
            return true;
        });
        assert.equal(calls, 1);
    } finally { client.dispose(); }
});


test('new native header template slots decode at ingress without altering raw metadata tables', async () => {
    const template = { fields: { collection: null, generation: null, truncated: null, request_id: null }, slots: [
        { path: ['collection'], kind: 'string' }, { path: ['generation'], kind: 'integer' },
        { path: ['request_id'], kind: 'null' }, { path: ['truncated'], kind: 'boolean' }] };
    const id = nativeHeaderTemplateDigest(template);
    const raw = { schema: 'airk.combined-read/1', shared_values: { schema: 'airk.shared-native-header-templates/1', values: { [id]: template } },
        members: [{ index_id: 'idx_' + '12'.repeat(16), outcome: 'complete', result: { $airk_shared_native_header_fields: { id, values: ['member', 7, null, false] },
            hits: [{ citation: { path: 'a.rs', start_line: 1, end_line: 1 }, text: 'inline evidence' }], source_verification: { retained: true } } }],
        delivery_observation: { digest: 'raw-header-only' } };
    const { client } = harness(async () => text(raw));
    try {
        const result = await client.call('airk_search', {}), body = (result.payload.members as any[])[0].result;
        assert.equal(body.collection, 'member'); assert.equal(body.generation, 7); assert.equal(body.truncated, false); assert.equal(body.request_id, null);
        assert.deepEqual(body.hits, raw.members[0].result.hits); assert.deepEqual(body.source_verification, { retained: true });
        assert.equal(result.payload.delivery_observation, undefined); assert.deepEqual(result.rawPayload, raw);
    } finally { client.dispose(); }
});


const selectionRetryCase=JSON.parse(fs.readFileSync(path.resolve(__dirname,'../../../../fixtures/wire/read_retry_envelopes.json'),'utf8')).cases
    .find((row:{name:string})=>row.name==='selection_delay_1');
assert.ok(selectionRetryCase,'shared Rust-emitted selection retry case');
for(const mode of ['same','changed','repeat'] as const){
    test(`selection retry ${mode} binds original live authority and never restarts a submitted plan`,async()=>{
        let current=structuredClone(selectionRetryCase.expected_scope),attempts=0;
        const timeouts:number[]=[],sent:unknown[]=[];
        const client=new EvidenceClient(spec,'1.8.35',{retryBudget:2000,timeout:2000,transport:()=>({
            closed:false,close(){},readScope:()=>structuredClone(current),
            async request(_method,params,timeout){
                attempts++;timeouts.push(timeout);sent.push(structuredClone(params));
                if(mode==='changed')current={...current,scope_identity:'09'.repeat(32)};
                return attempts===1||mode==='repeat'?structuredClone(selectionRetryCase.envelope):text({hits:[1]});
            },
        })});
        try{
            if(mode==='same'){
                assert.deepEqual((await client.call('airk_search',{query:'original'})).payload,{hits:[1]});
                assert.equal(attempts,2);assert.deepEqual(sent[0],sent[1],'exact original read arguments');
                assert.ok(timeouts[1]<timeouts[0],'retry keeps the first attempt deadline');
            }else{
                await assert.rejects(client.call('airk_search',{query:'original'}),error=>{
                    assert.ok(error instanceof EvidenceFailure);assert.deepEqual(error.nativeResponse,selectionRetryCase.envelope);return true;
                });
                assert.equal(attempts,mode==='changed'?1:2,'only one automatic retry, under the original active scope');
            }
        }finally{client.dispose();}
    });
}


for(const boundary of ['scope','deadline'] as const){
    test(`selection retry revalidates ${boundary} after the native requested delay`,async()=>{
        let current=structuredClone(selectionRetryCase.expected_scope),calls=0,delays=0;
        const originalTimer=global.setTimeout,originalNow=Date.now;let now=originalNow();
        const client=new EvidenceClient(spec,'1.8.35',{retryBudget:2000,timeout:2000,transport:()=>({
            closed:false,close(){},readScope:()=>structuredClone(current),
            async request(){calls++;return structuredClone(selectionRetryCase.envelope);},
        })});
        // Observe the requested native backoff, then inject the external epoch
        // change or clock advance before it completes; no scheduling race/sleep.
        Date.now=()=>now;
        global.setTimeout=((callback:(...args:any[])=>void,delay?:number,...args:any[])=>{
            if(delay===selectionRetryCase.expected.retry_after_ms){
                delays++;
                if(boundary==='scope')current={...current,scope_identity:'09'.repeat(32)};
                else now+=2000;
            }
            return originalTimer(callback,delay,...args);
        }) as typeof setTimeout;
        try{
            await assert.rejects(client.call('airk_search',{query:'original'}),error=>{
                assert.ok(error instanceof EvidenceFailure);assert.deepEqual(error.nativeResponse,selectionRetryCase.envelope);return true;
            });
            assert.equal(delays,1);assert.equal(calls,1,'changed epoch/deadline admits no further native read');
        }finally{Date.now=originalNow;global.setTimeout=originalTimer;client.dispose();}
    });
}


test('initial read deadline expires while the native connection is being prepared',async()=>{
    let release!:()=>void,entered!:()=>void,calls=0;
    const held=new Promise<void>(resolve=>{release=resolve;}),observed=new Promise<void>(resolve=>{entered=resolve;});
    const originalNow=Date.now;let now=originalNow();Date.now=()=>now;
    const client=new EvidenceClient(spec,'1.8.35',{timeout:2000,resolveConnection:async()=>{entered();await held;return spec;},
        transport:()=>({closed:false,close(){},async request(){calls++;return text({hits:[1]});}})});
    try{
        const failed=assert.rejects(client.call('airk_search',{query:'original'}),failure('REQUEST_TIMEOUT'));
        await observed;now+=2000;release();await failed;
        assert.equal(calls,0,'an expired original deadline cannot admit a one-millisecond replacement read');
    }finally{release();Date.now=originalNow;client.dispose();}
});


test('selection retry retains the original arguments when a caller edits its input object',async()=>{
    const original={query:'original'},sent:unknown[]=[];
    const client=new EvidenceClient(spec,'1.8.35',{timeout:2000,transport:()=>({closed:false,close(){},
        readScope:()=>structuredClone(selectionRetryCase.expected_scope),async request(_method,params){
            sent.push(structuredClone(params.arguments));
            if(sent.length===1){original.query='later caller edit';return structuredClone(selectionRetryCase.envelope);}
            return text({hits:[1]});
        }})});
    try{
        await client.call('airk_search',original);
        assert.deepEqual(sent,[{query:'original'},{query:'original'}],"a replay cannot submit the caller's later input");
        assert.deepEqual(original,{query:'later caller edit'},'the client does not modify caller-owned input');
    }finally{client.dispose();}
});


test('a retained combined catalog opener binds its original known scope before retry',async()=>{
    const bound=JSON.parse(fs.readFileSync(path.resolve(__dirname,'../../../../fixtures/wire/read_retry_decisions.json'),'utf8')).binding_cases
        .find((row:{name:string})=>row.name==='catalog_combined_active');
    assert.ok(bound);let calls=0;
    const client=new EvidenceClient({...spec,combinedSession:bound.expected_scope},'1.8.35',{timeout:2000,open:async()=>({session:{
        closed:false,close(){},async request(){
            calls++;
            return calls===1?{isError:true,content:[{type:'text',text:JSON.stringify(bound.input)}],structuredContent:bound.input}:text({hits:[1]});
        },
    } as never})});
    try{
        assert.deepEqual((await client.call('airk_search',bound.arguments)).payload,{hits:[1]});
        assert.equal(calls,2,'the known original combined catalog source participates in caller binding');
    }finally{client.dispose();}
});
