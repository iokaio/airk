// SPDX-License-Identifier: Apache-2.0
//! Service-backed extension activation. Native ownership outlives editor windows.
import * as vscode from 'vscode';
import * as fs from 'node:fs/promises';
import { statSync } from 'node:fs';
import { randomBytes } from 'node:crypto';
import * as path from 'node:path';
import { setupAgent, AgentSetupHost } from './agentSetup';
import { launchArgs, LaunchSpec, McpVerification, unifiedEntry, ServerEntry, UNIFIED_TOOLS } from './model';
import { probeMcp, runText } from './native';
import { automaticUpgrade, BootstrapObservation, nativeRunner, RuntimeBinding, RuntimeHost } from './serviceBootstrap';
import { ConsoleRuntime, ServiceConsole, serviceRuntimeOptions } from './serviceConsole';
import { identifier, record, ServiceError } from './serviceClient';
import { IntentJournal } from './serviceIntents';
import { nativeScopeLaunchSpec, readWorkspaceRegistrations, ServiceWorkspace } from './serviceWorkspace';
import { installLauncherCli, shellIntegration, ShellTouch } from './cliInstall';
import { defaultProfiles, removeShellIntegration } from './cliRemove';
import { openUsagePanel, type AdministrativeIndexSelection, type UsageAccess } from './usagePanel';
import { ADMINISTRATIVE_USAGE_LABEL, renderUsage } from './usageReport';
import { COMMAND_ROUTES, MANAGE_WORKSPACE_TRUST_COMMAND, NATIVE_COMMANDS } from './explorerCommands';
import { CatalogObject, WindowContextDescriptor, catalogBound, catalogId, catalogObject, catalogRevision, catalogHex, validateNativeHostContext, validateWindowContext } from './indexingCatalog';
import { reviewScopeSelection, reviewCombinedScopeSelection, scopeSelectionPreference, type ReviewedWindowScopeSelection } from './scopeSelection';
import type { SearchDraft } from './explorerDrafts';
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

/** Identity of the loaded extension code. VS Code keeps a same-version reinstall
 * in place, so the version alone cannot reveal that a rebuilt VSIX replaced it. */
function codeStamp(root:string|undefined):string|undefined{
    if(!root)return undefined;
    try{
        const info=statSync(path.join(root,'out','serviceExtension.js')),resolved=path.resolve(root);
        // VS Code's context and extension registry may spell the same Windows
        // path differently. Preserve file changes without treating casing as a rebuild.
        const identity=process.platform==='win32'?path.toNamespacedPath(resolved).toLowerCase():resolved;
        return `${identity}:${info.size}:${info.mtimeMs}`;
    }catch{return undefined;}
}
function same(left:string,right:string):boolean {
    return path.relative(process.platform==='win32'?path.toNamespacedPath(left):left,process.platform==='win32'?path.toNamespacedPath(right):right)==='';
}
class ServiceExtension implements vscode.Disposable,AgentSetupHost,vscode.TreeDataProvider<LaunchSpec> {
    // Capture the loaded code's version; extension metadata can change after a VSIX install.
    private readonly runningVersion:string;
    private readonly codeIdentity:string|undefined;
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
    private readonly transientWindowKey=randomBytes(32).toString('hex');
    private readonly windowContextChanges=new vscode.EventEmitter<void>();
    private windowContextRead?:{identity:string;work:Promise<void>};
    private windowContextValue?:{identity:string;generation:number;controller:ServiceConsole['controller'];controlIdentity:string;response:CatalogObject;descriptor:WindowContextDescriptor};
    private specs:LaunchSpec[]=[];
    private explicitCatalogScope?:{selection:ReviewedWindowScopeSelection;spec:LaunchSpec;generation:number;window:string;controlIdentity:string;runtime:string};
    // Early UI availability is deliberately separate from complete workspace MCP scopes.
    private selectedEligible?:{spec:LaunchSpec;opened:string;generation:number;folders:string;host:RuntimeHost};
    private selectedScope?:{root:string;collection:string|null;outcome:'registered'|'missing'|'removed'|'ambiguous';generation:number;folders:string;host:RuntimeHost};
    private roots:{opened:string;canonical:string}[]=[];
    private readonly checks=new Map<string,McpVerification>();
    private connectionCheck?:McpVerification;
    private checking?:Promise<void>;
    private startup?:{message:string;startedAt:number;failed?:boolean};
    private refreshing?:{generation:number;promise:Promise<void>};
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
        const extension=vscode.extensions?.getExtension(this.context.extension.id??'iokaio.airk');
        let installed=extension?.packageJSON.version;
        // A rebuilt VSIX with the same version replaces the code on disk without a version change.
        if(installed===this.runningVersion&&this.codeIdentity!==undefined&&codeStamp(extension?.extensionPath??this.context.extensionPath)!==this.codeIdentity)installed=`${installed} (rebuilt)`;
        if(!this.extensionUpdate&&typeof installed==='string'&&/^\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?(?: \(rebuilt\))?$/.test(installed)&&installed!==this.runningVersion){
            this.extensionUpdate={runningVersion:this.runningVersion,installedVersion:installed};
            this.publishNativeCommandAvailability();
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
        this.codeIdentity=codeStamp(context.extensionPath);
        const runtime:ConsoleRuntime={discover:()=>this.runtime().discover(),ensure:explicit=>this.runtime().ensure(explicit),
            binding:(inspection,install)=>this.runtime().binding(inspection,install),client:binding=>this.runtime().client(binding),
            snapshot:(binding,after,revision)=>this.runtime().snapshot(binding,after,revision),repair:(binding,preview)=>this.runtime().repair(binding,preview),
            prepareConfiguration:(binding,project,draft)=>this.runtime().prepareConfiguration(binding,project,draft),
            reviewLegacyStartup:(binding,target)=>this.runtime().reviewLegacyStartup(binding,target),loadLegacyStartup:(binding,approval)=>this.runtime().loadLegacyStartup(binding,approval),
            legacyStartupReceipts:binding=>this.runtime().legacyStartupReceipts(binding),removeLegacyStartup:(binding,review)=>this.runtime().removeLegacyStartup(binding,review),
            reviewTransition:(binding,action)=>this.runtime().reviewTransition(binding,action),transition:(binding,review)=>this.runtime().transition(binding,review),
            lifecycle:(binding,intent,force)=>this.runtime().lifecycle(binding,intent,force),abandonLifecycle:(binding,intent,id)=>this.runtime().abandonLifecycle(binding,intent,id)};
        this.workspace=new ServiceWorkspace(context.globalState,{reviewProject:(...args)=>this.runtime().reviewProject(...args),
            registerProject:(...args)=>this.runtime().registerProject(...args)},()=>vscode.workspace.isTrusted&&!this.disposed&&!this.extensionUpdate&&nativeExecutionAllowed());
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
        this.publishNativeCommandAvailability();
    }
    private runtime():RuntimeHost{
        if(!this.host){const generation=this.generation;
            this.host=new RuntimeHost(this.context.extensionPath,this.runningVersion,serviceRuntimeOptions(),undefined,
                ()=>vscode.workspace.isTrusted&&!this.disposed&&!this.extensionUpdate&&generation===this.generation&&nativeExecutionAllowed());}
        return this.host;
    }
    trusted():void {
        if(this.disposed)throw new ServiceError('runtime_client_no_longer_authorized');
        if(!vscode.workspace.isTrusted)throw new ServiceError('workspace_untrusted');
        if(this.checkForExtensionUpdate())throw new ServiceError('extension_reload_required');
        if(!nativeExecutionAllowed())throw new NativeExecutionDisabled();
    }
    private publishNativeCommandAvailability():void{
        void vscode.commands.executeCommand('setContext','airk.nativeCommandsAvailable',!this.disposed&&vscode.workspace.isTrusted&&!this.extensionUpdate&&nativeExecutionAllowed()).then(undefined,()=>{});
    }
    async manageWorkspaceTrust():Promise<void>{
        if(this.disposed)throw new ServiceError('runtime_client_no_longer_authorized');
        await vscode.commands.executeCommand(MANAGE_WORKSPACE_TRUST_COMMAND);
    }
    private windowIdentity():{identity:string;kind:WindowContextDescriptor['kind'];name:string;persistent:boolean}{
        const file=vscode.workspace.workspaceFile,folders=this.folders();
        const uri=(value:vscode.Uri)=>value.toString();
        const kind:WindowContextDescriptor['kind']=file?(file.scheme==='untitled'?'untitled':'workspace'):folders.length===1?'folder':folders.length?'untitled':'empty';
        const name=vscode.workspace.name??(kind==='folder'?folders[0].name:kind==='empty'?'Empty window':kind==='untitled'?'Untitled workspace':'Workspace');
        const context=file?uri(file):kind==='folder'?uri(folders[0].uri):this.transientWindowKey;
        return {identity:JSON.stringify([kind,context,name]),kind,name,persistent:!!file||kind==='folder'};
    }
    private currentWindowContext(){
        const value=this.windowContextValue;
        if(!value||this.disposed||!vscode.workspace.isTrusted||!nativeExecutionAllowed()||this.extensionUpdate||value.generation!==this.generation
            ||value.identity!==this.windowIdentity().identity||value.controller!==this.console.controller||value.controlIdentity!==this.console.controller.observationIdentity)return undefined;
        return value;
    }
    /** Explicit administrative read only. Getters never connect, register, grant or invoke native work. */
    async prepareWindowContext():Promise<void>{
        this.trusted();const window=this.windowIdentity(),generation=this.generation,controller=this.console.controller,controlIdentity=controller.observationIdentity;
        const binding=controller.connectedRuntimeBinding;
        if(!binding||binding.version!==this.runningVersion)throw new ServiceError('catalog_control_connection_unavailable');
        if(this.windowContextRead?.identity===JSON.stringify([window.identity,generation,controlIdentity]))return this.windowContextRead.work;
        const identity=JSON.stringify([window.identity,generation,controlIdentity]);
        const current=()=>{this.trusted();if(generation!==this.generation||controller!==this.console.controller||controlIdentity!==controller.observationIdentity||binding!==controller.connectedRuntimeBinding||window.identity!==this.windowIdentity().identity)throw new ServiceError('window_context_changed');};
        const work=(async()=>{
            current();if(!controller.state.connected)throw new ServiceError('catalog_control_connection_unavailable');
            let key=this.transientWindowKey;
            if(window.persistent){
                const saved=this.context.workspaceState.get<string>('airk.windowContextKey.1');
                if(saved!==undefined&&!/^[0-9a-f]{64}$/.test(saved))throw new ServiceError('invalid_saved_window_context_key');
                key=saved??randomBytes(32).toString('hex');
                if(saved===undefined){current();await this.context.workspaceState.update('airk.windowContextKey.1',key);current();}
            }
            const response=await controller.catalogHostContext(key);current();
            const native=validateNativeHostContext(response.descriptor);
            const descriptor=validateWindowContext({host:native.host,key:native.key,kind:window.kind,name:window.name});
            // Another window may have installed the runtime after our startup.
            // Adopt only this completed, authenticated console connection. Reading
            // its context selects no grant and restores no saved authority.
            if(!this.binding||JSON.stringify(this.binding)!==JSON.stringify(binding)){
                this.specs=[];this.roots=[];this.retireSelectedEligibility();this.explicitCatalogScope=undefined;
                this.binding=binding;
            }
            this.observedFolders=JSON.stringify(this.folders().map(folder=>folder.uri.fsPath));
            this.windowContextValue={identity:window.identity,generation,controller,controlIdentity,response,descriptor};
            this.windowContextChanges.fire();
        })();
        this.windowContextRead={identity,work};
        try{await work;}finally{if(this.windowContextRead?.work===work)this.windowContextRead=undefined;}
    }
    private currentCatalogScope(){
        const value=this.explicitCatalogScope,context=this.currentWindowContext();
        if(!value||!context||!this.binding||this.disposed||value.generation!==this.generation||value.window!==this.windowIdentity().identity||value.controlIdentity!==this.console.controller.observationIdentity
            ||value.runtime!==JSON.stringify([this.binding.installDir,this.binding.controlDir,this.binding.runtimeSha256])||value.selection.host!==context.descriptor.host||value.selection.session.context_key!==context.descriptor.key||catalogObject(context.response.descriptor).namespace!==value.selection.session.namespace)return undefined;
        return value;
    }
    private hasScopeWithdrawal():boolean{
        try{
            const saved=this.context.workspaceState.get('airk.scope.explicitWithdrawal.1');if(saved===undefined)return false;
            const value=catalogObject(saved);catalogBound(value,4096);
            if(value.schema!==1||!Array.isArray(value.project_ids)||!value.project_ids.length||value.project_ids.length>32||new Set(value.project_ids).size!==value.project_ids.length)throw new Error('invalid_scope_withdrawal');
            catalogHex(value.namespace);catalogHex(value.host,64);catalogHex(value.context_key,64);catalogId(value.grant_id,'grt_');catalogRevision(value.expected_grant_revision);value.project_ids.forEach(id=>catalogId(id,'prj_'));
            if(value.index_id!==undefined)catalogId(value.index_id,'idx_');
            if(value.index_ids!==undefined){if(!Array.isArray(value.index_ids)||!value.index_ids.length||value.index_ids.length>32||new Set(value.index_ids).size!==value.index_ids.length)throw new Error('invalid_scope_withdrawal');value.index_ids.forEach(id=>catalogId(id,'idx_'));}
            return true;
        }catch{return true;} // Uncertain local storage can withdraw access, never enable a legacy fallback.
    }
    /** Explicit user selection only: reads metadata, then exposes one native-reviewed selector. */
    async useGrant(input:CatalogObject,namespace:string,revision:number,valid:()=>boolean):Promise<CatalogObject>{
        this.trusted();const context=this.currentWindowContext(),binding=this.binding,controller=this.console.controller,generation=this.generation,controlIdentity=controller.observationIdentity;
        if(!context||!binding||!controller.state.connected)throw new ServiceError('read_current_native_window_context_before_use_grant');
        const descriptor=validateNativeHostContext(context.response.descriptor);catalogHex(namespace);catalogRevision(revision);
        if(descriptor.namespace!==namespace||descriptor.catalog_revision!==revision)throw new ServiceError('read_current_native_window_context_before_use_grant');
        const keys=Object.keys(input).sort().join(','),legacy=keys==='expected_grant_revision,grant_id,index_id,project_ids';
        if(!legacy&&keys!=='expected_grant_revision,grant_id,project_ids'&&keys!=='expected_grant_revision,grant_id,index_ids,project_ids')throw new ServiceError('invalid_scope_selection');
        if(!Array.isArray(input.project_ids)||!input.project_ids.length||input.project_ids.length>32||new Set(input.project_ids).size!==input.project_ids.length)throw new ServiceError('invalid_scope_selection');
        const common={grant_id:catalogId(input.grant_id,'grt_'),expected_grant_revision:catalogRevision(input.expected_grant_revision),project_ids:input.project_ids.map(id=>catalogId(id,'prj_'))};
        const current=()=>{this.trusted();if(!valid()||generation!==this.generation||binding!==this.binding||controller!==this.console.controller||controlIdentity!==controller.observationIdentity||context!==this.currentWindowContext())throw new ServiceError('stale_scope_selection');};
        current();const grant=await controller.catalogRead({catalog_detail:{entity:{kind:'grant',id:common.grant_id},expected_catalog_revision:revision}});current();
        let selection:ReviewedWindowScopeSelection,index:CatalogObject|undefined,indexes:CatalogObject[]|undefined;
        if(legacy){
            const choice={...common,index_id:catalogId(input.index_id,'idx_')};
            index=await controller.catalogRead({catalog_detail:{entity:{kind:'index',id:choice.index_id},expected_catalog_revision:revision}});current();
            selection=reviewScopeSelection(choice,descriptor,grant.detail,index.detail);
        }else{
            if(input.index_ids!==undefined&&(!Array.isArray(input.index_ids)||!input.index_ids.length||input.index_ids.length>32||new Set(input.index_ids).size!==input.index_ids.length))throw new ServiceError('invalid_scope_selection');
            const subset=input.index_ids===undefined?undefined:(input.index_ids as unknown[]).map(id=>catalogId(id,'idx_'));
            const frozen=catalogObject(catalogObject(catalogObject(grant.detail).item).value),members=catalogObject(frozen.grant).memberships;
            if(!Array.isArray(members)||members.length>1024)throw new ServiceError('reviewed_scope_grant_unavailable');
            const union=[...new Set(members.map(raw=>catalogObject(catalogObject(raw).membership)).filter(member=>common.project_ids.includes(String(member.project_id))).map(member=>catalogId(member.index_id,'idx_')))].sort();
            const ids=subset??union;if(!ids.length||ids.length>32||ids.some(id=>!union.includes(id)))throw new ServiceError('scope_selection_outside_reviewed_grant');
            indexes=[];
            for(const id of ids){current();indexes.push(await controller.catalogRead({catalog_detail:{entity:{kind:'index',id},expected_catalog_revision:revision}}));current();}
            selection=reviewCombinedScopeSelection({...common,...(subset?{index_ids:subset}:{})},descriptor,grant.detail,indexes.map(value=>value.detail));
        }
        const spec=nativeScopeLaunchSpec(binding,selection);current();
        // An inert local withdrawal survives reload; stored choices never activate access.
        await this.context.workspaceState.update('airk.scope.explicitWithdrawal.1',scopeSelectionPreference(selection));current();
        // No asynchronous gap between the final fence and publishing the new authority.
        this.explicitCatalogScope={selection,spec,generation,window:this.windowIdentity().identity,controlIdentity,runtime:JSON.stringify([binding.installDir,binding.controlDir,binding.runtimeSha256])};
        this.checks.clear();this.connectionCheck=undefined;this.treeChanges.fire();this.definitions.fire();
        // Workbench uses semantic scope identity and preserves histories from prior subsets.
        await this.workbench?.useScope(spec);
        this.trusted();if(this.currentCatalogScope()?.spec!==spec)throw new ServiceError('stale_scope_selection');
        await this.openWorkbenchAfterSetup();
        this.trusted();if(this.currentCatalogScope()?.spec!==spec)throw new ServiceError('stale_scope_selection');
        return {selection,host_context:context.response,grant_detail:grant,...(index?{index_detail:index}:{}),...(indexes?{index_details:indexes}:{}),selected_subset:true};
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
    isCurrent(spec:LaunchSpec):boolean{
        if(this.disposed||!this.currentBinding())return false;
        const selected=this.currentCatalogScope();
        if(!spec.catalogSession&&!spec.combinedSession&&(this.explicitCatalogScope||this.hasScopeWithdrawal()))return false;
        const candidates=spec.catalogSession||spec.combinedSession?selected?[selected.spec]:[]:this.specs;
        return candidates.some(current=>current.command===spec.command&&current.dataDir===spec.dataDir&&current.collection===spec.collection&&current.service?.project===spec.service?.project&&JSON.stringify(launchArgs(current))===JSON.stringify(launchArgs(spec)));
    }
    subscribe(panel:vscode.WebviewPanel):void{this.context.subscriptions.push(panel);}
    verification(spec:LaunchSpec):McpVerification|undefined{return this.checks.get(spec.service!.project);}
    private uiSpecs():LaunchSpec[]{
        const explicit=this.currentCatalogScope();if(this.explicitCatalogScope||this.hasScopeWithdrawal())return explicit?[explicit.spec]:[];
        const early=this.selectedEligible,scope=this.selectedScope;
        // Exact native absence/replacement suppresses stale UI receipts only.
        // Complete configured workspace MCP declarations still use this.specs.
        const full=scope&&this.selectedFence(scope)?this.specs.filter(spec=>!(same(spec.root,scope.root)&&(scope.collection===null||spec.collection===scope.collection))):this.specs;
        if(!early||!this.selectedFence(early))return full;
        return [early.spec,...(this.currentBinding()?full:[]).filter(spec=>spec.service?.project!==early.spec.service?.project)];
    }
    private isUiCurrent(spec:LaunchSpec):boolean{return this.uiSpecs().some(current=>JSON.stringify(current)===JSON.stringify(spec));}
    private retireSelectedEligibility():void{
        // A positive exclusion only accompanies its replacement UI entry.
        // Explicit native negative observations remain fenced until replaced.
        if(this.selectedScope?.outcome==='registered')this.selectedScope=undefined;
        this.selectedEligible=undefined;
    }
    private selectedFence(value:Pick<NonNullable<ServiceExtension['selectedEligible']>,'generation'|'folders'|'host'>):boolean{
        return !this.disposed&&vscode.workspace.isTrusted&&!this.extensionUpdate&&nativeExecutionAllowed()
            &&value.generation===this.generation&&value.host===this.host
            &&value.folders===JSON.stringify(this.folders().map(folder=>folder.uri.fsPath));
    }
    private async observeSelected(binding:RuntimeBinding):Promise<void>{
        this.trusted();const host=this.runtime(),generation=this.generation;
        const folders=this.folders(),identity=JSON.stringify(folders.map(folder=>folder.uri.fsPath));
        const valid=()=>{this.trusted();if(generation!==this.generation||host!==this.host||identity!==JSON.stringify(this.folders().map(folder=>folder.uri.fsPath)))throw new ServiceError('workspace_registration_changed');};
        valid();
        const active=vscode.window.activeTextEditor&&vscode.workspace.getWorkspaceFolder(vscode.window.activeTextEditor.document.uri);
        const explicit=this.explorerLoaded?.assistanceStatus.selected_project;
        const chosen=this.specs.find(spec=>spec.service?.project===explicit);
        const opened=chosen&&this.roots.find(root=>same(root.canonical,chosen.root))?.opened;
        const folder=opened&&folders.find(folder=>same(folder.uri.fsPath,opened))
            ||active&&folders.find(folder=>same(folder.uri.fsPath,active.uri.fsPath))||folders[0];
        const selected=folder&&await this.workspace.selectedObservation(binding,folder.uri.fsPath,async(root,collection)=>{const observed=await host.sourceScope(binding,root,collection);valid();return observed;});valid();
        this.selectedScope=selected?{root:selected.root,collection:selected.collection,outcome:selected.kind,generation,folders:identity,host}:undefined;
        this.selectedEligible=folder&&selected?.kind==='registered'?{spec:selected.spec,opened:folder.uri.fsPath,generation,folders:identity,host}:undefined;
        this.treeChanges.fire(); // Explorer/Workbench only: never definitions.fire here.
        if(selected?.kind==='ambiguous')throw new ServiceError('workspace_binding_review_required');
    }
    async refresh(observation?:BootstrapObservation):Promise<void>{
        // The authenticated explicit scope owns its native Index independently
        // of legacy workspace registrations. Re-enumerating those registrations
        // must not clear its runtime during another window's installer work.
        if(this.currentCatalogScope())return;
        if(this.refreshing?.generation===this.generation)return this.refreshing.promise;
        const owner={generation:this.generation,promise:Promise.resolve()};
        owner.promise=this.refreshOnce(observation).finally(()=>{if(this.refreshing===owner)this.refreshing=undefined;});
        this.refreshing=owner;return owner.promise;
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
        const clear=()=>{this.specs=[];this.roots=[];this.binding=undefined;this.selectedEligible=undefined;this.selectedScope=undefined;};
        try{
            this.trusted();const runtime=this.runtime();
            if(!observation){
                try{observation=await runtime.discoverForConnection();}
                catch(error){
                    // Stop forbids early eligibility/election, but does not erase
                    // the complete saved workspace MCP declaration.
                    if(error instanceof ServiceError&&error.code==='service_explicitly_stopped')observation=await runtime.discover();
                    else throw error;
                }
            }
            this.trusted();
            if(generation!==this.generation)throw new ServiceError('workspace_registration_changed');
            if(!observation.inspection||observation.inspection.payload_verified===false||observation.inspection.metadata_only===true){clear();this.status.text='AIRK: setup';return;}
            const binding=this.runtime().binding(observation.inspection,observation.installDir);
            if(binding.version!==this.runningVersion)throw new ServiceError('explicit_runtime_upgrade_or_matching_extension_required');
            const lifecycle=record(observation.inspection.lifecycle??{}),recovery=record(observation.inspection.connection_observation??{});
            try{
                if(runtime.options.managed||lifecycle.explicitly_stopped===true||lifecycle.pending||recovery.activation_pending===true
                    ||recovery.launcher_repair_pending===true||observation.diagnosis?.pending_activation)this.retireSelectedEligibility();
                else await this.observeSelected(binding);
            }catch(error){
                if(generation===this.generation){this.retireSelectedEligibility();this.output.appendLine(`Selected project availability is unconfirmed: ${String(error)}`);}
            }
            this.trusted();if(generation!==this.generation)throw new ServiceError('workspace_registration_changed');
            const roots:{opened:string;canonical:string}[]=[];
            for(const folder of this.folders()){
                try{roots.push({opened:folder.uri.fsPath,canonical:await fs.realpath(folder.uri.fsPath)});}catch{/* Saved missing-source bindings remain readable. */}
            }
            const specs=await this.workspace.definitions(binding,this.folders().map(folder=>folder.uri.fsPath));this.trusted();
            if(generation!==this.generation||folderIdentity!==JSON.stringify(this.folders().map(folder=>folder.uri.fsPath)))throw new ServiceError('workspace_registration_changed');
            this.binding=binding;this.roots=roots;this.specs=specs;
            this.observedFolders=folderIdentity;
            this.status.text=`AIRK: ${this.specs.length} registered`;
        }catch(error){if(!this.disposed&&generation===this.generation){
            if(this.settingUp&&error instanceof ServiceError&&error.code==='explicit_runtime_upgrade_or_matching_extension_required'){
                // Automatic setup owns this expected transition and reports its
                // actual outcome. Never wait here: setup later refreshes itself.
                clear();this.status.text='AIRK: preparing runtime';return;
            }
            if(this.selectedEligible&&this.selectedFence(this.selectedEligible)){
                this.status.text='AIRK: selected project ready';this.output.appendLine(`Other workspace preparation needs attention: ${String(error)}`);
            }else{clear();this.status.text='AIRK: inspect service';this.output.appendLine(String(error));}
        }}
        finally{if(!this.disposed&&generation===this.generation){this.treeChanges.fire();if(previous!==JSON.stringify(this.specs.map(spec=>[spec.command,launchArgs(spec)])))this.definitions.fire();}}
    }
    async registerWorkspace():Promise<void>{
        this.trusted();await this.showManagement('#indexing/library');
        this.output.appendLine('Set up an index in Indexing → Index library. Choose source and index-storage locations, review the native draft, then apply explicitly.');
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
        const explicit=this.currentCatalogScope();if(explicit){if(requested&&!this.isCurrent(requested))throw new ServiceError('workspace_registration_changed');return explicit.spec;}
        if(this.explicitCatalogScope||this.hasScopeWithdrawal())throw new ServiceError('explicit_scope_selection_changed');
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
    /** A combined evidence selection never implicitly targets its display member
     * for management. Keep native metadata and the explicit picker fenced. */
    private async managementProject(spec:LaunchSpec):Promise<string|undefined>{
        if(!spec.combinedSession)return spec.service?.project;
        const selected=this.currentCatalogScope(),context=this.currentWindowContext(),generation=this.generation,controller=this.console.controller,identity=controller.observationIdentity;
        if(!selected||!context||selected.spec!==spec||!('indexes' in selected.selection))throw new ServiceError('explicit_scope_selection_changed');
        const current=()=>{this.trusted();if(generation!==this.generation||selected!==this.currentCatalogScope()||context!==this.currentWindowContext()||identity!==controller.observationIdentity)throw new ServiceError('explicit_scope_selection_changed');};
        current();const indexes=selected.selection.indexes;
        const chosen=indexes.length===1?indexes[0]:(await vscode.window.showQuickPick(indexes.map(index=>({label:index.name,description:index.id,index})),{title:'Choose the index to manage'}))?.index;
        current();if(!chosen)return;
        const descriptor=validateNativeHostContext(context.response.descriptor),response=await controller.catalogRead({catalog_detail:{entity:{kind:'index',id:chosen.id},expected_catalog_revision:descriptor.catalog_revision}});current();
        const detail=catalogObject(response.detail),item=catalogObject(detail.item),index=catalogObject(item.value),backing=catalogObject(index.backing);
        if(detail.namespace!==descriptor.namespace||detail.revision!==descriptor.catalog_revision||item.kind!=='index'||index.id!==chosen.id||index.tombstone!==false||index.source_import!=='complete'
            ||backing.binding!==chosen.backing.binding||backing.root!==chosen.backing.root||backing.data_root!==chosen.backing.data_root||backing.collection!==chosen.backing.collection)throw new ServiceError('crossed_native_index_administration');
        return catalogHex(backing.binding,64);
    }
    async resolveAgentSpec(spec:LaunchSpec):Promise<LaunchSpec>{
        this.trusted();
        if(!this.binding||!this.isCurrent(spec))throw new ServiceError('workspace_registration_changed');
        if(spec.catalogSession||spec.combinedSession)return spec;
        const binding=this.binding,generation=this.generation,client=this.runtime().client(binding);
        try{
            const selected=await this.workspace.resolveConnection(binding,spec,async(after,revision)=>{
                const page=await client.projects(after,revision,32);this.trusted();
                if(generation!==this.generation||this.binding!==binding||!this.isCurrent(spec))throw new ServiceError('workspace_registration_changed');
                return page;
            });
            this.trusted();
            if(generation!==this.generation||this.binding!==binding||!this.isCurrent(spec))throw new ServiceError('workspace_registration_changed');
            // Saved registration metadata does not establish a legacy read grant.
            // New library indexes require explicit Use Grant before agent export.
            const controller=this.console.controller;
            if(!controller.state.connected){await controller.whenIdle();this.trusted();if(!controller.state.connected)await controller.refresh();this.trusted();if(!controller.state.connected)throw new ServiceError('catalog_control_connection_unavailable');}
            const identity=controller.observationIdentity,response=await controller.catalogRead({catalog_legacy_binding:{project:selected.service!.project,expected_catalog_revision:null}});
            this.trusted();if(generation!==this.generation||this.binding!==binding||controller!==this.console.controller||identity!==controller.observationIdentity||!this.isCurrent(spec))throw new ServiceError('workspace_registration_changed');
            const eligibility=catalogObject(catalogObject(response.observation).eligibility);
            if(eligibility.kind==='unavailable')throw new ServiceError('legacy_connection_unavailable_use_reviewed_grant',false,response);
            const registration=catalogObject(eligibility.registration);
            if(registration.binding!==selected.service!.project||typeof registration.root!=='string'||typeof registration.data_root!=='string'||!same(registration.root,selected.root)||!same(registration.data_root,selected.dataDir)||registration.collection!==selected.collection)throw new ServiceError('crossed_native_legacy_registration');
            this.specs=this.specs.map(current=>current.service?.project===spec.service?.project?selected:current);
            this.definitions.fire();return selected;
        }finally{client.dispose();}
    }
    async index(requested?:LaunchSpec):Promise<void>{
        // An explicit native selection already owns its exact Index. A broad
        // legacy-workspace refresh can encounter another window's launcher lock
        // and must not replace that choice with the new-index setup guide.
        if(!this.currentCatalogScope()){
            if(this.explicitCatalogScope){
                const saved=this.windowContextValue;
                this.output.appendLine('Selected Index authority is unavailable: '+JSON.stringify({
                    context:!!this.currentWindowContext(),binding:!!this.binding,
                    generation:saved?.generation===this.generation,window:saved?.identity===this.windowIdentity().identity,
                    controller:saved?.controller===this.console.controller,connection:saved?.controlIdentity===this.console.controller.observationIdentity,
                    trusted:vscode.workspace.isTrusted,nativeExecution:nativeExecutionAllowed(),extensionUpdate:!!this.extensionUpdate,
                }));
                throw new ServiceError('explicit_scope_selection_changed');
            }
            await this.refresh();
        }
        if(requested===undefined&&!this.uiSpecs().length){
            if(this.explicitCatalogScope||this.hasScopeWithdrawal())throw new ServiceError('explicit_scope_selection_changed');
            await this.registerWorkspace();return;
        }
        const spec=await this.chooseSpec(requested);if(!spec)return;this.trusted();const project=await this.managementProject(spec);if(!project)return;
        const generation=this.generation,binding=await this.runtime().ensure();this.trusted();
        if(generation!==this.generation||!this.isCurrent(spec))throw new ServiceError('workspace_registration_changed');
        await this.enqueue(binding,project,'index');
        await this.showManagement('#manage/activity');
    }
    async watching():Promise<void>{
        const spec=await this.chooseSpec();if(!spec)return;
        const selected=await vscode.window.showQuickPick([{label:'Automatic; continues after windows close',enabled:true},{label:'Manual',enabled:false}],{title:'Native source watching'});
        if(!selected)return;this.trusted();const project=await this.managementProject(spec);if(!project)return;const binding=await this.runtime().ensure(),client=this.runtime().client(binding);
        try{this.trusted();await client.rpc({configure_watch:{project,enabled:selected.enabled}});this.trusted();}finally{client.dispose();}
        await this.showManagement('#manage/indexing');
    }
    async repair(operation:'rebuild'|'auto'|'verify',requested?:LaunchSpec):Promise<void>{
        const spec=await this.chooseSpec(requested);if(!spec)return;this.trusted();const project=await this.managementProject(spec);if(!project)return;await this.runtime().ensure();this.trusted();
        await this.console.previewRepair(project,operation);await this.showManagement('#manage/repair');
    }
    async pause(paused:boolean,requested?:LaunchSpec):Promise<void>{
        const spec=await this.chooseSpec(requested);if(!spec)return;this.trusted();const project=await this.managementProject(spec);if(!project)return;const binding=await this.runtime().ensure(),client=this.runtime().client(binding);
        try{this.trusted();await client.rpc({pause:{project,paused}});this.trusted();}finally{client.dispose();}
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
            const result=await probeMcp(selected,UNIFIED_TOOLS.length,this.runningVersion,this.connectionEntries(selected)[0],
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
        const early=this.refresh(); // owned refresh may publish UI while full diagnostics continue
        try{await this.automaticInstall();if(!this.binding)await early;
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
        await openUsagePanel(spec,valid,this.usageWriter(spec,valid),undefined,this.usageAccess(spec,valid));
    }
    private usageWriter(spec:LaunchSpec,valid:()=>boolean):(args:string[],continuing?:()=>boolean)=>Promise<unknown>{
        return async(args,continuing)=>{
            this.trusted();if(!valid()||continuing?.()===false)throw new ServiceError('project_configuration_selection_changed');
            if(spec.catalogSession||spec.combinedSession)throw new ServiceError('explicit_index_administration_required');
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
    private usageAccess(spec:LaunchSpec,valid:()=>boolean):UsageAccess{
        return spec.catalogSession||spec.combinedSession?{openIndexAdministration:()=>this.openIndexAdministration(spec,valid)}:{};
    }
    private async openIndexAdministration(spec:LaunchSpec,valid:()=>boolean):Promise<void>{
        this.trusted();const selected=this.currentCatalogScope(),context=this.currentWindowContext(),binding=this.binding,controller=this.console.controller,generation=this.generation,controlIdentity=controller.observationIdentity;
        if(!selected||!context||!binding||(!spec.catalogSession&&!spec.combinedSession)||!this.isUiCurrent(spec))throw new ServiceError('explicit_scope_selection_changed');
        const descriptor=validateNativeHostContext(context.response.descriptor);
        const current=()=>{this.trusted();if(!valid()||generation!==this.generation||binding!==this.binding||context!==this.currentWindowContext()||selected!==this.currentCatalogScope()||controlIdentity!==controller.observationIdentity)throw new ServiceError('stale_index_administration_selection');};
        current();
        const chosen='indexes' in selected.selection?await vscode.window.showQuickPick(selected.selection.indexes.map(index=>({label:index.name,description:index.id,index})),{title:'Choose an index for administration'}).then(value=>value?.index):selected.selection.index;
        current();if(!chosen)return;
        const response=await controller.catalogRead({catalog_detail:{entity:{kind:'index',id:chosen.id},expected_catalog_revision:descriptor.catalog_revision}});current();
        const detail=catalogObject(response.detail),item=catalogObject(detail.item),index=catalogObject(item.value),backing=catalogObject(index.backing);
        if(detail.namespace!==descriptor.namespace||detail.revision!==descriptor.catalog_revision||item.kind!=='index'||index.id!==chosen.id||index.tombstone!==false||index.source_import!=='complete'
            ||backing.binding!==chosen.backing.binding||backing.root!==chosen.backing.root||backing.data_root!==chosen.backing.data_root||backing.collection!==chosen.backing.collection)throw new ServiceError('crossed_native_index_administration');
        const command=binding.universalLauncher??binding.launcher;
        if(!command||binding.payloadVerified===false)throw new ServiceError('stable_launcher_unavailable');
        const administration:AdministrativeIndexSelection & {nativeObservation:{host_context:CatalogObject;index_detail:CatalogObject}}={purpose:'index_administration',label:ADMINISTRATIVE_USAGE_LABEL,namespace:descriptor.namespace,catalogRevision:descriptor.catalog_revision,indexId:String(index.id),backing:catalogHex(backing.binding,64),
            runtime:{installDir:binding.installDir,controlDir:binding.controlDir,runtimeSha256:binding.runtimeSha256},nativeObservation:{host_context:context.response,index_detail:response}};
        const administrativeSpec:LaunchSpec={command,root:chosen.backing.root,dataDir:chosen.backing.data_root,collection:chosen.backing.collection,
            name:`${chosen.name} · Entire Index administration`,service:{installDir:binding.installDir,project:administration.backing,...(binding.universalLauncher?{universal:true}:{})}};
        const authorized=()=>{try{current();return true;}catch{return false;}};current();
        await openUsagePanel(administrativeSpec,authorized,this.usageWriter(administrativeSpec,authorized),undefined,{administration});
    }
    private async embeddedUsage(spec:LaunchSpec,current:()=>boolean):Promise<import('./explorerController').EmbeddedUsage>{
        this.trusted();if(!current()||!this.isUiCurrent(spec))throw new ServiceError('project_configuration_selection_changed');
        await this.runtime().ensure();this.trusted();
        const generation=this.generation,valid=()=>generation===this.generation&&this.isUiCurrent(spec)&&current();
        let value:object={},receive:(message:unknown)=>Promise<void>=async()=>undefined,dispose=()=>{};
        await openUsagePanel(spec,valid,this.usageWriter(spec,valid),{
            render:next=>{value=next;},onMessage:listener=>{receive=listener;return {dispose(){receive=async()=>{throw new ServiceError('usage_session_disposed');};}};},onDispose:listener=>{dispose=listener;},
        },this.usageAccess(spec,valid));
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
            `AIRK ${this.runningVersion} ready. This window is running the updated interface. Existing native work remains in Management; new indexes require explicit setup in Indexing.`;
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
        const detail=[`Installs the AIRK ${this.runningVersion} runtime for this user account. The saved storage preference is ${storage}; each new index requires an explicit storage choice.`,
            'Open folders are not registered or indexed automatically. Set up each new index with explicit source and storage locations in Indexing → Index library.',
            'Indexing continues after editor windows close; login startup is a separate opt-in.',shell,
            'Existing recognized AIRK agent entries in these projects may be updated.',
            'Hidden files can be indexed: review .airkignore first. AIRK is not a secret scanner.'].join('\n');
        const choice=await vscode.window.showInformationMessage('Set up AIRK on this computer?',{modal:true,detail},'Set up','Customize…','Not now',"Don't set up automatically");
        if(choice==="Don't set up automatically"){await vscode.workspace.getConfiguration('airk').update('cli.autoInstall',false,vscode.ConfigurationTarget.Global);return false;}
        if(choice==='Not now'){await this.context.globalState.update(key,{decision:'deferred',at:Date.now()});return this.firstUseReview(installed,false);}
        if(choice==='Customize…'){
            const items:vscode.QuickPickItem[]=[{label:'Terminal command in external shells',description:process.platform==='win32'?'User PATH':'Shell startup files',picked:scope!=='off'}];
            const picked=await vscode.window.showQuickPick(items,{canPickMany:true,title:'Choose what AIRK sets up'});
            if(!picked)return false;
            if(!picked.length&&scope!=='off')await vscode.workspace.getConfiguration('airk').update('cli.shellIntegration','off',vscode.ConfigurationTarget.Global);
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
    /** Read-only inventory first; logs/caches are purged only after a reviewed plan digest,
     * and project indexes go through Management's own unregister confirmation. */
    async reviewData(run=nativeRunner):Promise<void>{
        this.trusted();const binding=await this.runtime().ensure();this.trusted();
        const main=(await this.runtime().discover()).bundle.main;this.trusted();
        const report=await run(main,['runtime','inventory','--install-dir',binding.installDir],120000);this.trusted();
        const size=(bytes:unknown)=>typeof bytes==='number'?`${(bytes/1048576).toFixed(1)} MiB`:'size unknown';
        const items:(vscode.QuickPickItem&{location?:string;project?:boolean})[]=[];
        for(const raw of Array.isArray(report.locations)?report.locations:[]){
            const location=record(raw);if(location.exists!==true)continue;
            items.push({label:String(location.id),description:String(location.path),detail:`${size(location.bytes)}${location.complete===false?' (partial count)':''} · ${String(location.consequence)}`,
                ...(location.purge==='purgeable'?{location:String(location.id)}:{}),picked:false});
        }
        for(const raw of Array.isArray(report.projects)?report.projects:[]){
            const project=record(raw);
            items.push({label:`Project index: ${String(project.collection)}`,description:String(project.root),detail:`Index at ${String(project.data_root)} · removal opens Management for its own confirmation${typeof project.storage_warning==='string'?' · ⚠ other accounts may read this storage':''}`,project:true,picked:false});
        }
        for(const line of [`AIRK data inventory${report.incomplete===true?' (incomplete)':''}:`,...items.map(item=>`  ${item.label}: ${item.description} — ${item.detail}`),
            ...(Array.isArray(report.not_covered)?report.not_covered.map(value=>`  Not covered: ${String(value)}`):[])])this.output.appendLine(line);
        const picked=await vscode.window.showQuickPick(items,{canPickMany:true,title:'Review AIRK data (nothing is removed until you confirm)'});this.trusted();
        if(!picked?.length)return;
        if(picked.some(item=>item.project))await this.showManagement('#manage/projects');
        const select=picked.flatMap(item=>item.location?[item.location]:[]);
        const refused=picked.filter(item=>!item.location&&!item.project).map(item=>item.label);
        if(refused.length)this.output.appendLine(`Not removable here (see consequences above): ${refused.join(', ')}`);
        if(!select.length)return;
        const plan=await run(main,['runtime','purge','--install-dir',binding.installDir,...select.flatMap(id=>['--select',id])],120000);this.trusted();
        const detail=(Array.isArray(plan.plan)?plan.plan:[]).map(raw=>{const item=record(raw);return `${String(item.id)}: ${String(item.path)} (${size(item.bytes)})\n${String(item.consequence)}`;}).join('\n\n');
        if(await vscode.window.showWarningMessage('Delete the contents of these AIRK locations?',{modal:true,detail:`${detail}\n\nThe AIRK service must be stopped. This is logical deletion, not secure erasure.`},'Delete')!=='Delete')return;
        this.trusted();
        const result=await run(main,['runtime','purge','--install-dir',binding.installDir,...select.flatMap(id=>['--select',id]),'--confirm',String(plan.plan_digest)],300000);
        for(const raw of Array.isArray(result.purged)?result.purged:[]){const item=record(raw);this.output.appendLine(`Deleted ${String(item.files_deleted)} files from ${String(item.path)}`);}
        for(const line of Array.isArray(result.retained)?result.retained:[])this.output.appendLine(`Retained: ${String(line)}`);
        this.output.show(true);
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
                    await this.prepareWorkspaceProjects(binding);
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
            await this.prepareWorkspaceProjects(binding);await this.refresh(completed);
            this.reportInstall(this.installationWarnings?'Runtime installation finished with issues to review.':'Runtime ready. Set up new indexes explicitly in Indexing; existing native work remains in Management.');
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
    private async prepareWorkspaceProjects(binding:RuntimeBinding):Promise<void>{
        this.trusted();
        const generation=this.generation,host=this.runtime();
        try{await this.observeSelected(binding);}
        catch(error){
            this.trusted();
            if(generation!==this.generation||host!==this.host||error instanceof ServiceError&&error.code==='workspace_registration_changed')throw error;
            this.retireSelectedEligibility();
            this.output.appendLine(`Selected project availability is unconfirmed: ${String(error)}`);
        }
        this.trusted();
        // Full workspace MCP definitions still require the complete console snapshot.
        // Selected UI preparation precedes this complete observation and may be unavailable.
        const client=host.client(binding);
        let observed:Awaited<ReturnType<typeof readWorkspaceRegistrations>>;
        try{observed=await readWorkspaceRegistrations(binding,async(after,revision,inactive)=>{
            const page=await client.projects(after,revision,32,inactive);this.trusted();
            if(generation!==this.generation||host!==this.host)throw new ServiceError('workspace_registration_changed');return page;
        });}finally{client.dispose();}

        const folders=this.folders();this.trusted();
        if(generation!==this.generation)throw new ServiceError('workspace_registration_changed');
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
            this.output.appendLine(state==='removed'?`Index remains removed: ${folder.uri.fsPath}. Use the explicit native setup review in Indexing → Index library to set up an index again.`:
                `No native index is registered for ${folder.uri.fsPath}. Open Indexing → Index library to choose source and storage locations and review setup. Opening folders does not create indexes.`);
        }
        this.reportInstall('Checking saved agent connections…');
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
        if(!vscode.workspace.isTrusted||!this.registrationEnabled()||!nativeExecutionAllowed())return [];
        if(!this.currentBinding())await this.refresh();if(!vscode.workspace.isTrusted||!this.registrationEnabled())return [];
        if(await this.hasManualVscodeConnection())return [];
        const observation=await this.workspaceConnection();if(!observation)return [];
        this.trusted();if(observation.generation!==this.generation||observation.identity!==this.console.controller.observationIdentity||!this.registrationEnabled())throw new ServiceError('workspace_registration_changed');const entry=observation.entry;
        const definition=new vscode.McpStdioServerDefinition('AIRK',entry.command,[...entry.args],
            {AIRK_CLIENT:'vscode',AIRK_MCP_RESPONSE_MODE:this.responseMode()},`${this.runningVersion}:${JSON.stringify(entry.args)}:${this.responseMode()}`);
        definition.cwd=vscode.Uri.file(path.dirname(entry.command));return [definition];
    }
    async resolveDefinition(definition:vscode.McpServerDefinition):Promise<vscode.McpServerDefinition>{
        this.trusted();if(!this.registrationEnabled())throw new ServiceError('mcp_registration_disabled');
        await this.refresh();this.trusted();
        if(await this.hasManualVscodeConnection())throw new ServiceError('manual_mcp_configuration_owns_connection');
        const observation=await this.workspaceConnection(),entry=observation?.entry;
        this.trusted();if(observation&&(observation.generation!==this.generation||observation.identity!==this.console.controller.observationIdentity))throw new ServiceError('workspace_registration_changed');
        if(!(definition instanceof vscode.McpStdioServerDefinition)||!entry||entry.command!==definition.command||JSON.stringify(entry.args)!==JSON.stringify(definition.args))throw new ServiceError('workspace_registration_changed');
        const selected=this.currentCatalogScope();
        await this.runtime().ensure();this.trusted();if(!this.registrationEnabled())throw new ServiceError('mcp_registration_disabled');
        if(observation.generation!==this.generation||observation.identity!==this.console.controller.observationIdentity||selected!==this.currentCatalogScope())throw new ServiceError('workspace_registration_changed');
        return definition;
    }
    getTreeItem(spec:LaunchSpec):vscode.TreeItem {
        const item=new vscode.TreeItem(spec.name);item.id=spec.service!.project;item.description=spec.collection;item.tooltip=spec.root;item.contextValue='airkServiceProject';
        item.command={command:'airk.explorer.open',title:'Open in AIRK Explorer',arguments:['#manage/projects']};return item;
    }
    getChildren():LaunchSpec[]{return this.specs;}
    /** Refresh the shared console observations, then reveal a Management tab in the Explorer. */
    async showManagement(route:string):Promise<void>{
        (await this.explorerController()).open(route);
        void this.console.run('refresh').catch(error=>{if(!this.disposed)this.output.appendLine(String(error));});
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
        if(this.disposed||!vscode.workspace.isTrusted||this.checkForExtensionUpdate()||(!this.currentCatalogScope()&&!this.specs.length)||!this.workbench||this.context.globalState.get<boolean>(key))return;
        try{
            const generation=this.generation,scope=this.currentCatalogScope();
            // A saved/native registration alone also describes new ungranted indexes.
            // Initial legacy opening requires the same positive sealed-compatibility
            // observation as automatic MCP definitions; reviewed scope use is explicit.
            const legacy=scope?undefined:await this.workspaceConnection();
            const current=()=>!this.disposed&&vscode.workspace.isTrusted&&generation===this.generation&&!this.checkForExtensionUpdate()
                &&(scope?scope===this.currentCatalogScope():!!legacy&&legacy.generation===this.generation&&legacy.identity===this.console.controller.observationIdentity&&!this.currentCatalogScope()&&!this.explicitCatalogScope&&!this.hasScopeWithdrawal());
            if(!current())return;
            await this.workbench.focus();
            if(current())await this.context.globalState.update(key,true);
        }catch(error){this.output.appendLine(`Workbench could not open after setup: ${String(error)}. Use AIRK: Open Workbench.`);}
    }

    explorerHost():ExplorerHost{
        return {extensionUri:this.context.extensionUri,version:this.runningVersion,
            extensionUpdate:()=>this.extensionUpdate,reloadUpdatedExtension:()=>this.reloadUpdatedExtension(),
            connectionCheck:()=>this.connectionCheck,startup:()=>this.startup,
            workbenchVisible:()=>this.workbench?.visible??false,toggleWorkbench:async()=>{await this.workbench?.toggle();},
            onDidChangeWorkbenchVisibility:this.workbench?.onDidChangeVisibility,
            trusted:()=>vscode.workspace.isTrusted&&!this.disposed&&!this.extensionUpdate,workspaceTrusted:()=>vscode.workspace.isTrusted,nativeExecutionEnabled:()=>nativeExecutionAllowed(),specs:()=>this.uiSpecs(),
            windowContext:()=>this.currentWindowContext()?.descriptor,windowContextObservation:()=>this.currentWindowContext()?.response,
            prepareWindowContext:()=>this.prepareWindowContext(),onDidChangeWindowContext:this.windowContextChanges.event,
            useGrant:(choice,namespace,revision,valid)=>this.useGrant(choice,namespace,revision,valid),
            prefillCombinedSearch:async(search:SearchDraft)=>{
                this.trusted();const selected=this.currentCatalogScope(),spec=selected?.spec,session=spec?.combinedSession;
                const choice=search as SearchDraft&{project_ids?:string[];index_ids?:string[]};
                if(!selected||!spec||!session||!choice.project_ids||!this.workbench
                    ||JSON.stringify([...choice.project_ids].sort())!==JSON.stringify(session.project_ids)
                    ||JSON.stringify(choice.index_ids?[...choice.index_ids].sort():null)!==JSON.stringify(session.index_ids??null))throw new ServiceError('combined_draft_scope_changed');
                await this.workbench.prefillSearchForm(spec,search);
                this.trusted();if(this.currentCatalogScope()?.spec!==spec)throw new ServiceError('combined_draft_scope_changed');
            },
            prefillRecommendedSearch:async(search:SearchDraft)=>{
                this.trusted();const spec=this.currentCatalogScope()?.spec;
                if(!spec?.combinedSession||!this.workbench)throw new ServiceError('combined_draft_scope_changed');
                await this.workbench.prefillSearchForm(spec,search,true);
                this.trusted();if(this.currentCatalogScope()?.spec!==spec)throw new ServiceError('combined_draft_scope_changed');
            },
            activeProject:()=>{
                const active=vscode.window.activeTextEditor&&vscode.workspace.getWorkspaceFolder(vscode.window.activeTextEditor.document.uri);
                const root=active&&this.roots.find(root=>same(root.opened,active.uri.fsPath));
                return root?this.uiSpecs().find(spec=>same(spec.root,root.canonical))?.service?.project:this.selectedEligible&&this.selectedFence(this.selectedEligible)?this.selectedEligible.spec.service?.project:undefined;
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
        try{await writeLaunchDescriptor(storage,this.context.extensionPath,version,undefined,
            ()=>generation===this.assistanceGeneration&&this.assistanceEnabled());}
        catch(error){this.output.appendLine(`Explorer Assistance launch descriptor was not written: ${error instanceof Error?error.message:String(error)}`);}
        if(generation!==this.assistanceGeneration||!this.assistanceEnabled())return;
        try{
            const host=await AssistanceIpcHost.start({directory:assistanceDirectory(storage),version,roots:this.assistanceFolders().map(folder=>folder.uri.fsPath),
                authorized:()=>generation===this.assistanceGeneration&&this.assistanceEnabled(),
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
    private async workspaceConnection():Promise<{entry:ServerEntry;identity:string;generation:number}|undefined>{
        const selected=this.currentCatalogScope();
        if(selected)return {entry:unifiedEntry(selected.spec,this.explorerDescriptor()),identity:this.console.controller.observationIdentity,generation:this.generation};
        if(this.explicitCatalogScope||this.hasScopeWithdrawal())return undefined;
        const specs=[...this.specs],binding=this.binding,generation=this.generation,controller=this.console.controller;
        if(!binding||!specs.length)return undefined;
        if(specs.length>32)throw new ServiceError('legacy_mcp_binding_capacity_exceeded');
        const current=()=>{this.trusted();if(binding!==this.binding||generation!==this.generation||controller!==this.console.controller||this.explicitCatalogScope||this.hasScopeWithdrawal()||JSON.stringify(specs)!==JSON.stringify(this.specs))throw new ServiceError('workspace_registration_changed');};
        current();if(!controller.state.connected){await controller.whenIdle();current();if(!controller.state.connected)await controller.refresh();current();if(!controller.state.connected)throw new ServiceError('catalog_control_connection_unavailable');}
        const controlIdentity=controller.observationIdentity,resolved:LaunchSpec[]=[];let namespace:string|undefined,revision:number|null|undefined;
        for(const spec of specs){
            if(!spec.service)throw new ServiceError('native_project_binding_required');
            const response=await controller.catalogRead({catalog_legacy_binding:{project:spec.service.project,expected_catalog_revision:revision??null}});current();
            if(controlIdentity!==controller.observationIdentity)throw new ServiceError('stale_catalog_client');
            const observed=catalogObject(response.observation),eligibility=catalogObject(observed.eligibility);
            if(namespace!==undefined&&(observed.namespace!==namespace||observed.catalog_revision!==revision))throw new ServiceError('catalog_revision_changed_refresh');
            namespace=String(observed.namespace);revision=observed.catalog_revision===null?null:catalogRevision(observed.catalog_revision);
            if(eligibility.kind==='unavailable')return undefined;
            const registration=catalogObject(eligibility.registration);
            // Match the existing workspace adapter's Windows namespace/casing aliases,
            // then retain the complete native spelling for the actual connection.
            if(registration.binding!==spec.service.project||typeof registration.root!=='string'||typeof registration.data_root!=='string'||!path.isAbsolute(registration.root)||!path.isAbsolute(registration.data_root)
                ||!same(registration.root,spec.root)||!same(registration.data_root,spec.dataDir)||registration.collection!==spec.collection)throw new ServiceError('crossed_native_legacy_registration');
            resolved.push({...spec,root:registration.root,dataDir:registration.data_root});
        }
        current();const [first,...additional]=resolved;return {entry:unifiedEntry(first,this.explorerDescriptor(),additional),identity:controlIdentity,generation};
    }
    async verifyAssistance(spec:LaunchSpec):Promise<string[]>{
        this.trusted();
        const entry=this.connectionEntries(spec)[0];
        await writeLaunchDescriptor(this.context.globalStorageUri.fsPath,this.context.extensionPath,this.runningVersion,undefined,()=>this.assistanceEnabled());
        const result=await probeMcp(spec,UNIFIED_TOOLS.length,this.runningVersion,entry);this.trusted();
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
    reset():void{this.generation++;this.windowContextValue=undefined;this.publishNativeCommandAvailability();this.host=undefined;this.selectedEligible=undefined;this.selectedScope=undefined;this.specs=[];this.binding=undefined;this.checks.clear();this.checking=undefined;this.connectionCheck=undefined;this.console.reset();void this.refresh();}
    dispose():void{if(this.disposed)return;this.disposed=true;this.generation++;this.windowContextValue=undefined;this.publishNativeCommandAvailability();this.windowContextChanges.dispose();this.assistanceGeneration++;this.assistanceHost?.dispose();this.assistanceDefinitions.dispose();void this.explorer?.then(explorer=>explorer.dispose(),()=>undefined);this.console.dispose();this.status.dispose();this.definitions.dispose();this.treeChanges.dispose();this.output.dispose();}
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
    context.subscriptions.push(workbench,vscode.window.registerWebviewViewProvider('airk.workbench',workbench,{webviewOptions:{retainContextWhenHidden:true}}));
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
        if(error instanceof ServiceError&&error.code==='workspace_untrusted'){
            void vscode.window.showWarningMessage('AIRK native work is unavailable in VS Code Restricted Mode.','Manage Workspace Trust').then(action=>action==='Manage Workspace Trust'?controller.manageWorkspaceTrust():undefined).then(undefined,()=>{});
            throw error;
        }
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
    commands.chooseStorage=()=>controller.registerWorkspace();
    commands.installCli=()=>controller.repairInstallation();
    commands.enableNativeExecution=()=>controller.enableNativeExecution();
    commands.manageWorkspaceTrust=(...args)=>{if(args.length)throw new ServiceError('invalid_manage_workspace_trust_arguments');return controller.manageWorkspaceTrust();};
    commands.setUp=()=>controller.repairInstallation();
    commands.removeTerminalCommand=()=>controller.removeTerminalCommand();
    commands.showLastUninstallResult=()=>controller.showLastUninstallResult();
    commands.reviewData=()=>controller.reviewData();
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
    const nativeCommands=new Set(NATIVE_COMMANDS);
    for(const [name,action] of Object.entries(commands))context.subscriptions.push(vscode.commands.registerCommand(`airk.${name}`,safe((...args)=>{if(nativeCommands.has(name))controller.trusted();return action(...args);} )));
    if(vscode.lm?.registerMcpServerDefinitionProvider)context.subscriptions.push(vscode.lm.registerMcpServerDefinitionProvider('airk.collections',{
        onDidChangeMcpServerDefinitions:controller.definitions.event,provideMcpServerDefinitions:()=>controller.provideDefinitions(),resolveMcpServerDefinition:value=>controller.resolveDefinition(value)}));
    if(vscode.lm?.registerMcpServerDefinitionProvider)context.subscriptions.push(vscode.lm.registerMcpServerDefinitionProvider('airk.explorerAssistance',{
        onDidChangeMcpServerDefinitions:controller.assistanceDefinitions.event,provideMcpServerDefinitions:()=>controller.provideAssistanceDefinitions(),
        resolveMcpServerDefinition:value=>controller.resolveAssistanceDefinition(value)}));
    const assistanceChanged=()=>{void controller.startAssistance();controller.assistanceDefinitions.fire();controller.definitions.fire();};
    context.subscriptions.push(vscode.workspace.onDidChangeWorkspaceFolders(()=>{assistanceChanged();return controller.automaticInstall().then(()=>controller.refresh());}),
        vscode.workspace.onDidGrantWorkspaceTrust(()=>{controller.reset();assistanceChanged();void controller.automaticInstall().then(()=>controller.refresh());}),
        vscode.workspace.onDidChangeConfiguration(event=>{
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
