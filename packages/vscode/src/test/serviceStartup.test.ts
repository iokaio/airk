// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import { serviceTestHost } from './serviceTestHost';
import * as native from '../native';
import { UNIFIED_TOOLS } from '../model';

test('checking MCP without a registered project cannot register or start indexing',async context=>{
    const f=await serviceTestHost(context,{registered:false});
    try{
        await assert.rejects(f.commands.get('airk.verifyMcp')!(),/No project selected/);
        assert.equal(f.writes.length,0);assert.equal(f.registrations.size,0);assert.equal(f.state.picks,0);
        assert.ok(f.calls.some(line=>line.includes('MCP check failed: No project selected')));
    }finally{await f.close();}
});

test('closing the window during startup prevents late native discovery from installing or publishing success',async context=>{
    let release!:()=>void;
    const discoveryGate=new Promise<void>(resolve=>{release=resolve;});
    const f=await serviceTestHost(context,{automatic:true,deferStartup:true,discoveryGate});
    await f.close();const calls=f.calls.length;
    release();await f.api.whenReady();
    assert.equal(f.writes.length,0);assert.equal(f.calls.length,calls);
});

test('activation and Explorer rendering finish while native discovery is deliberately blocked',async context=>{
    let release!:()=>void;
    const discoveryGate=new Promise<void>(resolve=>{release=resolve;});
    const f=await serviceTestHost(context,{deferStartup:true,discoveryGate});
    try{
        let completed=false;void f.api.whenReady().then(()=>{completed=true;});
        await f.commands.get('airk.explorer.open')!();
        assert.equal(completed,false);
        const panel=f.panels.find(panel=>panel.viewType==='airk.explorer');
        assert.ok(panel,'the editor opens before any native discovery response');
        assert.match(panel.webview.html,/explorerWebview/);
        await panel.send({v:1,type:'ready',requestId:'startup-ready'});
        assert.ok(panel.posted.some((entry:any)=>entry.type==='state'&&entry.state.startup?.message.includes('Connecting')));
        assert.equal(f.writes.length,0);
        release();await f.api.whenReady();
        assert.ok((await f.api.getServerDefinitions()).length);
    }finally{release();await f.api.whenReady();await f.close();}
});

test('connection check publishes preparation, progress and failure, coalesces clicks, and can retry',async context=>{
    const f=await serviceTestHost(context);
    let release!:()=>void;
    const pending=new Promise<void>(resolve=>{release=resolve;});
    let probes=0,fail=true;
    context.mock.method(native,'probeMcp',async(...args:Parameters<typeof native.probeMcp>)=>{
        const report=args[4];
        probes++;report?.('MCP handshake completed. Reading the tool list…');
        await pending;if(fail)throw new Error('fixture: tools/list rejected');
        return {tools:[...UNIFIED_TOOLS],status:{}};
    });
    try{
        await f.commands.get('airk.explorer.open')!();
        const panel=f.panels.find(panel=>panel.viewType==='airk.explorer')!;
        await panel.send({v:1,type:'ready',requestId:'check-ready'});
        const project=[...f.registrations.keys()][0];
        const first=f.commands.get('airk.verifyMcp')!(project),second=f.commands.get('airk.verifyMcp')!(project);
        const settled=Promise.allSettled([first,second]);
        assert.ok(f.calls.some(value=>value.includes('MCP check: Checking the selected project')));
        for(let i=0;i<20&&probes===0;i++)await new Promise(resolve=>setImmediate(resolve));
        assert.equal(probes,1);
        assert.ok(panel.posted.some((entry:any)=>entry.state?.connectionCheck?.phase==='checking'));
        release();assert.ok((await settled).every(value=>value.status==='rejected'));
        assert.ok(panel.posted.some((entry:any)=>entry.state?.connectionCheck?.error?.includes('tools/list rejected')));
        fail=false;await f.commands.get('airk.verifyMcp')!(project);
        assert.equal(probes,2);
        const check=panel.posted.filter((entry:any)=>entry.state?.connectionCheck).at(-1) as any;
        assert.equal(check.state.connectionCheck.phase,'verified');
        assert.equal(check.state.connectionCheck.tools.length,26);
        assert.equal(check.state.connectionCheck.project,project);
        assert.equal(f.writes.length,0,'checking a connection cannot enqueue indexing or repair');
    }finally{release();await f.close();}
});
