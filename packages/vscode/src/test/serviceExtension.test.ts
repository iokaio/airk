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
    await fs.mkdir(path.join(root,'out'));
    for(const name of ['explorerController.js','explorerWebview.js','explorerWebview.css'])await fs.writeFile(path.join(root,'out',name),'');
    const editorListeners=new Set<(editor:any)=>void>();
    const values=new Map<string,unknown>(),commands=new Map<string,()=>Promise<unknown>>(),terminals:any[]=[];
    const storage={keys:()=>[...values.keys()],get:<T>(key:string)=>values.get(key) as T|undefined,update:async(key:string,value:unknown)=>{values.set(key,value);}};
    const binding={installDir:root,controlDir:path.join(root,'control'),executable:path.join(root,'selected'),launcher:path.join(root,'airk-launcher'),
        runtimeSha256:'ab'.repeat(32),version:'1.4.0',lifecycle:'on-demand' as const,verificationBasis:'trusted_distribution'};
    const registration={installDir:root,controlDir:binding.controlDir,runtimeSha256:binding.runtimeSha256,binding:'cd'.repeat(32),
        root:await fs.realpath(source),data_root:path.join(root,'data'),collection:'fixture'};
    await new ServiceWorkspace(storage,{reviewProject:async()=>registration,registerProject:async()=>registration},()=>true)
        .register(binding,source,registration.data_root,registration.collection,async()=>true);
    let discoveries=0,ensures=0,stopped=false,provider:any;const outputShows:Array<[string,boolean]>=[];
    context.mock.method(RuntimeHost.prototype,'discoverForConnection',function(this:RuntimeHost){return this.discover();});
    context.mock.method(RuntimeHost.prototype,'discover',async()=>{discoveries++;return {bundle:{},installDir:root,installed:true,location:{},inspection:{
        selection:{active:binding.runtimeSha256},approval:{manifest_sha256:binding.runtimeSha256,version:'1.4.0',verification_basis:'trusted_distribution'},
        executable:binding.executable,launcher:binding.launcher,control_dir:binding.controlDir}};});
    context.mock.method(RuntimeHost.prototype,'ensure',async(explicit?:boolean)=>{ensures++;assert.notEqual(explicit,true);if(stopped)throw new ServiceError('service_explicitly_stopped');return binding;});
    context.mock.method(RuntimeHost.prototype as any,'invoke',async(_command:string,args:string[])=>{
        assert.deepEqual(args.slice(0,2),['runtime','source-scope']);
        const row=registration;
        return {install_dir:binding.installDir,control_dir:binding.controlDir,runtime_sha256:binding.runtimeSha256,
            scope:{schema:1,root:row.root,collection:args.includes('--index-collection')?args[args.indexOf('--index-collection')+1]:null,
                namespace:'12'.repeat(16),revision:7,observed_unix_ms:1,outcome:'registered',registration:row,fencing_generation:1}};
    });
    context.mock.method(RuntimeHost.prototype,'client',()=>({projects:async(_after:unknown,_revision:unknown,_limit:unknown,inactive:boolean)=>({revision:1,cached:true,observed_unix_ms:1,after:null,
        projects:inactive?[]:[{project:{registration}}]}),dispose(){}}) as any);
    const disposable={dispose(){}},event=()=>disposable;
    const stub:any={workspace:{isTrusted:true,workspaceFolders:[{name:'Fixture',uri:{scheme:'file',fsPath:source}}],
        getConfiguration:()=>({get:(key:string,fallback?:unknown)=>key==='explorer.assistance.enabled'?false:fallback,inspect:(key:string)=>({globalValue:key==='mcp.responseMode'?'full':key==='cli.autoInstall'?false:undefined})}),
        getWorkspaceFolder:()=>stub.workspace.workspaceFolders[0],
        onDidChangeWorkspaceFolders:event,onDidGrantWorkspaceTrust:event,onDidChangeConfiguration:event,
        createFileSystemWatcher:()=>{throw new Error('Editor-owned source watching is forbidden');}},
    window:{createOutputChannel:(name:string)=>({appendLine(){},show(preserve:boolean){outputShows.push([name,preserve]);},dispose(){}}),createStatusBarItem:()=>({show(){},dispose(){}}),
        registerWebviewViewProvider:()=>disposable,createTreeView:()=>disposable,onDidChangeActiveTextEditor:(listener:(editor:any)=>void)=>{editorListeners.add(listener);return {dispose:()=>editorListeners.delete(listener)};},
        showErrorMessage:async()=>undefined,showWarningMessage:async()=>undefined,
        createTerminal:(options:any)=>{terminals.push(options);return {show(){},dispose(){}};}},
    commands:{registerCommand:(name:string,action:()=>Promise<unknown>)=>{commands.set(name,action);return disposable;},executeCommand:async()=>{}},
    lm:{registerMcpServerDefinitionProvider:(name:string,value:any)=>{if(name==='airk.collections')provider=value;return disposable;}},env:{},StatusBarAlignment:{Left:1},
    EventEmitter:class {event=event;fire(){}dispose(){}},TreeItem:class {},Uri:{file:(fsPath:string)=>({fsPath})},
    McpStdioServerDefinition:class {cwd:unknown;constructor(public label:string,public command:string,public args:string[],public env:unknown,public version:string){}}};
    const loader=Module as unknown as {_load:(...args:any[])=>any},original=loader._load;
    loader._load=function(request:string,...args:any[]){return request==='vscode'?stub:original.call(this,request,...args);};
    let extension:any;const extensionContext:any={extensionPath:root,extensionUri:stub.Uri.file(root),extension:{packageJSON:{version:'1.4.0'}},globalState:storage,subscriptions:[]};
    try{
        delete require.cache[require.resolve('../serviceExtension')];extension=require('../serviceExtension');
        const api=await extension.activate(extensionContext);await api.whenReady();assert.equal(ensures,0);assert.ok(discoveries>0);
        // Actual activation wiring and actual hidden Explorer; no panel/native transport is opened.
        const controller=extensionContext.subscriptions[0],explorer=await controller.explorerController();
        const consoleController=controller.console.controller;
        let refreshes=0;
        const refreshMock=context.mock.method(consoleController,'refresh',async()=>{refreshes++;});
        try{
            const editor={document:{uri:{scheme:'file',fsPath:path.join(source,'entry.rs')}}};
            stub.window.activeTextEditor=editor;
            for(const listener of [...editorListeners])listener(editor);
            await Promise.resolve();
            assert.equal(refreshes,0,'hidden editor changes must not request service observations');
            assert.equal(explorer.snapshot().sourceFile,'entry.rs','Explorer keeps tracking the source editor');
            // Visibility-owned polling remains active, and release stops its next timer.
            context.mock.timers.enable({apis:['setTimeout']});
            try{
                const retained=controller.console.retain();
                try{
                    assert.equal(refreshes,1);
                    context.mock.timers.tick(2000);await Promise.resolve();await Promise.resolve();
                    assert.equal(refreshes,2,'visible consumer retains periodic refresh');
                }finally{retained.dispose();}
                context.mock.timers.tick(30000);await Promise.resolve();
                assert.equal(refreshes,2,'hidden console has no remaining poll');
            }finally{context.mock.timers.reset();}
            const explicitActions:unknown[]=[];
            const actionMock=context.mock.method(consoleController,'action',async(message:unknown)=>{
                assert.deepEqual(message,{type:'service',revision:consoleController.state.revision,action:'refresh'});
                explicitActions.push(message);
            });
            try{
                await controller.console.run('refresh');
                assert.equal(explicitActions.length,1,'explicit Refresh remains available while hidden');
            }finally{actionMock.mock.restore();}
        }finally{refreshMock.mock.restore();}
        const definitions=await api.getServerDefinitions();assert.equal(definitions.length,1);assert.equal(ensures,0);
        assert.equal(definitions[0].command,binding.launcher);assert.equal(definitions[0].env.AIRK_MCP_RESPONSE_MODE,'full');
        assert.ok(definitions[0].args.includes(registration.root));assert.equal(definitions[0].cwd.fsPath,path.dirname(binding.launcher));
        stopped=true;await assert.rejects(provider.resolveMcpServerDefinition(definitions[0]),/service_explicitly_stopped/);assert.equal(ensures,1);
        stopped=false;await commands.get('airk.openTools')!();assert.equal(terminals.length,1);
        assert.deepEqual(terminals[0].shellArgs.slice(-2),['tools','shell']);assert.equal(terminals[0].shellPath,binding.launcher);
        await commands.get('airk.showExtensionOutput')!();assert.deepEqual(outputShows.at(-1),['AIRK',false]);
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


test('selected Explorer project is available while other root waits without narrowing workspace MCP scope',{timeout:30000},async context=>{
    const root=await fs.mkdtemp(path.join(os.tmpdir(),'airk-selected-startup-')),source=path.join(root,'first'),other=path.join(root,'second');
    await fs.mkdir(source);await fs.mkdir(other);const canonical=await fs.realpath(source),otherCanonical=await fs.realpath(other);
    const values=new Map<string,unknown>(),storage={keys:()=>[...values.keys()],get:<T>(key:string)=>values.get(key) as T|undefined,update:async(key:string,value:unknown)=>{values.set(key,value);}};
    const binding={installDir:root,controlDir:path.join(root,'control'),executable:path.join(root,'selected'),launcher:path.join(root,'launcher'),runtimeSha256:'ab'.repeat(32),version:'1.8.0',lifecycle:'on-demand' as const,verificationBasis:'trusted_distribution'};
    const registrations=[{...binding,binding:'cd'.repeat(32),root:canonical,data_root:path.join(root,'data'),collection:'first'},
        {...binding,binding:'ef'.repeat(32),root:otherCanonical,data_root:path.join(root,'data'),collection:'second'}];
    const workspace=new ServiceWorkspace(storage,{reviewProject:async(_binding,opened)=>registrations[opened===source?0:1],registerProject:async(_binding,review)=>review},()=>true);
    for(const [index,opened] of [source,other].entries())await workspace.register(binding,opened,registrations[index].data_root,registrations[index].collection,async()=>true);
    context.mock.method(RuntimeHost.prototype,'discoverForConnection',function(this:RuntimeHost){return this.discover();});
    let stopped=false,exactFailure=false;
    context.mock.method(RuntimeHost.prototype,'discover',async()=>({bundle:{},installDir:root,installed:true,location:{},inspection:{selection:{active:binding.runtimeSha256},approval:{manifest_sha256:binding.runtimeSha256,version:binding.version,verification_basis:binding.verificationBasis},executable:binding.executable,launcher:binding.launcher,control_dir:binding.controlDir,lifecycle:{explicitly_stopped:stopped}}}) as any);
    let selectedOutcome='registered',selectedProject=registrations[0].binding;
    context.mock.method(RuntimeHost.prototype as any,'invoke',async(_command:string,args:string[])=>{
        assert.deepEqual(args.slice(0,2),['runtime','source-scope']);
        if(exactFailure)throw new Error('original scoped metadata fixture failure');
        const row=registrations.find(value=>value.root===args[args.indexOf('--source-root')+1])!;
        return {install_dir:binding.installDir,control_dir:binding.controlDir,runtime_sha256:binding.runtimeSha256,
            scope:{schema:1,root:row.root,collection:args.includes('--index-collection')?args[args.indexOf('--index-collection')+1]:null,
                namespace:'12'.repeat(16),revision:7,observed_unix_ms:1,outcome:selectedOutcome,registration:selectedOutcome==='registered'?{...row,binding:selectedProject}:null,fencing_generation:selectedOutcome==='registered'?1:null}};
    });
    let pageReads=0;
    context.mock.method(RuntimeHost.prototype,'client',()=>({projects:async(_after:unknown,_revision:unknown,_limit:unknown,inactive:boolean)=>{pageReads++;await held;return {revision:7,cached:true,observed_unix_ms:1,after:null,projects:inactive?[]:registrations.map(registration=>({project:{registration}}))};},dispose(){}}) as any);
    let release!:()=>void,published!:()=>void,host:any,controller:any,definitionsChanged=0;
    const held=new Promise<void>(resolve=>{release=resolve;}),selected=new Promise<void>(resolve=>{published=resolve;});
    const nativeFs=require('node:fs/promises'),realpath=nativeFs.realpath;
    context.mock.method(nativeFs,'realpath',async(value:any,...args:any[])=>{if(String(value)===other)await held;return realpath(value,...args);});
    const disposable={dispose(){}},event=()=>disposable;
    class Emitter {private listeners:Array<()=>void>=[];event=(listener:()=>void)=>{this.listeners.push(listener);return disposable;};fire(){for(const listener of this.listeners)listener();}dispose(){}}
    class Workbench {visible=false;onDidChangeVisibility=event;constructor(_context:unknown,value:any){host=value;value.onDidChangeSpecs(()=>{if(value.specs().length===1)published();});}dispose(){}async toggle(){}}
    const stub:any={workspace:{isTrusted:true,workspaceFolders:[source,other].map((fsPath,index)=>({name:String(index),uri:{scheme:'file',fsPath}})),
        getConfiguration:()=>({get:(key:string,fallback?:unknown)=>key==='explorer.assistance.enabled'?false:fallback,inspect:(key:string)=>({globalValue:key==='cli.autoInstall'?false:undefined})}),onDidChangeWorkspaceFolders:event,onDidGrantWorkspaceTrust:event,onDidChangeConfiguration:event},
        window:{createOutputChannel:()=>({appendLine(){},dispose(){}}),createStatusBarItem:()=>({show(){},dispose(){}}),registerWebviewViewProvider:()=>disposable,createTreeView:(_name:string,options:any)=>{controller=options.treeDataProvider;return disposable;},onDidChangeActiveTextEditor:event,showWarningMessage:async()=>undefined},
        commands:{registerCommand:()=>disposable,executeCommand:async()=>{}},lm:{registerMcpServerDefinitionProvider:(name:string,provider:any)=>{if(name==='airk.collections')provider.onDidChangeMcpServerDefinitions(()=>{definitionsChanged++;});return disposable;}},
        env:{},StatusBarAlignment:{Left:1},EventEmitter:Emitter,TreeItem:class{},Uri:{file:(fsPath:string)=>({fsPath})},McpStdioServerDefinition:class{cwd:unknown;constructor(public label:string,public command:string,public args:string[],public env:unknown,public version:string){}}};
    const loader=Module as unknown as {_load:(...args:any[])=>any},original=loader._load;let extension:any;
    loader._load=function(request:string,...args:any[]){return request==='vscode'?stub:request==='./workbenchView'?{WorkbenchView:Workbench}:original.call(this,request,...args);};
    try{
        delete require.cache[require.resolve('../serviceExtension')];extension=require('../serviceExtension');
        const api=await extension.activate({extensionPath:root,extension:{packageJSON:{version:binding.version}},globalState:storage,workspaceState:storage,subscriptions:[]} as any);
        await selected;assert.equal(host.specs()[0].service.project,registrations[0].binding);
        assert.equal(pageReads,0,'selected UI must publish without requesting unrelated console pages');
        assert.equal(definitionsChanged,0,'early UI publication cannot emit a partial MCP definition');
        let completed=false;const definitions=api.getServerDefinitions().then((value:any)=>{completed=true;return value;});
        await Promise.resolve();assert.equal(completed,false,'complete workspace scopes still await the other root');
        release();const result=await definitions;assert.equal(result.length,1);
        assert.ok(result[0].args.includes('--no-default-project'));
        assert.ok(result[0].args.includes('--additional-root'));assert.ok(result[0].args.includes(otherCanonical));
        const configured=JSON.stringify(controller.specs),events=definitionsChanged;
        exactFailure=true;await controller.refresh();
        assert.deepEqual(host.specs().map((spec:any)=>spec.service.project).sort(),registrations.map(row=>row.binding).sort(),
            'a failed positive refresh falls back to complete UI specs without a stale exclusion');
        exactFailure=false;await controller.observeSelected(binding);
        stopped=true;await controller.refresh();
        assert.deepEqual(host.specs().map((spec:any)=>spec.service.project).sort(),registrations.map(row=>row.binding).sort(),
            'Stop removes early eligibility without hiding complete saved UI declarations');
        stopped=false;
        selectedOutcome='removed';await controller.observeSelected(binding);
        assert.ok(!host.specs().some((spec:any)=>spec.root===canonical),'native removed scope cannot remain visible through stale complete UI specs');
        assert.equal(JSON.stringify(controller.specs),configured,'UI removal does not rewrite configured MCP scopes');
        exactFailure=true;await controller.refresh();
        assert.ok(!host.specs().some((spec:any)=>spec.root===canonical),'transient failure cannot erase an explicit native negative fence');
        exactFailure=false;
        selectedOutcome='registered';selectedProject='56'.repeat(32);await controller.observeSelected(binding);
        assert.deepEqual(host.specs().filter((spec:any)=>spec.root===canonical).map((spec:any)=>spec.service.project),[selectedProject]);
        assert.equal(JSON.stringify(controller.specs),configured);assert.equal(definitionsChanged,events);

        stub.workspace.isTrusted=false;assert.deepEqual(await api.getServerDefinitions(),[]);
    }finally{release?.();extension?.deactivate();loader._load=original;delete require.cache[require.resolve('../serviceExtension')];await fs.rm(root,{recursive:true,force:true});}
});
