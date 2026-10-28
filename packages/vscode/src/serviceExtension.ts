// SPDX-License-Identifier: Apache-2.0
//! Service-backed extension activation. Native ownership outlives editor windows.
import * as vscode from 'vscode';
import * as fs from 'node:fs/promises';
import * as path from 'node:path';
import { setupAgent, AgentSetupHost } from './agentSetup';
import { collectionId, launchArgs, LaunchSpec, McpVerification, unifiedEntry, ServerEntry } from './model';
import { probeMcp, runText } from './native';
import { automaticUpgrade, RuntimeBinding, RuntimeHost } from './serviceBootstrap';
import { ConsoleRuntime, ServiceConsole, serviceRuntimeOptions } from './serviceConsole';
import { identifier, record, ServiceError } from './serviceClient';
import { IntentJournal } from './serviceIntents';
import { ServiceWorkspace } from './serviceWorkspace';
import { installLauncherCli } from './cliInstall';
import { openUsagePanel } from './usagePanel';
import { renderUsage } from './usageReport';
import { COMMAND_ROUTES } from './explorerCommands';
import { ExplorerSidebar } from './explorerSidebar';
import type { ExplorerController, ExplorerHost } from './explorerController';
import { hasUnifiedVscodeConfig, migrateLocalAgents } from './agentMigration';
import { readJsonConfig } from './agentConfig';
import { AssistanceIpcHost } from './explorerAssistanceHost';
import { HandoffStore } from './explorerHandoffs';
import { SuggestionStore } from './explorerSuggestions';
import { assistanceDirectory, launchDescriptorPath, writeLaunchDescriptor } from './explorerAssistanceLaunch';

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
    private binding?:RuntimeBinding;
    private generation=0;
    private disposed=false;
    private specs:LaunchSpec[]=[];
    private roots:{opened:string;canonical:string}[]=[];
    private readonly checks=new Map<string,McpVerification>();
    private refreshing?:Promise<void>;
    private settingUp?:Promise<void>;
    private migratedConnections=false;
    private installationWarnings=false;
    private installationFailed=false;
    private installProgress?:vscode.Progress<{message?:string}>;
    private explorer?:Promise<ExplorerController>;
    private explorerLoaded?:ExplorerController;
    private assistanceHost?:AssistanceIpcHost;
    private assistanceGeneration=0;
    /** Agent drafts for this window, kept in memory only; the Explorer Inbox shows them. */
    readonly suggestions=new SuggestionStore({binding:()=>this.assistanceHost?.bindingFingerprint??'',revisions:()=>this.explorerLoaded?.draftRevisions??{},
        publication:()=>this.explorerLoaded?.publication,
        ttlMs:()=>Math.min(168,Math.max(1,vscode.workspace.getConfiguration('airk').get<number>('explorer.suggestions.ttlHours',24)||24))*3600_000});

    private reportInstall(message:string):void{
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
        this.console=new ServiceConsole(context,runtime,{register:()=>this.registerWorkspace(),currentProjects:()=>this.currentProjects()});
        this.status.text='AIRK';this.status.command='airk.explorer.open';this.status.show();
    }
    private runtime():RuntimeHost{
        if(!this.host){const generation=this.generation;
            this.host=new RuntimeHost(this.context.extensionPath,this.runningVersion,serviceRuntimeOptions(),undefined,
                ()=>vscode.workspace.isTrusted&&!this.disposed&&!this.extensionUpdate&&generation===this.generation);}
        return this.host;
    }
    trusted():void {
        if(this.checkForExtensionUpdate())throw new ServiceError('extension_reload_required');
        if(!vscode.workspace.isTrusted||this.disposed)throw new ServiceError('runtime_client_no_longer_authorized');
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
    isCurrent(spec:LaunchSpec):boolean{return !this.disposed&&this.specs.some(current=>current.command===spec.command&&JSON.stringify(launchArgs(current))===JSON.stringify(launchArgs(spec)));}
    subscribe(panel:vscode.WebviewPanel):void{this.context.subscriptions.push(panel);}
    verification(spec:LaunchSpec):McpVerification|undefined{return this.checks.get(spec.service!.project);}
    async refresh():Promise<void>{
        if(this.refreshing)return this.refreshing;
        this.refreshing=this.refreshOnce().finally(()=>{this.refreshing=undefined;});return this.refreshing;
    }
    private async refreshOnce():Promise<void>{
        if(this.checkForExtensionUpdate()){this.renderUpdateStatus();return;}
        const previous=JSON.stringify(this.specs.map(spec=>[spec.command,launchArgs(spec)]));
        // Observe into locals and publish once: readers never see an empty registration
        // list in the middle of a refresh, only the previous or the new result.
        const clear=()=>{this.specs=[];this.roots=[];this.binding=undefined;};
        try{
            this.trusted();const observation=await this.runtime().discover();this.trusted();
            if(!observation.inspection||observation.inspection.payload_verified===false||observation.inspection.metadata_only===true){clear();this.status.text='AIRK: setup';return;}
            const binding=this.runtime().binding(observation.inspection,observation.installDir);
            if(binding.version!==this.runningVersion)throw new ServiceError('explicit_runtime_upgrade_or_matching_extension_required');
            const roots:{opened:string;canonical:string}[]=[];
            for(const folder of this.folders()){
                try{roots.push({opened:folder.uri.fsPath,canonical:await fs.realpath(folder.uri.fsPath)});}catch{/* Saved missing-source bindings remain readable. */}
            }
            const specs=await this.workspace.definitions(binding,this.folders().map(folder=>folder.uri.fsPath));this.trusted();
            this.binding=binding;this.roots=roots;this.specs=specs;
            this.status.text=`AIRK: ${this.specs.length} registered`;
        }catch(error){clear();this.status.text='AIRK: inspect service';this.output.appendLine(String(error));}
        finally{this.treeChanges.fire();if(previous!==JSON.stringify(this.specs.map(spec=>[spec.command,launchArgs(spec)])))this.definitions.fire();}
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
        const observation=await this.runtime().discover();this.trusted();
        const configured=vscode.workspace.getConfiguration('airk').inspect<string>('storage.path')?.globalValue;
        const nativeData=record(observation.location.user_paths).data;
        const dataRoot=configured||nativeData;
        if(typeof dataRoot!=='string'||!path.isAbsolute(dataRoot))throw new ServiceError('invalid_project_data_location');
        const binding=await this.runtime().ensure();this.trusted();
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
        const selected=spec??await this.chooseSpec();if(!selected)return;this.trusted();
        if(!this.isCurrent(selected))throw new ServiceError('workspace_registration_changed');
        try{const result=await probeMcp(selected,26,this.runningVersion,this.connectionEntries(selected)[0]);this.trusted();
            this.checks.set(selected.service!.project,{phase:'verified',tools:result.tools,checkedAt:Date.now(),
                indexState:record(result.status).state==='not_indexed'?'not_indexed':record(result.status).state==='index_unavailable'?'unavailable':'published'});
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
        const errors=await installLauncherCli(binding.universalLauncher??binding.launcher,this.context);this.trusted();
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
    private async migrateAgentConnections(binding:RuntimeBinding):Promise<void>{
        this.trusted();
        const generation=this.generation,folders=this.folders().map(folder=>folder.uri.fsPath);
        const specs=await this.workspace.definitions(binding,folders);
        for(const spec of specs){
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
            },vscode.workspace.getConfiguration('airk').get<boolean>('explorer.assistance.enabled',true));
            for(const message of messages){
                this.output.appendLine(message);
                if(message.startsWith('Updated '))this.migratedConnections=true;
                if(message.startsWith('Could not update'))this.installationWarnings=true;
            }
        }
    }
    automaticInstall(notify=true,explicit=false):Promise<void>{
        if(this.checkForExtensionUpdate())return Promise.resolve();
        if(!vscode.workspace.isTrusted||!explicit&&vscode.workspace.getConfiguration('airk').inspect<boolean>('cli.autoInstall')?.globalValue===false)return Promise.resolve();
        this.settingUp??=this.automaticSetup(notify,explicit).finally(()=>{this.settingUp=undefined;});return this.settingUp;
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
        try{
            this.reportInstall('Checking runtime files…');
            let installed=await this.runtime().discover();this.trusted();
            if(!explicit&&installed.inspection&&installed.inspection.payload_verified!==false&&installed.inspection.metadata_only!==true){
                const binding=this.runtime().binding(installed.inspection,installed.installDir);
                const identity=`${this.runningVersion}:${binding.runtimeSha256}`;
                if(binding.version===this.runningVersion&&binding.runtimeSha256===installed.bundle.digest
                    &&!installed.diagnosis?.pending_activation&&this.context.globalState.get<string>(`airk.installComplete.1.${binding.installDir}`)===identity){
                    runtimeInstalled=true;
                    if(installed.inspection.lifecycle&&record(installed.inspection.lifecycle).explicitly_stopped===true)throw new ServiceError('service_explicitly_stopped');
                    // A completed installation does not prove that its on-demand service survived editor/OS shutdown.
                    await this.runtime().ensure();this.trusted();
                    await this.prepareWorkspaceProjects(binding,installed);
                    this.reportInstall('Installed runtime verified. Saved project work is managed by the AIRK service.');
                    if(notify&&(this.installationWarnings||this.migratedConnections))await this.installationComplete();
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
            await this.prepareWorkspaceProjects(binding,await this.runtime().discover());
            this.reportInstall(this.installationWarnings?'Runtime installation finished with issues to review.':'Runtime ready. Project indexing continues in Management.');
            if(notify)await this.installationComplete();
            },progressTitle);
        }
        catch(error){
            if(error instanceof ServiceError&&error.code==='extension_reload_required')return;
            if(String(error).includes('service_explicitly_stopped')){
                this.output.appendLine('AIRK service_explicitly_stopped. Start the service from Management > Service when ready. Saved project work is retained.');return;
            }
            if(runtimeInstalled){
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
    private async prepareWorkspaceProjects(binding:RuntimeBinding,observation:import('./serviceBootstrap').BootstrapObservation):Promise<void>{
        this.trusted();
        const folders=this.folders(),specs=await this.workspace.definitions(binding,folders.map(folder=>folder.uri.fsPath));
        const dataRoot=vscode.workspace.getConfiguration('airk').inspect<string>('storage.path')?.globalValue||record(observation.location.user_paths).data;
        if(typeof dataRoot!=='string'||!path.isAbsolute(dataRoot))throw new ServiceError('invalid_project_data_location');
        for(const folder of folders){
            if(this.workspace.hasRegistration(binding,folder.uri.fsPath))continue;
            this.trusted();const canonical=await fs.realpath(folder.uri.fsPath);this.trusted();
            if(specs.some(spec=>same(spec.root,canonical)))continue;
            this.reportInstall('Registering a new workspace project...');
            const project=await this.workspace.register(binding,folder.uri.fsPath,dataRoot,collectionId(canonical),async()=>true,true);
            if(!project)continue;this.trusted();
            this.output.appendLine(`Registered ${folder.uri.fsPath}. Native model preparation and indexing continue independently of VS Code. Follow progress in Management.`);
        }
        this.reportInstall('Updating agent connections…');
        await this.migrateAgentConnections(binding);
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
        if(!vscode.workspace.isTrusted||!this.registrationEnabled())return [];
        await this.refresh();if(!vscode.workspace.isTrusted||!this.registrationEnabled())return [];
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
        await this.console.run('connect');
        (await this.explorerController()).open(route);
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
    private explorerHost():ExplorerHost{
        return {extensionUri:this.context.extensionUri,version:this.runningVersion,
            extensionUpdate:()=>this.extensionUpdate,reloadUpdatedExtension:()=>this.reloadUpdatedExtension(),
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
        const result=await probeMcp(spec,26,this.runningVersion,entry);this.trusted();
        return result.tools;
    }
    provideAssistanceDefinitions():vscode.McpStdioServerDefinition[]{
        // Retired provider remains registered long enough to withdraw old cached definitions.
        return [];
    }
    resolveAssistanceDefinition(_definition:vscode.McpServerDefinition):vscode.McpServerDefinition{
        throw new ServiceError('legacy_mcp_definition_retired_restart_airk');
    }
    reset():void{this.generation++;this.host=undefined;this.specs=[];this.binding=undefined;this.checks.clear();this.console.reset();void this.refresh();}
    dispose():void{if(this.disposed)return;this.disposed=true;this.generation++;this.assistanceGeneration++;this.assistanceHost?.dispose();this.assistanceDefinitions.dispose();void this.explorer?.then(explorer=>explorer.dispose(),()=>undefined);this.console.dispose();this.status.dispose();this.definitions.dispose();this.treeChanges.dispose();this.output.dispose();}
}

let active:ServiceExtension|undefined;
export function deactivate():void{active?.dispose();active=undefined;}
/** Activated by the final 1.4 adapter cutover, after native/package contract checks. */
export async function activate(context:vscode.ExtensionContext):Promise<{getServerDefinitions():Promise<vscode.McpStdioServerDefinition[]>}>{
    const controller=new ServiceExtension(context);active=controller;context.subscriptions.push(controller);
    const explorerOpen=(route?:unknown)=>controller.explorerController().then(explorer=>explorer.open(typeof route==='string'?route:undefined));
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
        setupAgent:()=>setupAgent(controller),manageMcp:()=>setupAgent(controller),manageVscodeMcp:()=>controller.manageVscodeMcp(),verifyMcp:()=>controller.verify(),
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
    commands.retryUpdate=()=>controller.retryUpdate();
    commands.reloadUpdatedExtension=()=>controller.reloadUpdatedExtension();
    commands.openTools=()=>controller.openTools();
    commands.usage=()=>controller.usage();
    commands.showExtensionOutput=()=>controller.output.show(false);
    commands['explorer.open']=route=>explorerOpen(route);
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
            if(event.affectsConfiguration('airk.service'))controller.reset();
            else if(event.affectsConfiguration('airk.mcp'))controller.registrationChanged();
        }));
    if(vscode.extensions?.onDidChange)context.subscriptions.push(vscode.extensions.onDidChange(()=>controller.checkForExtensionUpdate()));
    if(vscode.window.onDidChangeWindowState)context.subscriptions.push(vscode.window.onDidChangeWindowState(state=>{if(state.focused)controller.checkForExtensionUpdate();}));
    controller.checkForExtensionUpdate();
    void controller.startAssistance();
    await controller.automaticInstall();await controller.refresh();return {getServerDefinitions:()=>controller.provideDefinitions()};
}
