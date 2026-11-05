// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import { parseWebviewMessage } from '../explorerContracts';
import { EvidenceClient, EvidenceFailure, ToolResult } from '../explorerEvidence';
import { FactHandles } from '../explorerFacts';
import { RefRegistry } from '../explorerSearch';
import { CORE_PACK, Strategy } from '../explorerStrategies';
import { runStrategy } from '../explorerStrategyRunner';
import {readFileSync} from 'node:fs';
import {resolve} from 'node:path';

const PROJECT = 'ab'.repeat(32);
const HASH = (path: string) => (path.length.toString(16).padStart(2, '0')).repeat(32);
const cite = (path: string, start: number, end = start) => ({ path, start_line: start, end_line: end, content_hash: HASH(path) });
const template = (id: string) => CORE_PACK.find(entry => entry.strategy.id === `airk.core/${id}`)!.strategy;
const nativeReceiptFixture=JSON.parse(readFileSync(resolve(__dirname,'../../../../docs/research/contracts-v190/fixtures.json'),'utf8')).receipt_cases.find((entry:any)=>entry.name==='single_index_match_and_supplement').value;
function receipt(path:string,publication:string,state='matched'){
    const value=structuredClone(nativeReceiptFixture),source=value.sources[0];
    value.origins[0].publication=publication;source.path=path;source.original_root='/workspace';source.source_relative_path=path;source.expected_hash=HASH(path);source.observed_hash=state==='matched'?HASH(path):'b'.repeat(64);source.state=state;
    value.counts.bytes_read=1;source.bytes_read=1;
    if(state!=='matched'){value.status='partial';value.requirements_satisfied=false;value.counts.matched_sources=0;value.counts.mismatched_sources=state==='mismatch'?1:0;value.counts.unchecked_sources=state==='missing'?1:0;}
    return value;
}

/** A scripted evidence session; `publication` may change per call to simulate a republish. */
function fake(options: { publication?: (tool: string, call: number) => string;state?:(path:string)=>string } = {}) {
    const calls: { tool: string; args: Record<string, unknown> }[] = [];
    const payloads: Record<string, (args: Record<string, unknown>) => Record<string, unknown>> = {
        airk_status: () => ({ index_state: 'available', version: '1.5.0' }),
        airk_search: () => ({ hits: [{ citation: cite('src/session.ts', 40, 42), text: 'export function renew(token) {}', truncated: false },
            { citation: cite('src/auth/login.ts', 7), text: 'renew(currentToken)', truncated: false }] }),
        airk_symbols: () => ({ items: [{ kind: 'function', name: 'refresh', citation: cite('src/session.ts', 12, 14), symbol_id: 'syntax:refresh', basis: 'syntax' }] }),
        airk_references: () => ({ evidence: { hits: [{ citation: cite('src/auth/login.ts', 7), text: 'renew(currentToken)' }] } }),
        airk_read_file: args => ({ hits: [{ citation: cite(String(args.path), Number(args.start_line), Number(args.end_line)), text: `lines of ${args.path}`, truncated: false }] }),
        airk_file_info: args => ({content_hash:HASH(String(args.path)),indexed_lines:100,source: { root: '/workspace', relative_path: String(args.path) } }),
    };
    const session = {
        call: async (tool: string, args: Record<string, unknown>): Promise<ToolResult> => {
            calls.push({ tool, args });
            const payload = payloads[tool];
            if (!payload) { throw new EvidenceFailure({ code: 'TOOL_ERROR', message: `unexpected ${tool}` }); }
            const publication=options.publication?.(tool,calls.length)??'airk-r1:p:1',value=payload(args);
            if(tool==='airk_file_info'&&args.verification==='report')value.source_verification=receipt(String(args.path),publication,options.state?.(String(args.path))??'matched');
            return {payload:value,publication};
        },
    } as unknown as EvidenceClient;
    return { session, calls };
}
function deps(session: EvidenceClient) {
    return { session, facts: new FactHandles(new RefRegistry()), project: PROJECT, extensionVersion: '1.5.0',dirty:async()=>false,now: () => 1000 };
}

test('a runnable template pins one publication and is a verified snapshot only when every cited file matches', async () => {
    const { session, calls } = fake();
    const result = await runStrategy({ strategy: template('bug'), params: { query: 'renew session', symbol: 'refresh' }, scope: { pathPrefix: 'src/' } }, deps(session));
    assert.equal(result.manifest.outcome, 'verified_snapshot', result.manifest.reasons.join('; '));
    assert.equal(result.manifest.publication, 'airk-r1:p:1');
    assert.equal(result.manifest.serviceVersion, '1.5.0');
    assert.equal(result.manifest.template, 'airk.core/bug@1.0.0');
    assert.match(result.manifest.evidenceDigest, /^[0-9a-f]{64}$/);
    assert.deepEqual(result.records.map(record => `${record.path}:${record.startLine}`), [...result.records].map(record => `${record.path}:${record.startLine}`).sort(), 'records are in total order');
    const search = calls.find(call => call.tool === 'airk_search')!;
    assert.deepEqual({ mode: search.args.mode, path_prefix: search.args.path_prefix, query: search.args.query }, { mode: 'lexical', path_prefix: 'src/', query: 'renew session' });
    assert.ok(calls.every(call => call.tool !== 'airk_help'), 'help is never evidence');
    assert.deepEqual(result.manifest.steps.map(step => step.outcome), result.manifest.steps.map(() => 'ok'));

    const again = await runStrategy({ strategy: template('bug'), params: { query: 'renew session', symbol: 'refresh' }, scope: { pathPrefix: 'src/' } }, deps(fake().session));
    assert.equal(again.manifest.evidenceDigest, result.manifest.evidenceDigest, 'the same evidence gives the same digest');
});

test('a republish during the run fails closed: later steps are skipped and the run is partial', async () => {
    const { session } = fake({ publication: (tool, call) => call >= 3 ? 'airk-r1:p:2' : 'airk-r1:p:1' });
    const result = await runStrategy({ strategy: template('bug'), params: { query: 'renew', symbol: 'refresh' }, scope: {} }, deps(session));
    assert.equal(result.manifest.outcome, 'partial');
    assert.ok(result.manifest.reasons.some(reason => /republished/.test(reason)));
    assert.ok(result.manifest.steps.some(step => step.outcome === 'skipped'));
    assert.equal(result.manifest.publication, 'airk-r1:p:1');
});

test('live mismatches, budgets and unchecked files make the run partial with reasons', async () => {
    const mismatch = await runStrategy({ strategy: template('bug'), params: { query: 'renew', symbol: 'refresh' }, scope: {} },
        deps(fake({state:path=>path==='src/session.ts'?'mismatch':'matched'}).session));
    assert.equal(mismatch.manifest.outcome, 'partial');
    assert.ok(mismatch.manifest.reasons.some(reason => reason === 'live check: src/session.ts is changed since index'));

    const tight: Strategy = { ...template('bug'), budgets: { ...template('bug').budgets, calls: 3 } };
    const budget = await runStrategy({ strategy: tight, params: { query: 'renew', symbol: 'refresh' }, scope: {} }, deps(fake().session));
    assert.ok(budget.manifest.reasons.some(reason => /call budget of 3/.test(reason)));

    const noLive: Strategy = { ...template('bug'), steps: template('bug').steps.filter(step => step.op !== 'host.hashSavedFiles') };
    const unchecked = await runStrategy({ strategy: noLive, params: { query: 'renew', symbol: 'refresh' }, scope: {} }, deps(fake().session));
    assert.ok(unchecked.manifest.reasons.some(reason => /live files not checked/.test(reason)));
});

test('Preview templates, missing parameters and unsafe scopes are refused before any call', async () => {
    const { session, calls } = fake();
    await assert.rejects(runStrategy({ strategy: template('review'), params: {}, scope: {} }, deps(session)), /Git changes/);
    await assert.rejects(runStrategy({ strategy: template('bug'), params: { query: 'renew' }, scope: {} }, deps(session)), /Symbol or identifier is required/);
    await assert.rejects(runStrategy({ strategy: template('bug'), params: { query: 'a', symbol: 'b', extra: 'c' }, scope: {} }, deps(session)), /Unknown parameter/);
    await assert.rejects(runStrategy({ strategy: template('bug'), params: { query: 'a', symbol: 'b' }, scope: { pathPrefix: '../outside' } }, deps(session)), /relative to the project/);
    assert.equal(calls.length, 0);
});

test('JSON message fields accept bounded plain objects only', () => {
    const base = { v: 1, type: 'strategy.run', requestId: 'r1', params: {}, scope: {} };
    assert.ok(parseWebviewMessage({ ...base, strategy: { a: 1 } }));
    assert.equal(parseWebviewMessage({ ...base, strategy: [1] }), undefined);
    assert.equal(parseWebviewMessage({ ...base, strategy: 'text' }), undefined);
    assert.equal(parseWebviewMessage({ ...base, strategy: { big: 'x'.repeat(40000) } }), undefined);
    assert.equal(parseWebviewMessage({ ...base, strategy: Object.create({ inherited: true }) }), undefined);
});

test('a failing fan-out settles started siblings and stops admitting new items before finalizing', async () => {
    let release!: () => void, siblingStarted = false, returned = false;
    const sibling = new Promise<void>(resolve => { release = resolve; });
    const paths: string[] = [];
    const base = template('bug');
    const strategy: Strategy = { ...base, params: [], budgets: { ...base.budgets, concurrency: 2 }, steps: [
        { id: 'status', op: 'airk.status', args: {} }, { id: 'search', op: 'airk.search', args: { query: 'x' } },
        { id: 'read', op: 'airk.readFile', args: {}, input: 'search' }, { id: 'pack', op: 'core.renderPrompt', args: {} },
    ] };
    const session = { call: async (tool: string, args: any) => {
        if (tool === 'airk_status') { return { publication: 'p', payload: {} }; }
        if (tool === 'airk_search') { return { publication: 'p', payload: { hits: ['a', 'b', 'c'].map(path => ({ citation: cite(path, 1), text: path })) } }; }
        if (tool === 'airk_file_info') { return { publication: 'p', payload: { indexed_lines: 1, source: { root: '/r', relative_path: args.path } } }; }
        paths.push(args.path);
        if (args.path === 'a') { throw new Error('failed first read'); }
        siblingStarted = true; await sibling;
        return { publication: 'p', payload: { hits: [{ citation: cite(args.path, 1), text: 'settled' }] } };
    } } as any;
    const pending = runStrategy({ strategy, params: {}, scope: {} }, deps(session)).then(value => { returned = true; return value; });
    await new Promise(resolve => setTimeout(resolve, 10));
    assert.equal(siblingStarted, true); assert.equal(returned, false);
    release(); const result = await pending;
    assert.deepEqual(paths, ['a', 'b']);
    assert.equal(result.manifest.outcome, 'partial');
    assert.ok(result.records.some(record => record.text === 'settled'));
});

test('explicit saved-source checks bypass cached locations and never fall back to host hashing after native refusal',async()=>{
    for(const code of ['source_policy_unavailable','scoped_read_unavailable']){
        const base=fake(),native=base.session.call.bind(base.session);let reports=0,hostCaptures=0;
        const refused=new EvidenceFailure({code:'TOOL_ERROR',message:'Native policy refused the source.'},{isError:true},{error:{code,retryable:false}});
        base.session.call=async(tool,args,options)=>{if(tool==='airk_file_info'&&args.verification==='report'){reports++;throw refused;}return native(tool,args,options);};
        const dependencies={...deps(base.session),live:async()=>{hostCaptures++;return 'matches_index';}};
        await assert.rejects(runStrategy({strategy:template('bug'),params:{query:'renew',symbol:'refresh'},scope:{}},dependencies),error=>error===refused);
        assert.equal(reports,1);assert.equal(hostCaptures,0);assert.ok(base.calls.some(call=>call.tool==='airk_file_info'&&call.args.verification===undefined),'a prior cached location never substitutes for native admission');
    }
});

test('native source checks share remaining byte/time budgets and retain complete original replies',async()=>{
    const base=fake(),native=base.session.call.bind(base.session),limits:Record<string,unknown>[]=[];let elapsed=0;
    base.session.call=async(tool,args,options)=>{
        const result=await native(tool,args,options);
        if(tool==='airk_file_info'&&args.verification==='report'){
            limits.push({...args});const check=result.payload.source_verification as any,spent=limits.length===1?300*1024*1024:1;
            check.counts.bytes_read=spent;check.sources[0].bytes_read=spent;check.limits.max_bytes=args.verification_max_bytes;check.limits.deadline_ms=args.verification_deadline_ms;check.limits.effective_deadline_ms=args.verification_deadline_ms;
            result.payload.future_native_verification={retained:true};elapsed+=limits.length===1?4000:1;
        }
        return result;
    };
    const strategy={...template('bug'),budgets:{...template('bug').budgets,seconds:5}};
    const result=await runStrategy({strategy,params:{query:'renew',symbol:'refresh'},scope:{}},{...deps(base.session),now:()=>1000+elapsed});
    assert.equal(result.manifest.outcome,'verified_snapshot',result.manifest.reasons.join('; '));assert.equal(limits.length,2);
    assert.deepEqual(limits.map(args=>args.verification_max_bytes),[512*1024*1024,212*1024*1024]);
    assert.deepEqual(limits.map(args=>args.verification_deadline_ms),[5000,1000]);assert.ok(limits.every(args=>args.revision==='airk-r1:p:1'&&args.known_hash===HASH(String(args.path))));
    assert.equal(result.sourceChecks?.length,2);assert.match(JSON.stringify(result),/future_native_verification/);
});

test('missing native receipt accounting stops further capture and dirty buffers remain uncertain',async()=>{
    const base=fake(),native=base.session.call.bind(base.session);let captures=0;
    base.session.call=async(tool,args,options)=>{const result=await native(tool,args,options);if(tool==='airk_file_info'&&args.verification==='report'){captures++;delete result.payload.source_verification;}return result;};
    const result=await runStrategy({strategy:template('bug'),params:{query:'renew',symbol:'refresh'},scope:{}},deps(base.session));
    assert.equal(captures,1);assert.equal(result.manifest.outcome,'partial');assert.ok(result.manifest.reasons.some(reason=>reason.includes('accounting unavailable')));assert.ok(result.sourceChecks?.every(row=>row.live==='unchecked'));
    const dirty=await runStrategy({strategy:template('bug'),params:{query:'renew',symbol:'refresh'},scope:{}},{...deps(fake().session),dirty:async()=>true});assert.equal(dirty.manifest.outcome,'partial');assert.ok(dirty.sourceChecks?.every(row=>row.live==='not_saved'));
});
