// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { test, type TestContext } from 'node:test';
import { createHash, X509Certificate } from 'node:crypto';
import * as https from 'node:https';
import { ca, cert, key } from './fixtures/httpTlsFixture';
import { HttpConnectionInfo, HttpSelection, validateHttpConnection } from '../serviceHttp';
import Module from 'node:module';
import * as fs from 'node:fs/promises';
import * as os from 'node:os';
import * as path from 'node:path';
import { RuntimeHost } from '../serviceBootstrap';
import { ServiceWorkspace } from '../serviceWorkspace';
import { ServiceClient, ServiceError, record } from '../serviceClient';

function framedDigest(domain:string,fields:Buffer[]):Buffer {
    const hash=createHash('sha256').update(Buffer.from('AIRK-DIGEST-1\0'));
    for(const field of [Buffer.from(domain),...fields]){const length=Buffer.alloc(8);length.writeBigUInt64BE(BigInt(field.length));hash.update(length).update(field);}
    return hash.digest();
}
function savedRegistration(row:Record<string,unknown>){return {binding:String(row.binding),root:String(row.root),data_root:String(row.data_root),collection:String(row.collection)};}
function importedSnapshot(row:Record<string,unknown>,revision=1){
    const registration=savedRegistration(row),namespace='12'.repeat(16),host='56'.repeat(32);
    const provenance={kind:'imported_legacy_snapshot',upgrade_id:'44'.repeat(16),plan_digest:'55'.repeat(32),legacy_binding:registration.binding,configuration_digest:'66'.repeat(32)};
    const grantIdentity=framedDigest('airk/imported-legacy-grant/1',[Buffer.from(JSON.stringify([namespace,host,provenance]))]).subarray(0,16);grantIdentity[0]|=0x80;
    const identity=createHash('sha256').update(registration.binding).digest('hex').slice(0,32);
    return {status:'catalog_legacy_binding',observation:{schema:1,namespace,catalog_revision:revision,project:registration.binding,eligibility:{kind:'imported_snapshot',registration,
        grant:{grant:{id:'grt_'+grantIdentity.toString('hex'),host,revision:1,revoked:false,memberships:[{membership:{project_id:'prj_'+identity,index_id:'idx_'+identity,revision:1,scope:[{source_root_id:'src_'+identity,kind:'subtree',path:''}]},source_revision:1,policy_epoch:0,policy_digest:null}]},provenance}}}};
}
/** Real ephemeral TLS/material; only native enrollment is represented by owned metadata. */
async function standaloneHttp(context:TestContext,binding:{controlDir:string},registrations:readonly Record<string,unknown>[]){
    await fs.mkdir(path.join(binding.controlDir,'http-1'),{recursive:true,mode:0o700});
    // These standalone host adapters have no native user/profile config roots.
    // Keep their provider checks independent of ambient real agent settings.
    context.mock.method(require('../globalAgentInventory'),'inventoryGlobalAgentConfigs',async()=>({files:[],checked:[],blockers:[]}));
    const server=https.createServer({cert,key},(_request,response)=>{response.writeHead(401);response.end();});
    await new Promise<void>(resolve=>server.listen(0,'127.0.0.1',resolve));
    context.after(async()=>{server.closeAllConnections();await new Promise<void>(resolve=>server.close(()=>resolve()));});
    const url=`https://127.0.0.1:${(server.address() as {port:number}).port}/mcp`,connections:HttpConnectionInfo[]=[];
    const info=async(selection:HttpSelection,id:string)=>{
        const directory=path.join(binding.controlDir,'http-1');await fs.mkdir(directory,{recursive:true,mode:0o700});
        const caFile=path.join(directory,'ca.pem'),credentialFile=path.join(directory,`credential-${id}.token`);
        await fs.writeFile(caFile,ca,{mode:0o600});await fs.writeFile(credentialFile,`airk1.${id}.`+'a'.repeat(64),{mode:0o600});
        const result=validateHttpConnection({schema:1,connection_id:id,endpoint:{schema:1,installation:'aa'.repeat(32),user:'fixture',namespace:selection.selection.namespace,generation:1,service_instance:'56'.repeat(16),executable_sha256:'bb'.repeat(32),url,ca_sha256:createHash('sha256').update(new X509Certificate(ca).raw).digest('hex'),certificate_sha256:'cc'.repeat(32),state:'ready',predecessor_digest:null},selection,ca_certificate_file:caFile,credential_file:credentialFile},selection,directory);
        connections.push(result);return result;
    };
    const imported=(registration:ReturnType<typeof savedRegistration>,receipt:string)=>{
        const saved=registrations.find(row=>row.binding===registration.binding);assert.ok(saved,'enrollment must use an existing native registration');assert.deepEqual(registration,savedRegistration(saved));assert.match(receipt,/^[a-f0-9]{64}$/);
        const observation=importedSnapshot(saved).observation;return {namespace:observation.namespace,expected_catalog_revision:observation.catalog_revision,registration,grant:observation.eligibility.grant,configuration_receipt:receipt};
    };
    context.mock.method(RuntimeHost.prototype,'httpConnectionInfo',async(_binding:unknown,id:string)=>{const value=connections.find(row=>row.connection_id===id);assert.ok(value);return structuredClone(value);});
    context.mock.method(RuntimeHost.prototype,'httpConnection',async(_binding:unknown,selection:HttpSelection,file:string,id:string)=>{assert.deepEqual(JSON.parse(await fs.readFile(file,'utf8')),selection);return info(selection,id);});
    context.mock.method(RuntimeHost.prototype,'httpLegacyConnection',async(_binding:unknown,registration:ReturnType<typeof savedRegistration>,file:string,receipt:string,id:string)=>{assert.deepEqual(JSON.parse(await fs.readFile(file,'utf8')),registration);return info({kind:'imported_legacy',selection:imported(registration,receipt)},id);});
    context.mock.method(RuntimeHost.prototype,'httpLegacyUnionConnection',async(_binding:unknown,rows:{registration:ReturnType<typeof savedRegistration>;configuration_receipt:string}[],file:string,id:string)=>{
        assert.deepEqual(JSON.parse(await fs.readFile(file,'utf8')),{schema:1,registrations:rows,no_default_project:true});
        assert.equal(new Set(rows.map(row=>row.registration.binding)).size,rows.length);
        return info({kind:'imported_legacy_union',selection:{namespace:'12'.repeat(16),selections:rows.map(row=>imported(row.registration,row.configuration_receipt)),no_default_project:true}},id);
    });
    return {url,connections};
}
function httpContext(root:string){return {globalStorageUri:{fsPath:root},secrets:{store:async()=>{}},environmentVariableCollection:{get:()=>undefined,replace(){}}};}
/** Canonical metadata transport through the real persistent ServiceClient adapter. */
function metadataClient(registrations:()=>readonly Record<string,unknown>[],revision=1,beforePage?:()=>Promise<void>):ServiceClient{
    const namespace='12'.repeat(16);
    return new ServiceClient({dispose(){},async request(method){
        const name=typeof method==='string'?method:Object.keys(record(method))[0];
        if(name==='health')return {status:'health',observation:{instance:'56'.repeat(16),policy:{accepting:true}}};
        if(name==='events')return {status:'events',page:{namespace,events:[],retained_prefix_lost:false,next:null,tail:{namespace,project:null,after:0,through:null}}};
        if(name==='console'||name==='inactive_console'){await beforePage?.();return {status:'console',page:{revision,cached:true,observed_unix_ms:1,after:null,projects:name==='inactive_console'?[]:registrations().map(registration=>({project:{registration,paused:false,fencing_generation:1}}))}};}
        if(name==='catalog_legacy_binding'){const project=String(record(record(method).catalog_legacy_binding).project),row=registrations().find(row=>row.binding===project);return row?importedSnapshot(row,revision):{status:'catalog_legacy_binding',observation:{schema:1,namespace,catalog_revision:null,project,eligibility:{kind:'unavailable'}}};}
        if(name==='workers'){const projects=record(record(method).workers).projects as string[];return {status:'workers',age_ms:0,observations:projects.map(project=>({project,identity:null,restarting:false,automatic_start_blocked:false}))};}
        if(name==='logs')return {status:'logs',page:{schema:1,records:[]}};
        if(name==='resources')return {status:'resources',observation:{}};
        if(name==='embedding_status')return {status:'embedding_status',available:false};
        if(name==='source_status')return {status:'source_status',available:false};
        throw new Error(`Unexpected native mutation or unsupported fixture method: ${name}`);
    }});
}

test('service activation advertises reviewed bindings without indexing and preserves explicit Stop during resolution',async context=>{
    const root=await fs.realpath(await fs.mkdtemp(path.join(os.tmpdir(),'airk-service-activation-'))),source=path.join(root,'source');await fs.mkdir(source);
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
    const http=await standaloneHttp(context,binding,[registration]);
    let discoveries=0,ensures=0,stopped=false,provider:any;const outputShows:Array<[string,boolean]>=[];
    context.mock.method(RuntimeHost.prototype,'discoverForConnection',function(this:RuntimeHost){return this.discover();});
    context.mock.method(RuntimeHost.prototype,'replacementPlan',async()=>({required:false,reason:'fixture',locations:[]}));
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
    context.mock.method(RuntimeHost.prototype,'client',()=>metadataClient(()=>[registration]));
    const disposable={dispose(){}},event=()=>disposable;
    const stub:any={workspace:{isTrusted:true,workspaceFolders:[{name:'Fixture',uri:{scheme:'file',fsPath:source}}],
        getConfiguration:()=>({get:(key:string,fallback?:unknown)=>key==='explorer.assistance.enabled'?false:fallback,inspect:(key:string)=>({globalValue:key==='mcp.responseMode'?'full':key==='cli.autoInstall'?false:undefined})}),
        getWorkspaceFolder:()=>stub.workspace.workspaceFolders[0],
        onDidChangeWorkspaceFolders:event,onDidGrantWorkspaceTrust:event,onDidChangeConfiguration:event,
        createFileSystemWatcher:()=>{throw new Error('Editor-owned source watching is forbidden');}},
    window:{createOutputChannel:(name:string)=>({appendLine(){},show(preserve:boolean){outputShows.push([name,preserve]);},dispose(){}}),createStatusBarItem:()=>({show(){},dispose(){}}),
        registerWebviewViewProvider:()=>disposable,createTreeView:()=>disposable,onDidChangeActiveTextEditor:(listener:(editor:any)=>void)=>{editorListeners.add(listener);return {dispose:()=>editorListeners.delete(listener)};},
        showErrorMessage:async()=>undefined,showWarningMessage:async()=>undefined,showInformationMessage:async()=>undefined,
        createTerminal:(options:any)=>{terminals.push(options);return {show(){},dispose(){}};}},
    commands:{registerCommand:(name:string,action:()=>Promise<unknown>)=>{commands.set(name,action);return disposable;},executeCommand:async()=>{}},
    lm:{registerMcpServerDefinitionProvider:(name:string,value:any)=>{if(name==='airk.collections')provider=value;return disposable;}},env:{},StatusBarAlignment:{Left:1},
    EventEmitter:class {event=event;fire(){}dispose(){}},TreeItem:class {},Uri:{parse:(value:string)=>({toString:()=>value,scheme:'https'}),file:(fsPath:string)=>({fsPath})},McpHttpServerDefinition:class {constructor(public label:string,public uri:any,public headers:any,public version:string){}},
    McpStdioServerDefinition:class {cwd:unknown;constructor(public label:string,public command:string,public args:string[],public env:unknown,public version:string){}}};
    const loader=Module as unknown as {_load:(...args:any[])=>any},original=loader._load;
    loader._load=function(request:string,...args:any[]){return request==='vscode'?stub:original.call(this,request,...args);};
    let extension:any;const extensionContext:any={extensionPath:root,extensionUri:stub.Uri.file(root),extension:{packageJSON:{version:'1.4.0'}},...httpContext(root),globalState:storage,workspaceState:storage,subscriptions:[]};
    try{
        delete require.cache[require.resolve('../serviceExtension')];extension=require('../serviceExtension');
        const api=await extension.activate(extensionContext);await api.whenReady();assert.equal(ensures,0);assert.ok(discoveries>0);
        // Actual activation and hidden Explorer; metadata uses the real client adapter
        // with an in-memory native read transport, never an evidence process or indexer.
        const controller=extensionContext.subscriptions.find((value:any)=>typeof value.explorerController==='function');assert.ok(controller);const explorer=await controller.explorerController();
        const consoleController=controller.console.controller;
        let refreshes=0;
        const modes:boolean[]=[];
        const refreshMock=context.mock.method(consoleController,'refresh',async(automatic=false)=>{refreshes++;modes.push(automatic);});
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
                    assert.deepEqual(modes,[false,true],'initial observation is explicit; the timer permits bounded optional sampling');
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
        assert.equal(definitions[0].uri.toString(),http.url);assert.equal(definitions[0].headers['X-AIRK-Response-Mode'],'full');
        assert.match(definitions[0].headers.Authorization,/^Bearer airk1\.[a-f0-9]{32}\.[a-f0-9]{64}$/);
        const enrolled=http.connections.at(-1)!;assert.equal(enrolled.selection.kind,'imported_legacy');
        if(enrolled.selection.kind==='imported_legacy')assert.deepEqual(enrolled.selection.selection.registration,savedRegistration(registration));
        assert.equal(definitions[0].command,undefined,'HTTP MCP definitions do not start a per-client native process');
        stopped=true;await assert.rejects(provider.resolveMcpServerDefinition(definitions[0]),/service_explicitly_stopped/);assert.equal(ensures,1);
        stopped=false;await commands.get('airk.openTools')!();assert.equal(terminals.length,1);
        assert.deepEqual(terminals[0].shellArgs.slice(-2),['tools','shell']);assert.equal(terminals[0].shellPath,binding.executable);
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
        const before=ensures;await assert.rejects(provider.resolveMcpServerDefinition(definitions[0]),(error:unknown)=>error instanceof ServiceError&&error.code==='workspace_untrusted');assert.equal(ensures,before);
        stub.workspace.isTrusted=true;
        // Run the actual automatic installation owner with a previous-version
        // active index. Native installation is the boundary mocked here; no
        // reconnect, index setup or grant-selection command is invoked.
        for(const outcome of ['ready','activation_failed','connection_unconfirmed']){
            const failActivation=outcome==='activation_failed';
            const scoped=new controller.constructor({...extensionContext,globalStorageUri:stub.Uri.file(root),extension:{packageJSON:{version:'1.8.9'}}});
            const previous={...binding,version:'1.8.8'},next={...binding,version:'1.8.9',runtimeSha256:'de'.repeat(32)};
            const indexBefore=JSON.stringify(registration),calls:string[]=[],progress:any[]=[];
            let active=previous,connectionConfirmed=outcome!=='connection_unconfirmed';
            const observed=()=>({bundle:{digest:next.runtimeSha256},installDir:root,installed:true,location:{user_paths:{launcher:root}},
                inspection:{selection:{active:active.runtimeSha256},approval:{manifest_sha256:active.runtimeSha256,version:active.version,
                    verification_basis:'trusted_distribution'},executable:active.executable,launcher:active.launcher,control_dir:active.controlDir}});
            scoped.host={
                discover:async()=>observed(),replacementPlan:async()=>({required:false,reason:'fixture',locations:[]}),binding:()=>active,
                supersedePending:async()=>false,
                retirePrevious:async()=>{calls.push('retire');},
                retireLegacyCaches:async()=>{calls.push('remove-old-caches');return {scope:'context-only'};},
                inspectLegacyLaunchers:async()=>({schema:1,active:next.runtimeSha256,launcher:null,universal_launcher:null}),
                retireLegacyLaunchers:async()=>{calls.push('remove-old-launchers');return {schema:1,active:next.runtimeSha256,legacy_launchers_retired:true,launcher:null,universal_launcher:null,processes:{scope:'verified_launcher_images',remaining:0,observations:[]}};},
                reviewTransition:async(current:any,action:string)=>{assert.equal(current,previous);assert.equal(action,'upgrade');calls.push('review');return {action};},
                transition:async(current:any,_review:unknown,report:(message:string,stage:string)=>void)=>{
                    assert.equal(current,previous);calls.push('transition');
                    report('Staging verified runtime files…','staging');
                    report('Checking upgrade prerequisites and activating the selected runtime…','activating');
                    if(failActivation)throw new Error('fixture_activation_failed');
                    active=next;
                },
                ensure:async()=>{calls.push('ensure');assert.equal(active,next);return active;},
                reconnect:async()=>{calls.push('reconnect');assert.equal(active,next);return active;},
            };
            const originalReport=scoped.reportUpgrade.bind(scoped);
            context.mock.method(scoped,'reportUpgrade',(...args:any[])=>{originalReport(...args);progress.push(scoped.explorerHost().runtimeUpgrade());});
            context.mock.method(scoped,'migrateAgentConnections',async(current:any)=>{assert.equal(current,next);assert.ok(calls.includes('remove-old-launchers'));calls.push('verify-http-connections');});
            context.mock.method(scoped,'installCommandLine',async()=>{calls.push('install-cli');});
            context.mock.method(scoped,'prepareWorkspaceProjects',async(current:any)=>{assert.equal(current,next);scoped.legacyCachePreparation={active:next.runtimeSha256,scope:'context-only'};calls.push('prepare-existing');});
            context.mock.method(scoped,'refresh',async()=>{calls.push('refresh');scoped.binding=connectionConfirmed?next:undefined;});
            context.mock.method(scoped,'openWorkbenchAfterSetup',async()=>{});
            context.mock.method(scoped,'reportLastUninstall',async()=>{});
            context.mock.method(scoped,'withInstallProgress',async(action:()=>Promise<void>)=>action());
            const remember=context.mock.method(require('../extensionUninstall'),'rememberUninstallTarget',async()=>{});
            const originalConfiguration=stub.workspace.getConfiguration;
            const configuration=context.mock.method(stub.workspace,'getConfiguration',(...args:any[])=>{
                const current=originalConfiguration(...args);
                return {...current,inspect:(key:string)=>key==='cli.autoInstall'?{globalValue:true}:current.inspect(key)};
            });
            try{
                await scoped.automaticSetup(false,false);
                assert.equal(JSON.stringify(registration),indexBefore,'upgrade preserves the active index registration');
                assert.deepEqual(progress.map(value=>value.stage),failActivation
                    ?['checking','staging','activating','failed']
                    :['checking','staging','activating','reconnecting',outcome==='ready'?'ready':'failed']);
                assert.ok(progress.every(value=>value.fromVersion==='1.8.8'&&value.toVersion==='1.8.9'));
                assert.ok(progress.every(value=>value.startedAt===progress[0].startedAt&&value.updatedAt>=value.startedAt));
                assert.deepEqual(calls,failActivation?['review','transition']
                    :['review','transition','install-cli','ensure','prepare-existing','remove-old-caches','remove-old-launchers','retire','refresh']);
                assert.equal(scoped.explorerHost().runtimeUpgrade().stage,outcome==='ready'?'ready':'failed');
                if(failActivation)assert.match(scoped.explorerHost().runtimeUpgrade().message,/fixture_activation_failed/);
                if(outcome==='connection_unconfirmed')assert.match(scoped.explorerHost().runtimeUpgrade().message,/connection could not be confirmed/);
                if(outcome!=='ready'){
                    assert.ok(!progress.some(value=>value.stage==='ready'),'unconfirmed installation must not report completion');
                    // A later observation can find activation already completed.
                    // The exact completed-install identity takes the existing
                    // automatic reconnect path and clears retained failed state.
                    active=next;connectionConfirmed=true;
                    const key=`airk.installComplete.2.${next.installDir}`,prior=values.get(key);
                    values.set(key,`1.8.9:${next.runtimeSha256}`);
                    const beforeCalls=calls.length,beforeProgress=progress.length;
                    try{
                        await scoped.automaticSetup(false,false);
                        assert.deepEqual(calls.slice(beforeCalls),['reconnect','prepare-existing','remove-old-caches','remove-old-launchers','refresh']);
                        assert.deepEqual(progress.slice(beforeProgress).map(value=>value.stage),['ready']);
                        assert.equal(scoped.explorerHost().runtimeUpgrade().stage,'ready');
                        assert.equal(JSON.stringify(registration),indexBefore);
                    }finally{if(prior===undefined)values.delete(key);else values.set(key,prior);}
                }
                const completedProgress=progress.length;
                scoped.setupSucceeded(next,scoped.generation+1);
                assert.equal(progress.length,completedProgress,'a superseded setup generation cannot publish completion');
            }finally{configuration.mock.restore();remember.mock.restore();scoped.dispose();}
        }
        // A retained completed-rebuild receipt is historical evidence. It must
        // not hide a later reviewed upgrade, including a rebuilt same-version
        // bundle with a different approved manifest. The current exact bundle
        // remains eligible for the ordinary rebuilt reconnect shortcut.
        const defaultConfiguration=stub.workspace.getConfiguration;
        const automaticConfiguration=context.mock.method(stub.workspace,'getConfiguration',(...args:any[])=>{
            const value=defaultConfiguration(...args);
            return {...value,inspect:(key:string)=>key==='cli.autoInstall'?{globalValue:true}:value.inspect(key)};
        });
        const rememberRebuild=context.mock.method(require('../extensionUninstall'),'rememberUninstallTarget',async()=>{});
        try{
            for(const variant of ['older-version','same-version-old-manifest','exact-bundle','exact-unavailable']){
                const exact=variant.startsWith('exact-'),unavailable=variant==='exact-unavailable',digest='de'.repeat(32);
                const scoped=new controller.constructor({...extensionContext,globalStorageUri:stub.Uri.file(root),extension:{packageJSON:{version:'1.8.9'}}});
                const next:any={...binding,version:'1.8.9',runtimeSha256:digest};
                const previous:any={...binding,version:variant==='older-version'?'1.8.8':'1.8.9',runtimeSha256:exact?digest:'ac'.repeat(32)};
                const original={...savedRegistration(registration),data_root:path.join(root,'preserved-index')};
                const replacement={...original,binding:'dc'.repeat(32),data_root:path.join(root,'replacement-index')};
                const rebuild={schema:1,operation_id:(variant==='older-version'?'a1':variant==='same-version-old-manifest'?'a2':unavailable?'a4':'a3').repeat(16),stage:unavailable?'unavailable':'ready',
                    provenance:'native_rebuild_from_saved_configuration',candidate:previous.runtimeSha256,predecessor:'ad'.repeat(32),original_namespace:'b1'.repeat(16),
                    preserved_state:path.join(root,'preserved-state'),preserved_control:path.join(root,'preserved-state','control'),preserved_index_roots:[original.data_root],
                    successor_state:path.dirname(previous.controlDir),successor_control:previous.controlDir,successor_namespace:'b2'.repeat(16),original_failure:null,failure:null,index_scope:null,
                    indexes:[{original,replacement,sources:{},settings_sha256:'b3'.repeat(32),original_paused:false,paused:false,watching:false,
                        job:unavailable?null:{id:'b4'.repeat(32),project:replacement.binding,kind:'index',state:'succeeded',fencing_generation:1,attempts:1,outcome:{state:'succeeded',revision:null}},readiness:unavailable?'unavailable':'ready',
                        failure:unavailable?{code:'rebuild_observation_unavailable',message:'Original rebuild job receipt was pruned'}:null}]};
                previous.rebuild=rebuild;
                const savedReceipt=JSON.stringify(rebuild),calls:string[]=[];
                let active=previous;
                const observed=()=>({bundle:{digest},installDir:root,installed:true,location:{user_paths:{launcher:root}},
                    inspection:{selection:{active:active.runtimeSha256},approval:{manifest_sha256:active.runtimeSha256,version:active.version,verification_basis:'trusted_distribution'},
                        executable:active.executable,launcher:active.launcher,control_dir:active.controlDir,rebuild}});
                scoped.host={discover:async()=>observed(),replacementPlan:async()=>({required:false,reason:'fixture',locations:[]}),binding:()=>active,supersedePending:async()=>false,
                    reviewTransition:async(current:any,action:string)=>{assert.equal(current,previous);assert.equal(action,'upgrade');calls.push('review');return {action};},
                    transition:async(current:any,_review:any,report:(message:string,stage:string)=>void)=>{assert.equal(current,previous);calls.push('activate');report('Staging runtime','staging');report('Activating runtime','activating');active=next;},
                    reconnect:async()=>{calls.push('reconnect');assert.ok(exact,'a historical rebuild must reach upgrade review before reconnecting');return active;},
                    ensure:async()=>{calls.push('ensure');assert.equal(active,next);return next;}};
                context.mock.method(scoped,'installCommandLine',async()=>{calls.push('install-cli');});
                context.mock.method(scoped,'prepareWorkspaceProjects',async()=>{calls.push('prepare');});
                context.mock.method(scoped,'finalizeLegacyLauncherRemoval',async()=>{calls.push('finalize');});
                context.mock.method(scoped,'refresh',async()=>{calls.push('refresh');scoped.binding=next;});
                context.mock.method(scoped,'openWorkbenchAfterSetup',async()=>{});
                context.mock.method(scoped,'reportLastUninstall',async()=>{});
                context.mock.method(scoped,'withInstallProgress',async(action:()=>Promise<void>)=>action());
                try{
                    await scoped.automaticSetup(false,false);
                    assert.deepEqual(calls,exact?['reconnect']:['review','activate','install-cli','ensure','prepare','finalize','refresh'],variant);
                    assert.equal(scoped.installationFailed,false,variant);
                    assert.equal(JSON.stringify(rebuild),savedReceipt,'later upgrades retain the original complete rebuild receipt');
                    if(unavailable){
                        assert.equal(scoped.explorerHost().runtimeUpgrade(),undefined,'unavailable history cannot become an upgrade success/failure banner');
                        assert.equal(scoped.status.text,'AIRK: rebuild history unavailable');
                    }
                }finally{scoped.dispose();}
            }
        }finally{automaticConfiguration.mock.restore();rememberRebuild.mock.restore();}
        extension.deactivate();await assert.rejects(provider.resolveMcpServerDefinition(definitions[0]),(error:unknown)=>error instanceof ServiceError&&error.code==='runtime_client_no_longer_authorized');assert.equal(ensures,before);
    }finally{extension?.deactivate();loader._load=original;delete require.cache[require.resolve('../serviceExtension')];await fs.rm(root,{recursive:true,force:true});}
});


test('selected Explorer project is available while other root waits without narrowing workspace MCP scope',{timeout:30000},async context=>{
    const root=await fs.mkdtemp(path.join(os.tmpdir(),'airk-selected-startup-')),source=path.join(root,'first'),other=path.join(root,'second');
    await fs.mkdir(source);await fs.mkdir(other);const canonical=await fs.realpath(source),otherCanonical=await fs.realpath(other);
    const values=new Map<string,unknown>(),storage={keys:()=>[...values.keys()],get:<T>(key:string)=>values.get(key) as T|undefined,update:async(key:string,value:unknown)=>{values.set(key,value);}};
    const binding={installDir:root,controlDir:path.join(root,'control'),executable:path.join(root,'selected'),launcher:path.join(root,'launcher'),runtimeSha256:'ab'.repeat(32),version:'1.8.0',lifecycle:'on-demand' as const,verificationBasis:'trusted_distribution'};
    const registrations=[{...binding,binding:'cd'.repeat(32),root:canonical,data_root:path.join(root,'data'),collection:'first'},
        {...binding,binding:'ef'.repeat(32),root:otherCanonical,data_root:path.join(root,'data'),collection:'second'}];
    const http=await standaloneHttp(context,binding,registrations);
    const workspace=new ServiceWorkspace(storage,{reviewProject:async(_binding,opened)=>registrations[opened===source?0:1],registerProject:async(_binding,review)=>review},()=>true);
    for(const [index,opened] of [source,other].entries())await workspace.register(binding,opened,registrations[index].data_root,registrations[index].collection,async()=>true);
    context.mock.method(RuntimeHost.prototype,'discoverForConnection',function(this:RuntimeHost){return this.discover();});
    context.mock.method(RuntimeHost.prototype,'replacementPlan',async()=>({required:false,reason:'fixture',locations:[]}));
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
    context.mock.method(RuntimeHost.prototype,'client',()=>metadataClient(()=>registrations,7,async()=>{pageReads++;await held;}));
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
        env:{},StatusBarAlignment:{Left:1},EventEmitter:Emitter,TreeItem:class{},Uri:{parse:(value:string)=>({toString:()=>value,scheme:'https'}),file:(fsPath:string)=>({fsPath})},McpHttpServerDefinition:class{constructor(public label:string,public uri:any,public headers:any,public version:string){}},McpStdioServerDefinition:class{cwd:unknown;constructor(public label:string,public command:string,public args:string[],public env:unknown,public version:string){}}};
    const loader=Module as unknown as {_load:(...args:any[])=>any},original=loader._load;let extension:any;
    loader._load=function(request:string,...args:any[]){return request==='vscode'?stub:request==='./workbenchView'?{WorkbenchView:Workbench}:original.call(this,request,...args);};
    try{
        delete require.cache[require.resolve('../serviceExtension')];extension=require('../serviceExtension');
        const api=await extension.activate({extensionPath:root,extension:{packageJSON:{version:binding.version}},...httpContext(root),globalState:storage,workspaceState:storage,subscriptions:[]} as any);
        await selected;assert.equal(host.specs()[0].service.project,registrations[0].binding);
        assert.equal(pageReads,0,'selected UI must publish without requesting unrelated console pages');
        assert.equal(definitionsChanged,0,'early UI publication cannot emit a partial MCP definition');
        let completed=false;const definitions=api.getServerDefinitions().then((value:any)=>{completed=true;return value;});
        await Promise.resolve();assert.equal(completed,false,'complete workspace scopes still await the other root');
        release();const result=await definitions;assert.equal(result.length,1);
        assert.equal(result[0].uri.toString(),http.url);assert.match(result[0].headers.Authorization,/^Bearer airk1\./);
        const enrolled=http.connections.at(-1)!;assert.equal(enrolled.selection.kind,'imported_legacy_union');
        if(enrolled.selection.kind==='imported_legacy_union'){
            assert.equal(enrolled.selection.selection.no_default_project,true);
            assert.deepEqual(enrolled.selection.selection.selections.map(member=>member.registration.root).sort(),[canonical,otherCanonical].sort());
            assert.deepEqual(enrolled.selection.selection.selections.map(member=>member.registration.binding).sort(),registrations.map(row=>row.binding).sort());
        }
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
