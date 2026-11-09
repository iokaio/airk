// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import * as fs from 'node:fs/promises';
import * as path from 'node:path';
import { serviceTestHost } from './serviceTestHost';
import * as cliInstall from '../cliInstall';
import { assistanceEntry, retrievalEntry, unifiedEntry, serializeServers, launchArgs, type LaunchSpec } from '../model';
import { launchDescriptorPath } from '../explorerAssistanceLaunch';
import { runText } from '../native';
import * as native from '../native';
import { NativeExecutionDisabled, setNativeExecutionGate } from '../nativeGate';
import { NativeControlTransport, ServiceError } from '../serviceClient';
import { McpStdioSession } from '../mcpStdioSession';
import { MANAGE_WORKSPACE_TRUST_COMMAND, NATIVE_COMMANDS, NATIVE_COMMAND_ENABLEMENT } from '../explorerCommands';
import { catalogExport } from '../indexingCatalog';
import {RuntimeHost} from '../serviceBootstrap';
import {createHash} from 'node:crypto';
import {AssistanceIpcHost} from '../explorerAssistanceHost';

function definitionValue(value:any){return {label:value.label,url:value.uri.toString(),headers:value.headers,version:value.version};}
test('verified reopen migrates both old definitions quietly before publishing the connection',async context=>{
    const f=await serviceTestHost(context,{automatic:true});
    try{
        const project=[...f.registrations.values()][0];
        assert.ok(f.binding.launcher,'historical setup fixture has its explicit predecessor launcher');
        const spec:LaunchSpec={command:f.binding.launcher,root:project.root,dataDir:project.data_root,collection:project.collection,name:'fixture',service:{installDir:f.binding.installDir,project:project.binding}};
        const storage=path.join(f.root,'editor-storage');
        const before=serializeServers('claude',[retrievalEntry(spec),assistanceEntry(spec,launchDescriptorPath(storage))!]);
        const destination=path.join(project.root,'.mcp.json');await fs.writeFile(destination,before);
        let saves=0;
        const document={uri:{fsPath:destination},isDirty:false,version:1,text:before,getText(){return this.text;},positionAt:(offset:number)=>offset,
            async save(){saves++;await fs.writeFile(destination,this.text);return true;}};
        f.stub.workspace.openTextDocument=async()=>document;
        f.stub.workspace.applyEdit=async(edit:any)=>{document.text=edit.after;document.version++;return true;};
        const progress=f.progressRuns.length,writes=f.writes.length;
        await f.refreshFolders();
        const after=JSON.parse(await fs.readFile(destination,'utf8'));
        assert.deepEqual(Object.keys(after.mcpServers),['airk']);assert.equal(saves,1);
        assert.equal(f.progressRuns.length,progress,'completed setup remains quiet');
        assert.equal(f.writes.length,writes,'connection migration never queues another index');
        await f.refreshFolders();assert.equal(saves,1,'the next reopen does not rewrite the file');
    }finally{await f.close();}
});

test('actual startup and missing-index commands never register open folders or infer storage',async context=>{
    const f=await serviceTestHost(context,{automatic:true,registered:false}),controller=f.context.subscriptions.find((item:any)=>typeof item.prepareWindowContext==='function');
    try{
        assert.equal(f.registrations.size,0);assert.equal(f.writes.some(value=>value.reserve_project),false);
        await f.refreshFolders();await controller.chooseSpec();
        assert.equal(f.registrations.size,0);assert.equal(f.writes.some(value=>value.reserve_project),false);assert.ok(f.calls.some(line=>line.includes('Indexing → Index library')));
        await f.commands.get('airk.index')!();const panel=f.panels.find((panel:any)=>panel.viewType==='airk.explorer');assert.ok(panel);await panel.send({v:1,type:'ready',requestId:'missing-index-ready'});
        const navigate=f.panels.flatMap((panel:any)=>panel.posted).filter((message:any)=>message.type==='navigate');assert.equal(navigate.at(-1)?.route,'#indexing/library','missing Index Now leaves the explicit setup form visible after the real ready handshake');
        assert.equal(f.values.has('airk.setupApprovedRoots.1'),false);assert.equal(f.values.has('airk.setupDeclinedFolders.1'),false);
    }finally{await f.close();}
});

test('automatic legacy MCP definitions require full positive native eligibility and never treat failures as absence',async context=>{
    const f=await serviceTestHost(context),controller=f.context.subscriptions.find((item:any)=>typeof item.prepareWindowContext==='function');
    try{
        const registration=[...f.registrations.values()][0],writes=f.writes.length;
        const positive=await controller.provideDefinitions();assert.equal(positive.length,1);assert.deepEqual(f.state.legacyBindingProjects,[registration.binding,registration.binding]);assert.equal(f.writes.length,writes);
        if(process.platform==='win32'){
            const root=registration.root;registration.root=path.toNamespacedPath(root);const canonical=await controller.provideDefinitions();assert.equal(canonical[0].uri.toString(),positive[0].uri.toString());const native=f.state.httpConnections.at(-1)!.selection;assert.equal(native.kind,'imported_legacy');if(native.kind==='imported_legacy')assert.equal(native.selection.registration.root,registration.root,'native canonical spelling remains in enrolled authority');registration.root=root;
        }
        f.state.legacyBindingReply=project=>({status:'catalog_legacy_binding',observation:{schema:1,namespace:'34'.repeat(16),catalog_revision:1,project,eligibility:{kind:'unavailable'}}});
        assert.deepEqual(await controller.provideDefinitions(),[]);
        f.state.pick=items=>items.find(item=>item.client==='codex')??items[0];const panels=f.panels.length;await assert.rejects(f.commands.get('airk.setupAgent')!(),/legacy_connection_unavailable_use_reviewed_grant/);assert.equal(f.panels.length,panels);await assert.rejects(fs.stat(path.join(f.folders[0].uri.fsPath,'.codex','config.toml')),{code:'ENOENT'});
        f.state.legacyBindingReply=project=>({status:'catalog_legacy_binding',observation:{schema:1,namespace:'34'.repeat(16),catalog_revision:null,project,eligibility:{kind:'authority_zero',registration:{...registration,data_root:path.join(f.root,'crossed-storage')}}}});
        await assert.rejects(controller.provideDefinitions(),/crossed_native_legacy_registration/);
        f.state.legacyBindingReply=()=>({status:'failure',code:'registry_busy'});await assert.rejects(controller.provideDefinitions(),(error:unknown)=>{assert.ok(error instanceof ServiceError);assert.equal(error.code,'registry_busy');return true;});
        const reads=f.state.legacyBindingProjects.length;f.values.set('airk.scope.explicitWithdrawal.1',{schema:9});assert.deepEqual(await controller.provideDefinitions(),[]);assert.equal(f.state.legacyBindingProjects.length,reads,'withdrawal prevents legacy lookup even after a registry change');assert.equal(f.writes.length,writes);
    }finally{await f.close();}
});

test('missing Explorer assets fail explicitly and a later open retries after repair',async context=>{
    const f=await serviceTestHost(context,{trusted:false});
    try{
        const installed=f.context.extensionUri;
        f.context.extensionUri={scheme:'file',fsPath:path.join(f.root,'missing-explorer')};
        await assert.rejects(f.commands.get('airk.explorer.open')!(),/ENOENT/);
        assert.equal(f.panels.length,0);
        f.context.extensionUri=installed;
        await f.commands.get('airk.explorer.open')!();
        assert.equal(f.panels.length,1,'repair permits another load, including without workspace trust');
        assert.equal(f.writes.length,0,'opening Explorer does not start native indexing');
    }finally{await f.close();}
});

test('Install or Repair CLI migrates all three existing local agent connections through guarded editor saves',async context=>{
    const f=await serviceTestHost(context,{assistance:true});
    try{
        context.mock.method(cliInstall,'installLauncherCli',async()=>['Old CLI is locked; disconnect clients and retry cleanup.']);
        const project=[...f.registrations.values()][0],name=`airk-${project.collection}`;
        const command=path.join(f.root,'editor-storage','cli','bin',process.platform==='win32'?'airk.exe':'airk');
        const args=['--root',project.root,'--data-dir',project.data_root,'--collection',project.collection,'mcp'];
        const files=[path.join(project.root,'.codex','config.toml'),path.join(project.root,'.mcp.json'),path.join(project.root,'.vscode','mcp.json')];
        for(const file of files)await fs.mkdir(path.dirname(file),{recursive:true});
        await fs.writeFile(files[0],`[mcp_servers.${name}]\ncommand = ${JSON.stringify(command)}\nargs = ${JSON.stringify(args)}\n`);
        await fs.writeFile(files[1],JSON.stringify({mcpServers:{[name]:{command,args,disabled:true}}}));
        await fs.writeFile(files[2],JSON.stringify({servers:{[name]:{command,args,type:'stdio'}}}));
        const documents=new Map<string,any>();
        f.stub.workspace.openTextDocument=async(uri:any)=>{
            const document={uri,isDirty:false,version:1,text:await fs.readFile(uri.fsPath,'utf8'),getText(){return this.text;},positionAt:(offset:number)=>offset,
                async save(){await fs.writeFile(uri.fsPath,this.text);this.isDirty=false;return true;}};
            documents.set(uri.fsPath,document);return document;
        };
        f.stub.workspace.applyEdit=async(edit:any)=>{const document=documents.get(edit.uri.fsPath);document.text=edit.after;document.version++;return true;};
        await f.commands.get('airk.installCli')!();
        for(const file of files){const text=await fs.readFile(file,'utf8');assert.ok(!text.includes('--service-project'),f.calls.join('\n'));assert.ok(text.includes(f.httpUrl));assert.ok(!text.includes('airk_explorer')&&!text.includes('--explorer-descriptor')&&!text.includes('runtime\"'),'installation replaces legacy launches with one HTTP endpoint');}
        assert.equal(JSON.parse(await fs.readFile(files[1],'utf8')).mcpServers.airk.disabled,true);
        assert.equal(f.calls.filter(value=>value.startsWith('Updated ')).length,3);
        assert.ok(f.calls.some(value=>value.includes('Old CLI is locked')),'the underlying cleanup cause remains in the output');
        assert.ok(f.calls.some(value=>value.includes('AIRK upgrade did not complete')),'Locked cleanup is an incomplete upgrade, not installed-with-warning success');
        assert.equal([...f.values.keys()].some(key=>key.startsWith('airk.installComplete.2.')),false);
        assert.equal(f.notices.some(value=>value.message.includes('ready.')),false);
        // Unsaved content must block an installer rewrite.
        const before=`[mcp_servers.${name}]\ncommand = ${JSON.stringify(command)}\nargs = ${JSON.stringify(args)}\n`;
        await fs.writeFile(files[0],before);
        f.stub.workspace.textDocuments=[{uri:{fsPath:files[0]},isDirty:true}];
        await f.commands.get('airk.installCli')!();
        assert.equal(await fs.readFile(files[0],'utf8'),before);
        assert.ok(f.calls.some(value=>value.includes('unsaved edits')));
    }finally{await f.close();}
});

test('startup connection migration respects Windows document line endings for compact JSON',async context=>{
    const f=await serviceTestHost(context);
    try{
        const registration=[...f.registrations.values()][0],key=`airk-${registration.collection}`;
        const command=path.join(f.root,'editor-storage','cli','bin',process.platform==='win32'?'airk.exe':'airk');
        const args=['--root',registration.root,'--data-dir',registration.data_root,'--collection',registration.collection,'mcp'];
        const destination=path.join(registration.root,'.mcp.json');
        await fs.writeFile(destination,JSON.stringify({mcpServers:{[key]:{command,args,disabled:true},other:{command:'keep-other'}}}));
        let doc:any;
        f.stub.workspace.openTextDocument=async(uri:any)=>doc={uri,eol:2,isDirty:false,version:1,text:await fs.readFile(uri.fsPath,'utf8'),getText(){return this.text;},positionAt:(offset:number)=>offset,
            async save(){await fs.writeFile(uri.fsPath,this.text);this.isDirty=false;return true;}};
        f.stub.workspace.applyEdit=async(edit:any)=>{doc.text=edit.after.replace(/\r?\n/g,'\r\n');doc.version++;doc.isDirty=true;return true;};
        f.settings.set('cli.autoInstall',true);await f.refreshFolders();
        const saved=await fs.readFile(destination,'utf8'),servers=JSON.parse(saved).mcpServers;
        assert.equal(servers[key],undefined);assert.equal(servers.airk.disabled,true);assert.equal(servers.other.command,'keep-other');
        assert.ok(saved.includes('\r\n'));assert.equal(f.calls.some(line=>line.includes('Could not update claude')),false);
    }finally{await f.close();}
});

// 1.4 has no window scheduler or direct-helper fallback. Native continuation,
// contention and worker failures are qualified by the native service workloads.
test('an unavailable service fails explicitly without a helper or window scheduler fallback',async context=>{
    const f=await serviceTestHost(context);
    try{f.state.down=true;await assert.rejects(f.commands.get('airk.index')!(),/service_unavailable/);assert.equal(f.writes.length,0);
        f.state.down=false;await f.commands.get('airk.index')!();assert.equal(f.writes.filter(value=>value.enqueue).length,1);
    }finally{await f.close();}
});

test('opening storage setup preserves the active native binding and preference until native review is applied',async context=>{
    const f=await serviceTestHost(context);
    try{const before=await f.api.getServerDefinitions(),storage=f.settings.get('storage.path');await f.commands.get('airk.chooseStorage')!();assert.equal(f.writes.length,0);
        const selected=path.join(f.root,'new-data');await fs.mkdir(selected);f.state.selectedFolders=[{scheme:'file',fsPath:selected}];f.state.confirm=false;
        await f.commands.get('airk.chooseStorage')!();assert.equal(f.settings.get('storage.path'),storage);
        const after=await f.api.getServerDefinitions();assert.deepEqual(after.map(definitionValue),before.map(definitionValue));assert.equal(f.writes.length,0);
    }finally{await f.close();}
});

test('untrusted workspaces expose no MCP servers and cannot discover, start or mutate native work',async context=>{
    const f=await serviceTestHost(context,{trusted:false});
    try{assert.deepEqual(await f.api.getServerDefinitions(),[]);assert.equal(f.state.discovered,0);
        for(const name of ['airk.index','airk.reindex','airk.openTools','airk.installCli','airk.setupAgent'])await assert.rejects(f.commands.get(name)!(),/workspace_untrusted/);
        assert.equal(f.state.discovered,0);assert.equal(f.state.ensured,0);assert.equal(f.writes.length,0);
    }finally{await f.close();}
});

test('Restricted Mode keeps trust/navigation/log actions available while every classified native handler refuses',async context=>{
    const f=await serviceTestHost(context,{trusted:false});
    try{
        assert.equal(f.commandContexts.get('airk.nativeCommandsAvailable'),false);
        for(const name of NATIVE_COMMANDS){assert.ok(f.commands.has(`airk.${name}`),name);await assert.rejects(f.commands.get(`airk.${name}`)!(),/workspace_untrusted/);}
        await f.commands.get('airk.manageWorkspaceTrust')!();assert.ok(f.calls.includes(MANAGE_WORKSPACE_TRUST_COMMAND));
        await assert.rejects(f.commands.get('airk.manageWorkspaceTrust')!({url:'untrusted'}),/invalid_manage_workspace_trust_arguments/);
        await f.commands.get('airk.showExtensionOutput')!();await f.commands.get('airk.menu')!();
        assert.equal(f.state.discovered,0);assert.equal(f.state.ensured,0);assert.deepEqual(f.writes,[]);assert.deepEqual(f.state.hostContextKeys,[]);
        const controller=f.context.subscriptions.find((item:any)=>typeof item.prepareWindowContext==='function');
        f.grantTrust();await f.commands.get('airk.treeRefresh')!();
        assert.equal(f.commandContexts.get('airk.nativeCommandsAvailable'),true);
        await contextReadStarted(f.state.hostContextKeys);
        const deadline=Date.now()+2000;while(!controller.explorerHost().windowContextObservation()&&Date.now()<deadline)await new Promise(resolve=>setTimeout(resolve,1));
        const observed=controller.explorerHost().windowContextObservation();assert.equal(observed?.status,'catalog_host_context','trusted connection automatically reads capability metadata');
        assert.equal(observed.descriptor.context,null,'metadata observation creates no saved Project association');
        assert.equal(controller.currentCatalogScope(),undefined,'trust and metadata reads do not select a grant');
        assert.deepEqual(f.writes,[],'automatic capability observation starts no indexing, configuration or lifecycle mutations');
        controller.dispose();await assert.rejects(f.commands.get('airk.index')!(),/runtime_client_no_longer_authorized/);
        await assert.rejects(f.commands.get('airk.manageWorkspaceTrust')!(),/runtime_client_no_longer_authorized/);
        assert.equal(f.commandContexts.get('airk.nativeCommandsAvailable'),false);
    }finally{await f.close();}
});

test('native command manifest enablement and palette visibility match the registered classification',async()=>{
    const manifest=JSON.parse(await fs.readFile(path.resolve(__dirname,'../../package.json'),'utf8'));
    const commands=new Map<string,any>(manifest.contributes.commands.map((value:any)=>[value.command,value]));
    const palette=new Map<string,any>(manifest.contributes.menus.commandPalette.map((value:any)=>[value.command,value]));
    for(const name of NATIVE_COMMANDS){assert.equal(commands.get(`airk.${name}`)?.enablement,NATIVE_COMMAND_ENABLEMENT,name);assert.ok([NATIVE_COMMAND_ENABLEMENT,'false'].includes(palette.get(`airk.${name}`)?.when),name);}
    for(const name of ['explorer.open','menu','logs','serviceLogs','showExtensionOutput','manageWorkspaceTrust'])assert.equal(commands.get(`airk.${name}`)?.enablement,undefined,name);
});

async function contextReadStarted(keys:string[],count=1):Promise<void>{
    const deadline=Date.now()+2000;while(keys.length<count&&Date.now()<deadline)await new Promise(resolve=>setTimeout(resolve,1));
    assert.equal(keys.length,count,'explicit context read reached the existing control client');
}

test('HTTP enrollment binds the current real Assistance attachment across reload without changing grant authority',async context=>{
    const f=await serviceTestHost(context,{registered:false}),controller=f.context.subscriptions.find((item:any)=>typeof item.prepareWindowContext==='function');
    const nativeRecords=new Map<string,{descriptor:string|undefined;client:string}>(),original=RuntimeHost.prototype.httpConnection;
    let endpointGeneration=1,capacity=128,nativeCalls=0,held:Promise<void>|undefined;
    context.mock.method(RuntimeHost.prototype,'httpConnection',async function(this:RuntimeHost,...args:Parameters<RuntimeHost['httpConnection']>){
        const id=args[3],client=args[4],descriptor=args[5],previous=nativeRecords.get(id);
        nativeCalls++;
        if(previous&&(previous.descriptor!==descriptor||previous.client!==client))throw new ServiceError('http_enrollment_refused',false,{status:'failure',code:'http_enrollment_refused'});
        if(!previous&&nativeRecords.size>=capacity)throw new ServiceError('http_enrollment_refused',false,{status:'failure',code:'http_enrollment_refused'});
        nativeRecords.set(id,{descriptor,client});const info=await original.apply(this,args);info.endpoint.generation=endpointGeneration;if(held)await held;return info;
    });
    try{
        await controller.refresh();await controller.console.controller.refresh();
        const session={schema:1 as const,namespace:'34'.repeat(16),context_key:'56'.repeat(32),expected_catalog_revision:1,grant_id:'grt_'+'11'.repeat(16),expected_grant_revision:1,project_ids:['prj_'+'22'.repeat(16)]};
        const spec:LaunchSpec={command:f.binding.executable,root:f.folders[0].uri.fsPath,dataDir:path.join(f.root,'data'),collection:'fixture',name:'Saved grant',service:{installDir:f.binding.installDir,project:'77'.repeat(32)},combinedSession:session};
        f.settings.set('explorer.assistance.enabled',true);await controller.startAssistance();
        const firstHost=controller.assistanceHost;assert.ok(firstHost?.descriptorPath,'Real first Assistance host required');
        const before=f.state.httpConnections.length;
        const [first,coalesced]=await Promise.all([controller.enrollHttpSpec(spec,'vscode'),controller.enrollHttpSpec(spec,'vscode')]);
        assert.equal(first,coalesced);assert.equal(f.state.httpConnections.length,before+1,'Concurrent enrollment coalesces for the exact attachment');
        const oldId=first.http.connection_id,firstDescriptor=firstHost.descriptorPath;
        assert.equal(nativeRecords.get(oldId)?.descriptor,firstDescriptor);
        const savedKeys=[...f.values.keys()].filter(key=>key.startsWith('airk.http.connection.1.'));assert.equal(savedKeys.length,1);
        await controller.startAssistance();assert.notEqual(controller.assistanceHost,firstHost);
        const replacementDescriptor=controller.assistanceHost?.descriptorPath;assert.ok(replacementDescriptor);assert.notEqual(replacementDescriptor,firstDescriptor);
        assert.ok(f.values.has(savedKeys[0]),'The old saved connection identity survives the recreated window host');
        const restored=await controller.enrollHttpSpec(spec,'vscode');
        assert.notEqual(restored.http.connection_id,oldId,'New attachment requires a distinct native credential identity');
        assert.deepEqual(restored.http.selection,first.http.selection,'Reload never creates or widens the grant');
        assert.equal(nativeRecords.get(oldId)?.descriptor,firstDescriptor,'Old credential attachment stays untouched');
        assert.equal(nativeRecords.get(restored.http.connection_id)?.descriptor,replacementDescriptor);
        endpointGeneration=2;
        const advanced={...spec,combinedSession:{...session,expected_catalog_revision:2}};
        const refreshed=await controller.enrollHttpSpec(advanced,'vscode');
        assert.equal(refreshed.http.connection_id,restored.http.connection_id,'Same attachment retains its identity across clock and endpoint changes');
        assert.equal(refreshed.http.endpoint.generation,2);
        f.settings.set('explorer.assistance.enabled',false);
        const headless=await controller.enrollHttpSpec(spec,'vscode');
        assert.notEqual(headless.http.connection_id,restored.http.connection_id);
        assert.equal(nativeRecords.get(headless.http.connection_id)?.descriptor,undefined);
        const {expected_catalog_revision:_clock,...headlessIdentity}=session;
        const legacyKey=createHash('sha256').update(JSON.stringify(['vscode','combined',headlessIdentity,f.binding.installDir])).digest('hex');
        assert.equal(f.values.get('airk.http.connection.1.'+legacyKey),headless.http.connection_id,'None attachment retains the original four-part saved identity');
        assert.equal((await controller.enrollHttpSpec(spec,'vscode')).http.connection_id,headless.http.connection_id);
        f.settings.set('explorer.assistance.enabled',true);
        assert.equal((await controller.enrollHttpSpec(spec,'vscode')).http.connection_id,restored.http.connection_id,'Returning to the same Some attachment reuses its own identity');
        capacity=nativeRecords.size;const priorCalls=nativeCalls;
        await assert.rejects(controller.enrollHttpSpec(spec,'explorer'),/http_enrollment_refused/);assert.equal(nativeCalls,priorCalls+1,'Native capacity refusal is never retried');capacity=128;
        let release!:()=>void;held=new Promise<void>(resolve=>{release=resolve;});
        const stale=controller.enrollHttpSpec(spec,'workbench'),staleOutcome=assert.rejects(stale,/workspace_registration_changed/);
        const until=Date.now()+2000;while(![...nativeRecords.values()].some(row=>row.client==='workbench')&&Date.now()<until)await new Promise(resolve=>setTimeout(resolve,1));
        assert.ok([...nativeRecords.values()].some(row=>row.client==='workbench'),'In-flight native enrollment reached the captured attachment');
        await controller.startAssistance();release();await staleOutcome;held=undefined;
        assert.equal([...nativeRecords.values()].find(row=>row.client==='workbench')?.descriptor,replacementDescriptor,'An in-flight request never switches to the replacement attachment');
        assert.equal(f.writes.length,0,'Attachment replacement never submits a new catalog grant or indexing mutation');
    }finally{await f.close();}
});

test('fresh enrollment awaits deferred Assistance publication and notifies definitions only after its attachment is ready',async context=>{
    const f=await serviceTestHost(context,{registered:false}),controller=f.context.subscriptions.find((item:any)=>typeof item.prepareWindowContext==='function');
    const original=AssistanceIpcHost.start;let release!:()=>void,reached!:()=>void;
    const gate=new Promise<void>(resolve=>{release=resolve;}),started=new Promise<void>(resolve=>{reached=resolve;});
    const startup=context.mock.method(AssistanceIpcHost,'start',async (port:Parameters<typeof AssistanceIpcHost.start>[0])=>{reached();await gate;return original.call(AssistanceIpcHost,port);});
    let published=0;const subscription=controller.assistanceDefinitions.event(()=>{published++;});
    try{
        await controller.refresh();await controller.console.controller.refresh();published=0;
        await controller.startAssistance();assert.equal(published,0,'An unchanged headless attachment emits no premature MCP definition event');
        f.settings.set('explorer.assistance.enabled',true);
        const attaching=controller.startAssistance();await started;
        const spec:LaunchSpec={command:f.binding.executable,root:f.folders[0].uri.fsPath,dataDir:path.join(f.root,'data'),collection:'fixture',name:'Saved grant',service:{installDir:f.binding.installDir,project:'77'.repeat(32)},combinedSession:{schema:1,namespace:'34'.repeat(16),context_key:'56'.repeat(32),expected_catalog_revision:1,grant_id:'grt_'+'11'.repeat(16),expected_grant_revision:1,project_ids:['prj_'+'22'.repeat(16)]}};
        const outcome=controller.enrollHttpSpec(spec,'vscode').then((result:any)=>({result}),(error:unknown)=>({error}));
        assert.equal(published,0,'A pending host cannot publish a completed Assistance attachment notification');
        release();await attaching;const settled=await outcome;
        assert.equal(settled.error,undefined);assert.ok(controller.assistanceHost?.descriptorPath);
        assert.equal(f.state.httpConnections.length,1);assert.equal(f.state.httpConnections[0].descriptor,controller.assistanceHost.descriptorPath,'No temporary headless enrollment escapes during setup');
        assert.equal(settled.result.http.connection_id,f.state.httpConnections[0].id);assert.equal(published,1);
        startup.mock.mockImplementation(async()=>{throw new Error('fixture optional Assistance endpoint unavailable');});
        await controller.startAssistance();assert.equal(controller.assistanceHost,undefined);assert.equal(published,2,'Completed attachment removal emits one withdrawal notification');
        const headless=await controller.enrollHttpSpec(spec,'vscode');
        assert.ok(headless.http);assert.equal(f.state.httpConnections.at(-1)?.descriptor,undefined,'Completed optional setup failure retains supported headless enrollment');
        let releaseOld!:()=>void,reachedOld!:()=>void;
        const oldGate=new Promise<void>(resolve=>{releaseOld=resolve;}),oldStarted=new Promise<void>(resolve=>{reachedOld=resolve;});let setups=0;
        startup.mock.mockImplementation(async (port:Parameters<typeof AssistanceIpcHost.start>[0])=>{if(++setups===1){reachedOld();await oldGate;return undefined;}return original.call(AssistanceIpcHost,port);});
        const obsoleteSetup=controller.startAssistance();await oldStarted;
        const beforeReplacement=f.state.httpConnections.length;
        const obsoleteEnrollment=controller.enrollHttpSpec(spec,'vscode'),refused=assert.rejects(obsoleteEnrollment,/workspace_registration_changed/);
        try{await controller.startAssistance();}finally{releaseOld();}
        await obsoleteSetup;await refused;
        assert.equal(f.state.httpConnections.length,beforeReplacement,'A setup replacement cancels its waiter before any native enrollment');
    }finally{release();subscription.dispose();await f.close();}
});

test('Assistance withdrawal notifies once across superseded pending setup',async context=>{
    const f=await serviceTestHost(context,{registered:false}),controller=f.context.subscriptions.find((item:any)=>typeof item.prepareWindowContext==='function');
    const original=AssistanceIpcHost.start;let hold=false,release!:()=>void,reached!:()=>void;
    const gate=new Promise<void>(resolve=>{release=resolve;}),started=new Promise<void>(resolve=>{reached=resolve;});
    context.mock.method(AssistanceIpcHost,'start',async(port:Parameters<typeof AssistanceIpcHost.start>[0])=>{if(hold){reached();await gate;}return original.call(AssistanceIpcHost,port);});
    let obsolete:Promise<void>|undefined,assistanceChanges=0,definitionChanges=0,awaitingDefinition=false;const subscriptions:{dispose():void}[]=[];
    try{
        await controller.refresh();await controller.console.controller.refresh();await controller.prepareWindowContext();
        subscriptions.push(controller.assistanceDefinitions.event(()=>{assistanceChanges++;awaitingDefinition=true;}),controller.definitions.event(()=>{if(awaitingDefinition){definitionChanges++;awaitingDefinition=false;}}));
        f.settings.set('explorer.assistance.enabled',true);await controller.startAssistance();assert.ok(controller.assistanceHost?.descriptorPath);
        assert.deepEqual([assistanceChanges,definitionChanges],[1,1]);
        hold=true;obsolete=controller.startAssistance();await started;assert.deepEqual([assistanceChanges,definitionChanges],[1,1]);
        f.settings.set('explorer.assistance.enabled',false);await controller.startAssistance();assert.equal(controller.assistanceHost,undefined);
        assert.deepEqual([assistanceChanges,definitionChanges],[2,2],'Published attachment withdrawal survives pending replacements');
        release();await obsolete;assert.deepEqual([assistanceChanges,definitionChanges],[2,2],'Obsolete completion cannot notify or restore an attachment');
        await controller.startAssistance();assert.deepEqual([assistanceChanges,definitionChanges],[2,2],'None to None remains quiet');
    }finally{release();await obsolete;for(const subscription of subscriptions)subscription.dispose();await f.close();}
});

test('legacy single and union enrollment retain headless keys and preserve prior manual credential ownership across attachments',async context=>{
    const f=await serviceTestHost(context,{roots:2}),controller=f.context.subscriptions.find((item:any)=>typeof item.prepareWindowContext==='function');
    const records=new Map<string,string|undefined>(),legacy=RuntimeHost.prototype.httpLegacyConnection,union=RuntimeHost.prototype.httpLegacyUnionConnection;
    const accepted=(id:string,descriptor:string|undefined)=>{if(records.has(id)&&records.get(id)!==descriptor)throw new ServiceError('http_enrollment_refused');records.set(id,descriptor);};
    context.mock.method(RuntimeHost.prototype,'httpLegacyConnection',async function(this:RuntimeHost,...args:Parameters<RuntimeHost['httpLegacyConnection']>){accepted(args[4],args[6]);return legacy.apply(this,args);});
    context.mock.method(RuntimeHost.prototype,'httpLegacyUnionConnection',async function(this:RuntimeHost,...args:Parameters<RuntimeHost['httpLegacyUnionConnection']>){accepted(args[3],args[5]);return union.apply(this,args);});
    try{
        await controller.refresh();await controller.console.controller.refresh();const specs=controller.specs as LaunchSpec[];assert.equal(specs.length,2);
        const spec=specs[0],registration={binding:spec.service!.project,root:spec.root,data_root:spec.dataDir,collection:spec.collection},receipt='ab'.repeat(32);
        const headless=await controller.enrollLegacyHttpSpec(spec,'vscode',receipt);
        const base=createHash('sha256').update(JSON.stringify(['vscode',f.binding.installDir,registration])).digest('hex');
        assert.equal(f.values.get('airk.http.legacy.id.1.'+base),headless.http.connection_id);
        const originalHeadless=structuredClone(f.values.get('airk.http.legacy.1.'+base));
        const unionHeadless=await controller.enrollLegacyUnion(specs,f.binding,()=>{});
        f.settings.set('explorer.assistance.enabled',true);await controller.startAssistance();
        const first=await controller.enrollLegacyHttpSpec(spec,'vscode','cd'.repeat(32)),firstUnion=await controller.enrollLegacyUnion(specs,f.binding,()=>{});
        assert.notEqual(first.http.connection_id,headless.http.connection_id);assert.notEqual(firstUnion.http.connection_id,unionHeadless.http.connection_id);
        assert.equal((await controller.enrollLegacyHttpSpec(spec,'vscode',receipt)).http.connection_id,first.http.connection_id);
        const source=serializeServers('vscode',[unifiedEntry({...spec,http:first.http})]);
        await controller.startAssistance();
        const second=await controller.enrollLegacyHttpSpec(spec,'vscode',receipt),secondUnion=await controller.enrollLegacyUnion(specs,f.binding,()=>{});
        assert.notEqual(second.http.connection_id,first.http.connection_id);assert.notEqual(secondUnion.http.connection_id,firstUnion.http.connection_id);
        assert.equal((await controller.previousLegacyHttp(spec,'vscode',f.binding,new Set([first.http.connection_id]))).connection_id,first.http.connection_id,'Source-aware ownership finds the exact prior-attachment credential');
        const owned=await controller.migrationCandidate(spec,'copilot',source);
        assert.equal(owned.candidate.http.connection_id,first.http.connection_id);assert.equal(owned.old.http.connection_id,first.http.connection_id);
        assert.deepEqual(f.values.get('airk.http.legacy.1.'+base),originalHeadless,'Original headless credential/provenance stays untouched');
        assert.deepEqual(second.http.selection,first.http.selection,'Attachment changes retain existing imported native authority');
        assert.equal(first.http.selection.selection.configuration_receipt,receipt,'Fresh attachment does not replace original configuration provenance with a newer text digest');
        f.settings.set('explorer.assistance.enabled',false);
        assert.equal((await controller.enrollLegacyHttpSpec(spec,'vscode',receipt)).http.connection_id,headless.http.connection_id);
        assert.equal((await controller.enrollLegacyUnion(specs,f.binding,()=>{})).http.connection_id,unionHeadless.http.connection_id);
        for(let i=0;i<129;i++)f.values.set('airk.http.legacy.1.'+base+'.'+String(i).padStart(64,'0'),originalHeadless);
        context.mock.method(f.context.globalState,'keys',()=>{throw new Error('Attachment history must not be enumerated');});
        assert.equal((await controller.previousLegacyHttp(spec,'vscode',f.binding)).connection_id,headless.http.connection_id,'One direct origin record preserves original provenance regardless of attachment history');
        f.settings.set('explorer.assistance.enabled',true);await controller.startAssistance();
        const later=await controller.enrollLegacyHttpSpec(spec,'vscode','ef'.repeat(32));
        assert.equal(later.http.selection.selection.configuration_receipt,receipt,'A later attachment retains the original import proof beyond 128 historical keys');
        assert.notEqual(later.http.connection_id,second.http.connection_id);
        assert.equal((await controller.previousLegacyHttp(spec,'vscode',f.binding,new Set([first.http.connection_id]))).connection_id,first.http.connection_id,'Exact manually referenced older connection remains directly addressable');
        // Old extension state has no per-ID index. Native ownership can resolve
        // only the exact IDs present in the inspected configuration; arbitrary
        // old attachment values remain untouched and are never scanned.
        f.values.delete('airk.http.legacy.origin.1.'+base);
        f.values.delete('airk.http.legacy.connection.1.'+base+'.'+first.http.connection_id);
        const lookedUp:string[]=[],lookup=RuntimeHost.prototype.lookupHttpConnection,revoked='fc'.repeat(16),unknown='fd'.repeat(16);
        // Restore the fixture's existing mock directly, without stacking a
        // second mock of that same method in Node's restoration queue.
        RuntimeHost.prototype.lookupHttpConnection=async function(this:RuntimeHost,binding:any,id:string){lookedUp.push(id);if(id===revoked)throw new ServiceError('owned_connection_revoked');return lookup.call(this,binding,id);};
        try{
            const oldState=await controller.migrationCandidate(spec,'copilot',source);
            assert.equal(oldState.candidate.http.connection_id,first.http.connection_id);
            assert.deepEqual(lookedUp,[first.http.connection_id],'Pre-index history over 128 records resolves the actual configuration ID natively');
            assert.equal(oldState.candidate.http.selection.selection.configuration_receipt,receipt);
            await assert.rejects(controller.previousLegacyHttp(spec,'vscode',f.binding,new Set([revoked])),/owned_connection_revoked/);
            assert.equal(await controller.previousLegacyHttp(spec,'vscode',f.binding,new Set([unknown])),undefined,'Unowned manual configuration is preserved without inventing an import proof');
        }finally{RuntimeHost.prototype.lookupHttpConnection=lookup;}
        let releaseLookup!:(value:any)=>void,lookupStarted!:()=>void;
        const lookupEntered=new Promise<void>(resolve=>{lookupStarted=resolve;}),heldLookup=new Promise<any>(resolve=>{releaseLookup=resolve;}),stale='fa'.repeat(16);
        RuntimeHost.prototype.lookupHttpConnection=async function(this:RuntimeHost,binding:any,id:string){if(id===stale){lookupStarted();return heldLookup;}return lookup.call(this,binding,id);};
        const generation=controller.generation,enrollments=f.state.httpConnections.length,writes=f.writes.length,metadata=new Map(f.values);
        try{
            const migrating=controller.prepareOwnedHttpMigration(spec,'copilot',source.replaceAll(first.http.connection_id,stale));
            await lookupEntered;controller.generation++;
            releaseLookup({schema:1,active:f.binding.runtimeSha256,connection_id:stale,status:'owned',connection:{...first.http,connection_id:stale,credential_file:path.join(f.binding.controlDir,'http-1',`credential-${stale}.token`)}});
            await assert.rejects(migrating,/workspace_registration_changed/);
            assert.equal(f.state.httpConnections.length,enrollments,'A stale asynchronous lookup cannot enroll a replacement credential');
            assert.equal(f.writes.length,writes);assert.deepEqual(f.values,metadata,'A stale lookup cannot save provenance or configuration metadata');
        }finally{controller.generation=generation;RuntimeHost.prototype.lookupHttpConnection=lookup;}
        assert.equal(f.values.get('airk.http.legacy.1.'+base+'.'+String(128).padStart(64,'0')),originalHeadless,'Unknown historical metadata stays untouched');
        assert.equal(f.writes.length,0,'Reattachment never creates catalog grants or indexes');
    }finally{await f.close();}
});

test('combined Use Grant freezes selected Projects, deduplicates shared indexes and asks which index to manage',async context=>{
    const f=await serviceTestHost(context,{registered:false}),controller=f.context.subscriptions.find((item:any)=>typeof item.useGrant==='function');
    const namespace='34'.repeat(16),host='56'.repeat(32),grantId='grt_'+'11'.repeat(16),shared='idx_'+'22'.repeat(16),other='idx_'+'23'.repeat(16),a='prj_'+'33'.repeat(16),b='prj_'+'34'.repeat(16),unselected='prj_'+'35'.repeat(16);
    const backing=(suffix:string)=>({binding:suffix.repeat(32),root:path.join(f.root,'native-'+suffix),data_root:path.join(f.root,'storage-'+suffix),collection:'fixture-'+suffix});
    const indexes=[{id:shared,name:'Shared',backing:backing('77')},{id:other,name:'Other',backing:backing('88')}];const rows:any[]=[];let selected:LaunchSpec|undefined;
    try{
        await controller.refresh();await controller.console.controller.refresh();await controller.prepareWindowContext();
        const grant={grant:{id:grantId,host,revision:1,revoked:false,memberships:[[a,shared,'api'],[b,shared,'impl'],[b,other,''],[unselected,shared,'private']].map(([project_id,index_id,prefix])=>({
            membership:{project_id,index_id,revision:1,scope:[{source_root_id:'src_'+(index_id===shared?'44':'45').repeat(16),kind:'subtree',path:prefix}]},source_revision:1,policy_epoch:0,policy_digest:null}))},provenance:{kind:'user_reviewed_project_grant'}};
        f.state.catalogDetailReply=args=>{rows.push(args);const entity=args.entity as any,index=indexes.find(row=>row.id===entity.id);return {status:'catalog_detail',detail:{namespace,revision:1,item:{kind:entity.kind,value:entity.kind==='grant'?grant:{...index,definition_revision:1,source_revision:1,tombstone:false,source_import:'complete',policy:null}}}};};
        controller.attachWorkbench({async useScope(spec:LaunchSpec){selected=spec;}});
        const before=f.writes.length,result=await controller.useGrant({grant_id:grantId,expected_grant_revision:1,project_ids:[b,a]},namespace,1,()=>true);
        assert.equal(f.writes.length,before);assert.ok(selected?.combinedSession);assert.equal(selected?.catalogSession,undefined);
        assert.deepEqual(selected!.combinedSession!.project_ids,[a,b]);assert.equal(selected!.combinedSession!.index_ids,undefined);
        assert.deepEqual(result.selection.indexes.map((row:any)=>row.id),[shared,other]);assert.deepEqual(rows.map(row=>row.entity.kind),['grant','index','index']);
        assert.deepEqual(result.index_details.map((row:any)=>row.detail.item.value.id),[shared,other]);
        const definitions=await controller.provideDefinitions();assert.equal(definitions.length,1);const enrolled=f.state.httpConnections.at(-1)!.selection;assert.equal(enrolled.kind,'combined');assert.deepEqual(enrolled.selection,selected!.combinedSession);assert.equal(definitions[0].args,undefined);assert.equal(definitions[0].uri.toString(),f.httpUrl);
        f.state.pick=items=>items.find((item:any)=>item.index?.id===other)??items[0];
        await controller.index();assert.equal(f.writes.length,before+1);assert.equal(f.writes.at(-1)!.enqueue?.project,indexes[1].backing.binding,'management follows the explicit member picker rather than the display origin');
        assert.equal(selected!.service!.project,indexes[0].backing.binding,'display metadata does not become management authority');
        assert.equal(rows.at(-1).entity.id,other,'the chosen native backing was revalidated before management');
        const originalDetails=f.state.catalogDetailReply!;
        // A peer window can apply its own selection without changing this
        // window's selected Index or immutable backing. Its new grant advances
        // the global catalog clock, not this management intent's authority.
        f.state.catalogDetailReply=args=>{if(args.expected_catalog_revision!==null&&args.expected_catalog_revision!==2)throw new ServiceError('invalid_registry_request');const response=originalDetails(args) as any;return {...response,detail:{...response.detail,revision:2}};};
        const beforePeer=f.writes.length;
        await controller.index();
        assert.equal(f.writes.length,beforePeer+1);assert.equal(f.writes.at(-1)!.enqueue?.project,indexes[1].backing.binding);
        assert.equal(rows.at(-1).expected_catalog_revision,null,'exact chosen Index is revalidated at the current catalog clock');
        for(const change of [{revision:0},{namespace:'99'.repeat(16)},{item:{kind:'index',value:{...indexes[1],tombstone:true,source_import:'complete'}}}]){
            f.state.catalogDetailReply=args=>{const response=originalDetails(args) as any;return {...response,detail:{...response.detail,revision:2,...change}};};
            const writes=f.writes.length;
            await assert.rejects(controller.index(),/crossed_native_index_administration|invalid_catalog_revision|catalog_namespace_changed_refresh/);
            assert.equal(f.writes.length,writes,'regressed, crossed or retired metadata cannot dispatch native work');
        }
        f.state.catalogDetailReply=originalDetails;
        const prior=f.writes.length;f.state.pick=()=>undefined;await controller.pause(true);assert.equal(f.writes.length,prior,'cancelled member selection dispatches no pause');
        // A metadata refresh can finish while the explicit management picker is
        // open. Identical complete metadata retains that intent's owner; an
        // actual native descriptor change must still refuse before mutation.
        for(const changed of [false,true]){
            let entered!:()=>void,choose!:()=>void;
            const picker=new Promise<void>(resolve=>{entered=resolve;});
            f.state.pick=items=>new Promise(resolve=>{choose=()=>resolve(items.find((item:any)=>item.index?.id===other));entered();});
            const pausing=controller.pause(true),writes=f.writes.length;
            await picker;
            const previous=controller.currentWindowContext();
            if(changed){
                f.state.hostContextReply=key=>({...structuredClone(previous.response),descriptor:{...previous.response.descriptor,key,catalog_revision:2}});
            }
            await controller.prepareWindowContext();
            if(changed)assert.notEqual(controller.currentWindowContext(),previous,'changed native metadata replaces the captured owner');
            else assert.equal(controller.currentWindowContext(),previous,'identical complete metadata preserves the explicit picker owner');
            choose();
            if(changed){await assert.rejects(pausing,/explicit_scope_selection_changed/);assert.equal(f.writes.length,writes,'changed metadata cannot dispatch a pause');}
            else{await pausing;assert.equal(f.writes.length,writes+1);assert.deepEqual(f.writes.at(-1),{pause:{project:indexes[1].backing.binding,paused:true}});}
        }
    }finally{await f.close();}
});

test('repeated native context reads retain identical owners but publish changed capabilities and future fields',async context=>{
    const f=await serviceTestHost(context,{registered:false}),controller=f.context.subscriptions.find((item:any)=>typeof item.prepareWindowContext==='function');
    let contract=3;
    f.state.hostContextReply=key=>({status:'catalog_host_context',capabilities:{policy_contract:contract,future_capability:{value:contract}},descriptor:{schema:1,namespace:'34'.repeat(16),host:'56'.repeat(32),key,catalog_revision:1,context:null},future_native_context:{value:contract}});
    try{
        await controller.refresh();await controller.console.controller.refresh();await controller.prepareWindowContext();
        const prior=controller.currentWindowContext(),before=f.writes.length;
        await controller.prepareWindowContext();assert.equal(controller.currentWindowContext(),prior,'exact duplicate metadata is not an authority change');
        contract=4;await controller.prepareWindowContext();
        const changed=controller.currentWindowContext();assert.notEqual(changed,prior,'equal descriptors cannot hide changed capabilities or future response fields');
        assert.deepEqual(changed.descriptor,prior.descriptor);assert.deepEqual(changed.response.capabilities,{policy_contract:4,future_capability:{value:4}});assert.deepEqual(changed.response.future_native_context,{value:4});
        assert.equal(prior.response.capabilities.policy_contract,3,'the previously captured response remains immutable');
        assert.equal(f.writes.length,before);assert.equal(controller.currentCatalogScope(),undefined);
    }finally{await f.close();}
});

for(const peerInstalled of [false,true])test(`explicit Use Grant validates native metadata and publishes the exact selector (${peerInstalled?'installed by another window':'present at startup'})`,async context=>{
    const f=await serviceTestHost(context,{registered:false,fresh:peerInstalled}),controller=f.context.subscriptions.find((item:any)=>typeof item.useGrant==='function'),namespace='34'.repeat(16),host='56'.repeat(32),grantId='grt_'+'11'.repeat(16),indexId='idx_'+'22'.repeat(16),projectId='prj_'+'33'.repeat(16),sourceId='src_'+'44'.repeat(16);
    const rows:any[]=[];let workbenchSpec:LaunchSpec|undefined;
    try{
        await controller.refresh();
        if(peerInstalled){assert.equal(controller.binding,undefined);f.state.ensured=1;}
        await controller.console.controller.refresh();await controller.prepareWindowContext();
        assert.equal(controller.currentBinding(),true,'authenticated context adoption is current without a legacy registration refresh');
        const key=controller.explorerHost().windowContext().key;
        const backing={binding:'77'.repeat(32),root:path.join(f.root,'native-outside-window'),data_root:path.join(f.root,'native-index-storage'),collection:'native-index'};
        const grant={grant:{id:grantId,host,revision:1,revoked:false,memberships:[{membership:{project_id:projectId,index_id:indexId,revision:1,scope:[{source_root_id:sourceId,kind:'subtree',path:''}]},source_revision:1,policy_epoch:0,policy_digest:null}]},provenance:{kind:'user_reviewed_project_grant'}};
        f.state.catalogDetailReply=(args)=>{rows.push(args);const entity=args.entity as any;return {status:'catalog_detail',detail:{namespace,revision:1,item:{kind:entity.kind,value:entity.kind==='grant'?grant:{id:indexId,name:'Outside window',backing,definition_revision:1,source_revision:1,tombstone:false,source_import:'complete',policy:null}},future_native_detail:'retained'}};};
        controller.attachWorkbench({async useScope(spec:LaunchSpec){assert.deepEqual(controller.explorerHost().specs(),[spec],'new selector exposed before Workbench activation');workbenchSpec=spec;}});
        assert.deepEqual(await controller.provideDefinitions(),[],'new catalog metadata never becomes a legacy automatic definition');
        const before=f.writes.length,result=await controller.useGrant({grant_id:grantId,expected_grant_revision:1,index_id:indexId,project_ids:[projectId]},namespace,1,()=>true);
        assert.equal(f.writes.length,before);assert.equal(result.grant_detail.detail.future_native_detail,'retained');assert.equal(result.index_detail.detail.future_native_detail,'retained');assert.ok(workbenchSpec);
        assert.deepEqual(workbenchSpec!.catalogSession,{schema:1,context_key:key,namespace,expected_catalog_revision:1,grant_id:grantId,expected_grant_revision:1,index_id:indexId,project_ids:[projectId]});
        const definitions=await controller.provideDefinitions();assert.equal(definitions.length,1);assert.equal(definitions[0].command,undefined);assert.equal(definitions[0].args,undefined);assert.deepEqual(f.state.httpConnections.at(-1)!.selection,{kind:'scoped',selection:workbenchSpec!.catalogSession});assert.equal(workbenchSpec!.root,backing.root);
        const discovered=f.state.discovered;
        await controller.refresh();assert.equal(f.state.discovered,discovered,'valid explicit scope does not re-enumerate legacy registrations');
        assert.deepEqual(definitionValue(await controller.resolveDefinition(definitions[0])),definitionValue(definitions[0]),'actual MCP resolution preserves the deliberate HTTP connection');assert.deepEqual(f.state.httpConnections.at(-1)!.selection,{kind:'scoped',selection:workbenchSpec!.catalogSession});
        assert.deepEqual(rows.map(value=>value.expected_catalog_revision),[1,1]);assert.deepEqual(rows.map(value=>value.entity.kind),['grant','index']);
        const priorWrites=f.writes.length;
        const managementActions:string[]=[],run=controller.console.run.bind(controller.console);
        context.mock.method(controller.console,'run',async(action:string,...args:unknown[])=>{managementActions.push(action);return run(action,...args);});
        await controller.index();await controller.index();
        assert.deepEqual(managementActions,['refresh','refresh'],'opening Activity observes the existing client rather than replacing its authority');
        assert.equal(f.writes.length,priorWrites+2,'each explicit index action dispatches a native job');
        assert.ok(f.writes.slice(priorWrites).every(value=>value.enqueue?.project===backing.binding),'both jobs retain the selected Index backing');
        assert.deepEqual((await controller.provideDefinitions()).map(definitionValue),definitions.map(definitionValue),'index actions retain the deliberate native selection');assert.deepEqual(f.state.httpConnections.at(-1)!.selection,{kind:'scoped',selection:workbenchSpec!.catalogSession});
        // Subsequent read-only administration assertions start after those explicit jobs.
        const afterIndexWrites=f.writes.length;
        const reports:string[][]=[];context.mock.method(native,'runText',async(_command:string,args:string[])=>{reports.push(args);return JSON.stringify({future_native_usage:'retained'});});
        const usage=await controller.explorerHost().usage(workbenchSpec,()=>true);assert.match(usage.view().html,/Open Index administration/);assert.equal(reports.length,0,'selected Usage cannot read entire Index history');
        await assert.rejects(controller.usageWriter(workbenchSpec,()=>true)(['clear']),/explicit_index_administration_required/);assert.equal(f.writes.length,afterIndexWrites);
        await usage.action({type:'usage',action:'openIndexAdministration'});assert.equal(reports.length,1);assert.equal(reports[0].includes('--catalog-session'),false);assert.equal(reports[0][reports[0].indexOf('--service-project')+1],backing.binding);
        const beforePeerDetail=f.state.catalogDetailReply!;
        f.state.catalogDetailReply=args=>{if(args.expected_catalog_revision!==null&&args.expected_catalog_revision!==2)throw new ServiceError('invalid_registry_request');const response=beforePeerDetail(args) as any;return {...response,detail:{...response.detail,revision:2}};};
        await controller.openIndexAdministration(workbenchSpec,()=>true);assert.equal(reports.length,2,'Usage retains the selected Index across an unrelated forward catalog clock');
        assert.equal(rows.at(-1).expected_catalog_revision,null);
        f.state.catalogDetailReply=beforePeerDetail;
        const admin=f.panels.find((panel:any)=>panel.viewType==='airk.usage');assert.ok(admin);assert.match(admin.webview.html,/Entire Index history and shared data-root totals/);
        const actualDetail=f.state.catalogDetailReply;f.state.catalogDetailReply=args=>{const response=actualDetail!(args) as any;if((args.entity as any).kind==='index')return {...response,detail:{...response.detail,item:{...response.detail.item,value:{...response.detail.item.value,backing:{...backing,binding:'88'.repeat(32)}}}}};return response;};
        await assert.rejects(controller.openIndexAdministration(workbenchSpec,()=>true),/crossed_native_index_administration/);assert.equal(reports.length,2,'crossed native backing cannot open another administrative reader');f.state.catalogDetailReply=actualDetail;usage.dispose();
        assert.deepEqual(f.values.get('airk.scope.explicitWithdrawal.1'),{schema:1,namespace,host,context_key:key,grant_id:grantId,expected_grant_revision:1,index_id:indexId,project_ids:[projectId]});
        f.stub.workspace.workspaceFile={scheme:'file',toString:()=> 'file:///replaced-window.code-workspace'};
        assert.deepEqual(controller.explorerHost().specs(),[],'a stale explicit selection cannot fall back to broader legacy evidence');assert.deepEqual(await controller.provideDefinitions(),[]);
        await assert.rejects(controller.chooseSpec(),/explicit_scope_selection_changed/);
        controller.reset();assert.deepEqual(controller.explorerHost().specs(),[],'automatic reset retains inactive explicit selection intent');await controller.refresh();assert.deepEqual(controller.explorerHost().specs(),[]);assert.deepEqual(await controller.provideDefinitions(),[]);await assert.rejects(controller.chooseSpec(),/explicit_scope_selection_changed/);
    }finally{await f.close();}
});

test('stored explicit-scope withdrawal is inert after reload and cannot restore an evidence or MCP selector',async context=>{
    const f=await serviceTestHost(context),controller=f.context.subscriptions.find((item:any)=>typeof item.useGrant==='function');
    try{
        f.values.set('airk.scope.explicitWithdrawal.1',{schema:1,namespace:'34'.repeat(16),host:'56'.repeat(32),context_key:'78'.repeat(32),grant_id:'grt_'+'11'.repeat(16),expected_grant_revision:1,index_id:'idx_'+'22'.repeat(16),project_ids:['prj_'+'33'.repeat(16)]});
        await controller.refresh();assert.deepEqual(controller.explorerHost().specs(),[]);assert.deepEqual(await controller.provideDefinitions(),[]);await assert.rejects(controller.chooseSpec(),/explicit_scope_selection_changed/);assert.deepEqual(f.state.hostContextKeys,[],'restoring an inert withdrawal cannot perform native context or evidence work');
        f.values.set('airk.scope.explicitWithdrawal.1',{schema:9,context_key:'invalid'});assert.deepEqual(controller.explorerHost().specs(),[],'invalid withdrawal metadata cannot enable legacy access');
        const get=f.context.workspaceState.get;f.context.workspaceState.get=(key:string,...args:unknown[])=>{if(key==='airk.scope.explicitWithdrawal.1')throw new Error('Storage unavailable');return get(key,...args);};
        assert.deepEqual(controller.explorerHost().specs(),[]);await assert.rejects(controller.chooseSpec(),/explicit_scope_selection_changed/);
    }finally{await f.close();}
});

test('late Use Grant metadata cannot publish after trust, context, revision or client replacement',async context=>{
    for(const change of ['trust','context','reset','revision'] as const){
        const f=await serviceTestHost(context),controller=f.context.subscriptions.find((item:any)=>typeof item.useGrant==='function');let finish!:(value:unknown)=>void;let reached=false;
        try{
            await controller.refresh();await controller.console.controller.refresh();await controller.prepareWindowContext();
            f.state.catalogDetailReply=args=>(args.entity as {kind:string}).kind==='index'?{status:'catalog_detail',detail:{namespace:'34'.repeat(16),revision:1,item:{kind:'index',value:{}}}}:new Promise(resolve=>{reached=true;finish=resolve;});
            const pending=controller.useGrant({grant_id:'grt_'+'11'.repeat(16),expected_grant_revision:1,index_id:'idx_'+'22'.repeat(16),project_ids:['prj_'+'33'.repeat(16)]},'34'.repeat(16),1,()=>true);
            const deadline=Date.now()+2000;while(!reached&&Date.now()<deadline)await new Promise(resolve=>setTimeout(resolve,1));assert.ok(reached);
            if(change==='trust')f.stub.workspace.isTrusted=false;else if(change==='context')f.stub.workspace.workspaceFile={scheme:'file',toString:()=> 'file:///other.code-workspace'};else if(change==='reset')controller.reset();
            finish({status:'catalog_detail',detail:{namespace:'34'.repeat(16),revision:change==='revision'?2:1,item:{kind:'grant',value:{}}}});
            await assert.rejects(pending,(error:unknown)=>{
                if(change==='revision'){assert.ok(error instanceof Error);assert.equal(error.message,'crossed_scope_selection_metadata');}
                else {assert.ok(error instanceof ServiceError);assert.equal(error.code,change==='context'?'stale_scope_selection':'runtime_client_no_longer_authorized');}
                return true;
            });assert.equal(controller.explorerHost().specs().some((spec:LaunchSpec)=>spec.catalogSession),false);
        }finally{await f.close();}
    }
});

test('actual host context producer reads explicitly, deduplicates and preserves native identity and complete results',async context=>{
    const f=await serviceTestHost(context);const controller=f.context.subscriptions.find((item:any)=>typeof item.prepareWindowContext==='function');
    let finish!:(value:unknown)=>void;
    try{
        await controller.console.controller.refresh();const host=controller.explorerHost();
        assert.equal(host.windowContext(),undefined);assert.equal(host.windowContextObservation(),undefined);assert.deepEqual(f.state.hostContextKeys,[]);
        f.state.hostContextReply=()=>new Promise(resolve=>{finish=resolve;});
        const before={discovered:f.state.discovered,ensured:f.state.ensured,writes:f.writes.length};
        const first=host.prepareWindowContext(),second=host.prepareWindowContext();await contextReadStarted(f.state.hostContextKeys);
        const key=f.state.hostContextKeys[0],native={status:'catalog_host_context',descriptor:{schema:1,namespace:'34'.repeat(16),host:'56'.repeat(32),key,catalog_revision:1,context:null,future_native_field:{all:'retained'}},future_response_field:{retained:true}};
        finish(native);await Promise.all([first,second]);
        assert.deepEqual(host.windowContext(),{host:'56'.repeat(32),key,kind:'folder',name:f.folders[0].name});
        assert.deepEqual(host.windowContextObservation(),native);assert.equal(f.values.get('airk.windowContextKey.1'),key);
        assert.deepEqual(JSON.parse(catalogExport({generation:1,busy:false,hostContext:host.windowContextObservation()})).hostContext,native);
        for(let i=0;i<3;i++){host.windowContext();host.windowContextObservation();}assert.equal(f.state.hostContextKeys.length,1,'getters perform no hidden work');
        f.state.hostContextReply=()=>native;await host.prepareWindowContext();assert.deepEqual(f.state.hostContextKeys,[key,key]);
        assert.deepEqual({discovered:f.state.discovered,ensured:f.state.ensured,writes:f.writes.length},before,'the producer cannot discover/install/register or grant access');
    }finally{await f.close();}
});

test('empty windows ignore shared Memento context keys and retain only transient per-window identity',async context=>{
    const keys:string[]=[];
    for(let i=0;i<2;i++){
        const f=await serviceTestHost(context,{roots:0,registered:false});
        try{
            f.values.set('airk.windowContextKey.1','11'.repeat(32));
            const controller=f.context.subscriptions.find((item:any)=>typeof item.prepareWindowContext==='function');await controller.console.controller.refresh();
            await controller.prepareWindowContext();const value=controller.explorerHost().windowContext();assert.equal(value.kind,'empty');assert.notEqual(value.key,'11'.repeat(32));keys.push(value.key);
            assert.equal(f.values.get('airk.windowContextKey.1'),'11'.repeat(32));assert.deepEqual(f.writes,[]);
        }finally{await f.close();}
    }
    assert.notEqual(keys[0],keys[1]);
});

test('saved and untitled workspace-file identities use Core kinds and existing private workspace keys',async context=>{
    for(const scheme of ['file','untitled']){
        const f=await serviceTestHost(context);
        try{
            f.stub.workspace.workspaceFile={scheme,toString:()=>`${scheme}:///explicit-context.code-workspace`};f.stub.workspace.name='Explicit context';
            f.values.set('airk.windowContextKey.1','91'.repeat(32));
            const controller=f.context.subscriptions.find((item:any)=>typeof item.prepareWindowContext==='function');await controller.console.controller.refresh();await controller.prepareWindowContext();
            assert.deepEqual(controller.explorerHost().windowContext(),{host:'56'.repeat(32),key:'91'.repeat(32),kind:scheme==='file'?'workspace':'untitled',name:'Explicit context'});
            assert.deepEqual(f.state.hostContextKeys,['91'.repeat(32)]);assert.deepEqual(f.writes,[]);
        }finally{await f.close();}
    }
});

test('late actual producer replies cannot survive revoked trust, disposal, reset or window-context replacement',async context=>{
    for(const change of ['trust','dispose','reset','context','native-off'] as const){
        const f=await serviceTestHost(context),controller=f.context.subscriptions.find((item:any)=>typeof item.prepareWindowContext==='function');
        let finish!:(value:unknown)=>void;
        try{
            await controller.console.controller.refresh();f.state.hostContextReply=()=>new Promise(resolve=>{finish=resolve;});
            const host=controller.explorerHost(),pending=host.prepareWindowContext();await contextReadStarted(f.state.hostContextKeys);
            if(change==='trust')f.stub.workspace.isTrusted=false;
            else if(change==='dispose')controller.dispose();else if(change==='reset')controller.reset();
            else if(change==='native-off')f.settings.set('runtime.nativeExecution',false);
            else f.stub.workspace.workspaceFile={scheme:'file',toString:()=> 'file:///renamed.code-workspace'};
            finish({status:'catalog_host_context',descriptor:{schema:1,namespace:'34'.repeat(16),host:'56'.repeat(32),key:f.state.hostContextKeys[0],catalog_revision:1,context:null}});
            await assert.rejects(pending,/workspace_untrusted|no_longer_authorized|window_context_changed|stale_catalog_client|native_execution_disabled/);
            assert.equal(host.windowContext(),undefined);assert.equal(host.windowContextObservation(),undefined);assert.deepEqual(f.writes,[]);
        }finally{setNativeExecutionGate(()=>true);await f.close();}
    }
});

test('disabled native execution starts no AIRK process until the user enables it',async context=>{
    const f=await serviceTestHost(context,{automatic:true,nativeExecution:false});
    const launches:string[]=[];
    try{
        assert.deepEqual(await f.api.getServerDefinitions(),[]);
        assert.equal(f.state.discovered,0);assert.equal(f.state.ensured,0);
        assert.match(f.statusBar.text,/native execution off/);
        for(const name of ['airk.index','airk.reindex','airk.openTools','airk.installCli','airk.status','airk.usage'])
            await assert.rejects(Promise.resolve(f.commands.get(name)!()),/native_execution_disabled/);
        // Workspace changes and refreshes stay inert; low-level spawn points refuse first.
        await f.commands.get('airk.treeRefresh')!().catch(()=>undefined);
        await assert.rejects(runText(process.execPath,['--version']),NativeExecutionDisabled);
        assert.throws(()=>new NativeControlTransport('airk',[],1000,(command)=>{launches.push(command);throw new Error('spawned');}),NativeExecutionDisabled);
        await assert.rejects(McpStdioSession.open({command:'airk',args:[],clientName:'fixture',clientVersion:'0',launch:(command:string)=>{launches.push(command);throw new Error('spawned');}} as any),NativeExecutionDisabled);
        assert.deepEqual(launches,[]);
        assert.equal(f.state.discovered,0);assert.equal(f.state.ensured,0);assert.equal(f.writes.length,0);
        await f.commands.get('airk.enableNativeExecution')!();
        assert.equal(f.settings.get('runtime.nativeExecution'),true);
        await f.commands.get('airk.index')!();
        assert.ok(f.state.discovered>0,'enabling resumes the normal runtime path');
    }finally{setNativeExecutionGate(()=>true);await f.close();}
});

test('MCP definitions use reviewed native storage and user response settings while ignoring workspace overrides',async context=>{
    const f=await serviceTestHost(context);
    try{f.settings.set('mcp.responseMode','full');const definitions=await f.api.getServerDefinitions();assert.equal(definitions.length,1);
        assert.equal(definitions[0].command,undefined);assert.equal(definitions[0].headers['X-AIRK-Response-Mode'],'full');
        const enrolled=f.state.httpConnections.at(-1)!.selection;assert.equal(enrolled.kind,'imported_legacy');if(enrolled.kind==='imported_legacy'){assert.equal(enrolled.selection.registration.root,[...f.registrations.values()][0].root);assert.equal(enrolled.selection.registration.data_root,f.settings.get('storage.path'));assert.ok(!enrolled.selection.registration.data_root.includes('untrusted-storage'));}assert.equal(f.state.ensured,0);assert.equal(f.writes.length,0);
        f.settings.set('mcp.registrationEnabled',false);assert.deepEqual(await f.api.getServerDefinitions(),[]);
    }finally{await f.close();}
});

test('multi-root Index Now selects the active native binding and metadata refresh does not restart MCP definitions',async context=>{
    const f=await serviceTestHost(context,{roots:2});
    try{const before=await f.api.getServerDefinitions();assert.equal(before.length,1);assert.equal(before[0].uri.toString(),f.httpUrl);const union=f.state.httpConnections.at(-1)!.selection;assert.equal(union.kind,'imported_legacy_union');if(union.kind==='imported_legacy_union'){assert.equal(union.selection.no_default_project,true);assert.deepEqual(union.selection.selections.map(member=>member.registration.binding).sort(),[...f.registrations.keys()].sort());}let changed=0;f.providers[0].onDidChangeMcpServerDefinitions(()=>changed++);
        await f.api.getServerDefinitions();assert.equal(changed,0);
        f.stub.window.activeTextEditor={document:{uri:f.folders[1].uri}};const picks=f.state.picks;
        await f.commands.get('airk.index')!();assert.equal(f.state.picks,picks);
        const request=f.writes.find(value=>value.enqueue).enqueue;
        assert.equal(f.registrations.get(request.project).root,f.folders[1].uri.fsPath);
        await f.api.getServerDefinitions();assert.equal(changed,0);assert.equal(f.writes.filter(value=>value.enqueue).length,1);
    }finally{await f.close();}
});

test('native tree commands keep the clicked project when another workspace editor is active and reject stale bindings',async context=>{
    const f=await serviceTestHost(context,{roots:2});
    try{
        const project=[...f.registrations.values()][1];
        const spec={command:f.binding.executable,root:project.root,dataDir:project.data_root,collection:project.collection,name:'Clicked tree project',service:{installDir:f.binding.installDir,project:project.binding}};
        f.stub.window.activeTextEditor={document:{uri:f.folders[0].uri}};
        await f.commands.get('airk.treeIndex')!(spec);
        assert.equal(f.writes.find(value=>value.enqueue).enqueue.project,project.binding);
        await f.commands.get('airk.treePause')!(spec);
        assert.equal(f.writes.find(value=>value.pause).pause.project,project.binding);
        await f.commands.get('airk.treeReindex')!(spec);
        assert.deepEqual(f.writes.find(value=>value.plan_create).plan_create.request.selection,{selected:[project.binding]});
        const count=f.writes.length;
        const stale={...spec,service:{...spec.service,project:'ef'.repeat(32)}};
        for(const command of ['airk.treeIndex','airk.treePause','airk.treeReindex'])await assert.rejects(f.commands.get(command)!(stale),/workspace_registration_changed/);
        assert.equal(f.writes.length,count);
    }finally{await f.close();}
});

test('agent setup uses credential-referenced HTTP, writes only on request and refuses stale or untrusted guides',async context=>{
    const f=await serviceTestHost(context);
    try{f.state.pick=items=>items.find(item=>item.client==='codex')??items[0];await f.commands.get('airk.setupAgent')!();assert.equal(f.panels.length,1);
        const destination=path.join(f.folders[0].uri.fsPath,'.codex','config.toml');await assert.rejects(fs.stat(destination),{code:'ENOENT'});
        await f.panels[0].send({action:'copy'});assert.ok(f.clipboard[0].includes(f.httpUrl));assert.match(f.clipboard[0],/bearer_token_env_var/);assert.ok(!f.clipboard[0].includes('command ='));assert.ok(!f.clipboard[0].includes('--service-project'));
        await f.panels[0].send({action:'createCodex'});const before=await fs.readFile(destination,'utf8');assert.doesNotMatch(before,/service-project/);
        f.stub.workspace.workspaceFolders=[];await f.api.getServerDefinitions();await f.panels[0].send({action:'createCodex'});
        assert.match(f.panels[0].webview.html,/Workspace or storage changed/);assert.equal(await fs.readFile(destination,'utf8'),before);
        f.stub.workspace.isTrusted=false;const copied=f.clipboard.length;await f.panels[0].send({action:'copy'});assert.equal(f.clipboard.length,copied);
    }finally{await f.close();}
});

test('Codex setup consolidates an existing legacy connection through guarded editor saves',async context=>{
    const f=await serviceTestHost(context,{assistance:true});
    try{
        if(process.platform==='win32'){
            for(const registration of f.registrations.values())registration.root=path.toNamespacedPath(registration.root);
            await f.api.getServerDefinitions();
        }
        f.state.pick=items=>items.find(item=>item.client==='codex')??items[0];await f.commands.get('airk.setupAgent')!();
        const panel=f.panels.find(panel=>panel.viewType==='airk.agentSetup')??f.panels[0];
        await panel.send({action:'createCodex'});
        const destination=path.join(f.folders[0].uri.fsPath,'.codex','config.toml');
        const created=await fs.readFile(destination,'utf8');assert.ok(created.includes('[mcp_servers.airk]')&&!created.includes('airk_explorer'));
        const project=[...f.registrations.values()][0];
        const retrievalOnly=`[mcp_servers.airk-${project.collection}]\ncommand = ${JSON.stringify(f.binding.launcher)}\nargs = ${JSON.stringify(['runtime','launch','--install-dir',f.binding.installDir,'--','--root',project.root,'--data-dir',project.data_root,'--collection',project.collection,'--service-project',project.binding,'mcp'])}\n`;await fs.writeFile(destination,retrievalOnly);
        let saved=0;
        const document={uri:{fsPath:destination},isDirty:false,version:1,text:retrievalOnly,getText(){return this.text;},positionAt:(offset:number)=>offset,
            async save(){saved++;await fs.writeFile(destination,this.text);return true;}};
        f.stub.workspace.openTextDocument=async()=>document;
        f.stub.workspace.applyEdit=async(edit:any)=>{document.text=edit.after;document.version++;return true;};
        await panel.send({action:'updateCodex'});
        const updated=await fs.readFile(destination,'utf8');assert.ok(updated.includes('[mcp_servers.airk]')&&updated.includes('bearer_token_env_var')&&updated.includes('https://127.0.0.1:')&&!updated.includes('--explorer-descriptor')&&!updated.includes('airk_explorer'),panel.webview.html);assert.equal(saved,1);
        await panel.send({action:'updateCodex'});assert.equal(saved,1,'current connections require no repeat edit');
        f.stub.workspace.textDocuments=[{uri:{fsPath:destination},isDirty:true}];await panel.send({action:'updateCodex'});
        assert.match(panel.webview.html,/unsaved/i);assert.equal(saved,1);
    }finally{await f.close();}
});

test('lost indexing acknowledgment preserves the exact intent and read-only reconnect never replays it',async context=>{
    const f=await serviceTestHost(context);
    try{f.state.ackLost=true;await assert.rejects(f.commands.get('airk.index')!(),/service_request_outcome_unknown/);
        const keys=[...f.values.keys()].filter(key=>key.startsWith('airk.service.intent.'));assert.equal(keys.length,1);
        const saved=structuredClone(f.values.get(keys[0]));f.state.ackLost=false;
        await f.api.getServerDefinitions();await f.commands.get('airk.serviceConnect')!();
        assert.equal(f.writes.filter(value=>value.enqueue).length,1);assert.deepEqual(f.values.get(keys[0]),saved);
    }finally{await f.close();}
});
