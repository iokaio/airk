// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import { serviceTestHost } from './serviceTestHost';
import * as native from '../native';
import { UNIFIED_TOOLS } from '../model';
import { RuntimeHost } from '../serviceBootstrap';
import * as bootstrap from '../serviceBootstrap';
import * as fs from 'node:fs/promises';

test('opening an empty raw log opens an editor and releases Working without waiting for a notification',async context=>{
    const f=await serviceTestHost(context);let dismiss!:()=>void;
    const notification=new Promise<void>(resolve=>{dismiss=resolve;});
    f.stub.window.showInformationMessage=()=>notification;
    context.mock.method(bootstrap,'nativeRunner',async(...[_command,args,timeout]:Parameters<typeof bootstrap.nativeRunner>)=>{
        assert.ok(args.includes('--raw'));assert.equal(timeout,30000);await fs.writeFile(args[args.indexOf('--output')+1],'');return {written_bytes:0};
    });
    const {ServiceConsole}=require('../serviceConsole');
    const console=new ServiceConsole(f.context,new RuntimeHost(f.root,'1.4.0',{channel:'stable',installDir:f.binding.installDir}));
    const action=console.run('openRawLogs');let finished=false;void action.then(()=>{finished=true;});
    try{
        let timer:ReturnType<typeof setTimeout>|undefined;
        await Promise.race([action,new Promise(resolve=>{timer=setTimeout(resolve,2000);})]);
        if(timer)clearTimeout(timer);
        assert.equal(finished,true,'no notification dismissal is required to finish the action');
        assert.equal(console.controller.state.busy,false);
        assert.match(f.openedDocuments.at(-1).content,/No detailed errors/);
        assert.equal(f.openedDocuments.at(-1).language,'plaintext');
    }finally{dismiss();await action;console.dispose();await f.close();}
});

test('raw log failure clears Working and retains the concrete read failure in output',async context=>{
    const f=await serviceTestHost(context);
    context.mock.method(bootstrap,'nativeRunner',async()=>{throw new Error('fixture raw log read deadline exceeded');});
    const {ServiceConsole}=require('../serviceConsole');
    const console=new ServiceConsole(f.context,new RuntimeHost(f.root,'1.4.0',{channel:'stable',installDir:f.binding.installDir}));
    try{
        await console.run('openRawLogs');assert.equal(console.controller.state.busy,false);
        assert.equal(console.controller.state.message,'raw_diagnostic_log_read_failed_see_airk_service_output');
        assert.ok(f.calls.some(message=>message.includes('fixture raw log read deadline exceeded')));
    }finally{console.dispose();await f.close();}
});
test('saving an empty raw log opens the save dialog and writes the selected file',async context=>{
    const f=await serviceTestHost(context);let dismiss!:()=>void;
    const notification=new Promise<void>(resolve=>{dismiss=resolve;});f.stub.window.showInformationMessage=()=>notification;
    const saved:{target:unknown;text:string}[]=[],destination={scheme:'file',fsPath:'chosen-raw-log.jsonl'};let dialogs=0;
    f.stub.window.showSaveDialog=async()=>{dialogs++;return destination;};
    f.stub.workspace.fs={copy:async(source:{fsPath:string},target:unknown)=>{saved.push({target,text:await fs.readFile(source.fsPath,'utf8')});}};
    context.mock.method(bootstrap,'nativeRunner',async(...[_command,args]:Parameters<typeof bootstrap.nativeRunner>)=>{
        await fs.writeFile(args[args.indexOf('--output')+1],'');return {written_bytes:0};
    });
    const {ServiceConsole}=require('../serviceConsole');
    const console=new ServiceConsole(f.context,new RuntimeHost(f.root,'1.4.0',{channel:'stable',installDir:f.binding.installDir}));
    try{
        await console.run('saveRawLogs');assert.equal(console.controller.state.busy,false);assert.equal(dialogs,1);
        assert.deepEqual(saved,[{target:destination,text:''}]);
    }finally{dismiss();console.dispose();await f.close();}
});

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
        return {tools:[...UNIFIED_TOOLS],status:{state:'index_unknown'}};
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
        assert.equal(check.state.connectionCheck.tools.length,27);
        assert.equal(check.state.connectionCheck.project,project);
        assert.equal(check.state.connectionCheck.indexState,'unknown');
        assert.match(check.state.connectionCheck.message,/Index readiness is unconfirmed/);
        assert.equal(f.writes.length,0,'checking a connection cannot enqueue indexing or repair');
    }finally{release();await f.close();}
});
