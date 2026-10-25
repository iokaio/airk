// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import Module from 'node:module';
import * as fs from 'node:fs/promises';
import * as os from 'node:os';
import * as path from 'node:path';
import { RuntimeHost } from '../serviceBootstrap';
import { ServiceWorkspace } from '../serviceWorkspace';
import { ServiceError } from '../serviceClient';

test('service activation advertises reviewed bindings without indexing and preserves explicit Stop during resolution',async context=>{
    const root=await fs.mkdtemp(path.join(os.tmpdir(),'airk-service-activation-')),source=path.join(root,'source');await fs.mkdir(source);
    const values=new Map<string,unknown>(),commands=new Map<string,()=>Promise<unknown>>(),terminals:any[]=[];
    const storage={keys:()=>[...values.keys()],get:<T>(key:string)=>values.get(key) as T|undefined,update:async(key:string,value:unknown)=>{values.set(key,value);}};
    const binding={installDir:root,controlDir:path.join(root,'control'),executable:path.join(root,'selected'),launcher:path.join(root,'airk-launcher'),
        runtimeSha256:'ab'.repeat(32),version:'1.4.0',lifecycle:'on-demand' as const,verificationBasis:'trusted_distribution'};
    const registration={installDir:root,controlDir:binding.controlDir,runtimeSha256:binding.runtimeSha256,binding:'cd'.repeat(32),
        root:await fs.realpath(source),data_root:path.join(root,'data'),collection:'fixture'};
    await new ServiceWorkspace(storage,{reviewProject:async()=>registration,registerProject:async()=>registration},()=>true)
        .register(binding,source,registration.data_root,registration.collection,async()=>true);
    let discoveries=0,ensures=0,stopped=false,provider:any;
    context.mock.method(RuntimeHost.prototype,'discover',async()=>{discoveries++;return {bundle:{},installDir:root,installed:true,location:{},inspection:{
        selection:{active:binding.runtimeSha256},approval:{manifest_sha256:binding.runtimeSha256,version:'1.4.0',verification_basis:'trusted_distribution'},
        executable:binding.executable,launcher:binding.launcher,control_dir:binding.controlDir}};});
    context.mock.method(RuntimeHost.prototype,'ensure',async(explicit?:boolean)=>{ensures++;assert.notEqual(explicit,true);if(stopped)throw new ServiceError('service_explicitly_stopped');return binding;});
    const disposable={dispose(){}},event=()=>disposable;
    const stub:any={workspace:{isTrusted:true,workspaceFolders:[{name:'Fixture',uri:{scheme:'file',fsPath:source}}],
        getConfiguration:()=>({inspect:(key:string)=>({globalValue:key==='mcp.responseMode'?'full':key==='cli.autoInstall'?false:undefined})}),
        onDidChangeWorkspaceFolders:event,onDidGrantWorkspaceTrust:event,onDidChangeConfiguration:event,
        createFileSystemWatcher:()=>{throw new Error('Editor-owned source watching is forbidden');}},
    window:{createOutputChannel:()=>({appendLine(){},show(){},dispose(){}}),createStatusBarItem:()=>({show(){},dispose(){}}),
        registerWebviewViewProvider:()=>disposable,createTreeView:()=>disposable,onDidChangeActiveTextEditor:event,
        showErrorMessage:async()=>undefined,showWarningMessage:async()=>undefined,
        createTerminal:(options:any)=>{terminals.push(options);return {show(){},dispose(){}};}},
    commands:{registerCommand:(name:string,action:()=>Promise<unknown>)=>{commands.set(name,action);return disposable;},executeCommand:async()=>{}},
    lm:{registerMcpServerDefinitionProvider:(_name:string,value:any)=>{provider=value;return disposable;}},env:{},StatusBarAlignment:{Left:1},
    EventEmitter:class {event=event;fire(){}dispose(){}},TreeItem:class {},Uri:{file:(fsPath:string)=>({fsPath})},
    McpStdioServerDefinition:class {cwd:unknown;constructor(public label:string,public command:string,public args:string[],public env:unknown,public version:string){}}};
    const loader=Module as unknown as {_load:(...args:any[])=>any},original=loader._load;
    loader._load=function(request:string,...args:any[]){return request==='vscode'?stub:original.call(this,request,...args);};
    let extension:any;const extensionContext:any={extensionPath:root,extension:{packageJSON:{version:'1.4.0'}},globalState:storage,subscriptions:[]};
    try{
        delete require.cache[require.resolve('../serviceExtension')];extension=require('../serviceExtension');
        const api=await extension.activate(extensionContext);assert.equal(ensures,0);assert.ok(discoveries>0);
        const definitions=await api.getServerDefinitions();assert.equal(definitions.length,1);assert.equal(ensures,0);
        assert.equal(definitions[0].command,binding.launcher);assert.equal(definitions[0].env.AIRK_MCP_RESPONSE_MODE,'full');
        assert.ok(definitions[0].args.includes(registration.binding));assert.equal(definitions[0].cwd.fsPath,path.dirname(binding.launcher));
        stopped=true;await assert.rejects(provider.resolveMcpServerDefinition(definitions[0]),/service_explicitly_stopped/);assert.equal(ensures,1);
        stopped=false;await commands.get('airk.openTools')!();assert.equal(terminals.length,1);
        assert.deepEqual(terminals[0].shellArgs.slice(-2),['tools','shell']);assert.equal(terminals[0].shellPath,binding.launcher);
        const usageDrafts:unknown[]=[];
        context.mock.method(require('../serviceConsole').ServiceConsole.prototype,'configureProject',async(project:string,draft:unknown,valid:()=>boolean)=>{
            assert.equal(project,registration.binding);assert.equal(valid(),true);usageDrafts.push(draft);
        });
        context.mock.method(require('../usagePanel'),'openUsagePanel',async(_spec:unknown,_valid:unknown,write:(args:string[])=>Promise<unknown>)=>{
            for(const mode of ['disabled','aggregates-only','diagnostic-events'])await write(['configure','--mode',mode]);
            await assert.rejects(write(['configure','--mode','arbitrary']),/invalid_usage_action/);
        });
        await commands.get('airk.usage')!();
        assert.deepEqual(usageDrafts,[{operation:'usage',mode:'disabled'},{operation:'usage',mode:'aggregates_only'},{operation:'usage',mode:'diagnostic_events'}]);
        stub.workspace.isTrusted=false;assert.deepEqual(await api.getServerDefinitions(),[]);
        const before=ensures;await assert.rejects(provider.resolveMcpServerDefinition(definitions[0]),/runtime_client_no_longer_authorized/);assert.equal(ensures,before);
    }finally{extension?.deactivate();loader._load=original;delete require.cache[require.resolve('../serviceExtension')];await fs.rm(root,{recursive:true,force:true});}
});
