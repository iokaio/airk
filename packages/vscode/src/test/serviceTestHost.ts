// SPDX-License-Identifier: Apache-2.0
//! Native metadata fixture for the default service extension; no real user settings or helpers.
import type { TestContext } from 'node:test';
import { strict as assert } from 'node:assert';
import Module from 'node:module';
import { createHash } from 'node:crypto';
import * as fs from 'node:fs/promises';
import * as os from 'node:os';
import * as path from 'node:path';
import { RuntimeHost } from '../serviceBootstrap';
import { ServiceClient, ServiceError, record } from '../serviceClient';
import { ServiceWorkspace } from '../serviceWorkspace';
import * as native from '../native';
import * as cliInstall from '../cliInstall';

export async function serviceTestHost(test:TestContext,options:{trusted?:boolean;roots?:number;registered?:boolean;automatic?:boolean;upgrade?:boolean}={}){
    const root=await fs.mkdtemp(path.join(os.tmpdir(),'airk-default-service-'));
    const values=new Map<string,unknown>(),settings=new Map<string,unknown>(),commands=new Map<string,(...args:unknown[])=>Promise<unknown>>();
    const writes:any[]=[],calls:string[]=[],warnings:string[]=[],clipboard:string[]=[],panels:any[]=[],events:any[]=[],notices:{message:string;actions:string[]}[]=[],progressRuns:{title:string;messages:string[]}[]=[];
    const storage={keys:()=>[...values.keys()],get:<T>(key:string,fallback?:T)=>(values.has(key)?values.get(key):fallback) as T|undefined,update:async(key:string,value:unknown)=>{if(value===undefined)values.delete(key);else values.set(key,structuredClone(value));}};
    const install=path.join(root,'installation'),control=path.join(install,'control'),data=path.join(root,'data');await fs.mkdir(data);
    const binding={installDir:install,controlDir:control,executable:path.join(install,'main'),launcher:path.join(install,'airk-launcher'),
        runtimeSha256:'ab'.repeat(32),version:'1.4.0',lifecycle:'on-demand' as const,verificationBasis:'trusted_distribution'};
    const registrations=new Map<string,any>(),jobs=new Map<string,any>(),plans=new Map<string,any>();
    const folders:any[]=[];for(let i=0;i<(options.roots??1);i++){
        const source=path.join(root,`source-${i}`);await fs.mkdir(source);folders.push({name:`Source ${i}`,uri:{scheme:'file',fsPath:await fs.realpath(source)}});
    }
    settings.set('service.installDir',install);settings.set('storage.path',data);
    settings.set('cli.autoInstall',options.automatic??false);
    const digest=(value:string)=>createHash('sha256').update(value).digest('hex');
    const review=async(_binding:unknown,source:string,data_root:string,collection:string)=>({...binding,binding:digest(JSON.stringify([source,data_root,collection])),root:await fs.realpath(source),data_root,collection});
    const register=async(_binding:unknown,value:any)=>{registrations.set(value.binding,value);return value;};
    if(options.registered!==false){
        const workspace=new ServiceWorkspace(storage,{reviewProject:review,registerProject:register},()=>true);
        for(const folder of folders)await workspace.register(binding,folder.uri.fsPath,data,folder.name.replaceAll(' ','-'),async()=>true);
    }
    const state={down:false,stopped:false,ackLost:false,confirm:true,discovered:0,ensured:0,picks:0,
        pick:undefined as undefined|((items:any[])=>any),selectedFolders:undefined as undefined|any[],noticeAction:undefined as string|undefined};
    const disposable={dispose(){}},event=()=>disposable;
    let foldersChanged:()=>Promise<void>;
    const stub:any={workspace:{isTrusted:options.trusted??true,workspaceFolders:folders,textDocuments:[],
        getWorkspaceFolder:(uri:any)=>folders.find(folder=>uri.fsPath===folder.uri.fsPath||uri.fsPath.startsWith(folder.uri.fsPath+path.sep)),
        getConfiguration:()=>({inspect:(key:string)=>({globalValue:settings.get(key),workspaceValue:key==='storage.path'?path.join(root,'untrusted-storage'):'/workspace/untrusted'}),
            update:async(key:string,value:unknown)=>{settings.set(key,value);}}),
        onDidChangeWorkspaceFolders:(callback:()=>Promise<void>)=>{foldersChanged=callback;return disposable;},onDidGrantWorkspaceTrust:event,onDidChangeConfiguration:event,
        createFileSystemWatcher:()=>{throw new Error('Editor-owned source watcher is forbidden');}},
    window:{activeTextEditor:{document:{uri:folders[0]?.uri}},createOutputChannel:()=>({appendLine:(text:string)=>calls.push(text),show(){calls.push('output-shown');},dispose(){}}),
        withProgress:async(options:any,task:any)=>{const run={title:options.title,messages:[] as string[]};progressRuns.push(run);return task({report:(value:any)=>run.messages.push(value.message)});},
        createStatusBarItem:()=>({show(){},dispose(){}}),createTreeView:()=>disposable,registerWebviewViewProvider:()=>disposable,
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
        executeCommand:async(name:string)=>calls.push(name),getCommands:async()=>['workbench.mcp.listServer']},
    lm:{registerMcpServerDefinitionProvider:(_name:string,provider:unknown)=>{events.push(provider);return disposable;}},
    env:{clipboard:{writeText:async(text:string)=>{clipboard.push(text);}}},StatusBarAlignment:{Left:1},ConfigurationTarget:{Global:1},ViewColumn:{Active:1,Beside:2},ProgressLocation:{Notification:15},
    EventEmitter:class{listeners:any[]=[];event=(listener:any)=>{this.listeners.push(listener);return disposable;};fire(){for(const listener of this.listeners)listener();}dispose(){}},
    TreeItem:class{constructor(public label:string){}},Uri:{file:(fsPath:string)=>({scheme:'file',fsPath}),joinPath:(base:any,...parts:string[])=>({scheme:'file',fsPath:path.join(base?.fsPath??'',...parts),toString(){return this.fsPath;}})},
    Range:class{constructor(..._args:any[]){}},
    WorkspaceEdit:class{uri:any;after='';replace(uri:any,_range:any,after:string){this.uri=uri;this.after=after;}},
    McpStdioServerDefinition:class{cwd:unknown;constructor(public label:string,public command:string,public args:string[],public env:unknown,public version:string){}}};
    const transport={dispose(){},async request(method:unknown):Promise<any>{
        const name=typeof method==='string'?method:Object.keys(record(method))[0];
        if(state.down)throw new ServiceError('service_unavailable');
        if(['enqueue','configure_watch','pause','plan_create','plan_execute'].includes(name))writes.push(structuredClone(method));
        switch(name){
            case 'health':return {status:'health',observation:{instance:'12'.repeat(16),version:'1.4.0'}};
            case 'request_epoch':return {status:'request_epoch',namespace:'34'.repeat(16),epoch:1};
            case 'resources':return {status:'resources',observation:{configuration:{preset:'balanced'}}};
            case 'embedding_status':return {status:'embedding_status',available:false};
            case 'source_status':return {status:'source_status',available:false};
            case 'console':return {status:'console',page:{revision:1,cached:true,observed_unix_ms:1,after:null,projects:[...registrations.values()].map(registration=>({project:{registration,paused:false,fencing_generation:1}}))}};
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
        return {bundle:{main:path.join(root,'bundle-main'),helper:path.join(root,'bundle-helper'),source:root,digest:binding.runtimeSha256,version:'1.4.0',platform:'fixture'},
            installDir:install,installed:true,location:{user_paths:{data}},inspection:{selection:{active:binding.runtimeSha256},approval:{manifest_sha256:binding.runtimeSha256,version:binding.version,verification_basis:'trusted_distribution'},
            executable:binding.executable,launcher:binding.launcher,control_dir:control,registry_namespace:'34'.repeat(16),lifecycle:{explicitly_stopped:state.stopped}}};
    });
    test.mock.method(RuntimeHost.prototype,'ensure',async(explicit?:boolean)=>{state.ensured++;assert.notEqual(explicit,true);if(state.stopped)throw new ServiceError('service_explicitly_stopped');if(state.down)throw new ServiceError('service_unavailable');return binding;});
    test.mock.method(RuntimeHost.prototype,'client',()=>new ServiceClient(transport));
    test.mock.method(RuntimeHost.prototype,'retirePrevious',async()=>{calls.push('previous-runtime-retired');});
    test.mock.method(RuntimeHost.prototype,'reviewTransition',async(current:typeof binding,action:'upgrade'|'rollback'|'resume')=>{
        assert.equal(current.runtimeSha256,binding.runtimeSha256);assert.equal(action,'upgrade');calls.push('upgrade-reviewed');
        return {action,installDir:install,controlDir:control,observedActive:binding.runtimeSha256,expectedActive:binding.runtimeSha256,target:binding.runtimeSha256,version:'1.4.1'};
    });
    test.mock.method(RuntimeHost.prototype,'transition',async()=>{calls.push('upgrade-applied');binding.version='1.4.1';return {};});
    test.mock.method(RuntimeHost.prototype,'reviewProject',review);
    test.mock.method(RuntimeHost.prototype,'registerProject',register);
    test.mock.method(cliInstall,'installLauncherCli',async()=>[]);
    test.mock.method(native,'prepareHelper',async()=>{throw new Error('Legacy helper preparation is forbidden');});
    test.mock.method(native,'runJson',async()=>{throw new Error('Direct collection command is forbidden');});
    const loader=Module as unknown as {_load:(...args:any[])=>any},original=loader._load;
    const modules=['../extension','../serviceExtension','../serviceConsole','../agentSetup'];
    const clear=()=>{for(const name of modules)delete require.cache[require.resolve(name)];};clear();
    loader._load=function(request:string,...args:any[]){return request==='vscode'?stub:original.call(this,request,...args);};
    let extension:any;
    try{extension=require('../extension');const context:any={extensionPath:root,extension:{packageJSON:{version:options.upgrade?'1.4.1':'1.4.0'}},globalState:storage,
        globalStorageUri:{fsPath:path.join(root,'editor-storage')},subscriptions:[],environmentVariableCollection:{persistent:false,prepend(){}}};
        const api=await extension.activate(context);
        return {root,folders,binding,registrations,values,settings,commands,writes,calls,warnings,clipboard,panels,state,stub,api,providers:events,notices,progressRuns,
            async refreshFolders(){await foldersChanged!();},
            async close(){extension.deactivate();loader._load=original;clear();await fs.rm(root,{recursive:true,force:true});}};
    }catch(error){extension?.deactivate();loader._load=original;clear();await fs.rm(root,{recursive:true,force:true});throw error;}
}
