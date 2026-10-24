// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import Module = require('node:module');

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
    native.runText = async (_command: string, args: string[]) => {
        reads.push(args);
        return args.includes('csv') ? 'operation,calls\nsearch,1\n' : JSON.stringify({ report: { scope: { collection: 'fixture' }, calls: 1 }, status: 'partial' });
    };
    const loader = Module as unknown as { _load: (...args: any[]) => any }, original = loader._load;
    loader._load = function(request: string, ...args: any[]) { return request === 'vscode' ? vscode : original.call(this, request, ...args); };
    try {
        delete require.cache[require.resolve('../usagePanel')];
        const { openUsagePanel } = require('../usagePanel');
        await openUsagePanel({ command: 'fixture-helper', root: '/fixture/root', dataDir: '/fixture/state', collection: 'fixture', name: 'Fixture' }, () => valid,
            async (args: string[]) => { writes.push(args); });
        await message({ type: 'usage', action: 'days', days: 7 });
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
