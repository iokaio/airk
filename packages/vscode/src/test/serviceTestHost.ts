// SPDX-License-Identifier: Apache-2.0
//! Native metadata fixture for the default service extension; no real user settings or helpers.
import type { TestContext } from 'node:test';
import { strict as assert } from 'node:assert';
import Module from 'node:module';
import { createHash } from 'node:crypto';
import * as fs from 'node:fs/promises';
import * as os from 'node:os';
import * as path from 'node:path';
import { RuntimeHost, type RuntimeBinding } from '../serviceBootstrap';
import { ServiceClient, ServiceError, record } from '../serviceClient';
import { ServiceWorkspace } from '../serviceWorkspace';
import * as native from '../native';
import * as cliInstall from '../cliInstall';

export async function serviceTestHost(test:TestContext,options:{trusted?:boolean;roots?:number;workspaceFile?:boolean;registered?:boolean;automatic?:boolean;upgrade?:boolean;assistance?:boolean;installedVersion?:string;runtimeVersion?:string;bundledVersion?:string;discoveryGate?:Promise<void>;deferStartup?:boolean;nativeExecution?:boolean;fresh?:boolean;codeFile?:boolean;nativeSourceScope?:{binding:RuntimeBinding;source:string;collection:string}}={}){
    const bundledVersion=options.bundledVersion??(options.upgrade?'1.4.1':'1.4.0');
    // The fixture must honor the production receipt guard's canonical directory
    // identity, including macOS's /var -> /private/var temporary-path alias.
    const root=await fs.realpath(await fs.mkdtemp(path.join(os.tmpdir(),'airk-default-service-')));
    const values=new Map<string,unknown>(),settings=new Map<string,unknown>(),commands=new Map<string,(...args:unknown[])=>Promise<unknown>>();
    const assistance=new Map<string,any>(),writes:any[]=[],calls:string[]=[],warnings:string[]=[],clipboard:string[]=[],panels:any[]=[],events:any[]=[],openedDocuments:any[]=[],notices:{message:string;actions:string[]}[]=[],progressRuns:{title:string;messages:string[]}[]=[];
    // Explorer Assistance listens on a real endpoint; only tests that exercise it turn it on.
    settings.set('explorer.assistance.enabled',options.assistance??false);
    const storage={keys:()=>[...values.keys()],get:<T>(key:string,fallback?:T)=>(values.has(key)?values.get(key):fallback) as T|undefined,update:async(key:string,value:unknown)=>{if(value===undefined)values.delete(key);else values.set(key,structuredClone(value));}};
    const install=options.nativeSourceScope?.binding.installDir??path.join(root,'installation'),control=options.nativeSourceScope?.binding.controlDir??path.join(install,'control'),data=path.join(root,'data');await fs.mkdir(data);
    const candidateBinding=options.nativeSourceScope?.binding??{installDir:install,controlDir:control,executable:path.join(install,'main'),launcher:path.join(install,'airk-launcher'),
        runtimeSha256:'ab'.repeat(32),version:options.runtimeVersion??'1.4.0',lifecycle:'on-demand' as const,verificationBasis:'trusted_distribution'};
    assert.ok(candidateBinding.launcher,'service fixture requires its installed stable launcher');
    const binding={...candidateBinding,launcher:candidateBinding.launcher};
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
    const review=async(_binding:unknown,source:string,data_root:string,collection:string)=>({...binding,binding:digest(JSON.stringify([source,data_root,collection])),root:await fs.realpath(source),data_root,collection});
    const register=async(_binding:unknown,value:any,setupWatch?:boolean,onlyIfMissing=false)=>{
        if(onlyIfMissing&&inactiveRegistrations.has(value.binding))throw new ServiceError('project_was_removed_refresh_registration');
        registrations.set(value.binding,value);if(setupWatch!==undefined)writes.push({reserve_project:{project:value,setup_watch:setupWatch,...(onlyIfMissing?{only_if_missing:true}:{})}});return value;
    };
    if(options.registered!==false){
        const workspace=new ServiceWorkspace(storage,{reviewProject:review,registerProject:register},()=>true);
        for(const folder of folders)await workspace.register(binding,folder.uri.fsPath,data,folder.name.replaceAll(' ','-'),async()=>true);
    }
    const nativeMethods:string[]=[];
    const state={down:false,stopped:false,ackLost:false,confirm:true,discovered:0,ensured:0,picks:0,installedVersion:options.installedVersion??bundledVersion,
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
    EventEmitter:class{listeners:any[]=[];event=(listener:any)=>{this.listeners.push(listener);return disposable;};fire(){for(const listener of this.listeners)listener();}dispose(){}},
    TreeItem:class{constructor(public label:string){}},Uri:{file:(fsPath:string)=>({scheme:'file',fsPath}),joinPath:(base:any,...parts:string[])=>({scheme:'file',fsPath:path.join(base?.fsPath??'',...parts),toString(){return this.fsPath;}})},
    Range:class{constructor(..._args:any[]){}},
    RelativePattern:class{constructor(public base:any,public pattern:string){}},
    WorkspaceEdit:class{uri:any;after='';replace(uri:any,_range:any,after:string){this.uri=uri;this.after=after;}},
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
                const registration=registrations.get(project);return {status:'catalog_legacy_binding',observation:{schema:1,namespace:'34'.repeat(16),catalog_revision:null,project,eligibility:registration?{kind:'authority_zero',registration}:{kind:'unavailable'}}};
            }
            case 'catalog_detail':if(state.catalogDetailReply)return await state.catalogDetailReply(record(record(method).catalog_detail));throw new ServiceError('fixture_catalog_detail_unavailable');
            case 'catalog_host_context':{
                const key=String(record(record(method).catalog_host_context).key);state.hostContextKeys.push(key);
                return state.hostContextReply?await state.hostContextReply(key):{status:'catalog_host_context',descriptor:{schema:1,namespace:'34'.repeat(16),host:'56'.repeat(32),key,catalog_revision:1,context:null}};
            }
            case 'health':return {status:'health',observation:{instance:'12'.repeat(16),version:binding.version,runtime_sha256:binding.runtimeSha256,policy:{accepting:true}}};
            case 'request_epoch':return {status:'request_epoch',namespace:'34'.repeat(16),epoch:1};
            case 'resources':return {status:'resources',observation:{configuration:{preset:'balanced'}}};
            case 'embedding_status':return {status:'embedding_status',available:false};
            case 'source_status':return {status:'source_status',available:false};
            case 'console':return {status:'console',page:{revision:1,cached:true,observed_unix_ms:1,after:null,projects:[...registrations.values()].map(registration=>({project:{registration,paused:false,fencing_generation:1}}))}};
            case 'inactive_console':return {status:'console',page:{revision:1,cached:true,observed_unix_ms:1,after:null,projects:[...inactiveRegistrations.values()].map(registration=>({project:{registration,paused:false,fencing_generation:1}}))}};
            case 'workers':return {status:'workers',observations:(method as any).workers.projects.map((project:string)=>({project,identity:null,restarting:false,automatic_start_blocked:false})),age_ms:0};
            case 'events':return {status:'events',page:{namespace:'34'.repeat(16),retained_prefix_lost:false,events:[],next:null,tail:{namespace:'34'.repeat(16),project:null,after:0,through:null}}};
            case 'enqueue':{const request=(method as any).enqueue;
                assert.ok([...values.keys()].some(key=>key.startsWith('airk.service.intent.')),'request must be saved before submission');
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
        const fresh=options.fresh&&state.ensured===0;
        return {bundle:{main:path.join(root,'bundle-main'),helper:path.join(root,'bundle-helper'),source:root,digest:binding.runtimeSha256,version:bundledVersion,platform:'fixture'},
            installDir:install,installed:!fresh,location:{user_paths:{data,launcher:path.join(root,'launcher'),state:path.join(root,'native-state')}},inspection:fresh?undefined:{selection:{active:binding.runtimeSha256},approval:{manifest_sha256:binding.runtimeSha256,version:binding.version,verification_basis:binding.verificationBasis},
            executable:binding.executable,launcher:binding.launcher,control_dir:control,registry_namespace:'34'.repeat(16),lifecycle:{explicitly_stopped:state.stopped}}};
    });
    test.mock.method(RuntimeHost.prototype,'ensure',async(explicit?:boolean)=>{state.ensured++;assert.notEqual(explicit,true);if(state.stopped)throw new ServiceError('service_explicitly_stopped');if(state.down)throw new ServiceError('service_unavailable');if(binding.version!==bundledVersion)throw new ServiceError('explicit_runtime_upgrade_or_matching_extension_required');return binding;});
    // This fixture owns native discovery at its existing boundary. Exact-scope
    // responses still pass the production parser and private provenance brand.
    test.mock.method(RuntimeHost.prototype,'discoverForConnection',function(this:RuntimeHost){return this.discover();});
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
    test.mock.method(RuntimeHost.prototype,'client',()=>new ServiceClient(transport));
    test.mock.method(RuntimeHost.prototype,'retirePrevious',async()=>{calls.push('previous-runtime-retired');});
    test.mock.method(RuntimeHost.prototype,'reviewTransition',async(current:typeof binding,action:'upgrade'|'rollback'|'resume')=>{
        assert.equal(current.runtimeSha256,binding.runtimeSha256);assert.equal(action,'upgrade');calls.push('upgrade-reviewed');
        return {action,installDir:install,controlDir:control,observedActive:binding.runtimeSha256,expectedActive:binding.runtimeSha256,target:binding.runtimeSha256,version:bundledVersion};
    });
    test.mock.method(RuntimeHost.prototype,'transition',async()=>{calls.push('upgrade-applied');binding.version=bundledVersion;return {};});
    test.mock.method(RuntimeHost.prototype,'reviewProject',review);
    test.mock.method(RuntimeHost.prototype,'registerProject',register);
    test.mock.method(cliInstall,'installLauncherCli',async()=>[]);
    test.mock.method(native,'prepareHelper',async()=>{throw new Error('Legacy helper preparation is forbidden');});
    test.mock.method(native,'runJson',async()=>{throw new Error('Direct collection command is forbidden');});
    const loader=Module as unknown as {_load:(...args:any[])=>any},original=loader._load;
    const modules=['../extension','../serviceExtension','../serviceConsole','../agentSetup','../explorerController','../explorerPanel','../workbenchView','../usagePanel'];
    const clear=()=>{for(const name of modules)delete require.cache[require.resolve(name)];};clear();
    loader._load=function(request:string,...args:any[]){return request==='vscode'?stub:original.call(this,request,...args);};
    if(options.workspaceFile){const workspaceFile=path.join(root,'fixture.code-workspace');await fs.writeFile(workspaceFile,JSON.stringify({folders:folders.map(folder=>({path:folder.uri.fsPath}))}));stub.workspace.workspaceFile=stub.Uri.file(workspaceFile);}
    let extension:any;
    // Real installations carry compiled code; same-version rebuild detection stats it.
    if(options.codeFile){await fs.mkdir(path.join(root,'out'),{recursive:true});for(const name of ['serviceExtension.js','explorerWebview.js','explorerWebview.css','workbenchWebview.js','workbenchWebview.css'])await fs.writeFile(path.join(root,'out',name),'// fixture build 1\n');}
    try{extension=require('../extension');const context:any={extensionPath:root,extensionUri:{scheme:'file',fsPath:path.resolve(__dirname,'../..')},extension:{id:'iokaio.airk',packageJSON:{version:bundledVersion}},globalState:storage,workspaceState:storage,
        globalStorageUri:{fsPath:path.join(root,'editor-storage')},subscriptions:[],environmentVariableCollection:{persistent:false,prepend(){}}};
        const api=await extension.activate(context);
        if(!options.deferStartup)await api.whenReady();
        return {nativeMethods,views,root,folders,binding,registrations,inactiveRegistrations,values,settings,commands,commandContexts,writes,calls,warnings,clipboard,panels,openedDocuments,state,stub,api,context,providers:events,assistance,notices,progressRuns,statusBar,
            assetWatchers,assetChanged(){for(const watcher of assetWatchers)if(!watcher.disposed)watcher.change();},assetCreated(){for(const watcher of assetWatchers)if(!watcher.disposed)watcher.create();},
            extensionsChanged(){extensionsChanged!();},focus(){windowFocused!({focused:true});},
            async refreshFolders(){await foldersChanged!();},
            grantTrust(){stub.workspace.isTrusted=true;trustGranted!();},
            async close(){for(const subscription of context.subscriptions)subscription.dispose();extension.deactivate();loader._load=original;clear();await fs.rm(root,{recursive:true,force:true});}};
    }catch(error){extension?.deactivate();loader._load=original;clear();await fs.rm(root,{recursive:true,force:true});throw error;}
}
