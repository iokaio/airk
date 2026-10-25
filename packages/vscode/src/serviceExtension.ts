// SPDX-License-Identifier: Apache-2.0
//! Service-backed extension activation. Native ownership outlives editor windows.
import * as vscode from 'vscode';
import * as fs from 'node:fs/promises';
import * as path from 'node:path';
import { setupAgent, AgentSetupHost } from './agentSetup';
import { collectionId, launchArgs, LaunchSpec } from './model';
import { probeMcp, runText } from './native';
import { RuntimeBinding, RuntimeHost } from './serviceBootstrap';
import { ConsoleRuntime, ServiceConsole, serviceRuntimeOptions } from './serviceConsole';
import { identifier, record, ServiceError } from './serviceClient';
import { IntentJournal } from './serviceIntents';
import { ServiceWorkspace } from './serviceWorkspace';
import { McpVerification } from './sidebar';
import { installLauncherCli } from './cliInstall';
import { openUsagePanel } from './usagePanel';

function same(left:string,right:string):boolean {
    return path.relative(process.platform==='win32'?path.toNamespacedPath(left):left,process.platform==='win32'?path.toNamespacedPath(right):right)==='';
}
class ServiceExtension implements vscode.Disposable,AgentSetupHost,vscode.TreeDataProvider<LaunchSpec> {
    readonly output=vscode.window.createOutputChannel('AIRK');
    readonly definitions=new vscode.EventEmitter<void>();
    readonly treeChanges=new vscode.EventEmitter<void>();
    readonly onDidChangeTreeData=this.treeChanges.event;
    readonly console:ServiceConsole;
    private readonly status=vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Left,40);
    private readonly workspace:ServiceWorkspace;
    private host?:RuntimeHost;
    private binding?:RuntimeBinding;
    private generation=0;
    private disposed=false;
    private specs:LaunchSpec[]=[];
    private roots:{opened:string;canonical:string}[]=[];
    private readonly checks=new Map<string,McpVerification>();
    private refreshing?:Promise<void>;

    constructor(private readonly context:vscode.ExtensionContext){
        const runtime:ConsoleRuntime={discover:()=>this.runtime().discover(),ensure:explicit=>this.runtime().ensure(explicit),
            binding:(inspection,install)=>this.runtime().binding(inspection,install),client:binding=>this.runtime().client(binding),
            snapshot:(binding,after,revision)=>this.runtime().snapshot(binding,after,revision),repair:(binding,preview)=>this.runtime().repair(binding,preview),
            prepareConfiguration:(binding,project,draft)=>this.runtime().prepareConfiguration(binding,project,draft),
            reviewTransition:(binding,action)=>this.runtime().reviewTransition(binding,action),transition:(binding,review)=>this.runtime().transition(binding,review),
            lifecycle:(binding,intent,force)=>this.runtime().lifecycle(binding,intent,force),abandonLifecycle:(binding,intent,id)=>this.runtime().abandonLifecycle(binding,intent,id)};
        this.workspace=new ServiceWorkspace(context.globalState,{reviewProject:(...args)=>this.runtime().reviewProject(...args),
            registerProject:(...args)=>this.runtime().registerProject(...args)},()=>vscode.workspace.isTrusted&&!this.disposed);
        this.console=new ServiceConsole(context,runtime,{register:()=>this.registerWorkspace(),currentProjects:()=>this.currentProjects()});
        this.status.text='AIRK';this.status.command='airk.menu';this.status.show();
    }
    private runtime():RuntimeHost{
        if(!this.host){const generation=this.generation;
            this.host=new RuntimeHost(this.context.extensionPath,this.context.extension.packageJSON.version,serviceRuntimeOptions(),undefined,
                ()=>vscode.workspace.isTrusted&&!this.disposed&&generation===this.generation);}
        return this.host;
    }
    trusted():void {if(!vscode.workspace.isTrusted||this.disposed)throw new ServiceError('runtime_client_no_longer_authorized');}
    private folders():readonly vscode.WorkspaceFolder[]{
        return (vscode.workspace.workspaceFolders??[]).filter(folder=>['file','vscode-remote'].includes(folder.uri.scheme));
    }
    private currentProjects():ReadonlySet<string>{
        const active=vscode.window.activeTextEditor&&vscode.workspace.getWorkspaceFolder(vscode.window.activeTextEditor.document.uri);
        const root=active&&this.roots.find(root=>same(root.opened,active.uri.fsPath));
        return new Set(this.specs.filter(spec=>!root||same(spec.root,root.canonical)).map(spec=>spec.service!.project));
    }
    rootBindings():ReadonlyArray<{opened:string;canonical:string}>{return this.roots;}
    registrationEnabled():boolean{return vscode.workspace.getConfiguration('airk').inspect<boolean>('mcp.registrationEnabled')?.globalValue??true;}
    private responseMode():'terse'|'full'{return vscode.workspace.getConfiguration('airk').inspect<string>('mcp.responseMode')?.globalValue==='full'?'full':'terse';}
    registrationChanged():void{this.definitions.fire();}
    isCurrent(spec:LaunchSpec):boolean{return !this.disposed&&this.specs.some(current=>current.command===spec.command&&JSON.stringify(launchArgs(current))===JSON.stringify(launchArgs(spec)));}
    subscribe(panel:vscode.WebviewPanel):void{this.context.subscriptions.push(panel);}
    verification(spec:LaunchSpec):McpVerification|undefined{return this.checks.get(spec.service!.project);}
    async refresh():Promise<void>{
        if(this.refreshing)return this.refreshing;
        this.refreshing=this.refreshOnce().finally(()=>{this.refreshing=undefined;});return this.refreshing;
    }
    private async refreshOnce():Promise<void>{
        const previous=JSON.stringify(this.specs.map(spec=>[spec.command,launchArgs(spec)]));
        this.specs=[];this.roots=[];this.binding=undefined;
        try{
            this.trusted();const observation=await this.runtime().discover();this.trusted();
            if(!observation.inspection||observation.inspection.payload_verified===false||observation.inspection.metadata_only===true){this.status.text='AIRK: setup';return;}
            const binding=this.runtime().binding(observation.inspection,observation.installDir);
            if(binding.version!==this.context.extension.packageJSON.version)throw new ServiceError('explicit_runtime_upgrade_or_matching_extension_required');
            this.binding=binding;
            for(const folder of this.folders()){
                try{this.roots.push({opened:folder.uri.fsPath,canonical:await fs.realpath(folder.uri.fsPath)});}catch{/* Saved missing-source bindings remain readable. */}
            }
            this.specs=await this.workspace.definitions(binding,this.folders().map(folder=>folder.uri.fsPath));this.trusted();
            this.status.text=`AIRK: ${this.specs.length} registered`;
        }catch(error){this.status.text='AIRK: inspect service';this.output.appendLine(String(error));}
        finally{this.treeChanges.fire();if(previous!==JSON.stringify(this.specs.map(spec=>[spec.command,launchArgs(spec)])))this.definitions.fire();}
    }
    async registerWorkspace():Promise<void>{
        this.trusted();
        const folder=await vscode.window.showQuickPick(this.folders().map(folder=>({label:folder.name,description:folder.uri.fsPath,folder})),
            {title:'Register a trusted source root with AIRK'});if(!folder)return;this.trusted();
        const old=this.context.globalState.get<Record<string,unknown>>('collections-1.2',{});
        const explanation=`AIRK 1.4 uses a new service and fresh index namespace. Existing indexes, jobs and old agent entries are preserved and require separate review. ${Object.keys(old).length} legacy collection records are known on this extension host. Runtime installation and fresh indexes require additional disk space; indexing reads the selected sources. Login startup remains off.`;
        if(await vscode.window.showWarningMessage(explanation,{modal:true},'Review registration')!=='Review registration')return;this.trusted();
        const mode=await vscode.window.showQuickPick([
            {label:'Automatic source watching',description:'Native indexing continues after editor windows close',enabled:true},
            {label:'Manual indexing',description:'Only explicit indexing requests capture sources',enabled:false},
        ],{title:'AIRK indexing ownership',placeHolder:'The native service owns indexing; this is separate from login startup'});if(!mode)return;this.trusted();
        const observation=await this.runtime().discover();this.trusted();
        const configured=vscode.workspace.getConfiguration('airk').inspect<string>('storage.path')?.globalValue;
        const nativeData=record(observation.location.user_paths).data;
        const dataRoot=configured||nativeData;
        if(typeof dataRoot!=='string'||!path.isAbsolute(dataRoot))throw new ServiceError('invalid_project_data_location');
        const binding=await this.runtime().ensure();this.trusted();
        const canonical=await fs.realpath(folder.folder.uri.fsPath);this.trusted();
        const registered=await this.workspace.register(binding,folder.folder.uri.fsPath,dataRoot,collectionId(canonical),async review=>{
            const detail=`Source: ${review.root}\nIndex data: ${review.data_root}\nCollection: ${review.collection}\nProject binding: ${review.binding}\n${mode.enabled?'Automatic work continues after windows close.':'Automatic source watching is disabled.'}\nSet up the shared offline MiniLM model if this collection has no model; preserve an existing selection.\nEvery 1.4 index is rebuilt from sources; old state is not converted.`;
            return await vscode.window.showWarningMessage('Register this exact native project and start fresh indexing?',{modal:true,detail},'Register')==='Register';
        });if(!registered)return;this.trusted();
        const client=this.runtime().client(binding);
        try{
            await client.rpc({configure_watch:{project:registered.binding,enabled:mode.enabled}});this.trusted();
            await this.enqueue(binding,registered.binding,'model_setup',client);
            await this.enqueue(binding,registered.binding,'index',client);
        }finally{client.dispose();}
        await this.refresh();
        this.output.appendLine(`Registered ${registered.binding}. Native indexing continues independently; inspect its readiness in the Service Console.`);
    }
    private async enqueue(binding:RuntimeBinding,project:string,kind:'index'|'inspect'|'model_setup',existing?:ReturnType<RuntimeHost['client']>):Promise<void>{
        this.trusted();const client=existing??this.runtime().client(binding);
        try{
            const intent=await client.prepareJob(project,kind);this.trusted();
            const journal=new IntentJournal(this.context.globalState,binding.controlDir,intent.key.namespace);
            const id=await journal.prepare(`${kind} ${project}`,{enqueue:intent});this.trusted();
            const result=await journal.submit(client,id);this.trusted();
            this.output.appendLine(`Native ${kind} job accepted: ${identifier(record(result.job).id,32)}. Completion/readiness is separate.`);
        }finally{if(!existing)client.dispose();}
    }
    async chooseSpec(requested?:LaunchSpec):Promise<LaunchSpec|undefined>{
        this.trusted();await this.refresh();
        if(requested!==undefined){
            if(!this.isCurrent(requested))throw new ServiceError('workspace_registration_changed');
            return this.specs.find(spec=>spec.service!.project===requested.service!.project);
        }
        if(!this.specs.length){await this.registerWorkspace();return this.specs[0];}
        if(this.specs.length===1)return this.specs[0];
        const active=vscode.window.activeTextEditor&&vscode.workspace.getWorkspaceFolder(vscode.window.activeTextEditor.document.uri);
        const root=active&&this.roots.find(root=>same(root.opened,active.uri.fsPath));
        const selected=root&&this.specs.filter(spec=>same(spec.root,root.canonical));
        if(selected?.length===1)return selected[0];
        return (await vscode.window.showQuickPick(this.specs.map(spec=>({label:spec.name,description:spec.root,spec})),{title:'AIRK registered project'}))?.spec;
    }
    async index(requested?:LaunchSpec):Promise<void>{
        await this.refresh();if(requested===undefined&&!this.specs.length){await this.registerWorkspace();await this.console.command('connect');return;}
        const spec=await this.chooseSpec(requested);if(!spec)return;this.trusted();
        const binding=await this.runtime().ensure();this.trusted();await this.enqueue(binding,spec.service!.project,'index');
        await this.console.command('connect');
    }
    async watching():Promise<void>{
        const spec=await this.chooseSpec();if(!spec)return;
        const selected=await vscode.window.showQuickPick([{label:'Automatic; continues after windows close',enabled:true},{label:'Manual',enabled:false}],{title:'Native source watching'});
        if(!selected)return;this.trusted();const binding=await this.runtime().ensure(),client=this.runtime().client(binding);
        try{this.trusted();await client.rpc({configure_watch:{project:spec.service!.project,enabled:selected.enabled}});this.trusted();}finally{client.dispose();}
        await this.console.command('connect');
    }
    async repair(operation:'rebuild'|'auto'|'verify',requested?:LaunchSpec):Promise<void>{
        const spec=await this.chooseSpec(requested);if(!spec)return;this.trusted();await this.runtime().ensure();this.trusted();
        await this.console.previewRepair(spec.service!.project,operation);
    }
    async pause(paused:boolean,requested?:LaunchSpec):Promise<void>{
        const spec=await this.chooseSpec(requested);if(!spec)return;this.trusted();const binding=await this.runtime().ensure(),client=this.runtime().client(binding);
        try{this.trusted();await client.rpc({pause:{project:spec.service!.project,paused}});this.trusted();}finally{client.dispose();}
        await this.console.command('connect');
    }
    async verify(spec?:LaunchSpec):Promise<void>{
        const selected=spec??await this.chooseSpec();if(!selected)return;this.trusted();
        if(!this.isCurrent(selected))throw new ServiceError('workspace_registration_changed');
        try{const result=await probeMcp(selected,15,this.context.extension.packageJSON.version);this.trusted();
            this.checks.set(selected.service!.project,{phase:'verified',tools:result.tools,checkedAt:Date.now(),
                indexState:record(result.status).state==='not_indexed'?'not_indexed':'published'});
            this.output.appendLine(`Verified ${result.tools.length} MCP tools through the installed launcher. Agent connection is separate.`);
        }catch(error){this.checks.set(selected.service!.project,{phase:'failed',checkedAt:Date.now(),error:String(error).slice(0,600)});throw error;}
    }
    async openTools():Promise<void>{
        const spec=await this.chooseSpec();if(!spec)return;this.trusted();await this.runtime().ensure();this.trusted();
        const terminal=vscode.window.createTerminal({name:`AIRK Tools: ${spec.name}`,shellPath:spec.command,shellArgs:[...launchArgs(spec,'tools'),'shell'],
            cwd:path.dirname(spec.command),env:{AIRK_CLIENT:'cli',AIRK_MCP_RESPONSE_MODE:this.responseMode()}});
        this.context.subscriptions.push(terminal);terminal.show();
    }
    async showTool(tool:'airk_status'):Promise<void>{
        const spec=await this.chooseSpec();if(!spec)return;this.trusted();await this.runtime().ensure();this.trusted();
        const text=await runText(spec.command,[...launchArgs(spec,'tools'),'call',tool,'--args',JSON.stringify({response_mode:this.responseMode()})],path.dirname(spec.command),180000);
        this.trusted();this.output.appendLine(JSON.stringify(JSON.parse(text),null,2));this.output.show(true);
    }
    async usage():Promise<void>{
        const spec=await this.chooseSpec();if(!spec)return;this.trusted();await this.runtime().ensure();this.trusted();
        const generation=this.generation,valid=()=>generation===this.generation&&this.isCurrent(spec);
        await openUsagePanel(spec,valid,async(args,continuing)=>{
            this.trusted();if(!valid()||continuing?.()===false)throw new ServiceError('project_configuration_selection_changed');
            let draft:Record<string,unknown>;
            if(args.length===1&&args[0]==='clear')draft={operation:'clear_usage'};
            else if(args.length===3&&args[0]==='configure'&&args[1]==='--mode'&&['disabled','aggregates_only','diagnostic_events'].includes(args[2]))draft={operation:'usage',mode:args[2]};
            else if(args.length===4&&args[0]==='feedback'&&args[2]==='--usefulness'&&['useful','partly-useful','not-useful'].includes(args[3])){
                identifier(args[1],16);draft={operation:'usage_feedback',request_id:args[1],usefulness:args[3].replaceAll('-','_')};
            }else if(args.length===4&&args[0]==='import'&&args[2]==='--if-sha256'&&path.isAbsolute(args[1])){
                identifier(args[3],32);draft={operation:'import_usage',path:args[1],expected_sha256:args[3]};
            }else throw new ServiceError('invalid_usage_action');
            if(!spec.service)throw new ServiceError('native_project_binding_required');
            await this.console.configureProject(spec.service.project,draft,()=>valid()&&continuing?.()!==false);
        });
    }
    async installCommandLine():Promise<void>{
        this.trusted();const observation=await this.runtime().discover();this.trusted();
        if(!observation.inspection&&await vscode.window.showWarningMessage('Install AIRK’s native service runtime? It uses a fresh index namespace and requires additional disk space. Existing indexes and agent entries are preserved; source registration and indexing require separate review.',{modal:true},'Install runtime')!=='Install runtime')return;
        this.trusted();const binding=await this.runtime().ensure();this.trusted();
        if(!binding.launcher)throw new ServiceError('stable_launcher_unavailable');
        if(await vscode.window.showWarningMessage(`Add the approved launcher directory to your user PATH? ${path.dirname(binding.launcher)}. Existing CLI entries and running legacy processes are preserved.`,{modal:true},'Add launcher PATH')!=='Add launcher PATH')return;
        this.trusted();const errors=await installLauncherCli(binding.launcher,this.context);this.trusted();
        if(errors.length)throw new Error(errors.join('\n'));
        void vscode.window.showInformationMessage('AIRK launcher PATH setup complete. Open a new terminal and use airk-launcher. Restart external terminal applications to refresh PATH.');
    }
    async manageVscodeMcp():Promise<void>{this.trusted();await vscode.commands.executeCommand('workbench.mcp.listServer');}
    async provideDefinitions():Promise<vscode.McpStdioServerDefinition[]>{
        if(!vscode.workspace.isTrusted||!this.registrationEnabled())return [];
        await this.refresh();if(!vscode.workspace.isTrusted||!this.registrationEnabled())return [];
        return this.specs.map(spec=>{const definition=new vscode.McpStdioServerDefinition(`AIRK: ${spec.name} (${spec.collection})`,spec.command,launchArgs(spec),
            {AIRK_CLIENT:'vscode',AIRK_MCP_RESPONSE_MODE:this.responseMode()},`${this.context.extension.packageJSON.version}:${spec.service!.project}:${this.responseMode()}`);
            definition.cwd=vscode.Uri.file(path.dirname(spec.command));return definition;});
    }
    async resolveDefinition(definition:vscode.McpServerDefinition):Promise<vscode.McpServerDefinition>{
        this.trusted();if(!this.registrationEnabled())throw new ServiceError('mcp_registration_disabled');
        await this.refresh();this.trusted();
        if(!(definition instanceof vscode.McpStdioServerDefinition)||!this.specs.some(spec=>spec.command===definition.command&&JSON.stringify(launchArgs(spec))===JSON.stringify(definition.args)))throw new ServiceError('workspace_registration_changed');
        await this.runtime().ensure();this.trusted();if(!this.registrationEnabled())throw new ServiceError('mcp_registration_disabled');return definition;
    }
    getTreeItem(spec:LaunchSpec):vscode.TreeItem {
        const item=new vscode.TreeItem(spec.name);item.id=spec.service!.project;item.description=spec.collection;item.tooltip=spec.root;item.contextValue='airkServiceProject';
        item.command={command:'airk.serviceConnect',title:'Open Service Console'};return item;
    }
    getChildren():LaunchSpec[]{return this.specs;}
    reset():void{this.generation++;this.host=undefined;this.specs=[];this.binding=undefined;this.checks.clear();this.console.reset();void this.refresh();}
    dispose():void{if(this.disposed)return;this.disposed=true;this.generation++;this.console.dispose();this.status.dispose();this.definitions.dispose();this.treeChanges.dispose();this.output.dispose();}
}

let active:ServiceExtension|undefined;
export function deactivate():void{active?.dispose();active=undefined;}
/** Activated by the final 1.4 adapter cutover, after native/package contract checks. */
export async function activate(context:vscode.ExtensionContext):Promise<{getServerDefinitions():Promise<vscode.McpStdioServerDefinition[]>}>{
    const controller=new ServiceExtension(context);active=controller;context.subscriptions.push(controller);
    context.subscriptions.push(vscode.window.registerWebviewViewProvider('airk.service',controller.console),
        vscode.window.registerWebviewViewProvider('airk.dashboard',{resolveWebviewView(view){
            view.webview.options={enableCommandUris:['airk.index','airk.serviceConnect','airk.setupAgent']};
            view.webview.html='<!doctype html><html><head><meta charset="utf-8"><meta http-equiv="Content-Security-Policy" content="default-src \'none\'"></head><body><h1>AIRK</h1><p>Register trusted roots with the native service. Choose manual indexing or automatic watching that continues after editor windows close.</p><p><a href="command:airk.index">Review workspace registration / index</a></p><p><a href="command:airk.serviceConnect">Open Service Console</a></p><p><a href="command:airk.setupAgent">Connect a coding agent</a></p><p>New service indexes use a fresh namespace. Existing indexes and agent entries remain available for separate review.</p></body></html>';
        }}),
        vscode.window.createTreeView('airk.collectionsView',{treeDataProvider:controller}));
    const safe=(action:(...args:unknown[])=>unknown|Promise<unknown>)=>async(...args:unknown[])=>{try{return await action(...args);}catch(error){controller.output.appendLine(String(error));
        void vscode.window.showErrorMessage(`AIRK: ${error instanceof Error?error.message:String(error)}`);throw error;}};
    const consoleAction=(action='connect')=>()=>controller.console.command(action);
    const commands:Record<string,(...args:unknown[])=>unknown|Promise<unknown>>={index:()=>controller.index(),indexMode:()=>controller.watching(),pause:()=>controller.pause(true),resume:()=>controller.pause(false),
        setupAgent:()=>setupAgent(controller),manageMcp:()=>setupAgent(controller),manageVscodeMcp:()=>controller.manageVscodeMcp(),verifyMcp:()=>controller.verify(),
        logs:consoleAction('logs'),serviceConnect:consoleAction(),serviceStart:consoleAction('start'),serviceRestart:consoleAction('restart'),serviceStop:consoleAction('stop'),
        serviceLogs:consoleAction('logs'),serviceResources:consoleAction('resources'),serviceDiagnostics:consoleAction('previewReport'),treeRefresh:()=>controller.refresh(),
        treeIndex:spec=>controller.index(spec as LaunchSpec|undefined),treePause:spec=>controller.pause(true,spec as LaunchSpec|undefined)};
    for(const name of ['menu','reindex','diagnose','collections','removeCollection','currentMaintenance','extractionPolicy','configureExtractionPolicy','ingestionLimits','configureIngestionLimits','status','usage','storageLimits','treeReindex','treeRemove'])commands[name]=consoleAction();
    commands.reindex=()=>controller.repair('rebuild');commands.treeReindex=spec=>controller.repair('rebuild',spec as LaunchSpec|undefined);
    commands.diagnose=()=>controller.repair('verify');commands.currentMaintenance=()=>controller.repair('auto');
    commands.status=()=>controller.showTool('airk_status');
    commands.chooseStorage=async()=>{controller.trusted();const selected=await vscode.window.showOpenDialog({canSelectFolders:true,canSelectFiles:false,canSelectMany:false,openLabel:'Review new project storage'});
        if(selected?.length){controller.trusted();await vscode.workspace.getConfiguration('airk').update('storage.path',selected[0].fsPath,vscode.ConfigurationTarget.Global);
            await controller.registerWorkspace();}};
    commands.installCli=()=>controller.installCommandLine();
    commands.openTools=()=>controller.openTools();
    commands.usage=()=>controller.usage();
    for(const [name,action] of Object.entries(commands))context.subscriptions.push(vscode.commands.registerCommand(`airk.${name}`,safe(action)));
    if(vscode.lm?.registerMcpServerDefinitionProvider)context.subscriptions.push(vscode.lm.registerMcpServerDefinitionProvider('airk.collections',{
        onDidChangeMcpServerDefinitions:controller.definitions.event,provideMcpServerDefinitions:()=>controller.provideDefinitions(),resolveMcpServerDefinition:value=>controller.resolveDefinition(value)}));
    context.subscriptions.push(vscode.workspace.onDidChangeWorkspaceFolders(()=>void controller.refresh()),vscode.workspace.onDidGrantWorkspaceTrust(()=>void controller.refresh()),
        vscode.window.onDidChangeActiveTextEditor(()=>void controller.console.controller.refresh()),vscode.workspace.onDidChangeConfiguration(event=>{
            if(event.affectsConfiguration('airk.service'))controller.reset();
            else if(event.affectsConfiguration('airk.mcp'))controller.registrationChanged();
        }));
    await controller.refresh();return {getServerDefinitions:()=>controller.provideDefinitions()};
}
