// SPDX-License-Identifier: Apache-2.0
//! Service-backed extension activation. Native ownership outlives editor windows.
import * as vscode from 'vscode';
import * as fs from 'node:fs/promises';
import { readdirSync, statSync, watch, type FSWatcher } from 'node:fs';
import { randomBytes, createHash } from 'node:crypto';
import { isDeepStrictEqual } from 'node:util';
import { HttpSelection, HttpConnectionInfo, validateHttpConnection, preservedImportedHttpServerKey, matchesImportedHttpOrigin, sameHttpAuthority, credentialEnvironment, httpMaterial, httpPublicTrustFile, publishNativeInput, removeNativeInput } from './serviceHttp';
import * as path from 'node:path';
import * as os from 'node:os';
import {inventoryGlobalAgentConfigs,globalConfigDeclarations,transformGlobalContainer,type GlobalAgent,type GlobalConfigFile} from './globalAgentInventory';
import { setupAgent, AgentSetupHost } from './agentSetup';
import { launchArgs, LaunchSpec, McpVerification, unifiedEntry, ServerEntry, UNIFIED_TOOLS } from './model';
import { probeMcp, runText } from './native';
import { LegacyRetirementSession, retirementJson, retirementMarkdown } from './runtimeLegacyRetirement';
import { automaticUpgrade, BootstrapObservation, nativeRunner, RuntimeBinding, RuntimeHost, legacyOriginLocatorIdentity, type LegacyOriginLocator } from './serviceBootstrap';
import { ConsoleRuntime, ServiceConsole, serviceRuntimeOptions } from './serviceConsole';
import { identifier, record, ServiceError } from './serviceClient';
import { IntentJournal } from './serviceIntents';
import { nativeScopeLaunchSpec, readWorkspaceRegistrations, ServiceWorkspace } from './serviceWorkspace';
import { installLauncherCli, shellIntegration, ShellTouch } from './cliInstall';
import { defaultProfiles, removeShellIntegration } from './cliRemove';
import { openUsagePanel, type AdministrativeIndexSelection, type UsageAccess } from './usagePanel';
import { ADMINISTRATIVE_USAGE_LABEL, renderUsage } from './usageReport';
import { COMMAND_ROUTES, MANAGE_WORKSPACE_TRUST_COMMAND, NATIVE_COMMANDS } from './explorerCommands';
import { CatalogObject, WindowContextDescriptor, catalogBound, catalogId, catalogItems, catalogObject, catalogRevision, catalogHex, sameCatalogValue, validateNativeHostContext, validateWindowContext } from './indexingCatalog';
import { reviewScopeSelection, reviewCombinedScopeSelection, scopeSelectionPreference, type ReviewedWindowScopeSelection } from './scopeSelection';
import type { SearchDraft } from './explorerDrafts';
import { ExplorerSidebar } from './explorerSidebar';
import { WorkbenchView } from './workbenchView';
import type { ExplorerController, ExplorerHost } from './explorerController';
import { hasUnifiedVscodeConfig, migrateAgentConfig, migrateLocalAgents, locations as localAgentLocations, LocalAgent, httpConfigurationConnectionId, refreshHttpAgentConfig, migrateStandaloneHttpEntry } from './agentMigration';
import { readJsonConfig, vscodeCredentialConfiguration, vscodeManualConnectionIds, vscodeManualConfiguration } from './agentConfig';
import { AssistanceIpcHost } from './explorerAssistanceHost';
import {ClientTrustPublicationError,IssuerRotationStatus,publishOwnedIssuerBundle,updateOwnedIssuerBundles} from './httpIssuerRotation';
import { HandoffStore } from './explorerHandoffs';
import { SuggestionStore } from './explorerSuggestions';
import { assistanceDirectory, launchDescriptorPath, writeLaunchDescriptor } from './explorerAssistanceLaunch';
import { rememberShellIntegration, rememberUninstallTarget, shellProfiles } from './extensionUninstall';
import { recordDiagnosticFailure } from './diagnosticLogs';
import { nativeExecutionAllowed, NativeExecutionDisabled, setNativeExecutionGate } from './nativeGate';
import type {ServiceConsoleState} from './serviceConsoleView';
import type { RuntimeUpgradeProgress } from './explorerContracts';
import { applyLocalSearchScope, saveLocalIndexSet, localScopeChoice, managedScopeProject, type ManagedScopeProject, type LocalScopeAdapter } from './localSearchScope';

const SETUP_FAILED_STATUS='$(warning) AIRK: setup failed';

/** User-scope only: a workspace cannot turn native execution back on. */
function nativeExecutionSetting():boolean{
    return vscode.workspace.getConfiguration('airk').inspect<boolean>('runtime.nativeExecution')?.globalValue!==false;
}

/** Identity of the loaded extension code. VS Code keeps a same-version reinstall
 * in place, so the version alone cannot reveal that a rebuilt VSIX replaced it. */
function codeStamp(root:string|undefined,required:readonly string[]=['serviceExtension.js']):{stamp:string;files:string[]}|undefined{
    if(!root)return undefined;
    try{
        const directory=path.join(root,'out'),files=readdirSync(directory).filter(name=>/\.(?:js|css)$/.test(name)).sort(),resolved=path.resolve(root);
        // Installation can briefly remove files while replacing the payload. Wait
        // for every previously loaded asset before declaring a rebuilt install.
        if(required.some(name=>!files.includes(name)))return undefined;
        const metadata=files.map(name=>{const info=statSync(path.join(directory,name));if(!info.isFile())throw new Error('Incomplete extension payload');return [name,info.size,info.mtimeMs];});
        // VS Code's context and extension registry may spell the same Windows
        // path differently. Preserve file changes without treating casing as a rebuild.
        const identity=process.platform==='win32'?path.toNamespacedPath(resolved).toLowerCase():resolved;
        return {stamp:JSON.stringify([identity,metadata]),files};
    }catch{return undefined;}
}
function same(left:string,right:string):boolean {
    return path.relative(process.platform==='win32'?path.toNamespacedPath(left):left,process.platform==='win32'?path.toNamespacedPath(right):right)==='';
}
class ServiceExtension implements vscode.Disposable,AgentSetupHost,vscode.TreeDataProvider<LaunchSpec> {
    // Capture the loaded code's version; extension metadata can change after a VSIX install.
    private readonly runningVersion:string;
    private readonly codeIdentity:ReturnType<typeof codeStamp>;
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
    private legacyRetirement?:LegacyRetirementSession;
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
    private runtimeUpgrade?:RuntimeUpgradeProgress;
    private localScopeWorking=false;
    private localScopeEpoch=0;
    private localScopeLabelChoice?:{index_ids:string[];project_ids:string[]};
    private restoringLocalScope=false;
    private localScopeRestoreAttempt?:string;
    private refreshing?:{generation:number;promise:Promise<void>};
    private observedFolders?:string;
    private settingUp?:Promise<void>;
    private migratedConnections=false;
    private installationWarnings=false;
    private installationFailed=false;
    private migrationPredecessors:LaunchSpec[]=[];
    private legacyLauncherInspection?:Record<string,unknown>;
    private legacyCachePreparation?:Record<string,unknown>;
    private legacyMigrationFiles=new Map<string,{agent:LocalAgent;file:GlobalConfigFile}>();
    private migrationAbsentConfigurations=new Set<string>();
    private migratedConfigurationSnapshots=new Map<string,string>();
    private migrationHttpConnections=new Map<string,HttpConnectionInfo>();
    private migrationGlobalVerifier?:()=>Promise<void>;
    private readonly httpOwnershipLookups=new Map<string,Promise<Record<string,unknown>>>();
    private httpConfigurationRefresh?:Promise<void>;
    private observedHttpInstance?:string;
    private httpRefreshRetry?:{instance:string;attempt:number;next:number};
    private httpRefreshWarnedInstance?:string;
    private httpRefreshTimer?:ReturnType<typeof setTimeout>;
    private httpWatchKey?:string;
    private httpWatchParent?:FSWatcher;
    private httpWatchEndpoint?:FSWatcher;
    private httpWatchTimer?:ReturnType<typeof setTimeout>;
    private httpWatchRearm?:ReturnType<typeof setTimeout>;
    private httpWatchRead?:ReturnType<RuntimeHost['client']>;
    private httpWatchReading=false;
    private httpWatchFailures=0;
    private httpWatchHealthRetry?:ReturnType<typeof setTimeout>;
    private httpWatchPending=false;
    private latestHttpOwner?:{runtime:string;instance:string};
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
    private reportUpgrade(stage:RuntimeUpgradeProgress['stage'],message:string,fromVersion?:string):void{
        if(this.disposed)return;
        const now=Date.now(),prior=this.runtimeUpgrade;
        if(!prior&&!fromVersion)return;
        this.runtimeUpgrade={fromVersion:fromVersion??prior!.fromVersion,toVersion:this.runningVersion,
            startedAt:prior?.startedAt??now,updatedAt:now,stage,message};
        this.reportInstall(message);
        this.treeChanges.fire();
    }
    private setupSucceeded(binding:RuntimeBinding,generation:number):void{
        if(this.disposed||generation!==this.generation||!vscode.workspace.isTrusted||this.extensionUpdate||!nativeExecutionAllowed())return;
        if(!this.binding||this.binding.runtimeSha256!==binding.runtimeSha256||!same(this.binding.controlDir,binding.controlDir)){
            this.reportUpgrade('failed','Runtime activation finished, but the updated connection could not be confirmed. Review Management → Service and Logs.');
            return;
        }
        this.reportUpgrade('ready',`AIRK ${this.runningVersion} runtime upgrade complete. Saved indexes are connected; index contents and pending work are preserved.`);
        if(this.startup){this.startup=undefined;this.treeChanges.fire();}
    }
    /** Keep an unresolved setup failure visible when no usable runtime was observed. */
    private renderSetupFailed():boolean{
        if(!this.installationFailed)return false;
        if(this.renderUpdateStatus())return true;
        if(!nativeExecutionAllowed()){this.renderNativeDisabled();return true;}
        const lifecycle=this.console.controller.state.bootstrap.value?.lifecycle;
        if(lifecycle!==null&&typeof lifecycle==='object'&&'explicitly_stopped' in lifecycle&&lifecycle.explicitly_stopped===true
            ||this.selectedEligible&&this.selectedFence(this.selectedEligible))return false;
        this.status.text=SETUP_FAILED_STATUS;return true;
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
        // Packaged activation is owned by the stable loader, which replaces the
        // entire AIRK context and UI after verifying a complete installed payload.
        if(Reflect.get(this.context,Symbol.for('iokaio.airk.managedExtensionContext'))===true)return false;
        const extension=vscode.extensions?.getExtension(this.context.extension.id??'iokaio.airk');
        let installed=extension?.packageJSON.version;
        // A rebuilt VSIX with the same version replaces the code on disk without a version change.
        if(installed===this.runningVersion&&this.codeIdentity!==undefined){
            const current=codeStamp(extension?.extensionPath??this.context.extensionPath,this.codeIdentity.files);
            if(current!==undefined&&current.stamp!==this.codeIdentity.stamp)installed=`${installed} (rebuilt)`;
        }
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
        context.subscriptions.push(this.console.onDidChangeState(state=>this.observeHttpOwner(state)));
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
    /** Read-only administrative metadata. Getters never connect, register, grant or invoke native work. */
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
            // its context creates no grant. A saved local-user choice is restored
            // only after metadata revalidation of its exact existing grant.
            if(!this.binding||JSON.stringify(this.binding)!==JSON.stringify(binding)){
                this.specs=[];this.roots=[];this.retireSelectedEligibility();this.explicitCatalogScope=undefined;
                this.binding=binding;this.syncHttpEndpointWatcher();
            }
            this.observedFolders=JSON.stringify(this.folders().map(folder=>folder.uri.fsPath));
            const previous=this.currentWindowContext();
            // A repeated read of identical metadata does not change the owner
            // captured by an explicit grant review or management picker. Any
            // changed response field, runtime or window still replaces it.
            if(previous&&sameCatalogValue(previous.response,response)&&sameCatalogValue(previous.descriptor,descriptor))return;
            this.windowContextValue={identity:window.identity,generation,controller,controlIdentity,response,descriptor};
            this.windowContextChanges.fire();
            await this.restoreLocalSearchScope();
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
            if(value.schema===1&&value.withdrawn===true)return true;
            if(value.schema!==1||!Array.isArray(value.project_ids)||!value.project_ids.length||value.project_ids.length>32||new Set(value.project_ids).size!==value.project_ids.length)throw new Error('invalid_scope_withdrawal');
            catalogHex(value.namespace);catalogHex(value.host,64);catalogHex(value.context_key,64);catalogId(value.grant_id,'grt_');catalogRevision(value.expected_grant_revision);value.project_ids.forEach(id=>catalogId(id,'prj_'));
            if(value.index_id!==undefined)catalogId(value.index_id,'idx_');
            if(value.index_ids!==undefined){if(!Array.isArray(value.index_ids)||!value.index_ids.length||value.index_ids.length>32||new Set(value.index_ids).size!==value.index_ids.length)throw new Error('invalid_scope_withdrawal');value.index_ids.forEach(id=>catalogId(id,'idx_'));}
            return true;
        }catch{return true;} // Uncertain local storage can withdraw access, never enable a legacy fallback.
    }
    /** Explicit local choice or exact saved-grant restoration: native metadata revalidates the selector before exposure. */
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
        if(this.localScopeLabelChoice)selection.label=this.scopeLabel(this.localScopeLabelChoice);
        const spec=await this.enrollHttpSpec(nativeScopeLaunchSpec(binding,selection),'vscode');current();
        // Withdrawal survives reload; legacy preferences stay inert. The separate
        // local-user selection can restore only this exact native-revalidated grant.
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
    private managedScopeKey(native:ReturnType<typeof validateNativeHostContext>):string{return `airk.localScope.managed.1.${native.namespace}.${native.host}.${native.key}`;}
    private managedScope(fingerprint?:string):ManagedScopeProject|undefined{
        try{const context=this.currentWindowContext();if(!context)return undefined;
            const native=validateNativeHostContext(context.response.descriptor);
            const key=this.managedScopeKey(native)+(fingerprint?`.cache.${catalogHex(fingerprint,64)}`:'');
            return managedScopeProject(this.context.globalState.get(key),native);
        }catch{return undefined;}
    }
    private savedLocalScope():CatalogObject|undefined{
        try{const context=this.currentWindowContext();if(!context)return undefined;
            const native=validateNativeHostContext(context.response.descriptor),raw=this.context.workspaceState.get('airk.localScope.selection.1');
            if(raw===undefined)return undefined;
            const value=catalogObject(raw);catalogBound(value,8192);
            if(value.schema!==1||value.namespace!==native.namespace||value.host!==native.host||value.context_key!==native.key)return undefined;
            localScopeChoice(value.selection);return value;
        }catch{return undefined;}
    }
    searchScopeSelection():{index_ids:string[];project_ids:string[];managedProjectIds:string[];active:boolean}{
        const saved=this.savedLocalScope(),choice=saved?localScopeChoice(saved.selection):{index_ids:[],project_ids:[]},context=this.currentWindowContext();
        const managedProjectIds:string[]=[];
        if(context){const native=validateNativeHostContext(context.response.descriptor),prefix=`airk.localScope.managed.1.${native.namespace}.${native.host}.`;
            for(const key of this.context.globalState.keys().filter(key=>key.startsWith(prefix))){
                try{const row=catalogObject(this.context.globalState.get(key));if(row.schema===1&&row.namespace===native.namespace&&row.host===native.host)managedProjectIds.push(catalogId(row.id,'prj_'));}catch{/* Uncertain metadata never claims another set. */}
            }
        }
        const active=!!saved&&(saved.appliedEmpty===true&&!choice.index_ids.length&&!choice.project_ids.length&&this.hasScopeWithdrawal()||!!this.currentCatalogScope()&&!!saved.grant&&catalogObject(saved.grant).grant_id===this.currentCatalogScope()?.selection.session.grant_id);
        return {...choice,managedProjectIds:[...new Set(managedProjectIds)],active};
    }
    private scopeLabel(choice:{index_ids:string[];project_ids:string[]}):string{
        return [choice.index_ids.length?`${choice.index_ids.length} index${choice.index_ids.length===1?'':'es'}`:'',choice.project_ids.length?`${choice.project_ids.length} index set${choice.project_ids.length===1?'':'s'}`:''].filter(Boolean).join(' · ');
    }
    private async restoreLocalSearchScope():Promise<void>{
        if(this.localScopeWorking||this.restoringLocalScope)return;
        const saved=this.savedLocalScope(),context=this.currentWindowContext();if(!saved?.grant||!context)return;
        const native=validateNativeHostContext(context.response.descriptor),attempt=JSON.stringify([this.generation,this.console.controller.observationIdentity,native,saved.grant]);
        if(this.localScopeRestoreAttempt===attempt)return;this.localScopeRestoreAttempt=attempt;const epoch=this.localScopeEpoch;this.restoringLocalScope=true;this.localScopeLabelChoice=localScopeChoice(saved.selection);
        try{
            const choice=catalogObject(saved.grant),response=await this.console.controller.catalogRead({catalog_detail:{entity:{kind:'grant',id:choice.grant_id},expected_catalog_revision:native.catalog_revision}});
            const frozen=catalogItems(response,'detail')[0];if(frozen?.kind!=='grant')throw new Error('saved_scope_grant_unavailable');
            const grant=catalogObject(frozen.value.grant),members=grant.memberships;
            if(!Array.isArray(choice.project_ids)||!Array.isArray(members)||members.length>1024)throw new Error('saved_scope_grant_unavailable');
            for(const id of choice.project_ids){
                const project=await this.console.controller.catalogRead({catalog_detail:{entity:{kind:'project',id},expected_catalog_revision:native.catalog_revision}});
                const row=catalogItems(project,'detail')[0];if(row?.kind!=='project'||row.value.id!==id)throw new Error('saved_scope_set_unavailable');
            }
            for(const value of members){
                const member=catalogObject(catalogObject(value).membership);
                if(!choice.project_ids.includes(member.project_id)||Array.isArray(choice.index_ids)&&!choice.index_ids.includes(member.index_id))continue;
                const current=await this.console.controller.catalogRead({catalog_detail:{entity:{kind:'membership',project_id:member.project_id,index_id:member.index_id},expected_catalog_revision:native.catalog_revision}});
                const row=catalogItems(current,'detail')[0];if(row?.kind!=='membership'||!sameCatalogValue(row.value,member))throw new Error('saved_scope_membership_changed');
            }
            await this.useGrant(choice,native.namespace,native.catalog_revision,()=>!this.disposed&&!this.localScopeWorking&&epoch===this.localScopeEpoch);
            const active=this.currentCatalogScope();if(epoch===this.localScopeEpoch&&active)active.selection.label=this.scopeLabel(localScopeChoice(saved.selection));
        }catch(error){if(epoch===this.localScopeEpoch){this.explicitCatalogScope=undefined;this.checks.clear();this.connectionCheck=undefined;this.treeChanges.fire();this.definitions.fire();this.output.appendLine(`Saved search scope needs Apply selection: ${error instanceof Error?error.message:String(error)}`);}}
        finally{this.restoringLocalScope=false;if(epoch===this.localScopeEpoch)this.localScopeLabelChoice=undefined;}
    }
    private localScopeAdapter():LocalScopeAdapter{
        this.trusted();const controller=this.console.controller,binding=this.binding,generation=this.generation,window=this.windowIdentity().identity,identity=controller.observationIdentity;
        if(!binding||!this.currentWindowContext()||!controller.state.connected)throw new ServiceError('read_current_native_window_context');
        const current=()=>{this.trusted();if(this.disposed||controller!==this.console.controller||identity!==controller.observationIdentity||generation!==this.generation||binding!==this.binding||window!==this.windowIdentity().identity)throw new ServiceError('stale_search_scope_selection');};
        const context=()=>{current();const observed=this.currentWindowContext();if(!observed)throw new ServiceError('read_current_native_window_context');return catalogObject(observed.response.descriptor);};
        return {control:controller,current,context,managed:fingerprint=>this.managedScope(fingerprint),saveManaged:async(value,fingerprint)=>{current();const key=this.managedScopeKey(validateNativeHostContext(context()));await this.context.globalState.update(`${key}.${value.id}`,value);current();if(fingerprint){await this.context.globalState.update(`${key}.cache.${catalogHex(fingerprint,64)}`,value);current();}await this.context.globalState.update(key,value);current();},
            refreshContext:()=>this.prepareWindowContext(),use:(choice,namespace,revision)=>this.useGrant(choice,namespace,revision,()=>{try{current();return true;}catch{return false;}}),
            clear:async()=>{await this.context.workspaceState.update('airk.scope.explicitWithdrawal.1',{schema:1,withdrawn:true});current();this.explicitCatalogScope=undefined;this.checks.clear();this.connectionCheck=undefined;this.treeChanges.fire();this.definitions.fire();await this.workbench?.refresh();return {status:'search_scope_cleared'};}};
    }
    async applySearchScope(input:CatalogObject):Promise<CatalogObject>{
        if(this.localScopeWorking)throw new ServiceError('search_scope_apply_pending');
        const choice=localScopeChoice(input),adapter=this.localScopeAdapter(),native=validateNativeHostContext(adapter.context());
        if(choice.project_ids.some(id=>this.searchScopeSelection().managedProjectIds.includes(id)))throw new ServiceError('managed_scope_set_is_not_selectable');
        this.localScopeEpoch++;this.localScopeWorking=true;this.localScopeLabelChoice=choice;
        try{
            // Withdraw the prior session before changing its supporting membership.
            await this.context.workspaceState.update('airk.localScope.selection.1',{schema:1,namespace:native.namespace,host:native.host,context_key:native.key,selection:choice});adapter.current();
            await this.context.workspaceState.update('airk.scope.explicitWithdrawal.1',{schema:1,withdrawn:true});adapter.current();
            this.explicitCatalogScope=undefined;this.checks.clear();this.connectionCheck=undefined;this.treeChanges.fire();this.definitions.fire();
            const outcome=await applyLocalSearchScope(adapter,choice);adapter.current();
            const active=this.currentCatalogScope(),grant=active?{grant_id:active.selection.session.grant_id,expected_grant_revision:active.selection.session.expected_grant_revision,project_ids:active.selection.session.project_ids,...('index_ids' in active.selection.session&&active.selection.session.index_ids?{index_ids:active.selection.session.index_ids}:{})}:undefined;
            if(active)active.selection.label=this.scopeLabel(choice);
            await this.context.workspaceState.update('airk.localScope.selection.1',{schema:1,namespace:native.namespace,host:native.host,context_key:native.key,selection:choice,...(grant?{grant}:!choice.index_ids.length&&!choice.project_ids.length?{appliedEmpty:true}:{})});adapter.current();
            return outcome;
        }finally{this.localScopeWorking=false;this.localScopeLabelChoice=undefined;this.treeChanges.fire();}
    }
    async saveIndexSet(input:CatalogObject):Promise<CatalogObject>{
        if(this.localScopeWorking)throw new ServiceError('search_scope_apply_pending');
        if(typeof input.id==='string'&&this.searchScopeSelection().managedProjectIds.includes(input.id))throw new ServiceError('managed_scope_set_is_not_editable');
        const adapter=this.localScopeAdapter(),epoch=++this.localScopeEpoch;this.localScopeWorking=true;
        let completed=false;
        try{const outcome=await saveLocalIndexSet(adapter,input);completed=true;return outcome;}
        finally{
            let refreshed=completed;
            if(!completed&&epoch===this.localScopeEpoch){
                // Earlier steps may have committed before a later step failed.
                // Do not keep advertising the old grant from a cached context.
                this.explicitCatalogScope=undefined;this.checks.clear();this.connectionCheck=undefined;
                this.localScopeRestoreAttempt=undefined;this.treeChanges.fire();this.definitions.fire();
                try{await adapter.refreshContext();refreshed=true;}
                catch(error){this.output.appendLine(`Could not refresh search scope after the failed index-set change: ${error instanceof Error?error.message:String(error)}`);}
            }
            this.localScopeWorking=false;
            if(epoch===this.localScopeEpoch&&refreshed&&!adapter.control.catalogPending().length)await this.restoreLocalSearchScope();
            this.treeChanges.fire();
        }
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
        return candidates.some(current=>same(current.command,spec.command)&&same(current.root,spec.root)&&same(current.dataDir,spec.dataDir)&&current.collection===spec.collection&&current.service?.project===spec.service?.project&&same(current.service!.installDir,spec.service!.installDir)&&isDeepStrictEqual(current.catalogSession,spec.catalogSession)&&isDeepStrictEqual(current.combinedSession,spec.combinedSession));
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
            if(!observation.inspection||observation.inspection.payload_verified===false||observation.inspection.metadata_only===true){clear();if(!this.renderSetupFailed())this.status.text='AIRK: setup';return;}
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
            }else{clear();if(!this.renderSetupFailed())this.status.text='AIRK: inspect service';this.output.appendLine(String(error));}
        }}
        finally{if(!this.disposed&&generation===this.generation){this.syncHttpEndpointWatcher();this.treeChanges.fire();if(previous!==JSON.stringify(this.specs.map(spec=>[spec.command,launchArgs(spec)])))this.definitions.fire();}}
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
    private readonly httpSetups=new Map<string,Promise<LaunchSpec>>();
    private async enrollHttpSpec(spec:LaunchSpec,client:string):Promise<LaunchSpec>{
        this.trusted();const binding=this.binding,generation=this.generation;
        if(!binding||!spec.service||!same(binding.installDir,spec.service.installDir))throw new ServiceError('workspace_registration_changed');
        const selection:HttpSelection=spec.combinedSession?{kind:'combined',selection:spec.combinedSession}:spec.catalogSession?{kind:'scoped',selection:spec.catalogSession}:spec.http?.selection.kind==='imported_legacy'?spec.http.selection:(()=>{throw new ServiceError('http_connection_requires_reviewed_scope');})();
        const identity={...selection.selection};delete (identity as any).expected_catalog_revision;
        const key=createHash('sha256').update(JSON.stringify([client,selection.kind,identity,binding.installDir])).digest('hex');
        const active=this.httpSetups.get(key);if(active)return active;
        const pending=(async()=>{
            const current=()=>{this.trusted();if(binding!==this.binding||generation!==this.generation)throw new ServiceError('workspace_registration_changed');};current();
            const saved=this.context.globalState.get<string>('airk.http.connection.1.'+key),connectionId=saved&&/^[a-f0-9]{32}$/.test(saved)?saved:randomBytes(16).toString('hex');
            if(connectionId!==saved)await this.context.globalState.update('airk.http.connection.1.'+key,connectionId);current();
            const storage=await fs.realpath(this.context.globalStorageUri.fsPath);
            const selectionBytes=JSON.stringify(selection);if(Buffer.byteLength(selectionBytes)>49152)throw new ServiceError('http_selection_bound');
            const file=await publishNativeInput(binding.controlDir,selectionBytes,current);
            try{
                const info=await this.runtime().httpConnection(binding,selection,file,connectionId,client,this.assistanceEnabled()?this.assistanceHost?.descriptorPath:undefined);current();
                await this.provisionHttpInfo(info,client,current,storage);
                return {...spec,http:info};
            }finally{await removeNativeInput(file);}
        })().finally(()=>{if(this.httpSetups.get(key)===pending)this.httpSetups.delete(key);});
        this.httpSetups.set(key,pending);return pending;
    }
    private async provisionHttpInfo(info:HttpConnectionInfo,client:string,current:()=>void,storage:string):Promise<void>{
                const material=await httpMaterial(info);current();
                current();
                if(client==='codex'||client==='claude'){
                    const variable=client==='codex'?'CODEX_CA_CERTIFICATE':'NODE_EXTRA_CA_CERTS';
                    const existing=this.context.environmentVariableCollection.get?.(variable);
                    if(existing&&existing.type!==vscode.EnvironmentVariableMutatorType.Replace)throw new ServiceError('http_client_trust_requires_review');
                    const prior=existing?.value??process.env[variable]??(client==='codex'?process.env.SSL_CERT_FILE:undefined);
                    let trust=material.ca;
                    if(prior&&prior!==info.ca_certificate_file){
                        const stat=await fs.lstat(prior);if(!stat.isFile()||stat.isSymbolicLink()||stat.size>2097152)throw new ServiceError('http_client_trust_requires_review');
                        const previous=await httpPublicTrustFile(prior);if(previous.length!==stat.size||previous.includes('PRIVATE KEY')||!previous.includes('-----BEGIN CERTIFICATE-----'))throw new ServiceError('http_client_trust_requires_review');
                        if(path.dirname(prior)===path.join(storage,'http-trust')&&!previous.includes(material.ca))throw new ServiceError('http_owned_issuer_rotation_requires_review');
                        trust=previous.includes(material.ca)?previous:Buffer.concat([previous,Buffer.from('\n'),material.ca]);
                    }
                    const trustRoot=path.join(storage,'http-trust');await fs.mkdir(trustRoot,{recursive:true,mode:0o700});if(!same(await fs.realpath(trustRoot),trustRoot)||(await fs.lstat(trustRoot)).isSymbolicLink())throw new ServiceError('unsafe_http_trust_directory');
                    const trustFile=await publishOwnedIssuerBundle(trustRoot,trust,current);
                    current();this.context.environmentVariableCollection.persistent=false;this.context.environmentVariableCollection.replace(variable,trustFile);this.context.environmentVariableCollection.replace(credentialEnvironment(info),material.credential);
                }
    }
    /** Only a recognized, CAS-guarded owned configuration supplies this receipt.
     * Registration paths alone never manufacture a reviewed grant. */
    private async enrollLegacyHttpSpec(spec:LaunchSpec,client:string,receipt:string,migration?:{binding:RuntimeBinding;valid:()=>void}):Promise<LaunchSpec>{
        this.trusted();migration?.valid();const binding=migration?.binding??this.binding,generation=this.generation;
        if(!binding||!spec.service||!migration&&!this.isCurrent(spec)||!/^[a-f0-9]{64}$/.test(receipt))throw new ServiceError('workspace_registration_changed');
        const registration={binding:spec.service.project,root:spec.root,data_root:spec.dataDir,collection:spec.collection};
        const key=createHash('sha256').update(JSON.stringify([client,binding.installDir,registration])).digest('hex');
        const active=this.httpSetups.get(key);if(active)return active;
        const pending=(async()=>{
            const current=()=>{this.trusted();migration?.valid();if(generation!==this.generation||!migration&&(binding!==this.binding||!this.isCurrent(spec)))throw new ServiceError('workspace_registration_changed');};current();
            const prior=this.context.globalState.get<unknown>('airk.http.legacy.1.'+key);
            let previous:HttpConnectionInfo|undefined;
            if(prior!==undefined){previous=validateHttpConnection(prior,undefined,path.join(binding.controlDir,'http-1'));if(previous.selection.kind!=='imported_legacy'||!matchesImportedHttpOrigin(previous.selection.selection,registration))throw new ServiceError('crossed_http_legacy_record');}
            const configurationReceipt=previous?.selection.kind==='imported_legacy'?previous.selection.selection.configuration_receipt:receipt;
            const savedId=this.context.globalState.get<string>('airk.http.legacy.id.1.'+key);
            const connectionId=previous?.connection_id??(savedId&&/^[a-f0-9]{32}$/.test(savedId)?savedId:randomBytes(16).toString('hex'));
            if(connectionId!==savedId)await this.context.globalState.update('airk.http.legacy.id.1.'+key,connectionId);current();
            const storage=await fs.realpath(this.context.globalStorageUri.fsPath);
            if(previous){
                const info=validateHttpConnection(await this.runtime().httpConnectionInfo(binding,previous.connection_id),previous.selection,path.join(binding.controlDir,'http-1'));current();
                if(info.connection_id!==previous.connection_id)throw new ServiceError('crossed_http_legacy_connection');
                await this.provisionHttpInfo(info,client,current,storage);current();
                await this.context.globalState.update('airk.http.legacy.1.'+key,info);current();return {...spec,http:info};
            }
            const file=await publishNativeInput(binding.controlDir,JSON.stringify(registration),current);
            try{
                const info=await this.runtime().httpLegacyConnection(binding,registration,file,configurationReceipt,connectionId,client,this.assistanceEnabled()?this.assistanceHost?.descriptorPath:undefined);current();
                await this.provisionHttpInfo(info,client,current,storage);current();
                await this.context.globalState.update('airk.http.legacy.1.'+key,info);current();return {...spec,http:info};
            }finally{await removeNativeInput(file);}
        })().finally(()=>{if(this.httpSetups.get(key)===pending)this.httpSetups.delete(key);});
        this.httpSetups.set(key,pending);return pending;
    }
    private async enrollLegacyUnion(specs:LaunchSpec[],binding:RuntimeBinding,valid:()=>void):Promise<LaunchSpec>{
        valid();const registrations=specs.map(spec=>{
            const receipt=this.workspace.providerConfigurationReceipt(binding,spec);if(!receipt)throw new ServiceError('legacy_selection_review_required');
            return {registration:{binding:spec.service!.project,root:spec.root,data_root:spec.dataDir,collection:spec.collection},configuration_receipt:receipt};
        }).sort((left,right)=>left.registration.binding.localeCompare(right.registration.binding));
        const key=createHash('sha256').update(JSON.stringify(['legacy-union',binding.installDir,registrations])).digest('hex');
        const active=this.httpSetups.get(key);if(active)return active;
        const pending=(async()=>{
            const savedId=this.context.globalState.get<string>('airk.http.union.id.1.'+key),id=savedId&&/^[a-f0-9]{32}$/.test(savedId)?savedId:randomBytes(16).toString('hex');
            if(savedId!==id)await this.context.globalState.update('airk.http.union.id.1.'+key,id);valid();
            const storage=await fs.realpath(this.context.globalStorageUri.fsPath);
            const request=JSON.stringify({schema:1,registrations,no_default_project:true});if(Buffer.byteLength(request)>49152)throw new ServiceError('http_selection_bound');
            const file=await publishNativeInput(binding.controlDir,request,valid);
            try{
                const info=await this.runtime().httpLegacyUnionConnection(binding,registrations,file,id,'vscode',this.assistanceEnabled()?this.assistanceHost?.descriptorPath:undefined);valid();
                await this.provisionHttpInfo(info,'vscode',valid,storage);valid();return {...specs[0],http:info};
            }finally{await removeNativeInput(file);}
        })().finally(()=>{if(this.httpSetups.get(key)===pending)this.httpSetups.delete(key);});
        this.httpSetups.set(key,pending);return pending;
    }
    private previousLegacyHttp(spec:LaunchSpec,client:string,binding=this.binding):HttpConnectionInfo|undefined{
        const registration={binding:spec.service!.project,root:spec.root,data_root:spec.dataDir,collection:spec.collection};
        const key=createHash('sha256').update(JSON.stringify([client,spec.service!.installDir,registration])).digest('hex');
        const raw=this.context.globalState.get<unknown>('airk.http.legacy.1.'+key);if(raw===undefined)return;
        if(!binding||!same(binding.installDir,spec.service!.installDir))throw new ServiceError('workspace_registration_changed');
        const info=validateHttpConnection(raw,undefined,path.join(binding.controlDir,'http-1'));
        if(info.selection.kind!=='imported_legacy'||!matchesImportedHttpOrigin(info.selection.selection,registration))throw new ServiceError('crossed_http_legacy_record');return info;
    }
    private migrationCandidate(spec:LaunchSpec,agent:LocalAgent,before:string,previous?:LaunchSpec,binding?:RuntimeBinding,selectedKey?:string){
        const client=agent==='copilot'?'vscode':agent;
        const supported=['codex','claude','vscode'].includes(client);
        const prior=supported?this.previousLegacyHttp(spec,client,binding):undefined;
        const universal=this.legacyLauncherInspection?.universal_launcher;
        const old=prior?{...spec,http:prior}:previous??(universal?{...spec,command:String(record(universal).path),service:{...spec.service!,universal:true}}:undefined);
        // Detect actual owned declarations before enrollment. Unrelated client
        // settings must not trigger a new credential or an unsupported-client error.
        const candidate=prior?{...spec,http:prior}:{...spec,http:undefined};
        const declarations=globalConfigDeclarations(agent==='codex'?'codex':agent==='copilot'?'copilot':'claude',before).filter(row=>selectedKey===undefined||row.serverKey===selectedKey);
        const commands=[spec.command,path.join(spec.service!.installDir,process.platform==='win32'?'airk-launcher.exe':'airk-launcher'),...(old?[old.command]:[])];
        const ownCommand=declarations.some(row=>typeof row.fields.command==='string'&&path.isAbsolute(row.fields.command)&&commands.some(command=>same(command,String(row.fields.command))));
        const ownHttp=!!prior&&before.toLowerCase().includes(prior.connection_id);
        let removed:string;
        try{removed=migrateAgentConfig(agent,candidate,this.context.globalStorageUri.fsPath,before,true,()=>undefined,false,true,old,selectedKey);}
        catch(error){if(!ownCommand&&!ownHttp)return {client,supported,candidate,old,owned:false};throw error;}
        if(removed===before&&ownCommand)throw new ServiceError('owned_agent_alias_requires_review');
        if(removed!==before&&!prior&&!declarations.some(row=>row.serverKey!=='airk_explorer'&&Array.isArray(row.fields.args)&&row.fields.args.includes('mcp')))throw new ServiceError('owned_assistance_scope_requires_review');
        return {client,supported,candidate,old,owned:removed!==before};
    }
    private async prepareOwnedHttpMigration(spec:LaunchSpec,agent:LocalAgent,before:string,previous?:LaunchSpec,migration?:{binding:RuntimeBinding;valid:()=>void;configurationReceipt?:string;selectedKey?:string}):Promise<{spec:LaunchSpec;previous?:LaunchSpec;skip?:boolean}>{
        const {client,supported,candidate,old,owned}=this.migrationCandidate(spec,agent,before,previous,migration?.binding,migration?.selectedKey);
        if(!owned)return {spec:candidate,previous:old,skip:true};
        if(!supported)throw new ServiceError('http_client_compatibility_unverified');
        const next=spec.catalogSession||spec.combinedSession?await this.enrollHttpSpec(spec,client):await this.enrollLegacyHttpSpec(spec,client,migration?.configurationReceipt??createHash('sha256').update(before).digest('hex'),migration);
        if(next.http)this.migrationHttpConnections.set(next.http.connection_id,next.http);
        return {spec:next,previous:old};
    }
    async resolveAgentSpec(spec:LaunchSpec,clientKind='vscode'):Promise<LaunchSpec>{
        this.trusted();
        if(!this.binding||!this.isCurrent(spec))throw new ServiceError('workspace_registration_changed');
        if(!['vscode','codex','claude','airk-workbench','airk-explorer'].includes(clientKind))throw new ServiceError('http_client_compatibility_unverified');
        if(spec.catalogSession||spec.combinedSession)return this.enrollHttpSpec(spec,clientKind);
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
            const agent=clientKind==='vscode'?'copilot':clientKind as LocalAgent;
            const location=localAgentLocations[agent];
            const previous=this.previousLegacyHttp(selected,clientKind);
            if(previous){const resolved=await this.enrollLegacyHttpSpec(selected,clientKind,previous.selection.kind==='imported_legacy'?previous.selection.selection.configuration_receipt:'');return resolved;}
            const providerReceipt=this.workspace.providerConfigurationReceipt(binding,selected);
            if(providerReceipt){const resolved=await this.enrollLegacyHttpSpec(selected,clientKind,providerReceipt);return resolved;}
            if(!location)throw new ServiceError('legacy_selection_review_required');
            const destination=path.join(selected.root,...location),directory=path.dirname(destination);
            const stat=await fs.lstat(directory).catch(()=>undefined);
            if(!stat?.isDirectory()||stat.isSymbolicLink()||!same(await fs.realpath(directory),directory))throw new ServiceError('legacy_selection_review_required');
            const before=await readJsonConfig(destination);if(before===undefined)throw new ServiceError('legacy_selection_review_required');
            const resolved=await this.prepareOwnedHttpMigration(selected,agent,before);return resolved.spec;
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
            if(!selected.http)selected=await this.resolveAgentSpec(selected);else if(!this.isCurrent(selected))throw new ServiceError('workspace_registration_changed');
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
        const command=binding.executable;
        if(!command||binding.payloadVerified===false)throw new ServiceError('native_executable_unavailable');
        const administration:AdministrativeIndexSelection & {nativeObservation:{host_context:CatalogObject;index_detail:CatalogObject}}={purpose:'index_administration',label:ADMINISTRATIVE_USAGE_LABEL,namespace:descriptor.namespace,catalogRevision:descriptor.catalog_revision,indexId:String(index.id),backing:catalogHex(backing.binding,64),
            runtime:{installDir:binding.installDir,controlDir:binding.controlDir,runtimeSha256:binding.runtimeSha256},nativeObservation:{host_context:context.response,index_detail:response}};
        const administrativeSpec:LaunchSpec={command,root:chosen.backing.root,dataDir:chosen.backing.data_root,collection:chosen.backing.collection,
            name:`${chosen.name} · Entire Index administration`,service:{installDir:binding.installDir,project:administration.backing}};
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
        if(!binding.executable)throw new ServiceError('native_executable_unavailable');
        this.reportInstall('Installing the terminal command…');
        const touched:ShellTouch[]=[];
        const errors=await installLauncherCli(binding.executable,this.context,this.shellScope(),touched);this.trusted();
        // Uninstall cleanup reverses exactly these startup-file edits.
        if(this.uninstallDirectory)await rememberShellIntegration(this.uninstallDirectory,this.context.globalStorageUri.fsPath,touched)
            .catch(error=>errors.push(`Could not record terminal integration for uninstall cleanup: ${String(error)}`));
        for(const error of errors){this.installationWarnings=true;this.output.appendLine(error);}
    }
    private async verifyMigrationSnapshots():Promise<void>{
        await this.migrationGlobalVerifier?.();
        for(const destination of this.migrationAbsentConfigurations)if(await readJsonConfig(destination)!==undefined)throw new ServiceError('agent_configuration_appeared_after_migration');
        for(const [destination,expected]of this.migratedConfigurationSnapshots){
            const source=await readJsonConfig(destination);this.trusted();
            if(source===undefined||createHash('sha256').update(source).digest('hex')!==expected)throw new ServiceError('agent_http_migration_incomplete',false,{destination,reason:'configuration_changed_after_migration'});
        }
    }
    private async finalizeLegacyLauncherRemoval(binding:RuntimeBinding,publishCompletion=true,forcePayloadRetirement=false):Promise<void>{
        this.trusted();const generation=this.generation;
        await this.verifyMigrationSnapshots();
        if(!this.legacyCachePreparation)throw new ServiceError('legacy_cache_inspection_unconfirmed');
        let cacheReceipt:Record<string,unknown>;
        try{cacheReceipt=await this.runtime().retireLegacyCaches(binding,this.legacyCachePreparation);}
        catch(original){
            this.output.appendLine('AIRK cache retirement was not confirmed: '+String(original));
            if(original instanceof ServiceError&&original.nativeResponse)this.output.appendLine(JSON.stringify(original.nativeResponse));
            // Explicit native recovery checks actual inventory and refuses
            // pending/unknown work. An unchanged plan never authorizes replay.
            const previous=this.legacyCachePreparation;
            const recoveryCurrent=()=>{this.trusted();if(generation!==this.generation)throw new ServiceError('workspace_registration_changed');};
            const client=await this.migrationClient(binding);let inventory:Awaited<ReturnType<typeof readWorkspaceRegistrations>>;
            try{inventory=await readWorkspaceRegistrations(binding,async(after,revision,inactive)=>client.projects(after,revision,32,inactive));}finally{client.dispose();}recoveryCurrent();
            const roots=[...this.folders().map(folder=>folder.uri.fsPath),...this.workspace.rememberedRoots(binding),...[...inventory.active,...inventory.inactive].map(row=>row.root)];
            const locators=await this.scanLegacyCacheOrigins(roots,recoveryCurrent);
            const replacement=await this.withLegacyOriginFile(binding,locators,recoveryCurrent,file=>this.runtime().reprepareLegacyCaches(binding,previous,file));recoveryCurrent();
            if(generation!==this.generation)throw new ServiceError('workspace_registration_changed');
            if(replacement.inspection_sha256===previous.inspection_sha256)throw original;
            this.output.appendLine('AIRK native cache inventory changed; repeating the complete guarded configuration review once.');
            await this.migrateAgentConnections(binding,this.migrationPredecessors);this.trusted();await this.verifyMigrationSnapshots();
            if(!this.legacyCachePreparation||this.legacyCachePreparation.inspection_sha256!==replacement.inspection_sha256)throw new ServiceError('legacy_cache_reprepare_unconfirmed');
            cacheReceipt=await this.runtime().retireLegacyCaches(binding,this.legacyCachePreparation);
        }
        this.trusted();if(generation!==this.generation)throw new ServiceError('workspace_registration_changed');
        this.output.appendLine('AIRK old helper cache removal ('+String(cacheReceipt.scope)+'): '+JSON.stringify(cacheReceipt));
        this.reportInstall('Removing old AIRK launchers...');
        const receipt=await this.runtime().retireLegacyLaunchers(binding);this.trusted();
        if(generation!==this.generation)throw new ServiceError('workspace_registration_changed');
        this.output.appendLine('AIRK legacy launcher removal: '+JSON.stringify(receipt));
        // Native removal leaves the current service and explicit Stop untouched.
        // Retire old payloads only after original declarations have migrated and
        // held native identities prove their obsolete launchers are gone.
        const alreadyComplete=this.context.globalState.get<string>(`airk.installComplete.2.${binding.installDir}`)===`${binding.version}:${binding.runtimeSha256}`;
        const obsolete=this.legacyLauncherInspection?.legacy_runtime_images;
        const retired=forcePayloadRetirement||!alreadyComplete||Array.isArray(obsolete)&&obsolete.length?await this.runtime().retirePrevious(binding):undefined;this.trusted();
        // Previous-payload retirement can restart the native owner. Only that
        // actual native receipt warrants replacing its pipe and endpoint refs.
        if(retired?.complete!==true&&retired?.retired_previous!==undefined&&retired.retired_previous!==null){
            this.console.reset();const connected=await this.runtime().reconnect(await this.runtime().discover());this.trusted();
            if(connected.runtimeSha256!==binding.runtimeSha256||!same(connected.controlDir,binding.controlDir))throw new ServiceError('runtime_selection_changed_review_again');
            await this.migrateAgentConnections(connected,this.migrationPredecessors);
        }
        for(const info of this.migrationHttpConnections.values()){
            const current=validateHttpConnection(await this.runtime().httpConnectionInfo(binding,info.connection_id),info.selection,path.join(binding.controlDir,'http-1'));this.trusted();
            if(current.connection_id!==info.connection_id||!isDeepStrictEqual(current.endpoint,info.endpoint)||current.ca_certificate_file!==info.ca_certificate_file||current.credential_file!==info.credential_file)throw new ServiceError('agent_http_migration_incomplete',false,{reason:'native_http_endpoint_changed_after_cleanup'});
        }
        await this.verifyMigrationSnapshots();
        if(this.installationWarnings)throw new ServiceError('installation_cleanup_incomplete');
        if(publishCompletion)await this.context.globalState.update(`airk.installComplete.2.${binding.installDir}`,`${this.runningVersion}:${binding.runtimeSha256}`);
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
    private migrationCommandPaths(binding:RuntimeBinding,specs:readonly LaunchSpec[]):string[]{
        const inspected=this.legacyLauncherInspection,paths=[binding.executable,path.join(binding.installDir,process.platform==='win32'?'airk-launcher.exe':'airk-launcher'),...specs.map(spec=>spec.command),...this.migrationPredecessors.map(spec=>spec.command)];
        for(const field of ['launcher','universal_launcher'])if(inspected?.[field])paths.push(String(record(inspected[field]).path));
        if(Array.isArray(inspected?.legacy_runtime_images))for(const item of inspected.legacy_runtime_images)paths.push(String(record(item).path));
        if(Array.isArray(this.legacyCachePreparation?.caches))for(const item of this.legacyCachePreparation.caches)paths.push(String(record(item).executable));
        return [...new Set(paths)];
    }
    private legacyCommandKnown(command:unknown,paths:readonly string[]):boolean{
        if(typeof command!=='string'||!path.isAbsolute(command))return false;
        if(paths.some(value=>same(value,command)))return true;
        const relative=path.relative(this.context.globalStorageUri.fsPath,command).split(path.sep),name=process.platform==='win32'?'airk.exe':'airk';
        return relative.join('/')==='cli/bin/'+name||relative.length===3&&relative[0]==='helpers'&&/^[A-Za-z0-9._-]+$/.test(relative[1])&&relative[2]===name;
    }
    private async saveMigrationConfiguration(destination:string,before:string,after:string,check:()=>void):Promise<string>{
        const validate=async()=>{const parent=path.dirname(destination),stat=await fs.lstat(parent);check();if(!stat.isDirectory()||stat.isSymbolicLink()||!same(await fs.realpath(parent),parent))throw new ServiceError('unsafe_agent_configuration_directory');return readJsonConfig(destination);};
        check();const matches=vscode.workspace.textDocuments.filter(document=>same(document.uri.fsPath,destination));
        if(matches.some(document=>document.isDirty)){const error=new ServiceError('agent_configuration_has_unsaved_edits');error.message='Agent configuration has unsaved edits; save it and retry Install or Repair CLI.';throw error;}
        const document=matches[0]??await vscode.workspace.openTextDocument(vscode.Uri.file(destination));
        if(document.isDirty||document.getText()!==before||await validate()!==before)throw new ServiceError('agent_configuration_changed');
        const editorAfter=document.eol===2?after.replace(/\r?\n/g,'\r\n'):document.eol===1?after.replace(/\r\n/g,'\n'):after;
        check();const version=document.version,edit=new vscode.WorkspaceEdit();edit.replace(document.uri,new vscode.Range(document.positionAt(0),document.positionAt(before.length)),editorAfter);
        if(document.version!==version||!await vscode.workspace.applyEdit(edit))throw new ServiceError('agent_configuration_edit_failed');
        check();if(document.getText()!==editorAfter||!await document.save())throw new ServiceError('agent_configuration_save_failed');
        const saved=await validate();check();if(saved!==editorAfter)throw new ServiceError('agent_configuration_save_unconfirmed');return saved;
    }
    private closeHttpEndpointWatcher():void{
        this.httpWatchParent?.close();this.httpWatchParent=undefined;this.httpWatchEndpoint?.close();this.httpWatchEndpoint=undefined;
        clearTimeout(this.httpWatchTimer);this.httpWatchTimer=undefined;clearTimeout(this.httpWatchRearm);this.httpWatchRearm=undefined;
        clearTimeout(this.httpWatchHealthRetry);this.httpWatchHealthRetry=undefined;this.httpWatchFailures=0;
        this.httpWatchRead?.dispose();this.httpWatchRead=undefined;this.httpWatchKey=undefined;this.httpWatchPending=false;
    }
    /** Filesystem notifications are hints only. Native Health authenticates the
     * selected owner; no descriptor contents, client election or UI polling. */
    private syncHttpEndpointWatcher():void{
        const binding=this.binding;
        if(this.disposed||!vscode.workspace.isTrusted||!nativeExecutionAllowed()||!binding||binding.version!==this.runningVersion){this.closeHttpEndpointWatcher();return;}
        const key=JSON.stringify([binding.controlDir,binding.runtimeSha256,this.generation]);
        if(this.httpWatchKey===key&&this.httpWatchParent&&this.httpWatchEndpoint)return;
        const initial=this.httpWatchKey!==key;
        if(initial){this.closeHttpEndpointWatcher();this.httpWatchKey=key;}
        const attach=(directory:string,parent:boolean):FSWatcher|undefined=>{
            try{
                const watcher=watch(directory,{persistent:false},(_event,name)=>{
                    if(this.httpWatchKey!==key)return;
                    if(name!==null&&String(name)!==(parent?'http-1':'endpoint.json'))return;
                    if(parent){this.httpWatchEndpoint?.close();this.httpWatchEndpoint=undefined;}
                    this.queueHttpOwnerHint();
                });
                watcher.on('error',()=>{watcher.close();if(this.httpWatchKey!==key)return;if(parent)this.httpWatchParent=undefined;else this.httpWatchEndpoint=undefined;this.queueHttpOwnerHint();this.rearmHttpEndpointWatcher();});
                return watcher;
            }catch{return undefined;}
        };
        if(!this.httpWatchParent)this.httpWatchParent=attach(binding.controlDir,true);
        if(!this.httpWatchEndpoint)this.httpWatchEndpoint=attach(path.join(binding.controlDir,'http-1'),false);
        if(!this.httpWatchParent||!this.httpWatchEndpoint)this.rearmHttpEndpointWatcher();
        else{clearTimeout(this.httpWatchRearm);this.httpWatchRearm=undefined;}
        if(initial)this.queueHttpOwnerHint();
    }
    private rearmHttpEndpointWatcher():void{
        if(this.disposed||this.httpWatchRearm)return;
        this.httpWatchRearm=setTimeout(()=>{this.httpWatchRearm=undefined;this.syncHttpEndpointWatcher();this.queueHttpOwnerHint();},30000);
    }
    private queueHttpOwnerHint():void{
        if(this.disposed||this.httpWatchTimer)return;
        this.httpWatchTimer=setTimeout(()=>{this.httpWatchTimer=undefined;this.syncHttpEndpointWatcher();void this.readHttpOwnerHint();},200);
    }
    private async readHttpOwnerHint():Promise<void>{
        if(this.httpWatchReading){this.httpWatchPending=true;return;}
        const binding=this.binding,key=this.httpWatchKey;
        if(!key||!binding||this.disposed||this.settingUp||!vscode.workspace.isTrusted||!nativeExecutionAllowed())return;
        clearTimeout(this.httpWatchHealthRetry);this.httpWatchHealthRetry=undefined;
        this.httpWatchReading=true;let client:ReturnType<RuntimeHost['client']>|undefined;
        try{
            client=this.runtime().client(binding,3000);this.httpWatchRead=client;
            const response=await client.rpc('health','health');this.httpWatchFailures=0;
            if(!this.disposed&&this.httpWatchKey===key&&this.binding===binding)this.observeHttpIdentity(record(response.observation),binding);
        }catch(error){
            // A publication hint may precede listener readiness. Bound retries;
            // stopped owners remain stopped, and healthy watches need no polling.
            if(!this.disposed&&this.httpWatchKey===key&&vscode.workspace.isTrusted&&nativeExecutionAllowed()&&!(error instanceof ServiceError&&error.code==='service_explicitly_stopped')&&this.httpWatchFailures<3){
                const delay=1000*2**++this.httpWatchFailures;
                this.httpWatchHealthRetry=setTimeout(()=>{this.httpWatchHealthRetry=undefined;void this.readHttpOwnerHint();},delay);
            }
        }
        finally{client?.dispose();if(this.httpWatchRead===client)this.httpWatchRead=undefined;this.httpWatchReading=false;if(this.httpWatchPending){this.httpWatchPending=false;this.queueHttpOwnerHint();}}
    }
    private observeHttpOwner(state:ServiceConsoleState):void{
        if(state.health.state!=='live')return;
        const health=state.health.value?.observation as Record<string,unknown>|undefined,binding=this.binding??this.console.controller.connectedRuntimeBinding;
        if(health&&binding)this.observeHttpIdentity(health,binding);
    }
    private observeHttpIdentity(health:Record<string,unknown>,binding:RuntimeBinding):void{
        if(this.disposed||this.settingUp||!vscode.workspace.isTrusted||!nativeExecutionAllowed()||binding.version!==this.runningVersion||health.version!==this.runningVersion||health.runtime_sha256!==binding.runtimeSha256||typeof health.instance!=='string'||!/^[a-f0-9]{32}$/.test(health.instance))return;
        const instance=health.instance;this.latestHttpOwner={runtime:binding.runtimeSha256,instance};
        if(this.httpConfigurationRefresh||this.observedHttpInstance===instance)return;
        if(this.httpRefreshRetry?.instance===instance&&Date.now()<this.httpRefreshRetry.next)return;
        clearTimeout(this.httpRefreshTimer);this.httpRefreshTimer=undefined;
        const pending=this.refreshHttpConfigurationFiles(binding,instance).then(()=>{
            this.observedHttpInstance=instance;this.httpRefreshRetry=undefined;
            if(!this.disposed)this.definitions.fire();
        },error=>{
            const attempt=this.httpRefreshRetry?.instance===instance?Math.min(6,this.httpRefreshRetry.attempt+1):1;
            const delay=Math.min(30000,1000*2**attempt);
            this.httpRefreshRetry={instance,attempt,next:Date.now()+delay};
            if(!this.disposed)this.httpRefreshTimer=setTimeout(()=>{this.httpRefreshTimer=undefined;void this.readHttpOwnerHint();},delay);
            if(!this.disposed){
                this.output.appendLine('AIRK saved HTTPS connection refresh deferred: '+String(error));
                if(this.httpRefreshWarnedInstance!==instance){this.httpRefreshWarnedInstance=instance;void vscode.window.showWarningMessage('AIRK restarted. Some saved HTTPS connections still need refreshing. AIRK will retry with their current file contents; review AIRK output for blocked or edited configurations.');}
            }
        }).finally(()=>{if(this.httpConfigurationRefresh===pending)this.httpConfigurationRefresh=undefined;});
        this.httpConfigurationRefresh=pending;
    }
    /** Read-only discovery through the management channel refreshes only existing
     * owned HTTP declarations; it never enrolls, changes grants or retires files. */
    private async refreshHttpConfigurationFiles(binding:RuntimeBinding,instance?:string):Promise<void>{
        const generation=this.generation,check=()=>{this.trusted();const owner=this.latestHttpOwner;if(generation!==this.generation||this.settingUp||instance&&owner?.runtime===binding.runtimeSha256&&owner.instance!==instance)throw new ServiceError('workspace_registration_changed');};
        // Serialize with installation migration; retain a separate request cache.
        if(this.settingUp)return;this.httpOwnershipLookups.clear();check();
        const roots=new Set([...this.folders().map(folder=>folder.uri.fsPath),...this.workspace.rememberedRoots(binding)]);
        const client=this.runtime().client(binding);try{const native=await readWorkspaceRegistrations(binding,async(after,revision,inactive)=>client.projects(after,revision,32,inactive));for(const row of [...native.active,...native.inactive])roots.add(row.root);}finally{client.dispose();}check();
        for(const root of roots)for(const agent of ['codex','claude','copilot'] as const){
            const destination=path.join(root,...localAgentLocations[agent]),source=await readJsonConfig(destination);if(source===undefined)continue;
            const refreshed=await this.refreshOwnedHttpConfiguration(agent,source,binding,check,true);check();if(refreshed.source!==source)await this.saveMigrationConfiguration(destination,source,refreshed.source,check);
        }
        const globals=await inventoryGlobalAgentConfigs({home:os.homedir(),env:{...process.env},globalStoragePath:this.context.globalStorageUri.fsPath,check});check();
        if(globals.blockers.length)throw new ServiceError('http_configuration_refresh_unconfirmed',false,{blockers:globals.blockers});
        for(const original of globals.files){let file=original;
            for(const container of [...new Map(file.declarations.map(row=>[JSON.stringify(row.containerPath),row.containerPath])).values()]){
                if(file.agent==='codex'&&(container.length!==1||container[0]!=='mcp_servers')){
                    const declarations=file.declarations.filter(row=>JSON.stringify(row.containerPath)===JSON.stringify(container));
                    if(await this.hasOwnedHttpDeclarations(file.agent,declarations,binding,check))throw new ServiceError('owned_http_profile_requires_review',false,{destination:file.destination});
                    continue;
                }
                let before='';transformGlobalContainer(file,container,source=>{before=source;return source;});
                const refreshed=await this.refreshOwnedHttpConfiguration(file.agent,before,binding,check,true);check();
                if(refreshed.source!==before){const source=transformGlobalContainer(file,container,()=>refreshed.source);file={...file,source,declarations:globalConfigDeclarations(file.agent,source)};}
            }
            if(file.source!==original.source)await this.saveMigrationConfiguration(file.destination,original.source,file.source,check);
        }
    }
    private lookupHttpOwnership(binding:RuntimeBinding,id:string){
        const key=binding.installDir+':'+binding.runtimeSha256+':'+id;
        let pending=this.httpOwnershipLookups.get(key);
        if(!pending){pending=this.runtime().lookupHttpConnection(binding,id);this.httpOwnershipLookups.set(key,pending);}
        return pending;
    }
    private async hasOwnedHttpDeclarations(agent:GlobalAgent,declarations:GlobalConfigFile['declarations'],binding:RuntimeBinding,check:()=>void):Promise<boolean>{
        for(const row of declarations){
            const id=httpConfigurationConnectionId(agent,row.fields);if(!id)continue;
            const lookup=await this.lookupHttpOwnership(binding,id);check();if(lookup.status==='owned')return true;
        }
        return false;
    }
    private async refreshOwnedHttpConfiguration(agent:LocalAgent,source:string,binding:RuntimeBinding,check:()=>void,onlyEndpointChanges=false):Promise<{source:string;owned:boolean}>{
        if(!['codex','claude','copilot'].includes(agent))return {source,owned:false};
        const declarations=globalConfigDeclarations(agent as GlobalAgent,source);
        let result=source,owned=false;
        for(const row of declarations){
            const id=httpConfigurationConnectionId(agent,row.fields);if(!id)continue;
            const lookup=await this.lookupHttpOwnership(binding,id);check();if(lookup.status==='not_owned')continue;
            if(row.containerPath.length!==1)throw new ServiceError('owned_http_alias_requires_review',false,{connection_id:id});
            const info=validateHttpConnection(lookup.connection,undefined,path.join(binding.controlDir,'http-1'));
            if(row.serverKey!=='airk'&&preservedImportedHttpServerKey(info,row.serverKey)!==row.serverKey)throw new ServiceError('owned_http_alias_requires_review',false,{connection_id:id});
            const storage=await fs.realpath(this.context.globalStorageUri.fsPath);check();
            await this.provisionHttpInfo(info,agent==='copilot'?'vscode':agent,check,storage);check();
            if(!onlyEndpointChanges||row.fields.url!==info.endpoint.url)result=refreshHttpAgentConfig(agent,result,info,String(row.fields.url),row.serverKey);
            this.migrationHttpConnections.set(id,info);owned=true;
        }
        return {source:result,owned};
    }
    private originalMigrationReceipt(destination:string,fallback:string):string{
        const original=Array.isArray(this.legacyCachePreparation?.origin_proofs)?this.legacyCachePreparation.origin_proofs.map(record).filter(value=>same(String(value.config_path),destination)):[];
        const digests=new Set(original.map(value=>String(value.config_sha256)));
        if(digests.size>1)throw new ServiceError('legacy_original_configuration_changed');
        return [...digests][0]??[...this.legacyMigrationFiles.values()].find(value=>same(value.file.destination,destination))?.file.sha256??fallback;
    }
    private async migrateGeneratedServiceConfiguration(binding:RuntimeBinding,agent:LocalAgent,before:string,specs:readonly LaunchSpec[],restored:readonly LaunchSpec[],check:()=>void,configurationReceipt:string,destination:string,originContainer?:readonly string[]):Promise<{source:string;owned:boolean}>{
        const refreshed=await this.refreshOwnedHttpConfiguration(agent,before,binding,check);let source=refreshed.source,owned=refreshed.owned;
        const parser=agent==='codex'?'codex':agent==='copilot'?'copilot':'claude',commands=this.migrationCommandPaths(binding,specs),rows=globalConfigDeclarations(parser,source);
        const ownedReadRows=rows.filter(row=>row.serverKey!=='airk_explorer'&&this.legacyCommandKnown(row.fields.command,commands));
        const independent=rows.some(row=>typeof row.fields.url==='string')||ownedReadRows.length>1||rows.some(row=>row.serverKey==='airk'&&!this.legacyCommandKnown(row.fields.command,commands));
        for(const row of rows){
            if(row.containerPath.length!==1||!this.legacyCommandKnown(row.fields.command,commands))continue;
            if(!globalConfigDeclarations(parser,source).some(current=>current.serverKey===row.serverKey&&current.fields.command===row.fields.command))continue;
            if(Array.isArray(this.legacyCachePreparation?.caches)&&this.legacyCachePreparation.caches.some(cache=>same(String(record(cache).executable),String(row.fields.command)))){
                const managed=this.legacyCachePreparation?.managed_locators;
                if(!Array.isArray(managed)||!managed.some(value=>{const locator=value as unknown as LegacyOriginLocator,container=locator.container_path??[locator.format==='codex_toml'?'mcp_servers':locator.format==='vscode_servers_json'?'servers':'mcpServers'];if(!same(locator.path,destination)||locator.server!==row.serverKey||!isDeepStrictEqual(container,originContainer??row.containerPath))return false;return (this.legacyCachePreparation!.origin_proofs as unknown[]).map(record).some(origin=>origin.config_path===locator.path&&origin.server===locator.server&&isDeepStrictEqual(origin.container_path,container)&&same(String(origin.executable),String(row.fields.command))&&isDeepStrictEqual(record(origin.declaration).argv,row.fields.args));}))throw new ServiceError('standalone_index_adoption_required');
            }
            const args=row.fields.args;if(!Array.isArray(args)||args.some(value=>typeof value!=='string'))throw new ServiceError('owned_global_scope_unproven');
            if(args[0]==='runtime'&&(args[1]!=='launch'||args[2]!=='--install-dir'||!same(String(args[3]),binding.installDir)||args[4]!=='--'))throw new ServiceError('owned_global_scope_unproven');
            const candidates=specs.filter(spec=>{
                const at=args.indexOf('--root');if(at<0||typeof args[at+1]!=='string'||!same(args[at+1],spec.root))return false;
                for(const [flag,value]of [['--collection',spec.collection],['--service-project',spec.service!.project],['--data-dir',spec.dataDir]]){const index=args.indexOf(flag);if(index>=0&&(typeof args[index+1]!=='string'||(flag==='--data-dir'?!same(args[index+1],value):args[index+1]!==value)))return false;}
                return true;
            });
            if(candidates.length!==1)throw new ServiceError(candidates.length?'ambiguous_global_scope':'owned_global_scope_unproven');
            const spec=candidates[0];if(row.serverKey!=='airk'&&row.serverKey!=='airk-'+spec.collection)throw new ServiceError('owned_agent_alias_requires_review');
            const previous=restored.find(value=>same(value.root,spec.root)&&value.collection===spec.collection&&same(value.command,String(row.fields.command)))??{...spec,command:String(row.fields.command),http:undefined,service:{...spec.service!,direct:true}};
            const selectedKey=independent?row.serverKey:undefined;
            if(!this.migrationCandidate(spec,agent,source,previous,binding,selectedKey).owned)throw new ServiceError('owned_global_scope_unproven');
            const resolved=await this.prepareOwnedHttpMigration(spec,agent,source,previous,{binding,valid:check,configurationReceipt,selectedKey});check();
            source=migrateAgentConfig(agent,resolved.spec,this.context.globalStorageUri.fsPath,source,this.assistanceEnabled(),()=>undefined,false,false,resolved.previous,selectedKey);owned=true;
        }
        return {source,owned};
    }
    private async migrateGlobalAgentConnections(binding:RuntimeBinding,specs:readonly LaunchSpec[],restored:readonly LaunchSpec[],check:()=>void,observe:(destination:string,source:string)=>void):Promise<void>{
        const options={home:os.homedir(),env:{...process.env},globalStoragePath:this.context.globalStorageUri.fsPath,check};
        const inventory=await inventoryGlobalAgentConfigs(options);check();
        if(inventory.blockers.length)throw new ServiceError('agent_http_migration_incomplete',false,{blockers:inventory.blockers});
        const commands=this.migrationCommandPaths(binding,specs),sameCommand=(value:unknown)=>this.legacyCommandKnown(value,commands);
        for(const original of inventory.files){
            let file:GlobalConfigFile=original,relevant=false;
            const containers=[...new Map(file.declarations.map(row=>[JSON.stringify(row.containerPath),row.containerPath])).values()];
            for(const container of containers){
                const declarations=file.declarations.filter(row=>JSON.stringify(row.containerPath)===JSON.stringify(container));
                const ownedCommand=declarations.some(row=>sameCommand(row.fields.command));
                if(file.agent==='codex'&&(container.length!==1||container[0]!=='mcp_servers')){
                    const knownHttp=await this.hasOwnedHttpDeclarations(file.agent,declarations,binding,check);
                    if(ownedCommand||knownHttp)throw new ServiceError('agent_http_migration_incomplete',false,{destination:file.destination,reason:'owned_profile_transform_unverified'});
                    continue;
                }
                let before='';transformGlobalContainer(file,container,source=>{before=source;return source;});
                const refreshed=await this.migrateGeneratedServiceConfiguration(binding,file.agent,before,specs,restored,check,this.originalMigrationReceipt(file.destination,original.sha256),file.destination,container);check();
                if(refreshed.owned){relevant=true;const source=transformGlobalContainer(file,container,()=>refreshed.source);file={...file,source,declarations:globalConfigDeclarations(file.agent,source)};}

            }
            if(file.source!==original.source){const saved=await this.saveMigrationConfiguration(file.destination,original.source,file.source,check);file={...file,source:saved,declarations:globalConfigDeclarations(file.agent,saved)};this.migratedConnections=true;}
            // A recognized old image under an unsupported alias cannot escape
            // finalization merely because it was not a canonical AIRK key.
            if(file.declarations.some(row=>sameCommand(row.fields.command)))throw new ServiceError('agent_http_migration_incomplete',false,{destination:file.destination,reason:'legacy_global_declaration_remains'});
            if(relevant)observe(file.destination,file.source);
        }
        this.migrationGlobalVerifier=async()=>{
            const fresh=await inventoryGlobalAgentConfigs(options);check();
            if(fresh.blockers.length)throw new ServiceError('agent_http_migration_incomplete',false,{blockers:fresh.blockers});
            for(const file of fresh.files){
                if(file.declarations.some(row=>sameCommand(row.fields.command)))throw new ServiceError('agent_http_migration_incomplete',false,{destination:file.destination,reason:'legacy_global_declaration_appeared'});
                if(!this.migratedConfigurationSnapshots.has(file.destination)&&file.declarations.some(row=>{const id=httpConfigurationConnectionId(file.agent,row.fields);return !!id&&this.migrationHttpConnections.has(id);}))throw new ServiceError('agent_http_migration_incomplete',false,{destination:file.destination,reason:'owned_global_configuration_appeared'});
            }
        };
    }
    private async scanLegacyCacheOrigins(roots:readonly string[],check:()=>void):Promise<LegacyOriginLocator[]>{
        const locators:LegacyOriginLocator[]=[],seen=new Set<string>();this.legacyMigrationFiles.clear();
        const collect=(destination:string,agent:GlobalAgent,source:string,actualAgent:LocalAgent=agent)=>{
            const declarations=globalConfigDeclarations(agent,source);this.legacyMigrationFiles.set(destination,{agent:actualAgent,file:{agent,destination,source,sha256:createHash('sha256').update(source).digest('hex'),declarations}});
            for(const row of declarations){
                if(actualAgent!==agent&&(row.containerPath.length!==1||row.containerPath[0]!=='mcpServers'))continue;
                const command=row.fields.command;
                if(typeof command!=='string'||!path.isAbsolute(command)||path.basename(path.dirname(path.dirname(command))).toLowerCase()!=='helpers'||path.basename(path.dirname(path.dirname(path.dirname(command)))).toLowerCase()!=='iokaio.airk')continue;
                if(!['airk','airk.exe'].includes(path.basename(command).toLowerCase()))continue;
                const locator:LegacyOriginLocator={path:destination,format:agent==='codex'?'codex_toml':agent==='copilot'?'vscode_servers_json':'mcp_servers_json',server:row.serverKey,...(agent==='claude'?{container_path:row.containerPath}:{})};
                const key=JSON.stringify(locator);if(!seen.has(key)){seen.add(key);locators.push(locator);}
            }
        };
        for(const root of new Set(roots))for(const [agent,parts]of Object.entries(localAgentLocations)){
            const destination=path.join(root,...parts),source=await readJsonConfig(destination);check();if(source!==undefined)collect(destination,agent==='codex'?'codex':agent==='copilot'?'copilot':'claude',source,agent as LocalAgent);
        }
        const globals=await inventoryGlobalAgentConfigs({home:os.homedir(),env:{...process.env},globalStoragePath:this.context.globalStorageUri.fsPath,check});check();
        if(globals.blockers.length)throw new ServiceError('agent_http_migration_incomplete',false,{blockers:globals.blockers});
        for(const file of globals.files)collect(file.destination,file.agent,file.source);
        if(locators.length>128||Buffer.byteLength(JSON.stringify(locators))>524288)throw new ServiceError('legacy_cache_locator_inventory_oversize');
        return locators;
    }
    private async withLegacyOriginFile<T>(binding:RuntimeBinding,locators:readonly LegacyOriginLocator[],check:()=>void,action:(file:string|undefined)=>Promise<T>):Promise<T>{
        let locatorFile:string|undefined;
        try{
            if(locators.length){locatorFile=await publishNativeInput(binding.controlDir,JSON.stringify(locators),check);check();}
            const result=await action(locatorFile);check();return result;
        }finally{if(locatorFile)await removeNativeInput(locatorFile);}
    }
    private async prepareLegacyCacheMigration(binding:RuntimeBinding,roots:readonly string[],check:()=>void):Promise<void>{
        const locators=await this.scanLegacyCacheOrigins(roots,check);
        const storage=await fs.realpath(this.context.globalStorageUri.fsPath);check();
        this.legacyCachePreparation=await this.withLegacyOriginFile(binding,locators,check,file=>this.runtime().prepareLegacyCaches(binding,path.dirname(storage),file));check();
    }
    private async adoptStandaloneConfigurations(binding:RuntimeBinding,check:()=>void,observe:(destination:string,source:string)=>void):Promise<void>{
        const prepared=this.legacyCachePreparation;if(!prepared)throw new ServiceError('legacy_cache_inspection_unconfirmed');
        const classifications=Array.isArray(prepared.origin_classifications)?prepared.origin_classifications.map(record):[];
        if(classifications.some(value=>value.kind==='unsupported'))throw new ServiceError('legacy_original_configuration_unsupported',false,prepared);
        const candidates=Array.isArray(prepared.adoption_locators)?prepared.adoption_locators as unknown as LegacyOriginLocator[]:[];
        if(!candidates.length)return;
        const container=(locator:LegacyOriginLocator)=>locator.container_path??[locator.format==='codex_toml'?'mcp_servers':locator.format==='vscode_servers_json'?'servers':locator.format==='open_code_json'?'mcp':'mcpServers'];
        const files=new Map<string,{agent:LocalAgent;file:GlobalConfigFile;locators:LegacyOriginLocator[]}>(),needed:LegacyOriginLocator[]=[];
        for(const locator of candidates){
            const current=[...this.legacyMigrationFiles.values()].find(value=>same(value.file.destination,locator.path));
            if(!current){if(await readJsonConfig(locator.path)===undefined){this.migrationAbsentConfigurations.add(locator.path);continue;}throw new ServiceError('legacy_client_configuration_location_unverified');}
            if(!['codex','claude','copilot'].includes(current.agent))throw new ServiceError('http_client_compatibility_unverified');
            const row=current.file.declarations.find(row=>row.serverKey===locator.server&&isDeepStrictEqual(row.containerPath,container(locator)));
            if(!row||typeof row.fields.command!=='string')continue;
            const origin=(prepared.origin_proofs as unknown[]).map(record).find(value=>value.config_path===locator.path&&value.server===locator.server&&isDeepStrictEqual(value.container_path,container(locator)));
            if(!origin||!same(String(row.fields.command),String(origin.executable))||current.file.sha256!==origin.config_sha256)throw new ServiceError('legacy_original_configuration_changed',false,prepared);
            if(vscode.workspace.textDocuments.some(document=>same(document.uri.fsPath,locator.path)&&document.isDirty))throw new ServiceError('agent_configuration_has_unsaved_edits');
            const group=files.get(current.file.destination)??{...current,locators:[]};group.locators.push(locator);files.set(current.file.destination,group);needed.push(locator);
        }
        if(!needed.length)return;
        const storage=await fs.realpath(this.context.globalStorageUri.fsPath);check();
        const input=await publishNativeInput(binding.controlDir,JSON.stringify(needed),check,'native-input-legacy-batch-'+String(prepared.inspection_sha256)+'.json');let accepted:LegacyOriginLocator[];
        const stat=await fs.lstat(input);if(!stat.isFile()||stat.isSymbolicLink()||stat.nlink!==1||stat.size>1048576||!same(await fs.realpath(input),input))throw new ServiceError('legacy_batch_input_unconfirmed');accepted=JSON.parse(await fs.readFile(input,'utf8'));

        const allowed=new Set(candidates.map(legacyOriginLocatorIdentity));
        if(!Array.isArray(accepted)||!accepted.length||accepted.length>32||new Set(accepted.map(legacyOriginLocatorIdentity)).size!==accepted.length||accepted.some(locator=>!allowed.has(legacyOriginLocatorIdentity(locator)))||needed.some(locator=>!accepted.some(value=>legacyOriginLocatorIdentity(value)===legacyOriginLocatorIdentity(locator))))throw new ServiceError('legacy_batch_input_changed');
        const operation=createHash('sha256').update(JSON.stringify(['airk-standalone-batch/1',binding.installDir,binding.runtimeSha256,prepared.inspection_sha256])).digest('hex').slice(0,32);
        // The native owner chooses shared physical backings and retains the
        // complete frozen batch; all authority checks precede any config save.
        const batch=await this.runtime().adoptLegacyIndexes(binding,prepared,operation,input,accepted);check();
        this.output.appendLine('AIRK saved evidence adoption: '+JSON.stringify(batch));
        const mapped=(batch.claims as unknown[]).map(record),pending=new Map<string,GlobalConfigFile>();
        for(const [destination,entry]of files){
            let file=entry.file;
            if(await readJsonConfig(destination)!==entry.file.source)throw new ServiceError('agent_configuration_changed');check();
            for(const locator of entry.locators){
                const mapping=mapped.find(value=>legacyOriginLocatorIdentity(value.locator as unknown as LegacyOriginLocator)===legacyOriginLocatorIdentity(locator));if(!mapping||mapping.config_sha256!==entry.file.sha256)throw new ServiceError('legacy_original_configuration_changed');
                const original=record(mapping.original_registration),origin=String(mapping.origin_sha256),client=entry.agent==='copilot'?'vscode':entry.agent;
                const key=createHash('sha256').update(JSON.stringify(['standalone-http/1',binding.installDir,legacyOriginLocatorIdentity(locator),origin,client])).digest('hex'),id=key.slice(0,32),savedKey='airk.http.standalone.1.'+key;
                const prior=this.context.globalState.get<unknown>(savedKey);let info:HttpConnectionInfo;
                if(prior!==undefined){const old=validateHttpConnection(prior,undefined,path.join(binding.controlDir,'http-1'));info=validateHttpConnection(await this.runtime().httpConnectionInfo(binding,old.connection_id),old.selection,path.join(binding.controlDir,'http-1'));}
                else{const registrationFile=await publishNativeInput(binding.controlDir,JSON.stringify(original),check);try{check();info=await this.runtime().httpLegacyConnection(binding,original as unknown as {binding:string;root:string;data_root:string;collection:string},registrationFile,String(mapping.config_sha256),id,client,this.assistanceEnabled()?this.assistanceHost?.descriptorPath:undefined,origin);}finally{await removeNativeInput(registrationFile);}}
                check();if(info.selection.kind!=='imported_legacy'||!matchesImportedHttpOrigin(info.selection.selection,original as unknown as {binding:string;root:string;data_root:string;collection:string},String(mapping.config_sha256))||!isDeepStrictEqual(info.selection.selection.registration,mapping.registration)||!((info.selection.selection.grant.provenance.adoption.configuration_claims as Record<string,unknown>[]).some(claim=>claim.origin_sha256===origin&&claim.config_sha256===mapping.config_sha256)))throw new ServiceError('standalone_http_authority_changed');
                await this.provisionHttpInfo(info,client,check,storage);check();await this.context.globalState.update(savedKey,info);check();this.migrationHttpConnections.set(info.connection_id,info);
                const originalRow=entry.file.declarations.find(row=>row.serverKey===locator.server&&isDeepStrictEqual(row.containerPath,container(locator)));if(!originalRow)throw new ServiceError('legacy_original_configuration_changed');
                const after=transformGlobalContainer(file,container(locator),source=>migrateStandaloneHttpEntry(entry.agent,source,locator.server,originalRow.fields,info,String(mapping.config_sha256),origin,container(locator)));
                file={...file,source:after,declarations:globalConfigDeclarations(file.agent,after)};
            }
            pending.set(destination,file);
        }
        // One CAS per complete original file, preserving distinct entries and
        // policies even when several claims share one native publication.
        for(const [destination,file]of pending){const before=files.get(destination)!.file.source,saved=await this.saveMigrationConfiguration(destination,before,file.source,check);check();observe(destination,saved);this.migratedConnections=true;this.output.appendLine('Updated '+destination);}
    }
    private migrationClient(binding:RuntimeBinding){return binding.version===this.runningVersion?Promise.resolve(this.runtime().client(binding)):this.runtime().migrationClient(binding);}
    private async migrateAgentConnections(binding:RuntimeBinding,restored:readonly LaunchSpec[]=[]):Promise<void>{
        this.trusted();
        const generation=this.generation,host=this.host,folders=this.folders().map(folder=>folder.uri.fsPath);
        this.legacyCachePreparation=undefined;this.migrationAbsentConfigurations.clear();this.migrationHttpConnections=new Map();this.migrationGlobalVerifier=undefined;this.httpOwnershipLookups.clear();
        this.legacyLauncherInspection=await this.runtime().inspectLegacyLaunchers(binding);this.trusted();
        if(generation!==this.generation||host!==this.host)throw new ServiceError('workspace_registration_changed');
        const known=[...new Set([...folders,...this.workspace.rememberedRoots(binding)])];
        const specs=await this.workspace.definitions(binding,known),failures:string[]=[],snapshots=new Map<string,string>();
        const observed=(destination:string,source:string,terminal=true)=>{
            const agent=destination.endsWith('.toml')?'codex':path.basename(path.dirname(destination))==='.vscode'?'copilot':'claude';
            const rows=globalConfigDeclarations(agent,source);
            if(terminal&&rows.some(row=>this.legacyCommandKnown(row.fields.command,this.migrationCommandPaths(binding,specs))))throw new ServiceError('agent_http_migration_incomplete',false,{destination,reason:'legacy_owned_declaration_remains'});
            snapshots.set(destination,createHash('sha256').update(source).digest('hex'));
        };
        const inventoryClient=await this.migrationClient(binding);let inventory:Awaited<ReturnType<typeof readWorkspaceRegistrations>>;
        try{inventory=await readWorkspaceRegistrations(binding,async(after,revision,inactive)=>inventoryClient.projects(after,revision,32,inactive));}finally{inventoryClient.dispose();}
        this.trusted();if(generation!==this.generation||host!==this.host)throw new ServiceError('workspace_registration_changed');
        for(const row of [...inventory.active,...inventory.inactive])if(!specs.some(spec=>spec.service!.project===row.binding))specs.push({command:binding.executable,root:row.root,dataDir:row.data_root,collection:row.collection,name:row.collection,service:{installDir:binding.installDir,project:row.binding,direct:true}});
        const preparationCurrent=()=>{this.trusted();if(generation!==this.generation||host!==this.host)throw new ServiceError('workspace_registration_changed');};
        // Capture original cache/config preimages before either legacy migration
        // or a modern endpoint-only rewrite saves any configuration bytes.
        await this.prepareLegacyCacheMigration(binding,[...known,...specs.map(spec=>spec.root)],preparationCurrent);
        await this.adoptStandaloneConfigurations(binding,preparationCurrent,(destination,source)=>observed(destination,source,false));
        const localFiles=new Set<string>();
        for(const root of new Set([...known,...specs.map(spec=>spec.root)]))for(const [agent,parts]of Object.entries(localAgentLocations)){
            const destination=path.join(root,...parts);if(localFiles.has(destination))continue;localFiles.add(destination);
            const source=await readJsonConfig(destination);preparationCurrent();if(source===undefined)continue;
            const changed=await this.migrateGeneratedServiceConfiguration(binding,agent as LocalAgent,source,specs,restored,preparationCurrent,this.originalMigrationReceipt(destination,createHash('sha256').update(source).digest('hex')),destination);preparationCurrent();
            if(!changed.owned)continue;
            const saved=changed.source===source?source:await this.saveMigrationConfiguration(destination,source,changed.source,preparationCurrent);
            observed(destination,saved);if(saved!==source){this.migratedConnections=true;this.output.appendLine('Updated '+destination);}
        }
        if(failures.length)throw new ServiceError('agent_http_migration_incomplete',false,{failures});
        const current=()=>{this.trusted();if(generation!==this.generation||host!==this.host)throw new ServiceError('workspace_registration_changed');};
        await this.migrateGlobalAgentConnections(binding,specs,restored,current,observed);
        this.migratedConfigurationSnapshots=snapshots;await this.verifyMigrationSnapshots();
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
    /** Runtime administration remains separate from evidence tools and SessionStore. */
    async reviewLegacyUpgrade():Promise<void>{
        this.trusted();const generation=this.generation;let stopped=false;
        const current=()=>{this.trusted();if(stopped||generation!==this.generation)throw new ServiceError('legacy_retirement_review_stopped');};
        const session=this.legacyRetirement??=new LegacyRetirementSession(this.runtime(),()=>{
            try{this.trusted();return generation===this.generation;}catch{return false;}
        });
        return vscode.window.withProgress({location:vscode.ProgressLocation.Notification,title:'AIRK legacy upgrade retirement',cancellable:true},async(_progress,token)=>{
        const cancellation=token.onCancellationRequested(()=>{stopped=true;session.stop();});
        try{
        if(!session.last){
            let after:string|undefined;
            for(;;){
            const candidates=await session.candidates(after);current();
            if(!Array.isArray(candidates.items))throw new ServiceError('invalid_legacy_retirement_candidates');
            const items:{label:string;description:string;row:Record<string,unknown>;saved?:unknown;next?:string}[]=candidates.items.map(value=>{
                const row=record(value),registration=record(row.registration);
                return {label:String(registration.root),description:row.observation===null?String(row.index_id):'Check saved native retirement receipt',row,saved:row.observation??undefined};
            });
            if(typeof candidates.continuation==='string'){
                items.push({label:'Next page',description:'Explicitly retrieve the next bounded subset of saved pending-metadata indexes',row:{},next:candidates.continuation});
            }
            const selected=await vscode.window.showQuickPick(items,{title:'AIRK: Review unavailable legacy index retirement',
                placeHolder:`${candidates.items.length} shown of ${candidates.total_eligible} saved pending-metadata indexes. Sources and physical storage are preserved.`});
            current();if(!selected){session.stop();return;}
            if(selected.next){after=selected.next;continue;}
            if(selected.saved)session.restore(selected.saved);else await session.prepare(String(selected.row.index_id));current();
            break;
            }
        }
        for(;;){
            current();const reply=session.last;if(!reply)throw new ServiceError('legacy_retirement_review_missing');
            const actions=['Show complete details','Copy JSON','Copy Markdown','Export JSON','Export Markdown'];
            if(!session.uncertainOrAccepted)actions.push('Authorize reviewed retirement');
            else actions.push('Check exact receipt');
            if(['authorized','applied'].includes(String(reply.status)))actions.push('Resume accepted same-candidate activation');
            actions.push('Stop');
            const action=await vscode.window.showQuickPick(actions,{title:`AIRK legacy retirement: ${reply.status}`,
                placeHolder:'Logical retirement, physical cleanup and activation readiness are separate. Recovery data stays in memory only.'});
            current();if(!action||action==='Stop'){session.stop();return;}
            if(action==='Show complete details'){
                const document=await vscode.workspace.openTextDocument({language:'json',content:retirementJson(reply)});current();
                await vscode.window.showTextDocument(document);continue;
            }
            if(action==='Copy JSON'||action==='Copy Markdown'){
                await vscode.env.clipboard.writeText(action==='Copy JSON'?retirementJson(reply):retirementMarkdown(reply));continue;
            }
            if(action==='Export JSON'||action==='Export Markdown'){
                const target=await vscode.window.showSaveDialog({title:'Export complete native legacy retirement receipt',
                    filters:action==='Export JSON'?{JSON:['json']}:{Markdown:['md']}});current();
                if(target)await vscode.workspace.fs.writeFile(target,Buffer.from(action==='Export JSON'?retirementJson(reply):retirementMarkdown(reply)));
                continue;
            }
            try{
                if(action==='Authorize reviewed retirement'){
                    const yes=await vscode.window.showWarningMessage('Retire only this reviewed unavailable legacy index and cancel its exact accepted work?',
                        {modal:true,detail:'Saves this exact native authorization only. Sources and physical storage are preserved. Physical cleanup remains pending. Resume is a separate explicit action; this confirmation does not start activation.'},'Authorize retirement');
                    current();if(yes!=='Authorize retirement')continue;
                    await session.authorize(reply);current();
                }else if(action==='Check exact receipt'){
                    const saved=await session.reconcile();current();
                    if(saved===null)await vscode.window.showWarningMessage('The exact receipt is unavailable. The previous full observation is retained; no authorization or job was replayed.');
                }else if(action==='Resume accepted same-candidate activation'){
                    const yes=await vscode.window.showWarningMessage('Resume this already accepted retirement through the same verified candidate?',
                        {modal:true,detail:'This continues native activation. It creates no new retirement authorization and does not infer readiness from the logical receipt.'},'Resume accepted activation');
                    current();if(yes==='Resume accepted activation'){await session.resume();current();}
                }
            }catch(error){
                current();this.output.appendLine(String(error));
                await vscode.window.showWarningMessage('The native outcome may be incomplete or unknown. Check the exact saved receipt; authorization is never replayed automatically.');
            }
        }
        }finally{cancellation.dispose();}
        });
    }
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
        this.installationFailed=false;this.installationWarnings=false;
        let runtimeInstalled=false,finalized=false;
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
                    &&!installed.diagnosis?.pending_activation&&this.context.globalState.get<string>(`airk.installComplete.2.${binding.installDir}`)===identity){
                    runtimeInstalled=true;
                    if(installed.inspection.lifecycle&&record(installed.inspection.lifecycle).explicitly_stopped===true)throw new ServiceError('service_explicitly_stopped');
                    // A completed installation does not prove that its on-demand service survived editor/OS shutdown.
                    const connected=await this.runtime().reconnect(installed);this.trusted();
                    if(connected.runtimeSha256!==binding.runtimeSha256||!same(connected.controlDir,binding.controlDir))throw new ServiceError('runtime_selection_changed_review_again');
                    const generation=this.generation;
                    await this.prepareWorkspaceProjects(binding);
                    await this.finalizeLegacyLauncherRemoval(binding);finalized=true;
                    await this.refresh();
                    this.reportInstall('Connected to AIRK. Saved project work remains with the service.');
                    this.setupSucceeded(binding,generation);
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
                    if(current.launcher===null&&current.payloadVerified!==false){
                        // A fully occupied approved installation must finish its
                        // preceding modern transaction before staging another.
                        await this.migrateAgentConnections(current,this.migrationPredecessors);
                        await this.finalizeLegacyLauncherRemoval(current,false,true);
                    }
                    this.reportUpgrade('checking',`Upgrading ${current.version} → ${target}… Checking the installed runtime and saved indexes.`,current.version);
                    // Native bounded staging retains every approved predecessor until
                    // recognized clients have migrated and final removal is proven.
                    this.migrationPredecessors=await this.workspace.definitions(current,[...new Set([...this.folders().map(folder=>folder.uri.fsPath),...this.workspace.rememberedRoots(current)])]);
                    const review=await this.runtime().reviewTransition(current,installed.diagnosis?.pending_activation?'resume':'upgrade');this.trusted();
                    await this.runtime().transition(current,review,(message,stage)=>this.reportUpgrade(stage,message));this.trusted();
                }
            }
            await this.installCommandLine();this.trusted();
            // Activation/retirement can replace the incarnation behind an older metadata pipe.
            this.console.reset();
            this.reportUpgrade('reconnecting','Connecting to the updated runtime and loading saved index capabilities…');
            const binding=await this.runtime().ensure();this.trusted();
            runtimeInstalled=true;
            const completed=await this.runtime().discover();
            const generation=this.generation;
            await this.prepareWorkspaceProjects(binding);await this.finalizeLegacyLauncherRemoval(binding,true,true);finalized=true;await this.refresh();
            this.reportInstall(this.installationWarnings?'Runtime installation finished with issues to review.':'Runtime ready. Set up new indexes explicitly in Indexing; existing native work remains in Management.');
            this.setupSucceeded(binding,generation);
            await this.openWorkbenchAfterSetup();
            if(notify)await this.installationComplete();
            },progressTitle);
        }
        catch(error){
            if(this.disposed)return;
            this.reportUpgrade('failed',`AIRK upgrade needs attention: ${error instanceof Error?error.message:String(error)}. Review Management → Service and Logs.`);
            if(setupObservation)void recordDiagnosticFailure(setupObservation,'automatic_install',error)
                .catch(failure=>{if(!this.disposed)this.output.appendLine(`Could not retain installation diagnostic: ${String(failure)}`);});
            if(error instanceof ServiceError&&error.code==='extension_reload_required')return;
            if(String(error).includes('service_explicitly_stopped')){
                this.output.appendLine('AIRK service_explicitly_stopped. Start the service from Management > Service when ready. Saved project work is retained.');return;
            }
            if(runtimeInstalled&&!finalized){
                this.installationFailed=true;this.installationWarnings=true;
                this.reportInstall('AIRK upgrade is incomplete. Old launcher removal or HTTPS configuration needs attention; review AIRK output and retry Install or Repair CLI.');
                this.output.appendLine('AIRK upgrade completion blocked: '+String(error));
                const details=error instanceof ServiceError?error.nativeResponse:undefined;
                if(details)this.output.appendLine('AIRK upgrade completion details: '+JSON.stringify(details));
                const guidance=details&&typeof details.guidance==='string'?details.guidance:String(error);
                void vscode.window.showErrorMessage('AIRK upgrade did not complete: '+guidance);return;
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
            this.renderSetupFailed();
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
        await this.migrateAgentConnections(binding,this.migrationPredecessors);
        await this.context.workspaceState.update('airk.projectStartupNotice.1',undefined);
    }
    async reviewHttpTrust(remove=false):Promise<Record<string,unknown>|undefined>{
        this.trusted();const binding=this.binding,generation=this.generation;if(!binding)throw new ServiceError('native_service_connection_required');
        const current=()=>{this.trusted();if(binding!==this.binding||generation!==this.generation)throw new ServiceError('workspace_registration_changed');};
        const observed=await this.runtime().httpTrust(binding,'inspect');current();
        this.output.appendLine('AIRK HTTPS issuer trust: '+JSON.stringify(observed));
        if(observed.outcome==='unsupported'){void vscode.window.showWarningMessage('Automatic per-user VS Code trust is unavailable on this platform. Linux VS Code requires the reviewed issuer in the distro system store and user NSS store, installed by an administrator. Keep TLS verification enabled. '+String(observed.guidance));return observed;}
        if(observed.outcome==='outcome_unknown')throw new ServiceError(String(observed.outcome),false,observed);
        if(remove&&observed.owned!==true)throw new ServiceError('http_issuer_trust_not_owned',false,observed);
        if(!remove&&observed.trusted===true)return observed;
        const action=remove?'Remove owned trust':'Trust for this user';
        const accepted=await vscode.window.showWarningMessage(`${remove?'Remove':'Trust'} AIRK's local HTTPS issuer in ${String(observed.store)}? Certificate SHA256: ${String(observed.ca_sha256)}.`,{modal:true},action);current();
        if(accepted!==action)return;
        const result=await this.runtime().httpTrust(binding,remove?'remove':'install',String(observed.ca_sha256));current();
        this.output.appendLine('AIRK HTTPS issuer trust result: '+JSON.stringify(result));this.definitions.fire();
        void vscode.window.showInformationMessage(String(result.guidance));return result;
    }
    async httpIssuerRotationStatus():Promise<IssuerRotationStatus>{
        this.trusted();const binding=this.binding,generation=this.generation;if(!binding)throw new ServiceError('native_service_connection_required');
        const result=await this.runtime().httpIssuerRotation(binding,'status');this.trusted();if(binding!==this.binding||generation!==this.generation)throw new ServiceError('workspace_registration_changed');
        const document=await vscode.workspace.openTextDocument({content:JSON.stringify(result,null,2),language:'json'});await vscode.window.showTextDocument(document,{preview:true});return result;
    }
    async reviewHttpIssuerRotation():Promise<Record<string,unknown>|undefined>{
        this.trusted();const binding=this.binding,generation=this.generation;if(!binding)throw new ServiceError('native_service_connection_required');
        const current=()=>{this.trusted();if(binding!==this.binding||generation!==this.generation)throw new ServiceError('workspace_registration_changed');};
        const runtime=this.runtime(),observed=await runtime.httpIssuerRotation(binding,'status');current();
        const options=[{label:'Open complete native rotation receipt',action:'status'},...(!observed.rotation||['finalized','aborted'].includes(observed.rotation.phase)?[{label:'Prepare a new issuer rotation',action:'prepare'},...(observed.rotation?[{label:'Finish this window’s terminal rotation bundle cleanup',action:'cleanup'}]:[])]:[
            {label:'Prepare old/new trust in this window’s owned client bundles',action:'bundles'},
            {label:'Trust the staged issuer for this OS user',action:'trust-new'},
            {label:'Activate the reviewed new issuer',action:'commit'},
            {label:'Finalize expired overlap and retire owned old trust',action:'finalize'},
            ...(observed.rotation.phase==='prepared'?[{label:'Abort the prepared rotation and retain the old issuer',action:'abort'}]:[]),
        ])];
        const picked=await vscode.window.showQuickPick(options,{title:'AIRK HTTPS issuer rotation'});current();if(!picked)return;
        if(picked.action==='status')return this.httpIssuerRotationStatus();
        const rotation=observed.rotation,detail=rotation?`Operation ${rotation.operation_id}. Old CA ${rotation.old_ca_sha256}. New CA ${rotation.new_ca_sha256}. Native phase ${rotation.phase}.`:`Current CA ${observed.current_endpoint?.ca_sha256}. Descriptor ${observed.current_descriptor_sha256}.`;
        const descriptions:Record<string,string>={
            prepare:'Stage a new local issuer without activating it. Ordinary overlap is 24 hours; existing grants, credentials and selections stay unchanged.',
            bundles:'Add the staged issuer to this window’s exact AIRK-owned Codex/Claude trust bundles. Preserve the old issuer and unrelated roots. Existing external hosts and other windows are not acknowledged by this action.',
            'trust-new':'Install only the reviewed staged issuer in this user’s supported OS trust store. Preexisting trust remains separately owned.',
            commit:'Stop/drain and restart the selected native service with the reviewed issuer. Prepare client and OS trust first. Active requests can disconnect; reconnect and verify each client. Unobserved hosts remain pending.',
            finalize:'The native overlap must have expired. Remove only recorded owned old OS trust, then update this window’s owned bundles. Other windows and external trust are not proven cleaned.',
            cleanup:'Retry only this window’s exact owned bundle cleanup for a finalized or aborted native transaction. Preserve unrelated roots and unknown client configuration.',
            abort:'Abort only the prepared native transaction. Preserve the old issuer; remove only owned staged-new trust and this window’s staged certificate. Activation that began cannot be aborted here.',
        };
        const accepted=await vscode.window.showWarningMessage(descriptions[picked.action]+' '+detail,{modal:true},'Apply reviewed rotation action');current();if(accepted!=='Apply reviewed rotation action')return;
        let result=observed;
        if(picked.action==='bundles'||picked.action==='cleanup'){
            if(!rotation)throw new ServiceError('http_rotation_review_required');
            const verified=await runtime.httpIssuerRotation(binding,'status');current();if(verified.receipt_sha256!==observed.receipt_sha256)throw new ServiceError('http_rotation_review_changed');
            let updates;
            try{updates=await updateOwnedIssuerBundles(await fs.realpath(this.context.globalStorageUri.fsPath),this.context.environmentVariableCollection,rotation,picked.action==='cleanup'?(rotation.phase==='aborted'?'abort':'finalize'):'overlap',vscode.EnvironmentVariableMutatorType.Replace,current);current();}
            catch(error){const failure={...verified,client_cleanup_error:error instanceof Error?error.message:'unknown',...(error instanceof ClientTrustPublicationError?{client_bundle_updates:error.updates,published_variables:error.published}:{})};this.output.appendLine('AIRK HTTPS issuer bundles require review: '+JSON.stringify(failure));throw new ServiceError('http_rotation_client_bundle_requires_review',error instanceof ClientTrustPublicationError,failure);}
            result={...verified,client_bundle_updates:updates,client_scope:'this_window_only',external_clients:'pending_actual_reconnect_and_read'};
        }else{
            const action=picked.action as 'prepare'|'trust-new'|'commit'|'finalize'|'abort';
            result=await runtime.httpIssuerRotation(binding,action,observed,action==='prepare'?randomBytes(16).toString('hex'):undefined);current();
            if((action==='finalize'&&result.rotation?.phase==='finalized')||(action==='abort'&&result.rotation?.phase==='aborted')){
                try{const updates=await updateOwnedIssuerBundles(await fs.realpath(this.context.globalStorageUri.fsPath),this.context.environmentVariableCollection,result.rotation!,action==='abort'?'abort':'finalize',vscode.EnvironmentVariableMutatorType.Replace,current);result={...result,client_bundle_updates:updates,client_scope:'this_window_only'};}
                catch(error){const failure={...result,client_cleanup_error:error instanceof Error?error.message:'unknown',...(error instanceof ClientTrustPublicationError?{client_bundle_updates:error.updates,published_variables:error.published}:{})};this.output.appendLine('AIRK HTTPS issuer cleanup requires review: '+JSON.stringify(failure));throw new ServiceError('http_rotation_client_cleanup_requires_review',error instanceof ClientTrustPublicationError,failure);}
            }
        }
        if(picked.action==='commit'&&result.rotation?.phase==='activated'){
            try{await this.migrateAgentConnections(binding);current();result={...result,configuration_updates:'guarded_owned_config_migration_attempted',external_clients:'pending_actual_reconnect_and_read'};}
            catch(error){result={...result,configuration_updates:'requires_review',configuration_error:error instanceof Error?error.message:'unknown',external_clients:'pending_actual_reconnect_and_read'};}
        }
        this.output.appendLine('AIRK HTTPS issuer rotation: '+JSON.stringify(result));this.definitions.fire();
        const document=await vscode.workspace.openTextDocument({content:JSON.stringify(result,null,2),language:'json'});await vscode.window.showTextDocument(document,{preview:true});return result;
    }
    private async vscodeCredentialSources(check:()=>void):Promise<{file:string;before:string;root?:string}[]>{
        check();const sources:{file:string;before:string;root?:string}[]=[];
        const add=(file:string,before:string,root?:string)=>{
            const previous=sources.find(value=>same(value.file,file));
            if(previous){if(previous.before!==before)throw new ServiceError('http_credential_configuration_changed');return;}
            sources.push({file,before,...(root?{root}:{})});
        };
        const folders=this.folders();if(folders.length>32)throw new ServiceError('http_credential_workspace_capacity');
        for(const folder of folders){
            const root=await fs.realpath(folder.uri.fsPath),directory=path.join(root,'.vscode');check();
            const stat=await fs.lstat(directory).catch(error=>{if((error as NodeJS.ErrnoException).code==='ENOENT')return;throw error;});
            if(!stat)continue;if(!stat.isDirectory()||stat.isSymbolicLink()||!same(await fs.realpath(directory),directory))throw new ServiceError('unsafe_http_credential_configuration');
            const file=path.join(directory,'mcp.json'),before=await readJsonConfig(file);check();if(before!==undefined)add(file,before,root);
        }
        // Use the same native user/profile inventory as guarded migration. A
        // command argument never chooses a file, profile, or credential path.
        const inventory=await inventoryGlobalAgentConfigs({home:os.homedir(),env:{...process.env},globalStoragePath:this.context.globalStorageUri.fsPath,check});check();
        if(inventory.blockers.length)throw new ServiceError('http_credential_configuration_inventory_unavailable',false,{blockers:inventory.blockers});
        for(const file of inventory.files.filter(file=>file.agent==='copilot')){
            const before=await readJsonConfig(file.destination);check();
            if(before===undefined||before!==file.source)throw new ServiceError('http_credential_configuration_changed');add(file.destination,before);
        }
        if(sources.length>96||sources.reduce((total,value)=>total+Buffer.byteLength(value.before),0)>8*1024*1024)throw new ServiceError('http_credential_configuration_capacity');
        return sources;
    }
    /** VS Code input-command resolution, never a generic file/secret reader.
     * The native getter validates the live grant, expiry, verifier and file ACL;
     * only the matching current workspace configuration receives the token. */
    async httpCredential(input:unknown):Promise<string>{
        this.trusted();const value=record(input);if(Object.keys(value).join(',')!=='connection_id'||typeof value.connection_id!=='string'||!/^[a-f0-9]{32}$/.test(value.connection_id))throw new ServiceError('invalid_http_credential_input');
        if(!this.currentBinding())await this.refresh();this.trusted();const binding=this.binding,generation=this.generation;
        if(!binding)throw new ServiceError('native_service_connection_required');
        const current=()=>{this.trusted();if(binding!==this.binding||generation!==this.generation)throw new ServiceError('workspace_registration_changed');};
        const info=await this.runtime().httpConnectionInfo(binding,value.connection_id);current();
        const sources=await this.vscodeCredentialSources(current),matches=sources.filter(value=>vscodeCredentialConfiguration(value.before,info));
        if(matches.length!==1)throw new ServiceError('http_credential_requires_matching_workspace_configuration');
        const material=await httpMaterial(info);current();
        const after=await this.vscodeCredentialSources(current);current();
        if(!isDeepStrictEqual(after,sources))throw new ServiceError('http_credential_configuration_changed');
        return material.credential;
    }
    async manageVscodeMcp():Promise<void>{this.trusted();await vscode.commands.executeCommand('workbench.mcp.listServer');}
    private async hasManualVscodeConnection():Promise<boolean>{
        // The file owns client policies, including explicit disable. Do not bypass
        // it by registering a second copy through the extension provider.
        const binding=this.binding,generation=this.generation;
        const check=()=>{this.trusted();if(binding!==this.binding||generation!==this.generation)throw new ServiceError('workspace_registration_changed');};
        const sources=await this.vscodeCredentialSources(check);
        for(const {root,before:source}of sources){
            try{
                const selected=this.currentCatalogScope()?.spec,spec=root?(selected&&same(selected.root,root)?selected:this.specs.find(spec=>same(spec.root,root))):undefined;
                if(root&&spec){const known=spec.http??this.previousLegacyHttp(spec,'vscode');if(hasUnifiedVscodeConfig(known?{...spec,http:known}:spec,this.context.globalStorageUri?.fsPath??'',source))return true;}
                for(const id of vscodeManualConnectionIds(source))if(binding){try{const info=await this.runtime().httpConnectionInfo(binding,id);check();if(vscodeManualConfiguration(source,info))return true;}catch{check();}}
            }catch{/* Unsafe/custom configurations remain for review; never execute them here. */}
        }
        return false;
    }
    async provideDefinitions():Promise<vscode.McpHttpServerDefinition[]>{
        if(this.checkForExtensionUpdate())return [];
        if(!vscode.workspace.isTrusted||!this.registrationEnabled()||!nativeExecutionAllowed())return [];
        if(!this.currentBinding())await this.refresh();if(!vscode.workspace.isTrusted||!this.registrationEnabled())return [];
        if(await this.hasManualVscodeConnection())return [];
        const observation=await this.workspaceConnection();if(!observation)return [];
        this.trusted();if(observation.generation!==this.generation||observation.identity!==this.console.controller.observationIdentity||!this.registrationEnabled())throw new ServiceError('workspace_registration_changed');const entry=observation.entry;
        if(!entry.http)throw new ServiceError('http_connection_requires_reviewed_scope');
        const material=await httpMaterial(entry.http);this.trusted();
        if(observation.generation!==this.generation||observation.identity!==this.console.controller.observationIdentity)throw new ServiceError('workspace_registration_changed');
        return [new vscode.McpHttpServerDefinition('AIRK',vscode.Uri.parse(material.url),{Authorization:'Bearer '+material.credential,'X-AIRK-Response-Mode':this.responseMode()},`${this.runningVersion}:${entry.http.connection_id}:${entry.http.endpoint.generation}:${this.responseMode()}`)];
    }
    async resolveDefinition(definition:vscode.McpServerDefinition):Promise<vscode.McpServerDefinition>{
        this.trusted();if(!this.registrationEnabled())throw new ServiceError('mcp_registration_disabled');
        await this.refresh();this.trusted();
        if(await this.hasManualVscodeConnection())throw new ServiceError('manual_mcp_configuration_owns_connection');
        const observation=await this.workspaceConnection(),entry=observation?.entry;
        this.trusted();if(observation&&(observation.generation!==this.generation||observation.identity!==this.console.controller.observationIdentity))throw new ServiceError('workspace_registration_changed');
        if(!(definition instanceof vscode.McpHttpServerDefinition)||!observation||!entry?.http)throw new ServiceError('workspace_registration_changed');
        if(entry.http.endpoint.url!==definition.uri.toString()){this.definitions.fire();throw new ServiceError('http_endpoint_changed_refresh_definition');}
        const selected=this.currentCatalogScope();
        await this.runtime().ensure();this.trusted();if(!this.registrationEnabled())throw new ServiceError('mcp_registration_disabled');
        if(observation.generation!==this.generation||observation.identity!==this.console.controller.observationIdentity||selected!==this.currentCatalogScope())throw new ServiceError('workspace_registration_changed');
        const material=await httpMaterial(entry.http);this.trusted();
        return new vscode.McpHttpServerDefinition('AIRK',vscode.Uri.parse(material.url),{Authorization:'Bearer '+material.credential,'X-AIRK-Response-Mode':this.responseMode()},`${this.runningVersion}:${entry.http.connection_id}:${entry.http.endpoint.generation}:${this.responseMode()}`);
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
    explorerHandoff():{open:boolean;route?:string}{return {open:this.explorerLoaded?.panelOpen??false,route:this.explorerLoaded?.assistanceStatus.active_route};}
    async prepareForUpdate():Promise<void>{const explorer=this.explorerLoaded;this.dispose();await Promise.allSettled([...this.httpSetups.values()]);await Promise.all([this.workbench?.finishPendingSaves(),explorer?.finishPendingNoticeWrites()]);}
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
            prepareEvidenceConnection:async(spec,client='airk-explorer')=>{
                this.trusted();const active=this.currentCatalogScope()?.spec;
                if(!active){if(spec.catalogSession||spec.combinedSession||!this.isCurrent(spec))throw new ServiceError('http_connection_requires_reviewed_scope');return this.resolveAgentSpec(spec,client);}
                if(spec.combinedSession&&active.combinedSession){
                    const {index_ids:indexes,expected_catalog_revision:requestedRevision,...requested}=spec.combinedSession;
                    const {index_ids:allowed,expected_catalog_revision:originalRevision,...original}=active.combinedSession;
                    if(requestedRevision<originalRevision||!isDeepStrictEqual(requested,original)||allowed&&(!indexes||indexes.some(id=>!allowed.includes(id))))throw new ServiceError('http_connection_selection_changed');
                }else if(!this.isCurrent(spec))throw new ServiceError('http_connection_selection_changed');
                return this.enrollHttpSpec(spec,client);
            },
            extensionUpdate:()=>this.extensionUpdate,reloadUpdatedExtension:()=>this.reloadUpdatedExtension(),
            connectionCheck:()=>this.connectionCheck,startup:()=>this.startup,runtimeUpgrade:()=>this.runtimeUpgrade,
            workbenchVisible:()=>this.workbench?.visible??false,toggleWorkbench:async()=>{await this.workbench?.toggle();},
            onDidChangeWorkbenchVisibility:this.workbench?.onDidChangeVisibility,
            trusted:()=>vscode.workspace.isTrusted&&!this.disposed&&!this.extensionUpdate,workspaceTrusted:()=>vscode.workspace.isTrusted,nativeExecutionEnabled:()=>nativeExecutionAllowed(),specs:()=>this.uiSpecs(),
            windowContext:()=>this.currentWindowContext()?.descriptor,windowContextObservation:()=>this.currentWindowContext()?.response,
            windowContextIdentity:()=>JSON.stringify([this.windowIdentity().identity,this.generation]),
            prepareWindowContext:()=>this.prepareWindowContext(),onDidChangeWindowContext:this.windowContextChanges.event,
            useGrant:(choice,namespace,revision,valid)=>this.useGrant(choice,namespace,revision,valid),
            searchScopeSelection:()=>this.searchScopeSelection(),applySearchScope:selection=>this.applySearchScope(selection),saveIndexSet:selection=>this.saveIndexSet(selection),
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
            setStatusBar:(text,tooltip)=>{if(!this.disposed&&!this.renderUpdateStatus()&&!this.renderSetupFailed()){
                if(!nativeExecutionAllowed())this.renderNativeDisabled();else{this.status.text=text;this.status.tooltip=tooltip;}
            }},workspaceState:this.context.workspaceState,suggestions:this.suggestions,handoffs:this.handoffs};
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
        try{await writeLaunchDescriptor(storage,Reflect.get(this.context,Symbol.for('iokaio.airk.managedAssetRoot'))??this.context.extensionPath,version,undefined,
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
        if(selected){const identity=this.console.controller.observationIdentity,generation=this.generation,spec=await this.enrollHttpSpec(selected.spec,'vscode');this.trusted();if(generation!==this.generation||selected!==this.currentCatalogScope()||identity!==this.console.controller.observationIdentity)throw new ServiceError('workspace_registration_changed');return {entry:unifiedEntry(spec),identity,generation};}
        if(this.explicitCatalogScope||this.hasScopeWithdrawal())return undefined;
        const specs=[...this.specs],binding=this.binding,generation=this.generation,controller=this.console.controller;
        if(!binding||!specs.length)return undefined;
        if(specs.length>32)throw new ServiceError('legacy_mcp_binding_capacity_exceeded');
        const current=()=>{this.trusted();if(binding!==this.binding||generation!==this.generation||controller!==this.console.controller||this.explicitCatalogScope||this.hasScopeWithdrawal()||specs.length!==this.specs.length||!specs.every(spec=>this.isCurrent(spec)))throw new ServiceError('workspace_registration_changed');};
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
        current();const first=resolved.length>1?await this.enrollLegacyUnion(resolved,binding,current):await this.resolveAgentSpec(resolved[0],'vscode');current();return {entry:unifiedEntry(first),identity:controlIdentity,generation};
    }
    async verifyAssistance(spec:LaunchSpec):Promise<string[]>{
        this.trusted();
        const entry=this.connectionEntries(spec)[0];
        await writeLaunchDescriptor(this.context.globalStorageUri.fsPath,Reflect.get(this.context,Symbol.for('iokaio.airk.managedAssetRoot'))??this.context.extensionPath,this.runningVersion,undefined,()=>this.assistanceEnabled());
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
    reset():void{this.closeHttpEndpointWatcher();this.localScopeEpoch++;this.legacyRetirement?.stop();this.legacyRetirement=undefined;this.generation++;this.windowContextValue=undefined;this.publishNativeCommandAvailability();this.host=undefined;this.selectedEligible=undefined;this.selectedScope=undefined;this.specs=[];this.binding=undefined;this.checks.clear();this.checking=undefined;this.connectionCheck=undefined;this.console.reset();void this.refresh();}
    dispose():void{if(this.disposed)return;this.localScopeEpoch++;this.legacyRetirement?.stop();this.legacyRetirement=undefined;this.host=undefined;this.disposed=true;this.closeHttpEndpointWatcher();clearTimeout(this.httpRefreshTimer);this.httpRefreshTimer=undefined;this.generation++;this.windowContextValue=undefined;this.publishNativeCommandAvailability();this.windowContextChanges.dispose();this.assistanceGeneration++;this.assistanceHost?.dispose();this.assistanceDefinitions.dispose();void this.explorer?.then(explorer=>explorer.dispose(),()=>undefined);this.console.dispose();this.status.dispose();this.definitions.dispose();this.treeChanges.dispose();this.output.dispose();}
}

let active:ServiceExtension|undefined;
export function deactivate():void{active?.dispose();active=undefined;}
/** Activated by the final 1.4 adapter cutover, after native/package contract checks. */
export async function activate(context:vscode.ExtensionContext):Promise<{getServerDefinitions():Promise<vscode.McpServerDefinition[]>;whenReady():Promise<void>;explorerHandoff():{open:boolean;route?:string};prepareForUpdate():Promise<void>}>{
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
    commands.httpCredential=input=>controller.httpCredential(input);
    commands.trustHttpIssuer=()=>controller.reviewHttpTrust();
    commands.removeHttpIssuerTrust=()=>controller.reviewHttpTrust(true);
    commands.httpIssuerRotationStatus=()=>controller.httpIssuerRotationStatus();
    commands.rotateHttpIssuer=()=>controller.reviewHttpIssuerRotation();
    commands.enableNativeExecution=()=>controller.enableNativeExecution();
    commands.manageWorkspaceTrust=(...args)=>{if(args.length)throw new ServiceError('invalid_manage_workspace_trust_arguments');return controller.manageWorkspaceTrust();};
    commands.setUp=()=>controller.repairInstallation();
    commands.removeTerminalCommand=()=>controller.removeTerminalCommand();
    commands.showLastUninstallResult=()=>controller.showLastUninstallResult();
    commands.reviewData=()=>controller.reviewData();
    commands.reviewLegacyUpgrade=()=>controller.reviewLegacyUpgrade();
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
    // Watch extension assets only: source watching remains native-owned. A VSIX
    // replacement can happen while Explorer stays visible and the window focused.
    if(vscode.RelativePattern&&vscode.workspace.createFileSystemWatcher){
        const watcher=vscode.workspace.createFileSystemWatcher(new vscode.RelativePattern(context.extensionUri,'out/*.{js,css}'));
        const changed=()=>controller.checkForExtensionUpdate();
        context.subscriptions.push(watcher,watcher.onDidChange(changed),watcher.onDidCreate(changed));
    }
    if(vscode.window.onDidChangeWindowState)context.subscriptions.push(vscode.window.onDidChangeWindowState(state=>{if(state.focused)controller.checkForExtensionUpdate();}));
    controller.checkForExtensionUpdate();
    void controller.startAssistance();
    const ready=controller.start();
    return {getServerDefinitions:async()=>{await ready;return controller.provideDefinitions();},whenReady:()=>ready,
        explorerHandoff:()=>controller.explorerHandoff(),prepareForUpdate:()=>controller.prepareForUpdate()};
}
