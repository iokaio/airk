// SPDX-License-Identifier: Apache-2.0
//! Native metadata fixture for the default service extension; no real user settings or helpers.
import type { TestContext } from 'node:test';
import { strict as assert } from 'node:assert';
import Module from 'node:module';
import { X509Certificate, createHash } from 'node:crypto';
import * as fs from 'node:fs/promises';
import * as https from 'node:https';
import * as os from 'node:os';
import * as path from 'node:path';
import { RuntimeHost, type RuntimeBinding } from '../serviceBootstrap';
import { ServiceClient, ServiceError, record, type RecordValue } from '../serviceClient';
import { ServiceWorkspace } from '../serviceWorkspace';
import * as native from '../native';
import { NativeSession } from '../nativeSession';
import * as cliInstall from '../cliInstall';
import * as globalInventory from '../globalAgentInventory';
import {ca,cert,key} from './fixtures/httpTlsFixture';
import {HttpConnectionInfo,HttpSelection,validateHttpConnection} from '../serviceHttp';
import { runtimeCleanBreakRequired } from '../runtimeRebuild';
import {predecessorHttpFixture} from './fixtures/predecessorHttpFixture';

export async function serviceTestHost(test:TestContext,options:{trusted?:boolean;roots?:number;workspaceFile?:boolean;registered?:boolean;automatic?:boolean;upgrade?:boolean;assistance?:boolean;installedVersion?:string;runtimeVersion?:string;bundledVersion?:string;discoveryGate?:Promise<void>;connectionDiscoveryGate?:Promise<void>;deferStartup?:boolean;nativeExecution?:boolean;fresh?:boolean;codeFile?:boolean;nativeSourceScope?:{binding:RuntimeBinding;source:string;collection:string}}={}){
    const bundledVersion=options.bundledVersion??(options.upgrade?'1.4.1':'1.4.0');
    // The fixture must honor the production receipt guard's canonical directory
    // identity, including macOS's /var -> /private/var temporary-path alias.
    const root=await fs.realpath(await fs.mkdtemp(path.join(os.tmpdir(),'airk-default-service-')));
    const values=new Map<string,unknown>(),settings=new Map<string,unknown>(),commands=new Map<string,(...args:unknown[])=>Promise<unknown>>();
    let hostContext:{subscriptions:any[]}|undefined;
    const assistance=new Map<string,any>(),writes:any[]=[],calls:string[]=[],warnings:string[]=[],clipboard:string[]=[],panels:any[]=[],events:any[]=[],openedDocuments:any[]=[],notices:{message:string;actions:string[]}[]=[],progressRuns:{title:string;messages:string[]}[]=[];
    // Explorer Assistance listens on a real endpoint; only tests that exercise it turn it on.
    settings.set('explorer.assistance.enabled',options.assistance??false);
    const storage={keys:()=>[...values.keys()],get:<T>(key:string,fallback?:T)=>(values.has(key)?values.get(key):fallback) as T|undefined,update:async(key:string,value:unknown)=>{if(value===undefined)values.delete(key);else values.set(key,structuredClone(value));}};
    const intentStorage=()=>hostContext?.subscriptions.find(item=>item.console?.intentStorage)?.console.intentStorage??storage;
    const install=options.nativeSourceScope?.binding.installDir??path.join(root,'installation'),control=options.nativeSourceScope?.binding.controlDir??path.join(install,'control'),data=path.join(root,'data');await fs.mkdir(data);
    // A native Ready owner has already published its private HTTP directory.
    // Synthetic hosts model that prerequisite without claiming native ACL proof.
    if(!options.nativeSourceScope)await fs.mkdir(path.join(control,'http-1'),{recursive:true,mode:0o700});
    const candidateBinding=options.nativeSourceScope?.binding??{installDir:install,controlDir:control,executable:path.join(install,'runtimes','ab'.repeat(32),process.platform==='win32'?'airk.exe':'airk'),launcher:path.join(install,process.platform==='win32'?'airk-launcher.exe':'airk-launcher'),
        runtimeSha256:'ab'.repeat(32),version:options.runtimeVersion??'1.4.0',lifecycle:'on-demand' as const,verificationBasis:'trusted_distribution'};
    // A real schema-2 installation has no launcher. Preserve the actual main
    // and null launcher independently; historical synthetic defaults above are
    // intentionally unchanged and never relabel the main as a launcher.
    const binding:RuntimeBinding={...candidateBinding};
    const registrations=new Map<string,any>(),inactiveRegistrations=new Map<string,any>(),jobs=new Map<string,any>(),plans=new Map<string,any>();
    const folders:any[]=[];for(let i=0;i<(options.roots??1);i++){
        const external=i===0?options.nativeSourceScope:undefined;
        const source=external?.source??path.join(root,`source-${i}`);if(!external)await fs.mkdir(source);
        folders.push({name:external?.collection??`Source ${i}`,uri:{scheme:'file',fsPath:await fs.realpath(source)}});
    }
    settings.set('service.installDir',install);settings.set('storage.path',data);
    settings.set('cli.autoInstall',options.automatic??false);
    if(options.nativeExecution!==undefined)settings.set('runtime.nativeExecution',options.nativeExecution);
    const digest=(value:string)=>createHash('sha256').update(value).digest('hex');
    const review=async(_binding:unknown,source:string,data_root:string,collection:string)=>({installDir:binding.installDir,controlDir:binding.controlDir,runtimeSha256:binding.runtimeSha256,binding:digest(JSON.stringify([source,data_root,collection])),root:await fs.realpath(source),data_root,collection});
    const register=async(_binding:unknown,value:any,setupWatch?:boolean,onlyIfMissing=false)=>{
        if(onlyIfMissing&&inactiveRegistrations.has(value.binding))throw new ServiceError('project_was_removed_refresh_registration');
        registrations.set(value.binding,value);if(setupWatch!==undefined)writes.push({reserve_project:{project:value,setup_watch:setupWatch,...(onlyIfMissing?{only_if_missing:true}:{})}});return value;
    };
    if(options.registered!==false){
        const workspace=new ServiceWorkspace(storage,{reviewProject:review,registerProject:register},()=>true);
        for(const folder of folders)await workspace.register(binding,folder.uri.fsPath,data,folder.name.replaceAll(' ','-'),async()=>true);
    }
    const imported=(registration:any)=>({status:'catalog_legacy_binding',observation:{schema:1,namespace:'34'.repeat(16),catalog_revision:1,project:registration.binding,eligibility:{kind:'imported_snapshot',registration:{binding:registration.binding,root:registration.root,data_root:registration.data_root,collection:registration.collection},grant:{grant:{id:'grt_'+'91'.repeat(16),host:'56'.repeat(32),revision:1,revoked:false,memberships:[{membership:{project_id:'prj_'+'11'.repeat(16),index_id:'idx_'+'22'.repeat(16),revision:1,scope:[{source_root_id:'src_'+'33'.repeat(16),kind:'subtree',path:''}]},source_revision:1,policy_epoch:0,policy_digest:null}]},provenance:{kind:'imported_legacy_snapshot',upgrade_id:'44'.repeat(16),plan_digest:'55'.repeat(32),legacy_binding:registration.binding,configuration_digest:'66'.repeat(32)}}}}});
    const httpServer=https.createServer({cert,key},(_request,response)=>{response.writeHead(401);response.end();});
    await new Promise<void>(resolve=>httpServer.listen(0,'127.0.0.1',resolve));const httpUrl=`https://127.0.0.1:${(httpServer.address() as {port:number}).port}/mcp`;
    const closeHttp=async()=>{httpServer.closeAllConnections();await new Promise<void>(resolve=>httpServer.close(()=>resolve()));};test.after(closeHttp);
    const nativeMethods:string[]=[];
    const state={replacementPlan:undefined as undefined|Record<string,unknown>,replacedAt:undefined as undefined|number,nativeHealthFailures:0,nativeInstance:'12'.repeat(16),httpConnections:[] as {selection:HttpSelection;id:string;client:string;descriptor?:string}[],down:false,stopped:false,ackLost:false,confirm:true,discovered:0,ensured:0,picks:0,installedVersion:options.installedVersion??bundledVersion,
        hostContextKeys:[] as string[],hostContextReply:undefined as undefined|((key:string)=>unknown|Promise<unknown>),
        catalogDetailReply:undefined as undefined|((entity:Record<string,unknown>)=>unknown|Promise<unknown>),
        legacyBindingReply:undefined as undefined|((project:string)=>unknown|Promise<unknown>),legacyBindingProjects:[] as string[],
        pick:undefined as undefined|((items:any[])=>any),selectedFolders:undefined as undefined|any[],noticeAction:undefined as string|undefined};
    const disposable={dispose(){}},event=()=>disposable;
    let foldersChanged:()=>Promise<void>;
    let trustGranted:()=>void;
    const commandContexts=new Map<string,unknown>();
    let extensionsChanged:()=>void,windowFocused:(state:{focused:boolean})=>void;
    const assetWatchers:{pattern:any;disposed:boolean;change:()=>void;create:()=>void}[]=[];
    const views=new Map<string,any>();
    const statusBar={text:'',tooltip:'',command:'',show(){},dispose(){}};
    const stub:any={extensions:{getExtension:()=>({extensionPath:root,packageJSON:{version:state.installedVersion}}),onDidChange:(callback:()=>void)=>{extensionsChanged=callback;return disposable;}},workspace:{isTrusted:options.trusted??true,workspaceFolders:folders,textDocuments:[],
        getWorkspaceFolder:(uri:any)=>folders.find(folder=>uri.fsPath===folder.uri.fsPath||uri.fsPath.startsWith(folder.uri.fsPath+path.sep)),
        getConfiguration:()=>({get:(key:string,fallback?:unknown)=>settings.has(key)?settings.get(key):fallback,inspect:(key:string)=>({globalValue:settings.get(key),workspaceValue:key==='storage.path'?path.join(root,'untrusted-storage'):'/workspace/untrusted'}),
            update:async(key:string,value:unknown)=>{settings.set(key,value);}}),
        onDidChangeWorkspaceFolders:(callback:()=>Promise<void>)=>{foldersChanged=callback;return disposable;},onDidGrantWorkspaceTrust:(callback:()=>void)=>{trustGranted=callback;return disposable;},onDidChangeConfiguration:event,
        openTextDocument:async(value:any)=>{if(typeof value?.content!=='string')throw new Error('Fixture file editor not configured');openedDocuments.push(value);return value;},
        createFileSystemWatcher:(pattern:any)=>{
            assert.equal(pattern.base.fsPath,path.resolve(__dirname,'../..'),'editor watchers must stay inside the extension payload');
            assert.equal(pattern.pattern,'out/*.{js,css}','editor-owned source watcher is forbidden');
            const watcher={pattern,disposed:false,change:()=>{},create:()=>{}};assetWatchers.push(watcher);
            return {dispose(){watcher.disposed=true;},onDidChange:(callback:()=>void)=>{watcher.change=callback;return disposable;},onDidCreate:(callback:()=>void)=>{watcher.create=callback;return disposable;}};
        }},
    window:{activeTextEditor:{document:{uri:folders[0]?.uri}},createOutputChannel:()=>({appendLine:(text:string)=>calls.push(text),show(){calls.push('output-shown');},dispose(){}}),
        withProgress:async(options:any,task:any)=>{const run={title:options.title,messages:[] as string[]};progressRuns.push(run);return task({report:(value:any)=>run.messages.push(value.message)});},
        createStatusBarItem:()=>statusBar,createTreeView:()=>disposable,registerWebviewViewProvider:(id:string,provider:any)=>{views.set(id,provider);return disposable;},
        onDidChangeWindowState:(callback:(state:{focused:boolean})=>void)=>{windowFocused=callback;return disposable;},
        onDidChangeActiveTextEditor:event,showErrorMessage:async(message:string)=>calls.push(message),
        showInformationMessage:async(message:string,...actions:string[])=>{notices.push({message,actions});return state.noticeAction;},
        showWarningMessage:async(message:string,options:unknown,...actions:string[])=>{warnings.push(message);
            if(typeof options==='string'){notices.push({message,actions:[options,...actions]});return state.noticeAction;}
            return state.confirm?actions[0]:undefined;},
        showQuickPick:async(items:any[])=>{state.picks++;return state.pick?state.pick(items):items[0];},
        showOpenDialog:async()=>state.selectedFolders,
        createTerminal:()=>({show(){},dispose(){}}),
        createWebviewPanel:(viewType:string)=>{let handler:(message:unknown)=>Promise<void>,closed:()=>void;
            const panel={viewType,posted:[] as unknown[],webview:{html:'',cspSource:'vscode-resource:',asWebviewUri:(uri:any)=>uri,postMessage:async(message:unknown)=>{panel.posted.push(message);return true;},
                onDidReceiveMessage:(value:any)=>{handler=value;return disposable;}},onDidDispose:(value:any)=>{closed=value;return disposable;},onDidChangeViewState:()=>disposable,reveal(){},
                send:async(message:unknown)=>handler(message),dispose:()=>closed?.()};panels.push(panel);return panel;},
        showTextDocument:async()=>{},
    },
    commands:{registerCommand:(name:string,action:(...args:unknown[])=>Promise<unknown>)=>{commands.set(name,action);return disposable;},
        executeCommand:async(name:string,...args:unknown[])=>{calls.push(name);if(name==='setContext')commandContexts.set(String(args[0]),args[1]);},getCommands:async()=>['workbench.mcp.listServer']},
    lm:{registerMcpServerDefinitionProvider:(name:string,provider:unknown)=>{if(name==='airk.collections')events.push(provider);else assistance.set(name,provider);return disposable;}},
    env:{clipboard:{writeText:async(text:string)=>{clipboard.push(text);}}},StatusBarAlignment:{Left:1},ConfigurationTarget:{Global:1},ViewColumn:{Active:1,Beside:2},ProgressLocation:{Notification:15},EndOfLine:{LF:1,CRLF:2},
    EventEmitter:class{listeners:any[]=[];event=(listener:any)=>{this.listeners.push(listener);return disposable;};fire(value:any){for(const listener of this.listeners)listener(value);}dispose(){}},
    EnvironmentVariableMutatorType:{Replace:1},TreeItem:class{constructor(public label:string){}},Uri:{parse:(value:string)=>({toString:()=>value,scheme:'https'}),file:(fsPath:string)=>({scheme:'file',fsPath}),joinPath:(base:any,...parts:string[])=>({scheme:'file',fsPath:path.join(base?.fsPath??'',...parts),toString(){return this.fsPath;}})},
    Range:class{constructor(..._args:any[]){}},
    RelativePattern:class{constructor(public base:any,public pattern:string){}},
    WorkspaceEdit:class{uri:any;after='';replace(uri:any,_range:any,after:string){this.uri=uri;this.after=after;}},
    McpHttpServerDefinition:class{constructor(public label:string,public uri:any,public headers:any,public version:string){}},
    McpStdioServerDefinition:class{cwd:unknown;constructor(public label:string,public command:string,public args:string[],public env:unknown,public version:string){}}};
    const transport={dispose(){},async request(method:unknown):Promise<any>{
        const name=typeof method==='string'?method:Object.keys(record(method))[0];
        nativeMethods.push(name);
        if(state.down)throw new ServiceError('service_unavailable');
        if(['enqueue','configure_watch','pause','plan_create','plan_execute'].includes(name))writes.push(structuredClone(method));
        switch(name){
            case 'catalog_legacy_binding':{
                const project=String(record(record(method).catalog_legacy_binding).project);state.legacyBindingProjects.push(project);
                if(state.legacyBindingReply)return await state.legacyBindingReply(project);
                const registration=registrations.get(project);return registration?imported(registration):{status:'catalog_legacy_binding',observation:{schema:1,namespace:'34'.repeat(16),catalog_revision:null,project,eligibility:{kind:'unavailable'}}};
            }
            case 'catalog_detail':if(state.catalogDetailReply)return await state.catalogDetailReply(record(record(method).catalog_detail));throw new ServiceError('fixture_catalog_detail_unavailable');
            case 'catalog_host_context':{
                const key=String(record(record(method).catalog_host_context).key);state.hostContextKeys.push(key);
                return state.hostContextReply?await state.hostContextReply(key):{status:'catalog_host_context',descriptor:{schema:1,namespace:'34'.repeat(16),host:'56'.repeat(32),key,catalog_revision:1,context:null}};
            }
            case 'health':if(state.nativeHealthFailures>0){state.nativeHealthFailures--;throw new ServiceError('native_management_busy');}return {status:'health',observation:{instance:state.nativeInstance,version:binding.version,runtime_sha256:binding.runtimeSha256,policy:{accepting:true}}};
            case 'request_epoch':return {status:'request_epoch',namespace:'34'.repeat(16),epoch:1};
            case 'resources':return {status:'resources',observation:{configuration:{preset:'balanced'}}};
            case 'embedding_status':return {status:'embedding_status',available:false};
            case 'source_status':return {status:'source_status',available:false};
            case 'console':return {status:'console',page:{revision:1,cached:true,observed_unix_ms:1,after:null,projects:[...registrations.values()].map(registration=>({project:{registration,paused:false,fencing_generation:1}}))}};
            case 'inactive_console':return {status:'console',page:{revision:1,cached:true,observed_unix_ms:1,after:null,projects:[...inactiveRegistrations.values()].map(registration=>({project:{registration,paused:false,fencing_generation:1}}))}};
            case 'workers':return {status:'workers',observations:(method as any).workers.projects.map((project:string)=>({project,identity:null,restarting:false,automatic_start_blocked:false})),age_ms:0};
            case 'events':return {status:'events',page:{namespace:'34'.repeat(16),retained_prefix_lost:false,events:[],next:null,tail:{namespace:'34'.repeat(16),project:null,after:0,through:null}}};
            case 'enqueue':{const request=(method as any).enqueue;
                assert.ok(intentStorage().keys().some((key:string)=>key.startsWith('airk.service.intent.')),'request must be saved before submission');
                const id=digest(request.key.nonce),job={id,project:request.project,kind:request.kind,state:'pending'};jobs.set(id,job);
                if(state.ackLost)throw new ServiceError('service_request_outcome_unknown',true);return {status:'job',job};}
            case 'plan_create':{const request=(method as any).plan_create,id=digest(request.key.nonce),plan={id,preview_digest:'56'.repeat(32),counts:{selected:1},state:'ready',operation:request.request.operation};plans.set(id,plan);return {status:'plan',plan};}
            case 'plan':return {status:'plan',plan:plans.get((method as any).plan.id)};
            case 'plan_items':{const plan=plans.get((method as any).plan_items.id);return {status:'plan_items',page:{plan,items:[{project:[...registrations.keys()][0]}],after:null}};}
            default:return {status:name};
        }
    }};
    test.mock.method(RuntimeHost.prototype,'discover',async function(this:RuntimeHost){
        state.discovered++;assert.equal(this.options.installDir,install);
        await options.discoveryGate;
        // A fresh machine has no installed runtime until the first ensure.
        const fresh=options.fresh&&state.ensured===0||state.replacedAt!==undefined&&state.ensured===state.replacedAt;
        const cleanBreak=binding.cleanBreak?{...binding.cleanBreak,registrations_complete:true,registrations:[...registrations.values(),...inactiveRegistrations.values()]}:runtimeCleanBreakRequired(binding.version,bundledVersion)?{required:true,baseline_version:'1.8.16',state_contract:'airk-state/1',installed_contract:null,registrations_complete:true,registrations:[...registrations.values(),...inactiveRegistrations.values()]}:undefined;
        return {bundle:{main:path.join(root,'bundle-main'),helper:path.join(root,'bundle-helper'),source:root,digest:binding.runtimeSha256,version:bundledVersion,platform:'fixture'},
            installDir:install,installed:!fresh,location:{user_paths:{data,launcher:path.join(root,'launcher'),state:path.join(root,'native-state')}},inspection:fresh?undefined:{selection:{active:binding.runtimeSha256},approval:{manifest_sha256:binding.runtimeSha256,version:binding.version,verification_basis:binding.verificationBasis},
            executable:binding.executable,launcher:binding.launcher,control_dir:binding.controlDir,registry_namespace:'34'.repeat(16),lifecycle:{explicitly_stopped:state.stopped},...(cleanBreak?{clean_break:cleanBreak}:{}),...(binding.rebuild?{rebuild:binding.rebuild}:{})}};
    });
    test.mock.method(RuntimeHost.prototype,'ensure',async(explicit?:boolean)=>{state.ensured++;assert.notEqual(explicit,true);if(state.stopped)throw new ServiceError('service_explicitly_stopped');if(state.down)throw new ServiceError('service_unavailable');if(binding.version!==bundledVersion)throw new ServiceError('explicit_runtime_upgrade_or_matching_extension_required');return binding;});
    // This fixture owns native discovery at its existing boundary. Exact-scope
    // responses still pass the production parser and private provenance brand.
    test.mock.method(RuntimeHost.prototype,'discoverForConnection',async function(this:RuntimeHost){await options.connectionDiscoveryGate;return this.discover();});
    // Real-source qualification leaves invoke/NativeRunner entirely production.
    if(!options.nativeSourceScope)test.mock.method(RuntimeHost.prototype as any,'invoke',async(_command:string,args:string[])=>{
        assert.deepEqual(args.slice(0,2),['runtime','source-scope']);
        const source=args[args.indexOf('--source-root')+1];
        const collection=args.includes('--index-collection')?args[args.indexOf('--index-collection')+1]:null;
        const matches=[...registrations.values()].filter(row=>row.root===source&&(collection===null||row.collection===collection));
        const removed=[...inactiveRegistrations.values()].some(row=>row.root===source&&(collection===null||row.collection===collection));
        const outcome=matches.length>1?'ambiguous':matches.length===1?'registered':removed?'removed':'missing';
        return {install_dir:binding.installDir,control_dir:binding.controlDir,runtime_sha256:binding.runtimeSha256,
            scope:{schema:1,root:source,collection,namespace:'34'.repeat(16),revision:1,observed_unix_ms:1,outcome,
                registration:outcome==='registered'?matches[0]:null,fencing_generation:outcome==='registered'?1:null}};
    });
    const httpInfos=new Map<string,HttpConnectionInfo>(),privateMaterial=new Map<string,Promise<void>>();
    const publishPrivateMaterial=(filename:string,bytes:string|Buffer)=>{
        let pending=privateMaterial.get(filename);
        if(!pending){pending=fs.writeFile(filename,bytes,{mode:0o600,flag:'wx'}).catch(async error=>{if(error.code!=='EEXIST')throw error;assert.deepEqual(await fs.readFile(filename),Buffer.from(bytes));});privateMaterial.set(filename,pending);}
        return pending;
    };
    const httpInfo=async(selection:HttpSelection,id:string):Promise<HttpConnectionInfo>=>{
        // Source-scope qualification supplies a real native control directory.
        // Synthetic transport certificates/credentials must never overwrite it.
        if(options.nativeSourceScope)throw new ServiceError('fixture_http_enrollment_unavailable_for_native_control');
        const directory=path.join(control,'http-1');await fs.mkdir(directory,{recursive:true,mode:0o700});const caFile=path.join(directory,'ca.pem'),tokenFile=path.join(directory,`credential-${id}.token`);
        await publishPrivateMaterial(caFile,ca);await publishPrivateMaterial(tokenFile,`airk1.${id}.`+'a'.repeat(64));
        const info=validateHttpConnection({schema:1,connection_id:id,endpoint:{schema:1,installation:'aa'.repeat(32),user:'fixture',namespace:selection.selection.namespace,generation:1,service_instance:'12'.repeat(16),executable_sha256:'bb'.repeat(32),url:httpUrl,ca_sha256:createHash('sha256').update(new X509Certificate(ca).raw).digest('hex'),certificate_sha256:'cc'.repeat(32),state:'ready',predecessor_digest:null},selection,ca_certificate_file:caFile,credential_file:tokenFile},selection,directory);httpInfos.set(id,info);return info;
    };
    test.mock.method(RuntimeHost.prototype,'httpConnection',async(_binding:RuntimeBinding,selection:HttpSelection,_file:string,id:string,client:string,descriptor?:string)=>{state.httpConnections.push({selection:structuredClone(selection),id,client,descriptor});return httpInfo(selection,id);});
    test.mock.method(RuntimeHost.prototype,'httpLegacyConnection',async(_binding:RuntimeBinding,registration:{binding:string;root:string;data_root:string;collection:string},_file:string,receipt:string,id:string,client:string,descriptor?:string)=>{
        const response=state.legacyBindingReply?await state.legacyBindingReply(registration.binding):registrations.has(registration.binding)?imported(registrations.get(registration.binding)):undefined;
        const observation=record(record(response).observation),eligibility=record(observation.eligibility);
        if(eligibility.kind!=='imported_snapshot')throw new ServiceError('legacy_selection_review_required',false,response===undefined?undefined:record(response));
        const selection:HttpSelection={kind:'imported_legacy',selection:{namespace:String(observation.namespace),expected_catalog_revision:Number(observation.catalog_revision),registration,grant:eligibility.grant as Record<string,any>,configuration_receipt:receipt}};state.httpConnections.push({selection:structuredClone(selection),id,client,descriptor});return httpInfo(selection,id);
    });
    test.mock.method(RuntimeHost.prototype,'httpLegacyUnionConnection',async(_binding:RuntimeBinding,rows:{registration:{binding:string;root:string;data_root:string;collection:string};configuration_receipt:string}[],_file:string,id:string,client:string,descriptor?:string)=>{
        const selections=rows.map(row=>{const response=imported(row.registration);return {namespace:response.observation.namespace,expected_catalog_revision:response.observation.catalog_revision,registration:row.registration,grant:response.observation.eligibility.grant,configuration_receipt:row.configuration_receipt};});
        const selection:HttpSelection={kind:'imported_legacy_union',selection:{namespace:'34'.repeat(16),selections,no_default_project:true}};state.httpConnections.push({selection:structuredClone(selection),id,client,descriptor});return httpInfo(selection,id);
    });
    // Fixtures model existing installations explicitly; replacement is opt-in per test.
    test.mock.method(RuntimeHost.prototype,'replacementPlan',async()=>state.replacementPlan??{required:false,expected_predecessor:'ef'.repeat(32),reason:'same_runtime',locations:[]});
    test.mock.method(RuntimeHost.prototype,'replaceInstallation',async()=>{calls.push('installation-replaced');state.replacementPlan={required:false,expected_predecessor:'ef'.repeat(32),reason:'no_installation',locations:[]};state.replacedAt=state.ensured;binding.version=bundledVersion;binding.cleanBreak=undefined;binding.rebuild=undefined;return {replaced:true,removed:['fixture-previous'],residue:[],agents:'fixture agents notice'};});
    test.mock.method(RuntimeHost.prototype,'lookupHttpConnection',async(_binding:RuntimeBinding,id:string)=>{const connection=httpInfos.get(id);return {schema:1,active:_binding.runtimeSha256,connection_id:id,status:connection?'owned':'not_owned',...(connection?{connection:structuredClone(connection)}:{})};});
    let originalHttpSnapshot:any;
    const captureHttpSnapshot=(predecessor:string,originalControl:string)=>{
        const sample=[...httpInfos.values()][0],base=predecessorHttpFixture(originalControl,predecessor);
        if(sample){base.endpoint=structuredClone(sample.endpoint);base.snapshot.endpoint=structuredClone(sample.endpoint);base.snapshot.namespace=sample.endpoint.namespace;}
        base.snapshot.owners=[...httpInfos.values()].filter(info=>info.endpoint.namespace===base.snapshot.namespace).map(info=>({connection_id:info.connection_id,ownership:'expired' as const,credential_sha256:createHash('sha256').update(`airk1.${info.connection_id}.`+'a'.repeat(64)).digest('hex')}));
        return base.snapshot;
    };
    test.mock.method(RuntimeHost.prototype,'lookupPredecessorHttpOwnership',async(current:RuntimeBinding,id:string,scope:Parameters<RuntimeHost['lookupPredecessorHttpOwnership']>[2])=>{
        originalHttpSnapshot??=captureHttpSnapshot(scope.predecessor,scope.controlDir);
        const snapshot=structuredClone(originalHttpSnapshot),owner=snapshot.owners.find((row:any)=>row.connection_id===id),base=predecessorHttpFixture(scope.controlDir,scope.predecessor,id);
        return {...base,active:current.runtimeSha256,operation_id:scope.operationId??null,namespace:snapshot.namespace,endpoint:snapshot.endpoint,snapshot,status:owner?'owned':'not_owned',ownership:owner?.ownership??null,credential_sha256:owner?.credential_sha256??null,ca_certificate_file:owner?path.join(scope.controlDir,'http-1','ca.pem'):null,credential_file:owner?path.join(scope.controlDir,'http-1',`credential-${id}.token`):null,
            qualification:{...base.qualification,source:scope.operationId?'saved_predecessor_snapshot':'current_predecessor_metadata',current_namespace:scope.operationId?'78'.repeat(16):snapshot.namespace,current_namespace_ownership:scope.operationId?'not_owned':'same_predecessor'}} as any;
    });
    test.mock.method(RuntimeHost.prototype,'httpConnectionInfo',async(_binding:RuntimeBinding,id:string)=>{
        if(state.down||state.stopped)throw new ServiceError('fixture_http_unavailable');
        const info=httpInfos.get(id);if(!info)throw new ServiceError('http_connection_unavailable');return structuredClone(info);
    });
    test.mock.method(RuntimeHost.prototype,'client',()=>new ServiceClient(transport));
    test.mock.method(RuntimeHost.prototype,'migrationClient',async(selected:RuntimeBinding)=>{if(state.stopped)throw new ServiceError('service_explicitly_stopped');if(state.down)throw new ServiceError('service_unavailable');assert.equal(selected.runtimeSha256,binding.runtimeSha256);assert.equal(selected.version,binding.version);calls.push('selected-modern-metadata-verified');const client=new ServiceClient(transport);return {projects:(after:string|null=null,revision:number|null=null,limit=32,inactive=false)=>client.projects(after,revision,limit,inactive),dispose:()=>client.dispose()};});
    test.mock.method(globalInventory,'inventoryGlobalAgentConfigs',async()=>({files:[],checked:[],blockers:[]}));
    test.mock.method(RuntimeHost.prototype,'inspectLegacyLaunchers',async()=>({schema:1,active:binding.runtimeSha256,launcher:null,universal_launcher:null}));
    test.mock.method(RuntimeHost.prototype,'inspectLegacyCaches',async(selected:RuntimeBinding,contextGlobalStorage:string,locatorFile?:string)=>{if(locatorFile)JSON.parse(await fs.readFile(locatorFile,'utf8'));return {schema:1,active:selected.runtimeSha256,scope:'context-only',context_global_storage:contextGlobalStorage,global_storage_roots:[contextGlobalStorage],caches:[],origin_proofs:[],origin_locators:[],adoption_locators:[],managed_locators:[],origin_classifications:[],inspection_sha256:'fe'.repeat(32)};});
    test.mock.method(RuntimeHost.prototype,'prepareLegacyCaches',async(selected:RuntimeBinding,contextGlobalStorage:string,locatorFile?:string)=>{calls.push('legacy-caches-prepared');if(locatorFile)JSON.parse(await fs.readFile(locatorFile,'utf8'));return {schema:1,active:selected.runtimeSha256,scope:'context-only',context_global_storage:contextGlobalStorage,global_storage_roots:[contextGlobalStorage],caches:[],origin_proofs:[],origin_locators:[],adoption_locators:[],managed_locators:[],origin_classifications:[],inspection_sha256:'fe'.repeat(32)};});
    test.mock.method(RuntimeHost.prototype,'reprepareLegacyCaches',async(_selected:RuntimeBinding,previous:RecordValue)=>previous);
    test.mock.method(RuntimeHost.prototype,'retireLegacyCaches',async(selected:RuntimeBinding,prepared:RecordValue)=>{assert.equal(prepared.active,selected.runtimeSha256);calls.push('legacy-caches-retired');return {schema:1,active:selected.runtimeSha256,scope:prepared.scope,global_storage_roots:prepared.global_storage_roots,caches_retired:true,remaining:0,inspection_sha256:prepared.inspection_sha256,retired_caches:prepared.caches,processes:{scope:'verified_legacy_cache_images',remaining:0,observations:[],image_roster:[],image_exclusions:[]}};});
    test.mock.method(RuntimeHost.prototype,'retireLegacyLaunchers',async()=>{calls.push('legacy-launchers-retired');return {schema:1,active:binding.runtimeSha256,legacy_launchers_retired:true,launcher:null,universal_launcher:null,processes:{scope:'verified_launcher_images',remaining:0,observations:[]}};});
    test.mock.method(RuntimeHost.prototype,'retirePrevious',async()=>{calls.push('previous-runtime-retired');return {active:binding.runtimeSha256,retired_previous:null};});
    test.mock.method(RuntimeHost.prototype,'reviewTransition',async(current:typeof binding,action:'upgrade'|'rollback'|'resume')=>{
        assert.equal(current.runtimeSha256,binding.runtimeSha256);assert.equal(action,'upgrade');calls.push('upgrade-reviewed');
        return {action,installDir:install,controlDir:control,observedActive:binding.runtimeSha256,expectedActive:binding.runtimeSha256,target:binding.runtimeSha256,version:bundledVersion,...(runtimeCleanBreakRequired(current.version,bundledVersion,current.cleanBreak)?{cleanBreak:true,cleanBreakPolicy:current.cleanBreak}:{})};
    });
    test.mock.method(RuntimeHost.prototype,'transition',async()=>{
        calls.push('upgrade-applied');const cleanBreak=runtimeCleanBreakRequired(binding.version,bundledVersion,binding.cleanBreak);binding.version=bundledVersion;
        if(cleanBreak){
            const originalControl=binding.controlDir,originalExecutable=binding.executable;binding.controlDir=path.join(root,'fresh-control');
            // Stop belongs to its registry namespace. Genesis starts a fresh
            // lifecycle; a later Stop applied after this transition remains set.
            state.stopped=false;
            originalHttpSnapshot??=captureHttpSnapshot(binding.runtimeSha256,originalControl);
            binding.cleanBreak={required:false,baseline_version:'1.8.16',state_contract:'airk-state/1',installed_contract:{schema:1,fixture:true},registrations_complete:true,registrations:[...registrations.values(),...inactiveRegistrations.values()]};
            binding.rebuild={schema:1,operation_id:'ee'.repeat(16),stage:'ready',mode:'clean_break',indexes_complete:true,expected_indexes:registrations.size,cleanup:{complete:true,state:'complete',failure:null,observation:{fixture:true}},provenance:'native_rebuild_from_saved_configuration',
                candidate:binding.runtimeSha256,predecessor:binding.runtimeSha256,original_namespace:'34'.repeat(16),successor_namespace:'78'.repeat(16),preserved_state:path.join(root,'native-state'),preserved_control:originalControl,
                preserved_index_roots:[...registrations.values()].map(row=>row.data_root),successor_state:path.join(root,'fresh-state'),successor_control:binding.controlDir,original_failure:null,failure:null,index_scope:null,
                indexes:[...registrations.values()].map((original,index)=>{const replacement={...original,binding:(index+100).toString(16).padStart(64,'0'),data_root:path.join(root,'fresh-index-'+index)};return {original,replacement,sources:{source:original.root},settings_sha256:'56'.repeat(32),original_paused:false,paused:false,watching:false,job:{id:(index+200).toString(16).padStart(64,'0'),project:replacement.binding,kind:'index',state:'succeeded',fencing_generation:1,attempts:1,outcome:{state:'succeeded',revision:null}},readiness:'ready',failure:null};})};
            binding.cleanBreak={...binding.cleanBreak,http_ownership_snapshot:structuredClone(originalHttpSnapshot),predecessor_client_metadata:{schema:1,operation_id:binding.rebuild.operation_id,predecessor:binding.runtimeSha256,control_dir:originalControl,namespace:'34'.repeat(16),predecessor_executable:{path:originalExecutable,sha256:'bb'.repeat(32)},legacy_launchers:{schema:1,active:binding.runtimeSha256,launcher:null,universal_launcher:null,legacy_runtime_images:[]},command_images:[{path:originalExecutable,sha256:'bb'.repeat(32)}]}};
        }
        return {};
    });
    test.mock.method(RuntimeHost.prototype,'reviewProject',review);
    test.mock.method(RuntimeHost.prototype,'registerProject',register);
    test.mock.method(cliInstall,'installLauncherCli',async()=>[]);
    // The activation opens its real window session, but this fixture owns native
    // discovery, so no verified bundle image is ever bound to it. Commands that
    // reach the session keep the one-shot seam (`native.runText`) individual
    // tests already observe; session transport and lifetime have their own suite.
    test.mock.method(NativeSession.prototype,'serves',async()=>true);
    test.mock.method(NativeSession.prototype,'text',(command:string,args:readonly string[],timeout?:number,input?:string)=>
        native.runText(command,[...args],path.dirname(command),timeout,undefined,undefined,input));
    test.mock.method(native,'prepareHelper',async()=>{throw new Error('Legacy helper preparation is forbidden');});
    test.mock.method(native,'runJson',async()=>{throw new Error('Direct collection command is forbidden');});
    const loader=Module as unknown as {_load:(...args:any[])=>any},original=loader._load;
    const modules=['../extension','../extensionUpdateLoader','../serviceExtension','../serviceConsole','../agentSetup','../explorerController','../explorerPanel','../workbenchView','../usagePanel'];
    const clear=()=>{for(const name of modules)delete require.cache[require.resolve(name)];};clear();
    loader._load=function(request:string,...args:any[]){return request==='vscode'?stub:original.call(this,request,...args);};
    if(options.workspaceFile){const workspaceFile=path.join(root,'fixture.code-workspace');await fs.writeFile(workspaceFile,JSON.stringify({folders:folders.map(folder=>({path:folder.uri.fsPath}))}));stub.workspace.workspaceFile=stub.Uri.file(workspaceFile);}
    let extension:any;
    // Real installations carry compiled code; same-version rebuild detection stats it.
    if(options.codeFile){await fs.mkdir(path.join(root,'out'),{recursive:true});for(const name of ['serviceExtension.js','explorerWebview.js','explorerWebview.css','workbenchWebview.js','workbenchWebview.css'])await fs.writeFile(path.join(root,'out',name),'// fixture build 1\n');}
    try{extension=require('../extension');const context:any={extensionMode:2,extensionPath:root,extensionUri:{scheme:'file',fsPath:path.resolve(__dirname,'../..')},extension:{id:'iokaio.airk',packageJSON:{version:bundledVersion}},globalState:storage,workspaceState:storage,
        globalStorageUri:{fsPath:path.join(root,'editor-storage')},subscriptions:[],secrets:{store:async()=>{}},environmentVariableCollection:{persistent:false,get:()=>undefined,replace(){},prepend(){}}};
        await fs.mkdir(context.globalStorageUri.fsPath,{recursive:true});
        hostContext=context;const api=await extension.activate(context);
        if(!options.deferStartup)await api.whenReady();
        return {httpUrl,nativeMethods,views,root,folders,binding,registrations,inactiveRegistrations,values,get intentStorage(){return intentStorage();},settings,commands,commandContexts,writes,calls,warnings,clipboard,panels,openedDocuments,state,stub,api,context,providers:events,assistance,notices,progressRuns,statusBar,
            assetWatchers,assetChanged(){for(const watcher of assetWatchers)if(!watcher.disposed)watcher.change();},assetCreated(){for(const watcher of assetWatchers)if(!watcher.disposed)watcher.create();},
            extensionsChanged(){extensionsChanged!();},focus(){windowFocused!({focused:true});},
            async refreshFolders(){await foldersChanged!();},
            grantTrust(){stub.workspace.isTrusted=true;trustGranted!();},
            async close(){for(const subscription of context.subscriptions)subscription.dispose();extension.deactivate();loader._load=original;clear();await fs.rm(root,{recursive:true,force:true});}};
    }catch(error){extension?.deactivate();loader._load=original;clear();await fs.rm(root,{recursive:true,force:true});throw error;}
}
