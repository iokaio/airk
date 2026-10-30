// SPDX-License-Identifier: Apache-2.0
//! Service-backed extension activation. Native ownership outlives editor windows.
import * as vscode from 'vscode';
import * as fs from 'node:fs/promises';
import * as os from 'node:os';
import * as path from 'node:path';
import { setupAgent, AgentSetupHost } from './agentSetup';
import { collectionId, launchArgs, LaunchSpec, McpVerification, unifiedEntry, ServerEntry } from './model';
import { probeMcp, runText } from './native';
import { automaticUpgrade, BootstrapObservation, RuntimeBinding, RuntimeHost } from './serviceBootstrap';
import { ConsoleRuntime, ServiceConsole, serviceRuntimeOptions } from './serviceConsole';
import { identifier, record, ServiceError } from './serviceClient';
import { IntentJournal } from './serviceIntents';
import { readWorkspaceRegistrations, renamedDefaultDataRoot, ServiceWorkspace } from './serviceWorkspace';
import { installLauncherCli, shellIntegration, ShellTouch } from './cliInstall';
import { defaultProfiles, removeShellIntegration } from './cliRemove';
import { openUsagePanel } from './usagePanel';
import { renderUsage } from './usageReport';
import { COMMAND_ROUTES } from './explorerCommands';
import { ExplorerSidebar } from './explorerSidebar';
import { WorkbenchView } from './workbenchView';
import type { ExplorerController, ExplorerHost } from './explorerController';
import { hasUnifiedVscodeConfig, migrateLocalAgents } from './agentMigration';
import { readJsonConfig } from './agentConfig';
import { AssistanceIpcHost } from './explorerAssistanceHost';
import { HandoffStore } from './explorerHandoffs';
import { SuggestionStore } from './explorerSuggestions';
import { assistanceDirectory, launchDescriptorPath, writeLaunchDescriptor } from './explorerAssistanceLaunch';
import { rememberShellIntegration, rememberUninstallTarget, shellProfiles } from './extensionUninstall';
import { recordDiagnosticFailure } from './diagnosticLogs';
import { nativeExecutionAllowed, NativeExecutionDisabled, setNativeExecutionGate } from './nativeGate';

/** User-scope only: a workspace cannot turn native execution back on. */
function nativeExecutionSetting():boolean{
    return vscode.workspace.getConfiguration('airk').inspect<boolean>('runtime.nativeExecution')?.globalValue!==false;
}

function same(left:string,right:string):boolean {
    return path.relative(process.platform==='win32'?path.toNamespacedPath(left):left,process.platform==='win32'?path.toNamespacedPath(right):right)==='';
}
class ServiceExtension implements vscode.Disposable,AgentSetupHost,vscode.TreeDataProvider<LaunchSpec> {
    // Capture the loaded code's version; extension metadata can change after a VSIX install.
    private readonly runningVersion:string;
    private extensionUpdate?:{runningVersion:string;installedVersion:string};
    readonly output=vscode.window.createOutputChannel('AIRK');
    readonly definitions=new vscode.EventEmitter<void>();
    readonly assistanceDefinitions=new vscode.EventEmitter<void>();
    readonly treeChanges=new vscode.EventEmitter<void>();
    readonly onDidChangeTreeData=this.treeChanges.event;
    readonly console:ServiceConsole;
    private readonly status=vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Left,40);
    private readonly workspace:ServiceWorkspace;
    private host?:RuntimeHost;
    private uninstallDirectory?:string;
    private binding?:RuntimeBinding;
    private diagnosticObservation?:BootstrapObservation;
    private generation=0;
    private disposed=false;
    private specs:LaunchSpec[]=[];
    private roots:{opened:string;canonical:string}[]=[];
    private readonly checks=new Map<string,McpVerification>();
    private connectionCheck?:McpVerification;
    private checking?:Promise<void>;
    private startup?:{message:string;startedAt:number;failed?:boolean};
    private refreshing?:Promise<void>;
    private observedFolders?:string;
    private settingUp?:Promise<void>;
    private migratedConnections=false;
    private installationWarnings=false;
    private installationFailed=false;
    private installProgress?:vscode.Progress<{message?:string}>;
    private explorer?:Promise<ExplorerController>;
    private explorerLoaded?:ExplorerController;
    private workbench?:WorkbenchView;
    private assistanceHost?:AssistanceIpcHost;
    private assistanceGeneration=0;
    /** Agent drafts for this window, kept in memory only; the Explorer Inbox shows them. */
    readonly suggestions=new SuggestionStore({binding:()=>this.assistanceHost?.bindingFingerprint??'',revisions:()=>this.explorerLoaded?.draftRevisions??{},
        publication:()=>this.explorerLoaded?.publication,
        ttlMs:()=>Math.min(168,Math.max(1,vscode.workspace.getConfiguration('airk').get<number>('explorer.suggestions.ttlHours',24)||24))*3600_000});

    private reportInstall(message:string):void{
        if(this.disposed)return;
        if(this.startup){this.startup={...this.startup,message};this.treeChanges.fire();}
        this.installProgress?.report({message});this.output.appendLine(message);
    }
    private async withInstallProgress(action:()=>Promise<void>,title=`Checking AIRK ${this.runningVersion} installation`):Promise<void>{
        if(this.installProgress)return action();
        await vscode.window.withProgress({location:vscode.ProgressLocation.Notification,title,cancellable:false},async progress=>{
            this.installProgress=progress;
            try{await action();}finally{this.installProgress=undefined;}
        });
    }

    checkForExtensionUpdate():boolean{
        if(this.disposed)return false;
        const installed=vscode.extensions?.getExtension(this.context.extension.id??'iokaio.airk')?.packageJSON.version;
        if(!this.extensionUpdate&&typeof installed==='string'&&/^\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?$/.test(installed)&&installed!==this.runningVersion){
            this.extensionUpdate={runningVersion:this.runningVersion,installedVersion:installed};
            this.output.appendLine(`AIRK ${installed} is installed; this window is still running ${this.runningVersion}. Reload to finish the update. Saved indexing work remains with the native service.`);
            this.renderUpdateStatus();this.treeChanges.fire();this.definitions.fire();
            void vscode.window.showWarningMessage(`AIRK ${installed} is installed. Reload this window to replace the running ${this.runningVersion} interface. Saved indexing work is retained.`,
                'Reload to finish update').then(async action=>{
                if(this.disposed)return;
                if(action==='Reload to finish update')await this.reloadUpdatedExtension();
            }).then(undefined,error=>this.output.appendLine(`Update action failed: ${String(error)}`));
        }
        return !!this.extensionUpdate;
    }
    private renderUpdateStatus():boolean{
        if(!this.extensionUpdate)return false;
        this.status.text='$(sync) AIRK: Reload to finish update';this.status.command='airk.reloadUpdatedExtension';
        this.status.tooltip=`AIRK ${this.extensionUpdate.installedVersion} installed; ${this.runningVersion} running. Click to reload this window. Saved indexing work is retained.`;
        return true;
    }
    async reloadUpdatedExtension():Promise<void>{
        if(!this.disposed&&this.extensionUpdate)await vscode.commands.executeCommand('workbench.action.reloadWindow');
    }

    constructor(private readonly context:vscode.ExtensionContext){
        this.runningVersion=context.extension.packageJSON.version;
        const runtime:ConsoleRuntime={discover:()=>this.runtime().discover(),ensure:explicit=>this.runtime().ensure(explicit),
            binding:(inspection,install)=>this.runtime().binding(inspection,install),client:binding=>this.runtime().client(binding),
            snapshot:(binding,after,revision)=>this.runtime().snapshot(binding,after,revision),repair:(binding,preview)=>this.runtime().repair(binding,preview),
            prepareConfiguration:(binding,project,draft)=>this.runtime().prepareConfiguration(binding,project,draft),
            reviewLegacyStartup:(binding,target)=>this.runtime().reviewLegacyStartup(binding,target),loadLegacyStartup:(binding,approval)=>this.runtime().loadLegacyStartup(binding,approval),
            legacyStartupReceipts:binding=>this.runtime().legacyStartupReceipts(binding),removeLegacyStartup:(binding,review)=>this.runtime().removeLegacyStartup(binding,review),
            reviewTransition:(binding,action)=>this.runtime().reviewTransition(binding,action),transition:(binding,review)=>this.runtime().transition(binding,review),
            lifecycle:(binding,intent,force)=>this.runtime().lifecycle(binding,intent,force),abandonLifecycle:(binding,intent,id)=>this.runtime().abandonLifecycle(binding,intent,id)};
        this.workspace=new ServiceWorkspace(context.globalState,{reviewProject:(...args)=>this.runtime().reviewProject(...args),
            registerProject:(...args)=>this.runtime().registerProject(...args)},()=>vscode.workspace.isTrusted&&!this.disposed);
        this.console=new ServiceConsole(context,runtime,{register:()=>this.registerWorkspace(),currentProjects:()=>this.currentProjects(),removed:async project=>{
            if(this.binding){
                await this.workspace.withdrawProject(this.binding,project);this.trusted();
                await this.workbench?.removeProject(project).catch(error=>this.output.appendLine(`Workbench session cleanup: ${String(error)}`));
                this.specs=this.specs.filter(spec=>spec.service?.project!==project);
                this.roots=this.roots.filter(root=>this.specs.some(spec=>same(spec.root,root.canonical)));
                this.checks.delete(project);this.definitions.fire();
            }
        }});
        this.status.text='AIRK';this.status.command='airk.explorer.open';this.status.show();
    }
    private runtime():RuntimeHost{
        if(!this.host){const generation=this.generation;
            this.host=new RuntimeHost(this.context.extensionPath,this.runningVersion,serviceRuntimeOptions(),undefined,
                ()=>vscode.workspace.isTrusted&&!this.disposed&&!this.extensionUpdate&&generation===this.generation&&nativeExecutionAllowed());}
        return this.host;
    }
    trusted():void {
        if(this.checkForExtensionUpdate())throw new ServiceError('extension_reload_required');
        if(!vscode.workspace.isTrusted||this.disposed)throw new ServiceError('runtime_client_no_longer_authorized');
        if(!nativeExecutionAllowed())throw new NativeExecutionDisabled();
    }
    /** Reported instead of probing: native status is never inferred while execution is off. */
    private renderNativeDisabled():void{
        this.status.text='$(circle-slash) AIRK: native execution off';this.status.command='airk.enableNativeExecution';
        this.status.tooltip='AIRK native execution is disabled (airk.runtime.nativeExecution). No AIRK process is started. Click to enable.';
    }
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
    private currentBinding():boolean{return !!this.binding&&this.observedFolders===JSON.stringify(this.folders().map(folder=>folder.uri.fsPath));}
    isCurrent(spec:LaunchSpec):boolean{return !this.disposed&&this.currentBinding()&&this.specs.some(current=>current.command===spec.command&&current.dataDir===spec.dataDir&&current.collection===spec.collection&&current.service?.project===spec.service?.project&&JSON.stringify(launchArgs(current))===JSON.stringify(launchArgs(spec)));}
    subscribe(panel:vscode.WebviewPanel):void{this.context.subscriptions.push(panel);}
    verification(spec:LaunchSpec):McpVerification|undefined{return this.checks.get(spec.service!.project);}
    async refresh(observation?:BootstrapObservation):Promise<void>{
        if(this.refreshing)return this.refreshing;
        this.refreshing=this.refreshOnce(observation).finally(()=>{this.refreshing=undefined;});return this.refreshing;
    }
    private async refreshOnce(observation?:BootstrapObservation):Promise<void>{
        if(this.checkForExtensionUpdate()){this.renderUpdateStatus();return;}
        if(!nativeExecutionAllowed()){
            const had=this.specs.length>0;this.specs=[];this.roots=[];this.binding=undefined;this.renderNativeDisabled();
            if(!this.disposed){this.treeChanges.fire();if(had)this.definitions.fire();}return;
        }
        const previous=JSON.stringify(this.specs.map(spec=>[spec.command,launchArgs(spec)]));
        const folderIdentity=JSON.stringify(this.folders().map(folder=>folder.uri.fsPath));
        const generation=this.generation;
        // Observe into locals and publish once: readers never see an empty registration
        // list in the middle of a refresh, only the previous or the new result.
        const clear=()=>{this.specs=[];this.roots=[];this.binding=undefined;};
        try{
            this.trusted();const runtime=this.runtime();observation??=await runtime.discover();this.trusted();
            if(generation!==this.generation)throw new ServiceError('workspace_registration_changed');
            if(!observation.inspection||observation.inspection.payload_verified===false||observation.inspection.metadata_only===true){clear();this.status.text='AIRK: setup';return;}
            const binding=this.runtime().binding(observation.inspection,observation.installDir);
            if(binding.version!==this.runningVersion)throw new ServiceError('explicit_runtime_upgrade_or_matching_extension_required');
            const roots:{opened:string;canonical:string}[]=[];
            for(const folder of this.folders()){
                try{roots.push({opened:folder.uri.fsPath,canonical:await fs.realpath(folder.uri.fsPath)});}catch{/* Saved missing-source bindings remain readable. */}
            }
            const specs=await this.workspace.definitions(binding,this.folders().map(folder=>folder.uri.fsPath));this.trusted();
            if(generation!==this.generation||folderIdentity!==JSON.stringify(this.folders().map(folder=>folder.uri.fsPath)))throw new ServiceError('workspace_registration_changed');
            this.binding=binding;this.roots=roots;this.specs=specs;
            this.observedFolders=folderIdentity;
            this.status.text=`AIRK: ${this.specs.length} registered`;
        }catch(error){if(!this.disposed&&generation===this.generation){clear();this.status.text='AIRK: inspect service';this.output.appendLine(String(error));}}
        finally{if(!this.disposed){this.treeChanges.fire();if(previous!==JSON.stringify(this.specs.map(spec=>[spec.command,launchArgs(spec)])))this.definitions.fire();}}
    }
    async registerWorkspace():Promise<void>{
        this.trusted();
        const folder=await vscode.window.showQuickPick(this.folders().map(folder=>({label:folder.name,description:folder.uri.fsPath,folder})),
            {title:'Register a trusted source root with AIRK'});if(!folder)return;this.trusted();
        const old=this.context.globalState.get<Record<string,unknown>>('collections-1.2',{});
        const mode=await vscode.window.showQuickPick([
            {label:'Automatic source watching',description:'Native indexing continues after editor windows close',enabled:true},
            {label:'Manual indexing',description:'Only explicit indexing requests capture sources',enabled:false},
        ],{title:'AIRK indexing ownership',placeHolder:'The native service owns indexing; this is separate from login startup'});if(!mode)return;this.trusted();
        const binding=await this.runtime().ensure();this.trusted();
        const observation=await this.runtime().discover();this.trusted();
        const dataRoot=await this.projectDataRoot(observation);
        const canonical=await fs.realpath(folder.folder.uri.fsPath);this.trusted();
        const registered=await this.workspace.register(binding,folder.folder.uri.fsPath,dataRoot,collectionId(canonical),async review=>{
            const legacy=Object.keys(old).length?`\n${Object.keys(old).length} older collection records are retained; old startup entries need separate review.`:'';
            const detail=`Source: ${review.root}\nIndex storage: ${review.data_root}\nCollection: ${review.collection}\n${mode.enabled?'Automatic updates continue after windows close.':'Manual updates: initial setup runs once; later changes need Update index.'}\nExisting settings and model selections are preserved. New projects use the shared offline semantic model. Original source files are unchanged.${legacy}`;
            return await vscode.window.showWarningMessage('Add this project to AIRK and prepare its index?',{modal:true,detail},'Register')==='Register';
        },mode.enabled);if(!registered)return;this.trusted();
        await this.refresh();
        this.output.appendLine(`Registered ${registered.binding}. Native setup is queued and continues independently. Management shows model preparation, indexing and saved outcomes.`);
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
    async chooseSpec(requested?:LaunchSpec,registerMissing=true):Promise<LaunchSpec|undefined>{
        this.trusted();if(!this.currentBinding())await this.refresh();
        if(requested!==undefined){
            if(!this.isCurrent(requested))throw new ServiceError('workspace_registration_changed');
            return this.specs.find(spec=>spec.service!.project===requested.service!.project);
        }
        if(!this.specs.length){if(registerMissing)await this.registerWorkspace();return this.specs[0];}
        if(this.specs.length===1)return this.specs[0];
        const active=vscode.window.activeTextEditor&&vscode.workspace.getWorkspaceFolder(vscode.window.activeTextEditor.document.uri);
        const root=active&&this.roots.find(root=>same(root.opened,active.uri.fsPath));
        const selected=root&&this.specs.filter(spec=>same(spec.root,root.canonical));
        if(selected?.length===1)return selected[0];
        return (await vscode.window.showQuickPick(this.specs.map(spec=>({label:spec.name,description:spec.root,spec})),{title:'AIRK registered project'}))?.spec;
    }
    async resolveAgentSpec(spec:LaunchSpec):Promise<LaunchSpec>{
        this.trusted();
        if(!this.binding||!this.isCurrent(spec))throw new ServiceError('workspace_registration_changed');
        const binding=this.binding,generation=this.generation,client=this.runtime().client(binding);
        try{
            const selected=await this.workspace.resolveConnection(binding,spec,async(after,revision)=>{
                const page=await client.projects(after,revision,32);this.trusted();
                if(generation!==this.generation||this.binding!==binding||!this.isCurrent(spec))throw new ServiceError('workspace_registration_changed');
                return page;
            });
            this.trusted();
            if(generation!==this.generation||this.binding!==binding||!this.isCurrent(spec))throw new ServiceError('workspace_registration_changed');
            this.specs=this.specs.map(current=>current.service?.project===spec.service?.project?selected:current);
            this.definitions.fire();return selected;
        }finally{client.dispose();}
    }
    async index(requested?:LaunchSpec):Promise<void>{
        await this.refresh();if(requested===undefined&&!this.specs.length){await this.registerWorkspace();await this.showManagement('#manage/activity');return;}
        const spec=await this.chooseSpec(requested);if(!spec)return;this.trusted();
        const binding=await this.runtime().ensure();this.trusted();await this.enqueue(binding,spec.service!.project,'index');
        await this.showManagement('#manage/activity');
    }
    async watching():Promise<void>{
        const spec=await this.chooseSpec();if(!spec)return;
        const selected=await vscode.window.showQuickPick([{label:'Automatic; continues after windows close',enabled:true},{label:'Manual',enabled:false}],{title:'Native source watching'});
        if(!selected)return;this.trusted();const binding=await this.runtime().ensure(),client=this.runtime().client(binding);
        try{this.trusted();await client.rpc({configure_watch:{project:spec.service!.project,enabled:selected.enabled}});this.trusted();}finally{client.dispose();}
        await this.showManagement('#manage/indexing');
    }
    async repair(operation:'rebuild'|'auto'|'verify',requested?:LaunchSpec):Promise<void>{
        const spec=await this.chooseSpec(requested);if(!spec)return;this.trusted();await this.runtime().ensure();this.trusted();
        await this.console.previewRepair(spec.service!.project,operation);await this.showManagement('#manage/repair');
    }
    async pause(paused:boolean,requested?:LaunchSpec):Promise<void>{
        const spec=await this.chooseSpec(requested);if(!spec)return;this.trusted();const binding=await this.runtime().ensure(),client=this.runtime().client(binding);
        try{this.trusted();await client.rpc({pause:{project:spec.service!.project,paused}});this.trusted();}finally{client.dispose();}
        await this.showManagement('#manage/activity');
    }
    async verify(spec?:LaunchSpec):Promise<void>{
        if(this.checking)return this.checking;
        const attempt=this.verifyConnection(spec).finally(()=>{if(this.checking===attempt)this.checking=undefined;});
        this.checking=attempt;return attempt;
    }
    private async verifyConnection(spec?:LaunchSpec):Promise<void>{
        const startedAt=Date.now(),generation=this.generation;
        let selected=spec;
        const publish=(value:McpVerification)=>{
            this.trusted();if(generation!==this.generation)throw new ServiceError('workspace_registration_changed');
            this.connectionCheck={...value,startedAt,...(selected?{project:selected.service!.project}:{})};
            if(selected)this.checks.set(selected.service!.project,this.connectionCheck);
            this.treeChanges.fire();if(value.message)this.output.appendLine(`MCP check: ${value.message}`);
        };
        publish({phase:'preparing',message:'Checking the selected project and current runtime…'});
        try{
            selected??=await this.chooseSpec(undefined,false);this.trusted();
            if(!selected)throw new Error('No project selected. Select a registered project and check again.');
            publish({phase:'preparing',message:`Checking the native registration for ${selected.name}…`});
            selected=await this.resolveAgentSpec(selected);
            const result=await probeMcp(selected,28,this.runningVersion,this.connectionEntries(selected)[0],
                message=>publish({phase:'checking',message}));
            publish({phase:'verified',tools:result.tools,checkedAt:Date.now(),handshakeMs:result.handshakeMs,
                message:`AIRK responded with ${result.tools.length} tools. This checks AIRK; each agent must load its saved configuration separately.${record(result.status).state==='index_unknown'?' Index readiness is unconfirmed; call diagnostics or check again.':''}${result.handshakeMs!==undefined&&result.handshakeMs>10000?` The handshake took ${(result.handshakeMs/1000).toFixed(1)} seconds; an agent with a shorter startup deadline may time out.`:''}`,
                indexState:record(result.status).state==='not_indexed'?'not_indexed':record(result.status).state==='index_unknown'?'unknown':record(result.status).state==='index_unavailable'?'unavailable':'published'});
        }catch(error){
            if(!this.disposed&&generation===this.generation){
                const message=error instanceof Error?error.message:String(error);
                this.connectionCheck={phase:'failed',startedAt,checkedAt:Date.now(),error:message.slice(0,8192),project:selected?.service?.project};
                if(selected)this.checks.set(selected.service!.project,this.connectionCheck);
                this.treeChanges.fire();this.output.appendLine(`MCP check failed: ${message}`);
                void recordDiagnosticFailure(this.diagnosticObservation,'mcp_check',error).catch(failure=>this.output.appendLine(`Could not retain MCP diagnostic: ${String(failure)}`));
            }
            throw error;
        }
    }
    verifyProject(project?:unknown):Promise<void>{
        const spec=typeof project==='string'?this.specs.find(value=>value.service?.project===project):undefined;
        if(typeof project==='string'&&!spec)throw new ServiceError('workspace_registration_changed');
        return this.verify(spec);
    }
    async start():Promise<void>{
        const startedAt=Date.now();
        if(!nativeExecutionAllowed()){
            this.startup={startedAt,message:'AIRK native execution is turned off. No AIRK process was started. Run AIRK: Enable Native Execution to continue.'};
            this.renderNativeDisabled();this.treeChanges.fire();return;
        }
        this.startup={message:'Connecting to the installed AIRK runtime…',startedAt};this.treeChanges.fire();
        try{await this.automaticInstall();if(!this.binding)await this.refresh();
            if(!this.disposed)this.startup=this.startup?.failed?this.startup:this.installationFailed?{...this.startup,message:'AIRK startup needs attention. Open Logs for the recorded failure.',failed:true}:undefined;
        }catch(error){if(!this.disposed){this.startup={startedAt,message:String(error),failed:true};this.output.appendLine(String(error));}}
        finally{if(!this.disposed)this.treeChanges.fire();}
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
        await openUsagePanel(spec,valid,this.usageWriter(spec,valid));
    }
    private usageWriter(spec:LaunchSpec,valid:()=>boolean):(args:string[],continuing?:()=>boolean)=>Promise<unknown>{
        return async(args,continuing)=>{
            this.trusted();if(!valid()||continuing?.()===false)throw new ServiceError('project_configuration_selection_changed');
            let draft:Record<string,unknown>;
            if(args.length===1&&args[0]==='clear')draft={operation:'clear_usage'};
            else if(args.length===3&&args[0]==='configure'&&args[1]==='--mode'&&['disabled','aggregates-only','diagnostic-events'].includes(args[2]))draft={operation:'usage',mode:args[2].replaceAll('-','_')};
            else if(args.length===4&&args[0]==='feedback'&&args[2]==='--usefulness'&&['useful','partly-useful','not-useful'].includes(args[3])){
                identifier(args[1],16);draft={operation:'usage_feedback',request_id:args[1],usefulness:args[3].replaceAll('-','_')};
            }else if(args.length===4&&args[0]==='import'&&args[2]==='--if-sha256'&&path.isAbsolute(args[1])){
                identifier(args[3],32);draft={operation:'import_usage',path:args[1],expected_sha256:args[3]};
            }else throw new ServiceError('invalid_usage_action');
            if(!spec.service)throw new ServiceError('native_project_binding_required');
            await this.console.configureProject(spec.service.project,draft,()=>valid()&&continuing?.()!==false);
        };
    }
    private async embeddedUsage(spec:LaunchSpec,current:()=>boolean):Promise<import('./explorerController').EmbeddedUsage>{
        this.trusted();if(!current()||!this.isCurrent(spec))throw new ServiceError('project_configuration_selection_changed');
        await this.runtime().ensure();this.trusted();
        const generation=this.generation,valid=()=>generation===this.generation&&this.isCurrent(spec)&&current();
        let value:object={},receive:(message:unknown)=>Promise<void>=async()=>undefined,dispose=()=>{};
        await openUsagePanel(spec,valid,this.usageWriter(spec,valid),{
            render:next=>{value=next;},onMessage:listener=>{receive=listener;return {dispose(){receive=async()=>{throw new ServiceError('usage_session_disposed');};}};},onDispose:listener=>{dispose=listener;},
        });
        const view=()=>{this.trusted();if(!valid())throw new ServiceError('project_configuration_selection_changed');return {html:renderUsage(value)};};
        return {view,action:async message=>{await receive(message);return view();},dispose};
    }

    async installCommandLine():Promise<void>{
        this.installationWarnings=false;
        this.reportInstall('Checking selected runtime…');
        this.trusted();const binding=await this.runtime().ensure();this.trusted();
        if(!binding.launcher)throw new ServiceError('stable_launcher_unavailable');
        this.reportInstall('Installing the terminal command…');
        const touched:ShellTouch[]=[];
        const errors=await installLauncherCli(binding.universalLauncher??binding.launcher,this.context,this.shellScope(),touched);this.trusted();
        // Uninstall cleanup reverses exactly these startup-file edits.
        if(this.uninstallDirectory)await rememberShellIntegration(this.uninstallDirectory,this.context.globalStorageUri.fsPath,touched)
            .catch(error=>errors.push(`Could not record terminal integration for uninstall cleanup: ${String(error)}`));
        for(const error of errors){this.installationWarnings=true;this.output.appendLine(error);}
        this.reportInstall('Cleaning up previous runtime…');
        await this.runtime().retirePrevious(binding);this.trusted();
        // Runtime completion is independent of project registration/indexing.
        if(!errors.length)await this.context.globalState.update(`airk.installComplete.1.${binding.installDir}`,`${this.runningVersion}:${binding.runtimeSha256}`);
    }
    async installationComplete(manual=false):Promise<void>{
        this.trusted();
        const binding=await this.runtime().ensure();this.trusted();
        const key=`airk.installNotice.1.${binding.installDir}`,identity=`${this.runningVersion}:${binding.runtimeSha256}`;
        if(!manual&&!this.migratedConnections&&!this.installationWarnings&&this.context.globalState.get<string>(key)===identity)return;
        await this.context.globalState.update(key,identity);this.trusted();
        this.migratedConnections=false;
        const message=this.installationWarnings?'AIRK runtime installed with connection or cleanup issues. Review the details before reconnecting tools.':
            `AIRK ${this.runningVersion} ready. This window is running the updated interface. Project indexing continues in Management.`;
        this.output.appendLine(message);
        this.output.appendLine('Copilot: MCP: List Servers → select AIRK → Restart Server. Codex and Claude Code running outside VS Code need their own session restart. Reload Window reloads editor extensions. If VS Code refuses a VSIX reinstall and requests a restart, fully exit all VS Code windows, reopen, then install the VSIX again.');
        // Successful activation needs no decision. Only actionable problems produce a notification.
        if(!this.installationWarnings)return;
        const warningKey=`airk.installWarning.1.${binding.installDir}`;
        if(!manual&&this.context.globalState.get<string>(warningKey)===identity)return;
        await this.context.globalState.update(warningKey,identity);
        const generation=this.generation;
        const notice=vscode.window.showWarningMessage(message,'Review details');
        void notice.then(async action=>{
            if(this.disposed||generation!==this.generation||!vscode.workspace.isTrusted)return;
            if(action==='Review details')this.output.show();
        }).then(undefined,error=>this.output.appendLine(`Installation action failed: ${String(error)}`));
    }
    private async migrateAgentConnections(binding:RuntimeBinding,restored:readonly LaunchSpec[]=[]):Promise<void>{
        this.trusted();
        const generation=this.generation,folders=this.folders().map(folder=>folder.uri.fsPath);
        const specs=await this.workspace.definitions(binding,folders);
        for(const savedSpec of specs){
            // Extension receipts may survive storage replacement. Never rewrite an
            // agent's corrected configuration from those receipts alone.
            const client=this.runtime().client(binding);
            let spec:LaunchSpec;
            try{spec=await this.workspace.resolveConnection(binding,savedSpec,async(after,revision)=>{
                const page=await client.projects(after,revision,32);this.trusted();
                if(generation!==this.generation)throw new ServiceError('workspace_registration_changed');
                return page;
            });}finally{client.dispose();}
            if(this.uninstallDirectory)await rememberUninstallTarget(this.uninstallDirectory,binding.installDir,this.context.globalStorageUri.fsPath,spec);
            const opened: string[]=[];
            for(const folder of folders){try{if(same(await fs.realpath(folder),spec.root))opened.push(folder);}catch{/* Missing sources cannot host configuration edits. */}}
            if(!opened.length)continue;
            const check=()=>{
                this.trusted();
                if(generation!==this.generation||!opened.some(root=>this.folders().some(folder=>same(folder.uri.fsPath,root))))throw new ServiceError('workspace_registration_changed');
            };
            const messages=await migrateLocalAgents(spec,this.context.globalStorageUri.fsPath,check,async(destination,before,after)=>{
                check();
                const matches=vscode.workspace.textDocuments.filter(document=>same(document.uri.fsPath,destination)
                    ||opened.some(root=>same(document.uri.fsPath,path.join(root,path.relative(spec.root,destination)))));
                if(matches.some(document=>document.isDirty))throw new Error('Agent configuration has unsaved edits; save it and retry Install or Repair CLI.');
                const document=matches[0]??await vscode.workspace.openTextDocument(vscode.Uri.file(destination));
                if(document.isDirty||document.getText()!==before||await readJsonConfig(destination)!==before)throw new Error('Agent configuration changed; retry Install or Repair CLI.');
                // VS Code normalizes inserted newlines to the document's EOL mode.
                // A compact one-line JSON file can inherit Windows CRLF even when the transform uses LF.
                const editorAfter=document.eol===2?after.replace(/\r?\n/g,'\r\n'):document.eol===1?after.replace(/\r\n/g,'\n'):after;
                check();const version=document.version,edit=new vscode.WorkspaceEdit();
                edit.replace(document.uri,new vscode.Range(document.positionAt(0),document.positionAt(before.length)),editorAfter);
                if(document.version!==version||!await vscode.workspace.applyEdit(edit))throw new Error('Agent configuration edit failed.');
                check();
                if(document.getText()!==editorAfter||!await document.save())throw new Error('Agent configuration was not saved; review the editor changes.');
            },vscode.workspace.getConfiguration('airk').get<boolean>('explorer.assistance.enabled',true),false,
                restored.find(previous=>same(previous.root,spec.root)&&previous.collection===spec.collection));
            for(const message of messages){
                this.output.appendLine(message);
                if(message.startsWith('Updated '))this.migratedConnections=true;
                if(message.startsWith('Could not update'))this.installationWarnings=true;
            }
        }
    }
    private shellScope(){return shellIntegration(vscode.workspace.getConfiguration('airk').inspect<string>('cli.shellIntegration')?.globalValue);}
    /** Home folders and filesystem/volume roots need an explicit choice before registration. */
    private async broadRoot(canonical:string):Promise<boolean>{
        const home=await fs.realpath(os.homedir()).catch(()=>os.homedir());
        return same(canonical,home)||same(path.dirname(canonical),canonical);
    }
    private folderChoices():{declined:string[];approved:string[]}{
        return {declined:this.context.globalState.get<string[]>('airk.setupDeclinedFolders.1',[]),approved:this.context.globalState.get<string[]>('airk.setupApprovedRoots.1',[])};
    }
    /** Before the first installation only: show persistent effects and let the user
     * accept, customize, defer or disable. Existing and managed installations skip it. */
    private async firstUseReview(installed:BootstrapObservation,explicit:boolean):Promise<boolean>{
        const key='airk.setupReview.1',saved=this.context.globalState.get<{decision?:string}>(key);
        if(explicit){await this.context.globalState.update(key,{decision:'accepted',at:Date.now()});return true;}
        if(saved?.decision==='accepted')return true;
        if(saved?.decision==='deferred'){
            if(this.startup)this.startup={...this.startup,message:'AIRK setup is deferred. Run AIRK: Set Up to review and install.'};
            this.status.text='AIRK: setup deferred';this.status.command='airk.setUp';return false;
        }
        const configured=vscode.workspace.getConfiguration('airk').inspect<string>('storage.path')?.globalValue;
        const paths=record(installed.location.user_paths),storage=configured??(typeof paths.data==='string'?paths.data:'the AIRK per-user data directory');
        const scope=this.shellScope();
        const shell=scope==='off'?'No shell startup file or user PATH is changed (airk.cli.shellIntegration is off).'
            :process.platform==='win32'?'Adds the airk command directory to the front of your user PATH.'
            :scope==='all'?'Adds a marked PATH block to Bash, Zsh and fish startup files.':'Adds a marked PATH block to ~/.profile and the startup files of shells you use.';
        const folders:{opened:string;canonical:string;broad:boolean}[]=[];
        for(const folder of this.folders()){
            try{const canonical=await fs.realpath(folder.uri.fsPath);folders.push({opened:folder.uri.fsPath,canonical,broad:await this.broadRoot(canonical)});}catch{/* Missing folders are reported during preparation. */}
        }
        const detail=[`Installs the AIRK ${this.runningVersion} runtime for this user account and stores indexes in ${storage}.`,
            `Registers these folders for background indexing: ${folders.filter(folder=>!folder.broad).map(folder=>folder.opened).join(', ')||'none'}.`,
            ...(folders.some(folder=>folder.broad)?['Home folders and drive roots are not registered unless you choose them in Customize.']:[]),
            'Indexing continues after editor windows close; login startup is a separate opt-in.',shell,
            'Existing recognized AIRK agent entries in these projects may be updated.',
            'Hidden files can be indexed: review .airkignore first. AIRK is not a secret scanner.'].join('\n');
        const choice=await vscode.window.showInformationMessage('Set up AIRK on this computer?',{modal:true,detail},'Set up','Customize…','Not now',"Don't set up automatically");
        if(choice==="Don't set up automatically"){await vscode.workspace.getConfiguration('airk').update('cli.autoInstall',false,vscode.ConfigurationTarget.Global);return false;}
        if(choice==='Not now'){await this.context.globalState.update(key,{decision:'deferred',at:Date.now()});return this.firstUseReview(installed,false);}
        if(choice==='Customize…'){
            const {declined,approved}=this.folderChoices();
            const items:(vscode.QuickPickItem&{folder?:string})[]=[...folders.map(folder=>({label:path.basename(folder.opened)||folder.opened,description:folder.opened,
                detail:folder.broad?'Home folder or drive root: registered only if you select it':undefined,picked:!folder.broad&&!declined.some(value=>same(value,folder.canonical)),folder:folder.canonical})),
                {label:'Terminal command in external shells',description:process.platform==='win32'?'User PATH':'Shell startup files',picked:scope!=='off'}];
            const picked=await vscode.window.showQuickPick(items,{canPickMany:true,title:'Choose what AIRK sets up'});
            if(!picked)return false;
            const selected=new Set(picked.flatMap(item=>item.folder?[item.folder]:[]));
            await this.context.globalState.update('airk.setupDeclinedFolders.1',[...declined.filter(value=>!folders.some(folder=>same(folder.canonical,value))),...folders.filter(folder=>!selected.has(folder.canonical)).map(folder=>folder.canonical)]);
            await this.context.globalState.update('airk.setupApprovedRoots.1',[...approved,...folders.filter(folder=>folder.broad&&selected.has(folder.canonical)).map(folder=>folder.canonical)]);
            if(!picked.some(item=>!item.folder)&&scope!=='off')await vscode.workspace.getConfiguration('airk').update('cli.shellIntegration','off',vscode.ConfigurationTarget.Global);
        }else if(choice!=='Set up')return false;
        await this.context.globalState.update(key,{decision:'accepted',at:Date.now()});return true;
    }
    automaticInstall(notify=true,explicit=false):Promise<void>{
        if(this.checkForExtensionUpdate())return Promise.resolve();
        if(!nativeExecutionAllowed())return explicit?Promise.reject(new NativeExecutionDisabled()):Promise.resolve();
        if(!vscode.workspace.isTrusted||!explicit&&vscode.workspace.getConfiguration('airk').inspect<boolean>('cli.autoInstall')?.globalValue===false)return Promise.resolve();
        this.settingUp??=this.automaticSetup(notify,explicit).finally(()=>{this.settingUp=undefined;});return this.settingUp;
    }
    /** The detached uninstall worker mirrors its newest result into the native state directory. */
    private async lastUninstall(observation:BootstrapObservation):Promise<{text:string;attention:boolean;identity:string}|undefined>{
        const state=record(observation.location.user_paths).state;
        if(typeof state!=='string'||!path.isAbsolute(state))return undefined;
        const source=await fs.readFile(path.join(state,'extension-uninstall','last-result.json'),'utf8').catch(()=>undefined);
        if(!source||source.length>65536)return undefined;
        const value=record(JSON.parse(source));
        const alive=(pid:unknown)=>{try{return typeof pid==='number'&&Number.isSafeInteger(pid)&&pid>0&&(process.kill(pid,0),true);}catch{return false;}};
        const outcome=value.state==='complete'?'complete':value.state==='needs_attention'?'needs attention'
            :value.state==='started'?alive(value.pid)?'still running':'interrupted or unknown (no final result was written)':'unknown';
        const text=[`Last AIRK uninstall cleanup: ${outcome}.`,typeof value.startedAt==='string'?`Started: ${value.startedAt}`:'',
            typeof value.finishedAt==='string'?`Finished: ${value.finishedAt}`:'',typeof value.job==='string'?`Log and result: ${value.job}`:'',
            typeof value.failure==='string'?`Details: ${value.failure}`:''].filter(Boolean).join('\n');
        return {text,attention:outcome!=='complete'&&outcome!=='still running',identity:`${value.startedAt}:${value.state}`};
    }
    private async reportLastUninstall(observation:BootstrapObservation):Promise<void>{
        const result=await this.lastUninstall(observation).catch(()=>undefined);
        if(!result?.attention||this.context.globalState.get<string>('airk.uninstallNotice.1')===result.identity)return;
        await this.context.globalState.update('airk.uninstallNotice.1',result.identity);
        this.output.appendLine(result.text);
        void vscode.window.showWarningMessage('A previous AIRK uninstall did not finish cleanly.','Show Details').then(action=>{if(action==='Show Details')this.output.show();});
    }
    async showLastUninstallResult():Promise<void>{
        this.trusted();const result=await this.lastUninstall(await this.runtime().discover());
        this.output.appendLine(result?.text??'No AIRK uninstall result is recorded for this user.');this.output.show(true);
    }
    /** Remove external-shell PATH integration now; VS Code terminals keep the command. */
    async removeTerminalCommand():Promise<void>{
        const choice=await vscode.window.showWarningMessage('Remove the airk command from external terminals?',{modal:true,
            detail:'Removes AIRK-owned PATH blocks from shell startup files (or the user PATH on Windows) and stops future setup from adding them. Modified entries are preserved and reported. Indexes and the runtime are unchanged.'},'Remove');
        if(choice!=='Remove')return;
        await vscode.workspace.getConfiguration('airk').update('cli.shellIntegration','off',vscode.ConfigurationTarget.Global);
        const storage=this.context.globalStorageUri.fsPath;
        const recorded=this.uninstallDirectory?await shellProfiles(this.uninstallDirectory,storage).catch(()=>[]):[];
        const result=await removeShellIntegration(storage,[...defaultProfiles(),...recorded],process.platform,{wrappers:false});
        for(const line of [...result.removed,...result.preserved])this.output.appendLine(line);
        if(result.preserved.length)void vscode.window.showWarningMessage('Some AIRK terminal entries were preserved for manual review.','Show Details').then(action=>{if(action==='Show Details')this.output.show();});
        else void vscode.window.showInformationMessage(result.removed.length?'AIRK terminal command removed from external shells. Open a new terminal to see the change.':'No AIRK terminal entries were found outside VS Code.');
    }
    async repairInstallation():Promise<void>{
        await this.settingUp;
        this.trusted();
        await this.withInstallProgress(async()=>{
            await this.automaticInstall(false,true);
            if(this.installationFailed)return;
            await this.refresh();await this.installationComplete(true);
        });
    }
    async retryUpdate():Promise<void>{
        await this.settingUp;
        await this.automaticInstall(true,true);await this.refresh();
    }
    private async automaticSetup(notify:boolean,explicit:boolean):Promise<void>{
        if(!vscode.workspace.isTrusted||!explicit&&vscode.workspace.getConfiguration('airk').inspect<boolean>('cli.autoInstall')?.globalValue===false)return;
        this.installationFailed=false;
        let runtimeInstalled=false;
        let setupObservation:BootstrapObservation|undefined;
        try{
            this.reportInstall('Connecting to AIRK; checking the installed runtime…');
            let installed=await this.runtime().discover();this.trusted();
            setupObservation=installed;this.diagnosticObservation=installed;
            void this.reportLastUninstall(installed);
            // Nothing persistent (receipts, runtime, PATH or registrations) happens before this review.
            if(!installed.inspection&&!serviceRuntimeOptions().managed&&!await this.firstUseReview(installed,explicit))return;
            if(!serviceRuntimeOptions().managed){
                const launcherDirectory=record(installed.location.user_paths).launcher;
                if(typeof launcherDirectory!=='string'||!path.isAbsolute(launcherDirectory))throw new ServiceError('invalid_launcher_location');
                this.uninstallDirectory=launcherDirectory;
                await rememberUninstallTarget(launcherDirectory,installed.installDir,this.context.globalStorageUri.fsPath);
            }
            if(!explicit&&installed.inspection&&installed.inspection.payload_verified!==false&&installed.inspection.metadata_only!==true){
                const binding=this.runtime().binding(installed.inspection,installed.installDir);
                const identity=`${this.runningVersion}:${binding.runtimeSha256}`;
                if(binding.version===this.runningVersion&&binding.runtimeSha256===installed.bundle.digest
                    &&!installed.diagnosis?.pending_activation&&this.context.globalState.get<string>(`airk.installComplete.1.${binding.installDir}`)===identity){
                    runtimeInstalled=true;
                    if(installed.inspection.lifecycle&&record(installed.inspection.lifecycle).explicitly_stopped===true)throw new ServiceError('service_explicitly_stopped');
                    // A completed installation does not prove that its on-demand service survived editor/OS shutdown.
                    const connected=await this.runtime().reconnect(installed);this.trusted();
                    if(connected.runtimeSha256!==binding.runtimeSha256||!same(connected.controlDir,binding.controlDir))throw new ServiceError('runtime_selection_changed_review_again');
                    await this.prepareWorkspaceProjects(binding,installed);
                    await this.refresh(installed);
                    this.reportInstall('Connected to AIRK. Saved project work remains with the service.');
                    await this.openWorkbenchAfterSetup();
                    if(this.installationWarnings)this.output.appendLine('Some agent connections need review in Management > Agents.');
                    return;
                }
            }
            const progressTitle=!installed.inspection?`Installing AIRK ${this.runningVersion} runtime`
                :this.runtime().binding(installed.inspection,installed.installDir).version!==this.runningVersion?`Updating AIRK ${this.runningVersion} runtime`
                :`Checking AIRK ${this.runningVersion} installation`;
            await this.withInstallProgress(async()=>{
            if(installed.inspection&&!serviceRuntimeOptions().managed){
                const preceding=this.runtime().binding(installed.inspection,installed.installDir);
                if(automaticUpgrade(preceding.version,this.runningVersion as string)
                    &&await this.runtime().supersedePending(preceding,installed.diagnosis)){
                    this.reportInstall('Resolved previous unfinished upgrade…');
                    installed=await this.runtime().discover();this.trusted();
                }
                if(!installed.inspection)throw new ServiceError('runtime_selection_changed_review_again');
                const current=this.runtime().binding(installed.inspection,installed.installDir),target=this.runningVersion as string;
                if(automaticUpgrade(current.version,target)&&(current.version!==target||current.runtimeSha256!==installed.bundle.digest)){
                    this.reportInstall(`Upgrading ${current.version} → ${target}…`);
                    // Finish a prior installation's deferred cleanup before staging another replacement.
                    if(!installed.diagnosis?.pending_activation)await this.runtime().retirePrevious(current);
                    const review=await this.runtime().reviewTransition(current,installed.diagnosis?.pending_activation?'resume':'upgrade');this.trusted();
                    await this.runtime().transition(current,review,message=>this.reportInstall(message));this.trusted();
                }
            }
            await this.installCommandLine();this.trusted();
            // Activation/retirement can replace the incarnation behind an older metadata pipe.
            this.console.reset();
            const binding=await this.runtime().ensure();this.trusted();
            runtimeInstalled=true;
            const completed=await this.runtime().discover();
            await this.prepareWorkspaceProjects(binding,completed);await this.refresh(completed);
            this.reportInstall(this.installationWarnings?'Runtime installation finished with issues to review.':'Runtime ready. Project indexing continues in Management.');
            await this.openWorkbenchAfterSetup();
            if(notify)await this.installationComplete();
            },progressTitle);
        }
        catch(error){
            if(this.disposed)return;
            if(setupObservation)void recordDiagnosticFailure(setupObservation,'automatic_install',error)
                .catch(failure=>{if(!this.disposed)this.output.appendLine(`Could not retain installation diagnostic: ${String(failure)}`);});
            if(error instanceof ServiceError&&error.code==='extension_reload_required')return;
            if(String(error).includes('service_explicitly_stopped')){
                this.output.appendLine('AIRK service_explicitly_stopped. Start the service from Management > Service when ready. Saved project work is retained.');return;
            }
            if(runtimeInstalled){
                if(this.startup)this.startup={...this.startup,message:`Project preparation needs attention: ${String(error)}`,failed:true};
                const code=error instanceof ServiceError?error.code:'project_preparation_failed';
                this.output.appendLine(`AIRK installation is complete. Project preparation needs attention: ${String(error)}. Review Management; native accepted jobs are retained.`);
                this.reportInstall('AIRK installed. Project preparation needs attention; review Management.');
                const key='airk.projectStartupNotice.1',identity=`${this.runningVersion}:${code}`;
                if(this.context.workspaceState.get<string>(key)!==identity){
                    await this.context.workspaceState.update(key,identity);
                    void vscode.window.showWarningMessage(`AIRK is installed. Project preparation needs attention (${code}). Open Management to review indexing or registration; accepted work is retained.`);
                }
                return;
            }
            this.installationFailed=true;this.output.appendLine(`Automatic installation failed: ${String(error)}`);
            if(String(error).includes('airk_client_restart_required')){
                this.reportInstall('Upgrade paused: waiting for connected AIRK clients to disconnect. Choose Retry Update when ready.');
                const generation=this.generation;
                void vscode.window.showWarningMessage('AIRK update is waiting for connected tools. Stop AIRK in MCP Servers and close external AIRK agent sessions and tools terminals, then choose Retry Update. The update has not completed.',
                    'MCP Servers','Retry Update','Show Details').then(async action=>{
                    if(this.disposed||generation!==this.generation||!vscode.workspace.isTrusted)return;
                    if(action==='MCP Servers')await this.manageVscodeMcp();
                    else if(action==='Show Details')this.output.show();
                    else if(action==='Retry Update')await this.retryUpdate();
                }).then(undefined,failure=>this.output.appendLine(`Update action failed: ${String(failure)}`));
                return;
            }
            this.reportInstall('Installation did not complete. See AIRK output for the failure and retry instructions.');
            void vscode.window.showErrorMessage(`AIRK installation needs attention: ${error instanceof Error?error.message:String(error)}. Use AIRK: Install or Repair CLI to retry.`);}
    }
    private async projectDataRoot(observation:BootstrapObservation):Promise<string>{
        const configuration=vscode.workspace.getConfiguration('airk'),configured=configuration.inspect<string>('storage.path')?.globalValue;
        const nativeData=record(observation.location.user_paths).data;
        const renamed=configured&&typeof nativeData==='string'?await renamedDefaultDataRoot(configured,nativeData):undefined;
        this.trusted();
        if(renamed){
            if(vscode.workspace.getConfiguration('airk').inspect<string>('storage.path')?.globalValue!==configured)throw new ServiceError('project_storage_preference_changed');
            await configuration.update('storage.path',renamed,vscode.ConfigurationTarget.Global);this.trusted();
            this.output.appendLine('Updated the saved storage preference to the renamed native index directory.');
        }
        const dataRoot=renamed||configured||nativeData;
        if(typeof dataRoot!=='string'||!path.isAbsolute(dataRoot))throw new ServiceError('invalid_project_data_location');
        return dataRoot;
    }
    private async prepareWorkspaceProjects(binding:RuntimeBinding,observation:import('./serviceBootstrap').BootstrapObservation):Promise<void>{
        this.trusted();
        const folders=this.folders(),specs=await this.workspace.definitions(binding,folders.map(folder=>folder.uri.fsPath));
        const dataRoot=await this.projectDataRoot(observation);
        const generation=this.generation,client=this.runtime().client(binding);
        let observed:Awaited<ReturnType<typeof readWorkspaceRegistrations>>;
        try{observed=await readWorkspaceRegistrations(binding,async(after,revision,inactive)=>{
            const page=await client.projects(after,revision,32,inactive);this.trusted();
            if(generation!==this.generation)throw new ServiceError('workspace_registration_changed');return page;
        });}finally{client.dispose();}
        const restored:LaunchSpec[]=[];
        for(const folder of folders){
            try{await fs.realpath(folder.uri.fsPath);}
            catch(error){
                if(['ENOENT','ENOTDIR'].includes((error as NodeJS.ErrnoException).code??'')){
                    this.output.appendLine(`Source folder is missing: ${folder.uri.fsPath}. Native removal checks continue; preparing other open folders.`);continue;
                }
                throw error;
            }
            const state=await this.workspace.reconcileStartup(binding,folder.uri.fsPath,observed);this.trusted();
            if(state==='registered')continue;
            const {declined,approved}=this.folderChoices(),real=await fs.realpath(folder.uri.fsPath);
            if(declined.some(value=>same(value,real))){this.output.appendLine(`Not registering ${folder.uri.fsPath}: it was left out during setup review. Use Add Project to register it.`);continue;}
            if(await this.broadRoot(real)&&!approved.some(value=>same(value,real))){
                this.output.appendLine(`Not registering ${folder.uri.fsPath} automatically: it is a home folder or drive root. Use Add Project to register it deliberately.`);continue;
            }
            if(state==='removed'){this.output.appendLine(`Project remains removed: ${folder.uri.fsPath}. Add it in Management to register it again.`);continue;}
            this.trusted();const canonical=await fs.realpath(folder.uri.fsPath);this.trusted();
            const saved=specs.find(spec=>same(spec.root,canonical));
            this.reportInstall(saved?'Restoring workspace registration after service data was reset…':'Registering a new workspace project...');
            let project;
            try{project=await this.workspace.register(binding,folder.uri.fsPath,dataRoot,saved?.collection??collectionId(canonical),async()=>true,true,true);}
            catch(error){
                if(error instanceof ServiceError&&error.code==='project_was_removed_refresh_registration'){
                    await this.workspace.withdraw(binding,folder.uri.fsPath);
                    this.output.appendLine(`Project remains removed: ${folder.uri.fsPath}. Add it in Management to register it again.`);continue;
                }
                throw error;
            }
            if(!project)continue;this.trusted();
            if(saved)restored.push(saved);
            this.output.appendLine(`Registered ${folder.uri.fsPath}. Native model preparation and indexing continue independently of VS Code. Follow progress in Management.`);
        }
        this.reportInstall('Checking saved agent connections…');
        await this.migrateAgentConnections(binding,restored);
        await this.context.workspaceState.update('airk.projectStartupNotice.1',undefined);
    }
    async manageVscodeMcp():Promise<void>{this.trusted();await vscode.commands.executeCommand('workbench.mcp.listServer');}
    private async hasManualVscodeConnection():Promise<boolean>{
        // The file owns client policies, including explicit disable. Do not bypass
        // it by registering a second copy through the extension provider.
        for(const spec of this.specs){
            try{
                const source=await readJsonConfig(path.join(spec.root,'.vscode','mcp.json'));
                if(source&&hasUnifiedVscodeConfig(spec,this.context.globalStorageUri?.fsPath??'',source))return true;
            }catch{/* Unsafe/custom configurations remain for review; never execute them here. */}
        }
        return false;
    }
    async provideDefinitions():Promise<vscode.McpStdioServerDefinition[]>{
        if(this.checkForExtensionUpdate())return [];
        if(!vscode.workspace.isTrusted||!this.registrationEnabled()||!nativeExecutionAllowed())return [];
        if(!this.currentBinding())await this.refresh();if(!vscode.workspace.isTrusted||!this.registrationEnabled())return [];
        if(await this.hasManualVscodeConnection())return [];
        const entry=this.workspaceConnection();if(!entry)return [];
        const definition=new vscode.McpStdioServerDefinition('AIRK',entry.command,[...entry.args],
            {AIRK_CLIENT:'vscode',AIRK_MCP_RESPONSE_MODE:this.responseMode()},`${this.runningVersion}:${JSON.stringify(entry.args)}:${this.responseMode()}`);
        definition.cwd=vscode.Uri.file(path.dirname(entry.command));return [definition];
    }
    async resolveDefinition(definition:vscode.McpServerDefinition):Promise<vscode.McpServerDefinition>{
        this.trusted();if(!this.registrationEnabled())throw new ServiceError('mcp_registration_disabled');
        await this.refresh();this.trusted();
        if(await this.hasManualVscodeConnection())throw new ServiceError('manual_mcp_configuration_owns_connection');
        const entry=this.workspaceConnection();
        if(!(definition instanceof vscode.McpStdioServerDefinition)||!entry||entry.command!==definition.command||JSON.stringify(entry.args)!==JSON.stringify(definition.args))throw new ServiceError('workspace_registration_changed');
        await this.runtime().ensure();this.trusted();if(!this.registrationEnabled())throw new ServiceError('mcp_registration_disabled');return definition;
    }
    getTreeItem(spec:LaunchSpec):vscode.TreeItem {
        const item=new vscode.TreeItem(spec.name);item.id=spec.service!.project;item.description=spec.collection;item.tooltip=spec.root;item.contextValue='airkServiceProject';
        item.command={command:'airk.explorer.open',title:'Open in AIRK Explorer',arguments:['#manage/projects']};return item;
    }
    getChildren():LaunchSpec[]{return this.specs;}
    /** Refresh the shared console observations, then reveal a Management tab in the Explorer. */
    async showManagement(route:string):Promise<void>{
        (await this.explorerController()).open(route);
        void this.console.run('connect').catch(error=>{if(!this.disposed)this.output.appendLine(String(error));});
    }
    /** The AIRK Explorer is loaded on first use; it shares this extension's console and registrations. */
    explorerController():Promise<ExplorerController>{
        this.explorer??=Promise.all(['explorerController.js','explorerWebview.js','explorerWebview.css'].map(name=>fs.access(path.join(this.context.extensionUri.fsPath,'out',name))))
        .then(()=>import('./explorerController')).then(({ExplorerController})=>{
            if(this.disposed)throw new ServiceError('runtime_client_no_longer_authorized');
            return this.explorerLoaded=new ExplorerController(this.explorerHost());
        }).catch(error=>{this.explorer=undefined;throw error;});
        return this.explorer;
    }
    attachWorkbench(view:WorkbenchView):void{this.workbench=view;}
    private async openWorkbenchAfterSetup():Promise<void>{
        const key='airk.workbench.welcome.1';
        if(this.disposed||!vscode.workspace.isTrusted||this.checkForExtensionUpdate()||!this.specs.length||!this.workbench||this.context.globalState.get<boolean>(key))return;
        try{
            await this.workbench.focus();
            if(!this.disposed&&vscode.workspace.isTrusted)await this.context.globalState.update(key,true);
        }catch(error){this.output.appendLine(`Workbench could not open after setup: ${String(error)}. Use AIRK: Open Workbench.`);}
    }

    explorerHost():ExplorerHost{
        return {extensionUri:this.context.extensionUri,version:this.runningVersion,
            extensionUpdate:()=>this.extensionUpdate,reloadUpdatedExtension:()=>this.reloadUpdatedExtension(),
            connectionCheck:()=>this.connectionCheck,startup:()=>this.startup,
            workbenchVisible:()=>this.workbench?.visible??false,toggleWorkbench:async()=>{await this.workbench?.toggle();},
            onDidChangeWorkbenchVisibility:this.workbench?.onDidChangeVisibility,
            trusted:()=>vscode.workspace.isTrusted&&!this.disposed,specs:()=>this.specs,
            activeProject:()=>{
                const active=vscode.window.activeTextEditor&&vscode.workspace.getWorkspaceFolder(vscode.window.activeTextEditor.document.uri);
                const root=active&&this.roots.find(root=>same(root.opened,active.uri.fsPath));
                return root?this.specs.find(spec=>same(spec.root,root.canonical))?.service?.project:undefined;
            },
            usage:(spec,valid)=>this.embeddedUsage(spec,valid),
            onDidChangeSpecs:this.treeChanges.event,control:this.console,log:line=>this.output.appendLine(line),
            setStatusBar:(text,tooltip)=>{if(!this.disposed&&!this.renderUpdateStatus()){this.status.text=text;this.status.tooltip=tooltip;}},workspaceState:this.context.workspaceState,suggestions:this.suggestions,handoffs:this.handoffs};
    }
    private readonly handoffs=new HandoffStore({authorized:()=>this.assistanceEnabled(),binding:()=>this.assistanceHost?.bindingFingerprint??'',
        revisions:()=>this.explorerLoaded?.draftRevisions??{},publication:()=>this.explorerLoaded?.publication});
    private assistanceEnabled():boolean{return vscode.workspace.isTrusted&&!this.disposed&&!this.extensionUpdate&&vscode.workspace.getConfiguration('airk').get<boolean>('explorer.assistance.enabled',true);}
    private assistanceFolders():vscode.WorkspaceFolder[]{return (vscode.workspace.workspaceFolders??[]).filter(folder=>folder.uri.scheme==='file'||folder.uri.scheme==='vscode-remote');}
    /** Refresh the external launch descriptor and this window's attachment endpoint; untrusted windows publish neither. */
    async startAssistance():Promise<void>{
        const generation=++this.assistanceGeneration;
        this.assistanceHost?.dispose();this.assistanceHost=undefined;
        if(!this.assistanceEnabled())return;
        const storage=this.context.globalStorageUri.fsPath,version=this.runningVersion;
        try{writeLaunchDescriptor(storage,this.context.extensionPath,version);}
        catch(error){this.output.appendLine(`Explorer Assistance launch descriptor was not written: ${error instanceof Error?error.message:String(error)}`);}
        try{
            const host=await AssistanceIpcHost.start({directory:assistanceDirectory(storage),version,roots:this.assistanceFolders().map(folder=>folder.uri.fsPath),
                authorized:()=>this.assistanceEnabled(),
                explorerOpen:()=>this.explorerLoaded?.panelOpen??false,targetRevisions:()=>this.explorerLoaded?.draftRevisions??{},
                uiStatus:()=>this.explorerLoaded?.assistanceStatus??{binding_generation:0,blockers:['EXPLORER_CLOSED']},
                validate:(kind,draft)=>this.assistanceEnabled()?this.suggestions.validate(kind,draft):{valid:false,code:'WORKSPACE_UNTRUSTED'},
                handoff:token=>this.handoffs.get(token),
                suggest:(kind,draft,client)=>this.assistanceEnabled()?this.suggestions.submit(kind,draft,client):{ok:false,code:'WORKSPACE_UNTRUSTED',message:'The workspace is not trusted.'},
                proposalStatus:id=>this.suggestions.status(id)});
            if(generation!==this.assistanceGeneration||!this.assistanceEnabled()){host?.dispose();return;}
            this.assistanceHost=host;
        }catch(error){this.output.appendLine(`Explorer Assistance window endpoint is unavailable: ${error instanceof Error?error.message:String(error)}`);}
    }
    connectionEntries(spec:LaunchSpec):ServerEntry[]{
        return [unifiedEntry(spec,this.explorerDescriptor())];
    }
    agentStorage():string{return this.context.globalStorageUri.fsPath;}
    private explorerDescriptor():string|undefined{
        return vscode.workspace.getConfiguration('airk').get<boolean>('explorer.assistance.enabled',true)?launchDescriptorPath(this.context.globalStorageUri.fsPath):undefined;
    }
    private workspaceConnection():ServerEntry|undefined{
        const [first,...additional]=this.specs;
        return first?unifiedEntry(first,this.explorerDescriptor(),additional):undefined;
    }
    async verifyAssistance(spec:LaunchSpec):Promise<string[]>{
        this.trusted();
        const entry=this.connectionEntries(spec)[0];
        writeLaunchDescriptor(this.context.globalStorageUri.fsPath,this.context.extensionPath,this.runningVersion);
        const result=await probeMcp(spec,28,this.runningVersion,entry);this.trusted();
        return result.tools;
    }
    provideAssistanceDefinitions():vscode.McpStdioServerDefinition[]{
        // Retired provider remains registered long enough to withdraw old cached definitions.
        return [];
    }
    resolveAssistanceDefinition(_definition:vscode.McpServerDefinition):vscode.McpServerDefinition{
        throw new ServiceError('legacy_mcp_definition_retired_restart_airk');
    }
    /** Enabling is an explicit User-scope choice; it resumes the normal verified-runtime path. */
    async enableNativeExecution():Promise<void>{
        await vscode.workspace.getConfiguration('airk').update('runtime.nativeExecution',true,vscode.ConfigurationTarget.Global);
    }
    nativeExecutionChanged():void{
        if(nativeExecutionAllowed()){this.status.text='AIRK';this.status.command='airk.explorer.open';this.status.tooltip=undefined;}
        this.reset();this.definitions.fire();void this.start();
    }
    reset():void{this.generation++;this.host=undefined;this.specs=[];this.binding=undefined;this.checks.clear();this.checking=undefined;this.connectionCheck=undefined;this.console.reset();void this.refresh();}
    dispose():void{if(this.disposed)return;this.disposed=true;this.generation++;this.assistanceGeneration++;this.assistanceHost?.dispose();this.assistanceDefinitions.dispose();void this.explorer?.then(explorer=>explorer.dispose(),()=>undefined);this.console.dispose();this.status.dispose();this.definitions.dispose();this.treeChanges.dispose();this.output.dispose();}
}

let active:ServiceExtension|undefined;
export function deactivate():void{active?.dispose();active=undefined;}
/** Activated by the final 1.4 adapter cutover, after native/package contract checks. */
export async function activate(context:vscode.ExtensionContext):Promise<{getServerDefinitions():Promise<vscode.McpStdioServerDefinition[]>;whenReady():Promise<void>}>{
    setNativeExecutionGate(nativeExecutionSetting);
    const controller=new ServiceExtension(context);active=controller;context.subscriptions.push(controller);
    const explorerOpen=(route?:unknown)=>controller.explorerController().then(explorer=>explorer.open(typeof route==='string'?route:undefined));
    const workbench=new WorkbenchView(context,controller.explorerHost());
    controller.attachWorkbench(workbench);
    context.subscriptions.push(workbench,vscode.window.registerWebviewViewProvider('airk.workbench',workbench));
    const sidebar=new ExplorerSidebar({
        commandUris:['airk.index','airk.serviceConnect','airk.setupAgent','airk.installCli','airk.chooseStorage','airk.indexMode','airk.manageVscodeMcp','airk.explorer.open','airk.collections','airk.explorer.showSuggestions'],
        folders:()=>(vscode.workspace.workspaceFolders??[]).filter(folder=>['file','vscode-remote'].includes(folder.uri.scheme)).length,
        attach:async listener=>(await controller.explorerController()).onSnapshot(listener),
        open:()=>explorerOpen(),
        hide:async()=>{await vscode.commands.executeCommand('workbench.action.closeSidebar');},
        fallback:unavailable=>{void vscode.commands.executeCommand('setContext','airk.explorerUnavailable',unavailable);},
        search:query=>{void controller.explorerController().then(explorer=>explorer.search(query),error=>controller.output.appendLine(String(error)));},
    });
    context.subscriptions.push(sidebar,
        vscode.window.registerWebviewViewProvider('airk.dashboard',sidebar),
        vscode.window.createTreeView('airk.collectionsView',{treeDataProvider:controller}));
    const safe=(action:(...args:unknown[])=>unknown|Promise<unknown>)=>async(...args:unknown[])=>{try{return await action(...args);}catch(error){controller.output.appendLine(String(error));
        if(error instanceof ServiceError&&error.code==='extension_reload_required')return;
        void vscode.window.showErrorMessage(`AIRK: ${error instanceof Error?error.message:String(error)}`);throw error;}};
    // Former Service Console commands open the matching Explorer tab (design Appendix B).
    const routed=(name:string)=>async()=>{
        const target=COMMAND_ROUTES[name];
        if(target.action)await controller.console.run(target.action,{...target.fields});
        await explorerOpen(target.route);
    };
    const commands:Record<string,(...args:unknown[])=>unknown|Promise<unknown>>={index:()=>controller.index(),indexMode:()=>controller.watching(),pause:()=>controller.pause(true),resume:()=>controller.pause(false),
        setupAgent:()=>setupAgent(controller),manageMcp:()=>setupAgent(controller),manageVscodeMcp:()=>controller.manageVscodeMcp(),verifyMcp:project=>controller.verifyProject(project),
        treeRefresh:()=>controller.refresh(),
        treeIndex:spec=>controller.index(spec as LaunchSpec|undefined),treePause:spec=>controller.pause(true,spec as LaunchSpec|undefined)};
    for(const name of Object.keys(COMMAND_ROUTES))commands[name]=routed(name);
    commands.reindex=()=>controller.repair('rebuild');commands.treeReindex=spec=>controller.repair('rebuild',spec as LaunchSpec|undefined);
    commands.diagnose=()=>controller.repair('verify');commands.currentMaintenance=()=>controller.repair('auto');
    commands.status=()=>controller.showTool('airk_status');
    commands.chooseStorage=async()=>{controller.trusted();const selected=await vscode.window.showOpenDialog({canSelectFolders:true,canSelectFiles:false,canSelectMany:false,openLabel:'Review new project storage'});
        if(selected?.length){controller.trusted();await vscode.workspace.getConfiguration('airk').update('storage.path',selected[0].fsPath,vscode.ConfigurationTarget.Global);
            await controller.registerWorkspace();}};
    commands.installCli=()=>controller.repairInstallation();
    commands.enableNativeExecution=()=>controller.enableNativeExecution();
    commands.setUp=()=>controller.repairInstallation();
    commands.removeTerminalCommand=()=>controller.removeTerminalCommand();
    commands.showLastUninstallResult=()=>controller.showLastUninstallResult();
    commands.retryUpdate=()=>controller.retryUpdate();
    commands.reloadUpdatedExtension=()=>controller.reloadUpdatedExtension();
    commands.openTools=()=>controller.openTools();
    commands.usage=()=>controller.usage();
    commands.showExtensionOutput=()=>controller.output.show(false);
    commands['explorer.open']=route=>explorerOpen(route);
    commands['workbench.focus']=()=>workbench.focus();
    commands['workbench.newSession']=()=>workbench.newSession();
    commands['workbench.clearSessions']=()=>workbench.clear();
    commands['workbench.searchSelection']=async()=>{
        const editor=vscode.window.activeTextEditor;
        const text=editor?.document.getText(editor.selection).slice(0,4096);
        await workbench.focus(text?`/search ${JSON.stringify(text.replace(/\s+/g,' '))}`:undefined);
    };
    commands['explorer.showSuggestions']=async()=>(await controller.explorerController()).showInbox();
    commands['explorer.search']=async query=>{
        const text=typeof query==='string'?query:await vscode.window.showInputBox({title:'AIRK: Search indexed evidence',prompt:'All words must appear (lexical) unless the project uses vector or hybrid search.'});
        if(text?.trim())(await controller.explorerController()).search(text);
    };
    for(const [name,action] of Object.entries(commands))context.subscriptions.push(vscode.commands.registerCommand(`airk.${name}`,safe(action)));
    if(vscode.lm?.registerMcpServerDefinitionProvider)context.subscriptions.push(vscode.lm.registerMcpServerDefinitionProvider('airk.collections',{
        onDidChangeMcpServerDefinitions:controller.definitions.event,provideMcpServerDefinitions:()=>controller.provideDefinitions(),resolveMcpServerDefinition:value=>controller.resolveDefinition(value)}));
    if(vscode.lm?.registerMcpServerDefinitionProvider)context.subscriptions.push(vscode.lm.registerMcpServerDefinitionProvider('airk.explorerAssistance',{
        onDidChangeMcpServerDefinitions:controller.assistanceDefinitions.event,provideMcpServerDefinitions:()=>controller.provideAssistanceDefinitions(),
        resolveMcpServerDefinition:value=>controller.resolveAssistanceDefinition(value)}));
    const assistanceChanged=()=>{void controller.startAssistance();controller.assistanceDefinitions.fire();controller.definitions.fire();};
    context.subscriptions.push(vscode.workspace.onDidChangeWorkspaceFolders(()=>{assistanceChanged();return controller.automaticInstall().then(()=>controller.refresh());}),
        vscode.workspace.onDidGrantWorkspaceTrust(()=>{assistanceChanged();void controller.automaticInstall().then(()=>controller.refresh());}),
        vscode.window.onDidChangeActiveTextEditor(()=>void controller.console.controller.refresh()),vscode.workspace.onDidChangeConfiguration(event=>{
            if(event.affectsConfiguration('airk.explorer.assistance'))assistanceChanged();
            if(event.affectsConfiguration('airk.runtime.nativeExecution'))controller.nativeExecutionChanged();
            else if(event.affectsConfiguration('airk.service'))controller.reset();
            else if(event.affectsConfiguration('airk.mcp'))controller.registrationChanged();
        }));
    if(vscode.extensions?.onDidChange)context.subscriptions.push(vscode.extensions.onDidChange(()=>controller.checkForExtensionUpdate()));
    if(vscode.window.onDidChangeWindowState)context.subscriptions.push(vscode.window.onDidChangeWindowState(state=>{if(state.focused)controller.checkForExtensionUpdate();}));
    controller.checkForExtensionUpdate();
    void controller.startAssistance();
    const ready=controller.start();
    return {getServerDefinitions:async()=>{await ready;return controller.provideDefinitions();},whenReady:()=>ready};
}
