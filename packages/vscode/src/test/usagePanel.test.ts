// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import Module = require('node:module');
import * as path from 'node:path';

test('selected usage performs no native reads or writes and only opens administration after an explicit current action',async()=>{
    let receive:(value:unknown)=>Promise<void>=async()=>undefined,dispose=()=>{},valid=true,reads=0,writes=0,opened=0;
    const rendered:any[]=[];
    const stub={workspace:{isTrusted:true},window:{createWebviewPanel:()=>{throw new Error('embedded only');}}};
    const native=require('../native'),previousRun=native.runText;
    native.runText=async()=>{reads++;throw new Error('selected scope must not read global usage');};
    const loader=Module as unknown as {_load:(...args:any[])=>any},original=loader._load;
    loader._load=function(request:string,...args:any[]){return request==='vscode'?stub:original.call(this,request,...args);};
    try{
        delete require.cache[require.resolve('../usagePanel')];const {openUsagePanel}=require('../usagePanel');
        const spec={command:'fixture',root:path.resolve('source'),dataDir:path.resolve('data'),collection:'fixture',name:'Selected evidence',
            service:{installDir:path.resolve('install'),project:'ab'.repeat(32)},catalogSession:{schema:1,context_key:'cd'.repeat(32),namespace:'ef'.repeat(16),
                expected_catalog_revision:1,grant_id:'grt_'+'01'.repeat(16),expected_grant_revision:1,index_id:'idx_'+'02'.repeat(16),project_ids:['prj_'+'03'.repeat(16)]}};
        await openUsagePanel(spec,()=>valid,async()=>{writes++;}, {render:(value:unknown)=>rendered.push(value),
            onMessage:(listener:typeof receive)=>{receive=listener;return {dispose(){}};},onDispose:(listener:typeof dispose)=>{dispose=listener;}},
            {openIndexAdministration:async()=>{opened++;}});
        assert.equal(rendered.at(-1).scoped_unavailable,true);assert.equal(opened,0);
        for(const action of ['refresh','json','csv','markdown','recording','clear','import','prices','removePrices','template','feedback']){
            await assert.rejects(receive({type:'usage',action}),/explicit Index administration/);
        }
        await assert.rejects(receive({type:'openJson'}),/explicit Index administration/);
        await assert.rejects(receive({type:'usage',action:'days',days:7}),/explicit Index administration/);
        await receive({type:'usage',action:'openIndexAdministration',backing:'forged'});
        assert.equal(reads,0);assert.equal(writes,0);assert.equal(opened,0);
        await receive({type:'usage',action:'openIndexAdministration'});assert.equal(opened,1);
        valid=false;await assert.rejects(receive({type:'usage',action:'openIndexAdministration'}),/selection changed/);
        assert.equal(opened,1);assert.equal(reads,0);assert.equal(writes,0);dispose();
    }finally{loader._load=original;native.runText=previousRun;delete require.cache[require.resolve('../usagePanel')];}
});

test('explicit administration is labeled and refuses crossed native backing/runtime or a catalog selector',async()=>{
    let reads=0,receive:(value:unknown)=>Promise<void>=async()=>undefined;const rendered:any[]=[];
    const stub={workspace:{isTrusted:true},window:{createWebviewPanel:()=>{throw new Error('embedded only');}}};
    const native=require('../native'),previousRun=native.runText;
    native.runText=async()=>{reads++;return JSON.stringify({report:{calls:7}});};
    const loader=Module as unknown as {_load:(...args:any[])=>any},original=loader._load;
    loader._load=function(request:string,...args:any[]){return request==='vscode'?stub:original.call(this,request,...args);};
    try{
        delete require.cache[require.resolve('../usagePanel')];const {openUsagePanel}=require('../usagePanel');
        const spec={command:'fixture',root:path.resolve('source'),dataDir:path.resolve('data'),collection:'fixture',name:'Native Index',
            service:{installDir:path.resolve('install'),project:'ab'.repeat(32)}};
        const administration={purpose:'index_administration',label:'Entire Index history and shared data-root totals',namespace:'cd'.repeat(16),catalogRevision:2,
            indexId:'idx_'+'03'.repeat(16),backing:spec.service.project,runtime:{installDir:spec.service.installDir,controlDir:path.resolve('control'),runtimeSha256:'ef'.repeat(32)},
            nativeObservation:{host_context:{status:'catalog_host_context',descriptor:{namespace:'cd'.repeat(16),catalog_revision:2},future_host:{retained:['original']}},
                index_detail:{status:'catalog_detail',namespace:'cd'.repeat(16),revision:2,item:{kind:'index',value:{id:'idx_'+'03'.repeat(16),future_native_index:{available:null}}},future_transport:'retained'}}};
        const surface={render:(value:unknown)=>rendered.push(value),onMessage:(listener:typeof receive)=>{receive=listener;return {dispose(){}};},onDispose:()=>{}};
        for(const crossed of [{...administration,backing:'04'.repeat(32)},
            {...administration,runtime:{...administration.runtime,installDir:path.resolve('other')}},
            {...administration,namespace:'forged'},{...administration,catalogRevision:0}]){
            await assert.rejects(openUsagePanel(spec,()=>true,async()=>{},surface,{administration:crossed}));
        }
        await assert.rejects(openUsagePanel({...spec,catalogSession:{}},()=>true,async()=>{},surface,{administration}));
        assert.equal(reads,0);
        await openUsagePanel(spec,()=>true,async()=>{},surface,{administration});
        assert.equal(reads,1);assert.equal(rendered.at(-1).administrative_scope.purpose,'index_administration');
        assert.equal(rendered.at(-1).report.calls,7);
        const originalObservation=structuredClone(administration.nativeObservation);
        assert.deepEqual(rendered.at(-1).administrative_scope.native_observation,originalObservation);
        assert.equal(rendered.at(-1).administrative_scope.qualification,'host_administrative_context_not_read_grant');
        administration.nativeObservation.host_context.future_host.retained[0]='mutated later host cache';
        administration.nativeObservation.index_detail.future_transport='changed';
        await receive({type:'usage',action:'refresh'});
        assert.deepEqual(rendered.at(-1).administrative_scope.native_observation,originalObservation,
            'full unknown native fields remain the captured administrative observation, independent of later host cache mutation');
    }finally{loader._load=original;native.runText=previousRun;delete require.cache[require.resolve('../usagePanel')];}
});

test('embedded usage loads and changes windows without creating a panel, and rejects stale actions', async () => {
    let receive: (message: unknown) => Promise<void> = async () => undefined, dispose = () => {}, valid = true;
    const rendered: any[] = [], reads: string[][] = [];
    const stub = { workspace: { isTrusted: true }, window: { createWebviewPanel: () => { throw new Error('An embedded report must not open a panel'); } } };
    const native = require('../native'), previousRun = native.runText;
    native.runText = async (_command: string, args: string[]) => { reads.push(args); return JSON.stringify({ report: { collection: 'fixture', calls: 0 } }); };
    const loader = Module as unknown as { _load: (...args: any[]) => any }, original = loader._load;
    loader._load = function (request: string, ...args: any[]) { return request === 'vscode' ? stub : original.call(this, request, ...args); };
    try {
        delete require.cache[require.resolve('../usagePanel')]; const { openUsagePanel } = require('../usagePanel');
        await openUsagePanel({ command: 'fixture', root: '/source', dataDir: '/data', collection: 'fixture', name: 'Fixture' }, () => valid,
            async () => { throw new Error('Read-only test must not mutate'); }, {
                render: (value: unknown) => rendered.push(value), onMessage: (listener: typeof receive) => { receive = listener; return { dispose() {} }; }, onDispose: (listener: typeof dispose) => { dispose = listener; },
            });
        assert.equal(rendered.at(-1).days, 30);
        await receive({ type: 'usage', action: 'days', days: 7 });
        assert.equal(rendered.at(-1).days, 7); assert.ok(reads.at(-1)!.includes('7'));
        const before = reads.length;
        await receive({ type: 'usage', action: 'days', days: 8 }); assert.equal(reads.length, before);
        valid = false;
        await assert.rejects(receive({ type: 'usage', action: 'refresh' }), /selection changed/);
        assert.equal(reads.length, before);
        dispose();
    } finally { loader._load = original; native.runText = previousRun; delete require.cache[require.resolve('../usagePanel')]; }
});

test('usage import binds reviewed bytes and refuses approval after the captured panel becomes stale',async()=>{
    let message:(value:unknown)=>Promise<void>=async()=>undefined,disposed=()=>undefined,valid=true,staleOnApproval=false;
    const writes:string[][]=[],errors:string[]=[],reads:string[][]=[];let continuing:(()=>boolean)|undefined;
    const hash='ab'.repeat(32),file='/chosen/original-measurement.json';
    const webview={html:'',onDidReceiveMessage(callback:typeof message){message=callback;return {dispose(){}};}};
    const vscode={ViewColumn:{Active:1},workspace:{isTrusted:true,openTextDocument:async(value:unknown)=>value},window:{
        createWebviewPanel:()=>({webview,onDidDispose(callback:typeof disposed){disposed=callback;},dispose:()=>disposed()}),
        showOpenDialog:async()=>[{fsPath:file}],showTextDocument:async()=>undefined,
        showInformationMessage:async()=>{if(staleOnApproval)valid=false;return 'Import measurement';},
        showErrorMessage:(error:string)=>errors.push(error)}};
    const native=require('../native'),previousRun=native.runText;
    native.runText=async(_command:string,args:string[])=>{reads.push(args);return JSON.stringify(args.includes('--preview')?{input_sha256:hash,structure_only:true}:{report:{}});};
    const loader=Module as unknown as {_load:(...args:any[])=>any},original=loader._load;
    loader._load=function(request:string,...args:any[]){return request==='vscode'?vscode:original.call(this,request,...args);};
    try{
        delete require.cache[require.resolve('../usagePanel')];const {openUsagePanel}=require('../usagePanel');
        await openUsagePanel({command:'fixture-launcher',root:'/scope/source',dataDir:'/scope/state',collection:'fixture',name:'Fixture'},()=>valid,
            async(args:string[],guard:()=>boolean)=>{writes.push(args);continuing=guard;});
        await message({type:'usage',action:'import'});
        assert.deepEqual(writes,[['import',file,'--if-sha256',hash]]);assert.equal(continuing!(),true);
        assert.ok(reads.some(args=>args.includes(file)&&args.includes('--preview')));
        staleOnApproval=true;await message({type:'usage',action:'import'});
        assert.equal(writes.length,1);assert.equal(continuing!(),false);assert.ok(errors.some(error=>error.includes('Reopen Usage & Savings')));
        disposed();
    }finally{loader._load=original;native.runText=previousRun;delete require.cache[require.resolve('../usagePanel')];}
});

test('usage panel pins collection scope, exports chosen window, confirms clear and rejects stale actions', async () => {
    let message: (value: unknown) => Promise<void> = async () => undefined;
    let disposed = () => undefined;
    let valid = true, confirm: string | undefined;
    const reads: string[][] = [], writes: string[][] = [], exports: string[] = [], errors: string[] = [];
    const webview = { html: '', onDidReceiveMessage: (callback: typeof message) => { message = callback; return { dispose() {} }; } };
    const vscode = {
        ViewColumn: { Active: 1 },
        workspace: { isTrusted: true, fs: { writeFile: async (_uri: unknown, data: Buffer) => { exports.push(data.toString()); } } },
        window: { createWebviewPanel: () => ({ webview, onDidDispose: (callback: typeof disposed) => { disposed = callback; }, dispose: () => disposed() }),
            showSaveDialog: async () => ({ scheme: 'file' }), showWarningMessage: async () => confirm,
            showErrorMessage: (error: string) => errors.push(error) }
    };
    const native = require('../native'), previousRun = native.runText;
    let holdNextRead = false, releaseRead: (() => void) | undefined;
    native.runText = async (_command: string, args: string[]) => {
        reads.push(args);
        if (holdNextRead) {
            holdNextRead = false;
            await new Promise<void>(resolve => { releaseRead = resolve; });
        }
        return args.includes('csv') ? 'operation,calls\nsearch,1\n' : JSON.stringify({ report: { scope: { collection: 'fixture' }, calls: 1 }, status: 'partial' });
    };
    const loader = Module as unknown as { _load: (...args: any[]) => any }, original = loader._load;
    loader._load = function(request: string, ...args: any[]) { return request === 'vscode' ? vscode : original.call(this, request, ...args); };
    try {
        delete require.cache[require.resolve('../usagePanel')];
        const { openUsagePanel } = require('../usagePanel');
        await openUsagePanel({ command: 'fixture-helper', root: '/fixture/root', dataDir: '/fixture/state', collection: 'fixture', name: 'Fixture' }, () => valid,
            async (args: string[]) => { writes.push(args); });
        holdNextRead = true;
        const changing = message({ type: 'usage', action: 'days', days: 7 });
        await new Promise(resolve => setImmediate(resolve));
        assert.match(webview.html, /aria-busy="true"/);
        assert.match(webview.html, /<fieldset disabled>/);
        await message({ type: 'usage', action: 'csv' });
        assert.equal(exports.length, 0, 'No second action is submitted while a report is pending');
        releaseRead!(); await changing;
        assert.match(webview.html, /aria-busy="false"/);
        assert.ok(reads.at(-1)!.includes('7'));
        await message({ type: 'usage', action: 'csv' });
        assert.equal(exports[0], 'operation,calls\nsearch,1\n');
        assert.ok(reads.every(args => args.includes('/fixture/root') && args.includes('/fixture/state') && args.includes('fixture')));
        await message({ type: 'usage', action: 'clear' });
        assert.equal(writes.length, 0, 'Cancelled clear preserves history');
        confirm = 'Clear history';
        await message({ type: 'usage', action: 'clear' });
        assert.deepEqual(writes, [['clear']]);
        const before = reads.length;
        valid = false;
        await message({ type: 'usage', action: 'refresh' });
        await message({ type: 'usage', action: 'clear' });
        assert.equal(reads.length, before);
        assert.equal(writes.length, 1);
        assert.ok(errors.every(error => error.includes('Reopen Usage & Savings')));
        disposed();
    } finally { loader._load = original; native.runText = previousRun; delete require.cache[require.resolve('../usagePanel')]; }
});
