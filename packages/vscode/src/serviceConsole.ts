// SPDX-License-Identifier: Apache-2.0
import * as vscode from 'vscode';
import * as path from 'node:path';
import { homedir } from 'node:os';
import { BootstrapObservation, LegacyStartupTarget, nativeRunner, RuntimeHost, RuntimeOptions } from './serviceBootstrap';
import { RecordValue, ServiceError, record } from './serviceClient';
import { ServiceConsoleController } from './serviceConsoleController';
import { ServiceConsoleState } from './serviceConsoleView';
import { runText } from './native';
export type ConsoleRuntime=Pick<RuntimeHost,'discover'|'binding'|'ensure'|'client'|'snapshot'|'repair'|'prepareConfiguration'|'reviewTransition'|'transition'|'lifecycle'|'abandonLifecycle'> & Partial<Pick<RuntimeHost,'reviewLegacyStartup'|'loadLegacyStartup'|'legacyStartupReceipts'|'removeLegacyStartup'>>;

/** Machine/user settings only. A repository cannot nominate an executable or approval. */
export function serviceRuntimeOptions():RuntimeOptions {
    const configuration=vscode.workspace.getConfiguration('airk');
    const user=<T>(key:string,fallback:T):T=>{
        const setting=configuration.inspect<T>(key);return setting?.globalValue??setting?.defaultValue??fallback;
    };
    const channel=user<string>('service.channel','stable');
    if(channel!=='stable'&&channel!=='preview')throw new ServiceError('invalid_runtime_channel');
    const installDir=user<string>('service.installDir','');
    const source=user<string>('service.managedSource','');
    const options:RuntimeOptions={channel,...(installDir?{installDir}:{})};
    if(source){
        const lifecycle=user<string>('service.managedLifecycle','external');
        if(lifecycle!=='external'&&lifecycle!=='user-on-demand')throw new ServiceError('invalid_managed_lifecycle');
        const stateDir=user<string>('service.managedStateDir','');
        options.managed={source,approvedSha256:user<string>('service.managedSha256',''),lifecycle,...(stateDir?{stateDir}:{})};
    }
    return options;
}
/**
 * Hosts the shared service console controller for AIRK Explorer › Management. The separate
 * Service Console webview view was removed in 1.5.0; nothing here renders HTML.
 */
export class ServiceConsole implements vscode.Disposable {
    controller:ServiceConsoleController;
    private timer?:NodeJS.Timeout;
    private disposed=false;
    private failures=0;
    private readonly output=vscode.window.createOutputChannel('AIRK Service');
    private readonly listeners:vscode.Disposable[]=[];
    private readonly stateChanged=new vscode.EventEmitter<ServiceConsoleState>();
    /** Fired for every published controller state. */
    readonly onDidChangeState=this.stateChanged.event;
    /** Consumers (the AIRK Explorer) that need observations to keep refreshing. */
    private consumers=0;
    constructor(private readonly context:vscode.ExtensionContext,readonly runtime:ConsoleRuntime,
        private readonly workspace?:{register():Promise<void>;currentProjects():ReadonlySet<string>}){
        this.controller=this.createController();
    }
    private createController():ServiceConsoleController{
        return new ServiceConsoleController({
            version:this.context.extension.packageJSON.version,authority:vscode.env.remoteName?`Remote execution host (${vscode.env.remoteName})`:'Local execution host',
            storage:this.context.globalState,runtime:this.runtime,trusted:()=>vscode.workspace.isTrusted&&!this.disposed,
            changed:state=>this.changed(state),confirm:async(message,acceptLabel='Continue')=>(await vscode.window.showWarningMessage(message,{modal:true},acceptLabel))===acceptLabel,
            copy:async text=>vscode.env.clipboard.writeText(text),logs:observation=>this.logs(observation),resources:current=>this.resources(current),
            openReport:async text=>{await vscode.window.showTextDocument(await vscode.workspace.openTextDocument({content:text,language:'json'}),{preview:false});},
            saveReport:text=>this.saveDiagnostic(text,'airk-diagnostic-report'),
            exportLogs:observation=>this.exportLogs(observation),
            configuration:(row,operation)=>this.configuration(row,operation),
            ...(this.runtime.reviewLegacyStartup&&this.runtime.loadLegacyStartup&&this.runtime.legacyStartupReceipts&&this.runtime.removeLegacyStartup?{legacy:{
                choose:()=>this.legacyTarget(),review:(binding:import('./serviceBootstrap').RuntimeBinding,target:LegacyStartupTarget)=>this.runtime.reviewLegacyStartup!(binding,target),
                load:(binding:import('./serviceBootstrap').RuntimeBinding,approval:string)=>this.runtime.loadLegacyStartup!(binding,approval),
                list:(binding:import('./serviceBootstrap').RuntimeBinding)=>this.runtime.legacyStartupReceipts!(binding),
                remove:(binding:import('./serviceBootstrap').RuntimeBinding,review:import('./serviceBootstrap').LegacyStartupReview)=>this.runtime.removeLegacyStartup!(binding,review),
            }}:{}),
            lifecycle:(binding,intent,force)=>this.runtime.lifecycle(binding,intent,force),
            abandonLifecycle:(binding,intent,id)=>this.runtime.abandonLifecycle(binding,intent,id),
            responseMode:()=>vscode.workspace.getConfiguration('airk').inspect<string>('mcp.responseMode')?.globalValue==='full'?'full':'terse',
            chooseResponseMode:async()=>{
                const selected=await vscode.window.showQuickPick([
                    {label:'Terse (default)',value:'terse',description:'Keep evidence and exact validators; omit repeated diagnostics'},
                    {label:'Full',value:'full',description:'Include provenance, readiness and coverage for diagnosis'},
                ],{title:'AIRK MCP response mode',placeHolder:'Agents can also request response_mode: full for an individual call'});
                if(selected&&vscode.workspace.isTrusted&&!this.disposed)await vscode.workspace.getConfiguration('airk').update('mcp.responseMode',selected.value,vscode.ConfigurationTarget.Global);
            },
            registerWorkspace:this.workspace?()=>this.workspace!.register():undefined,
            currentProjects:()=>this.workspace?.currentProjects()??new Set<string>(),
        });
    }
    reset():void{
        this.controller.dispose();this.controller=this.createController();
        if(this.observed()){void this.controller.refresh();this.schedule();}
    }
    private observed():boolean{return !this.disposed&&this.consumers>0;}
    /** Keep polling while a consumer is visible; disposing the handle releases it once. */
    retain():vscode.Disposable{
        this.consumers++;if(this.consumers===1){void this.controller.refresh();this.schedule();}
        let released=false;
        return {dispose:()=>{if(released)return;released=true;this.consumers--;if(!this.observed())clearTimeout(this.timer);}};
    }
    private changed(state:ServiceConsoleState):void{
        if(this.disposed)return;
        this.stateChanged.fire(state);
    }
    private schedule():void{
        clearTimeout(this.timer);if(!this.observed())return;
        this.failures=this.controller.state.connected?0:Math.min(this.failures+1,5);
        this.timer=setTimeout(()=>{
            void this.controller.refresh().finally(()=>this.schedule());
        },this.failures?Math.min(30000,1000*2**this.failures):2000);
    }
    /**
     * Run one allowlisted console action without moving focus. Fields go through the
     * same consoleAction validation as webview messages, including page scoping.
     */
    async run(action:string,fields:Record<string,unknown>={}):Promise<void>{
        const controller=this.controller;
        if(!await controller.whenIdle())return;
        await controller.action({...fields,type:'service',revision:controller.state.revision,action});
    }
    /** Capture a native repair preview; approval stays a separate, explicit step. */
    async previewRepair(project:string,operation:import('./serviceClient').RepairOperation):Promise<void>{
        await this.controller.projectPlan(project,operation);
    }
    async configureProject(project:string,draft:RecordValue,valid:()=>boolean):Promise<void>{
        await this.controller.projectConfiguration(project,draft,valid);
    }
    private async saveDiagnostic(text:string,name:string):Promise<void>{
        const target=await vscode.window.showSaveDialog({title:'Save AIRK diagnostics',saveLabel:'Save',filters:{JSON:['json']},
            defaultUri:vscode.Uri.file(path.join(homedir(),`${name}-${new Date().toISOString().replace(/[:.]/g,'-')}.json`))});
        if(!target||this.disposed)return;
        await vscode.workspace.fs.writeFile(target,Buffer.from(text,'utf8'));
    }
    private async exportLogs(observation?:BootstrapObservation):Promise<void>{
        if(!vscode.workspace.isTrusted)throw new ServiceError('runtime_client_no_longer_authorized');
        const bootstrap=observation??await this.runtime.discover();
        const control=bootstrap.inspection?.log_control_dir??bootstrap.inspection?.control_dir;
        if(typeof control!=='string'||!path.isAbsolute(control)||control.length>4096||/[\u0000-\u001f\u007f]/.test(control))throw new ServiceError('log_control_binding_unavailable');
        const text=await runText(bootstrap.bundle.main,['service','logs','--runtime-dir',control,'--all','--format','json'],path.dirname(bootstrap.bundle.main),60000);
        if(!vscode.workspace.isTrusted||this.disposed)return;
        await this.saveDiagnostic(text,'airk-retained-service-logs');
    }
    private async logs(observation?:BootstrapObservation):Promise<void>{
        if(!vscode.workspace.isTrusted)throw new ServiceError('runtime_client_no_longer_authorized');
        const bootstrap=observation??await this.runtime.discover();
        if(!bootstrap.inspection){this.output.appendLine('No selected runtime is available. Inspect installation settings and bootstrap diagnostics.');this.output.show(true);return;}
        const control=bootstrap.inspection.log_control_dir??bootstrap.inspection.control_dir;
        if(typeof control!=='string'||!path.isAbsolute(control)||control.length>4096||/[\u0000-\u001f\u007f]/.test(control))throw new ServiceError('log_control_binding_unavailable');
        let result:RecordValue;
        try{result=await nativeRunner(bootstrap.bundle.main,['service','logs','--runtime-dir',control,'--limit','32','--format','json']);}
        catch(error){this.output.appendLine(`Unable to read native service logs: ${error instanceof Error?error.message:String(error)}`);this.output.show(false);throw new ServiceError('service_logs_read_failed_see_airk_service_output');}
        if(!vscode.workspace.isTrusted||this.disposed)return;
        this.output.appendLine(JSON.stringify(result,null,2));this.output.show(false);
    }
    private async legacyTarget():Promise<LegacyStartupTarget|undefined>{
        const authorized=()=>{if(!vscode.workspace.isTrusted||this.disposed)throw new ServiceError('runtime_client_no_longer_authorized');};
        const folder=async(title:string)=>{authorized();const chosen=await vscode.window.showOpenDialog({title,canSelectFolders:true,canSelectFiles:false,canSelectMany:false,openLabel:'Review this old path'});authorized();return chosen?.[0]?.fsPath;};
        const root=await folder('Old AIRK source root: read-only startup review');if(!root)return;
        const data_dir=await folder('Old AIRK index data root: preserve its settings and indexes');if(!data_dir)return;
        const selected=await vscode.window.showOpenDialog({title:'Old AIRK executable: compare its configured path; never run it',canSelectFolders:false,canSelectFiles:true,canSelectMany:false,openLabel:'Review executable path'});authorized();
        const executable=selected?.[0]?.fsPath;if(!executable)return;
        const collection=await vscode.window.showInputBox({title:'Old AIRK collection name',prompt:'Use the collection from its old MCP/startup configuration. This review never converts an index.',validateInput:value=>value.length>0&&value.length<=128&&value!=='.'&&value!=='..'&&/^[A-Za-z0-9._-]+$/.test(value)?undefined:'Enter the exact old collection name (letters, digits, dot, dash or underscore).'});authorized();
        return collection?{root,data_dir,collection,executable}:undefined;
    }
    private async resources(current:RecordValue):Promise<RecordValue|undefined>{
        const choice=await vscode.window.showQuickPick([
            {label:'Responsive',value:'responsive',description:'Favor interactive responsiveness'},
            {label:'Balanced',value:'balanced',description:'Default bounded background work'},
            {label:'Throughput',value:'throughput',description:'Allow more admitted background work'},
            {label:'Custom overrides',value:'custom',description:'Set native thread, memory and background I/O limits'},
        ],{title:'AIRK service resources',placeHolder:'Changes apply to the selected service host and all registered projects'});
        if(!choice)return;
        if(choice.value!=='custom')return {preset:choice.value,extraction_threads:null,inference_threads:null,memory_bytes:null,background_io_bytes_per_second:null};
        const result={...current};
        for(const [key,label,min,max,scale] of [
            ['extraction_threads','Extraction threads',1,64,1],['inference_threads','Inference threads',1,4,1],
            ['memory_bytes','Working memory allowance (GiB)',1,64,2**30],['background_io_bytes_per_second','Background I/O allowance (MiB/s)',1,1024,2**20],
        ] as const){
            const value=await vscode.window.showInputBox({title:`AIRK: ${label}`,prompt:`Whole number ${min}–${max}; empty uses the selected preset. This is native admission policy, not an OS hard limit.`,
                value:typeof result[key]==='number'?String(Number(result[key])/scale):'',
                validateInput:text=>text===''||(/^\d+$/.test(text)&&Number(text)>=min&&Number(text)<=max)?undefined:`Enter ${min}–${max}, or leave empty.`});
            if(value===undefined)return;result[key]=value===''?null:Number(value)*scale;
        }
        return result;
    }
    private async configuration(row:RecordValue,requested?:'detail'|'advanced'):Promise<RecordValue|undefined>{
        const registration=record(record(row.project).registration);
        const common=requested?{operation:requested}:await vscode.window.showQuickPick([
            {label:'Semantic search',operation:'vectors',description:'Enable with offline model preparation, or turn off'},
            {label:'Index detail',operation:'detail',description:'Choose source evidence detail; independent of semantic search'},
            {label:'Advanced settings…',operation:'advanced',description:'Models, source roots, limits, quotas and usage'},
        ],{title:`AIRK settings: ${String(registration.collection)}`,placeHolder:'Changes are queued; Activity shows when they apply'});
        if(!common)return;
        const selection=common.operation==='advanced'?await vscode.window.showQuickPick([
            {label:'Vector search',operation:'vectors',description:'Enable or disable semantic indexing for this project'},
            {label:'Index detail',operation:'detail',description:'Choose evidence detail for the next source pass'},
            {label:'Vector eligibility',operation:'vector_selection',description:'Set bounded inclusion/exclusion policy JSON'},
            {label:'Add source root',operation:'add_root',description:'Register an additional source directory under an alias'},
            {label:'Remove source root',operation:'remove_root',description:'Remove a saved alias; source files are preserved'},
            {label:'Select custom offline model',operation:'install_model',description:'Select and pin model.json from a local model directory'},
            {label:'Select bundled MiniLM',operation:'bundled_model',description:'Replace the current selection with the paired offline default'},
            {label:'Usage recording',operation:'usage',description:'Choose disabled, aggregates or diagnostic events'},
            {label:'Extraction policy file',operation:'extraction_policy',description:'Apply selected adapter rules and request a source pass'},
            {label:'Source scan limits file',operation:'scan_limits',description:'Set bounded extraction limits for this project'},
            {label:'Reset source scan limits',operation:'reset_scan_limits',description:'Remove this project override and use native automatic limits'},
            {label:'Shared storage quota file',operation:'quota_policy',description:'Change storage admission for all collections sharing this data root'},
            {label:'Calibrate token counts',operation:'calibrate',description:'Count explicit logical JSON samples with a local tokenizer'},
            {label:'Unregister project',operation:'unregister',description:'Stop new connections and automatic work; preserve sources, settings and index data'},
        ],{title:`AIRK project settings: ${String(registration.collection)}`,placeHolder:'Advanced changes become guarded native jobs; source files stay unchanged'}):common;
        if(!selection)return;
        const operation=selection.operation;
        if(operation==='bundled_model'||operation==='unregister'||operation==='reset_scan_limits')return {operation};
        if(operation==='vectors'){
            const enabled=await vscode.window.showQuickPick([{label:'Enable semantic search',description:'Prepare the offline model, then index sources and build vectors',value:true},{label:'Turn off semantic search',description:'Use lexical evidence; sources and saved settings are retained',value:false}],{title:'AIRK vector search'});
            return enabled?{operation,enabled:enabled.value}:undefined;
        }
        if(operation==='detail'){
            const level=await vscode.window.showQuickPick([
                {label:'Agent Lean',level:'agent_lean',description:'Selected code and document facts; default for new collections'},
                {label:'Search Only',level:'essential',description:'Text and citations; omits rich structured facts'},
                {label:'Rich Structure',level:'balanced',description:'Broader supported structure and declaration continuity; more retained data'},
                {label:'Detailed',level:'detailed',description:'Advanced supported evidence and document-unit continuity; highest detail'},
            ],{title:'AIRK evidence detail',placeHolder:'Choose for your purpose and source formats; vectors are configured separately'});
            return level?{operation,level:level.level}:undefined;
        }
        if(operation==='usage'){
            const mode=await vscode.window.showQuickPick([
                {label:'Disabled',value:'disabled'},{label:'Aggregates only',value:'aggregates_only'},{label:'Diagnostic events',value:'diagnostic_events'},
            ],{title:'AIRK local usage recording'});
            return mode?{operation,mode:mode.value}:undefined;
        }
        if(operation==='vector_selection'){
            const text=await vscode.window.showInputBox({title:'AIRK vector eligibility policy',prompt:'JSON policy, at most 16 KiB. Native validation checks rule conflicts before applying.',
                value:'{"version":1,"exclusions":[],"agent_defaults":false}',validateInput:text=>{
                    if(Buffer.byteLength(text)>16000)return 'Policy exceeds the native job budget.';
                    try{const value=record(JSON.parse(text));return value.version===1&&Array.isArray(value.exclusions)?undefined:'Use a version 1 policy with an exclusions array.';}catch{return 'Enter a JSON policy object.';}
                }});
            return text===undefined?undefined:{operation,policy:JSON.parse(text)};
        }
        const choose=async(title:string,folder=false):Promise<string|undefined>=>{
            const selected=await vscode.window.showOpenDialog({title,canSelectFiles:!folder,canSelectFolders:folder,canSelectMany:false,openLabel:'Select'});
            if(!selected?.length)return;
            if(!['file','vscode-remote'].includes(selected[0].scheme))throw new ServiceError('native_configuration_requires_file');
            return selected[0].fsPath;
        };
        if(['extraction_policy','scan_limits','quota_policy'].includes(operation)){
            const path=await choose(`Select ${operation==='quota_policy'?'shared storage quota':operation==='scan_limits'?'source scan limits':'extraction policy'} JSON on this execution host`);
            if(!path)return;
            if(operation==='quota_policy'){
                const registration=record(record(row.project).registration);
                const answer=await vscode.window.showInformationMessage(`Apply this storage quota policy to every collection sharing ${String(registration.data_root)}? It controls new storage admission.`,{modal:true},'Apply shared quotas');
                if(answer!=='Apply shared quotas')return;
            }
            return {operation,path};
        }
        if(operation==='install_model'){
            const directory=await choose('Select the offline model directory on this execution host',true);
            return directory?{operation,directory}:undefined;
        }
        if(operation==='add_root'||operation==='remove_root'){
            const alias=await vscode.window.showInputBox({title:operation==='add_root'?'AIRK additional root alias':'AIRK saved root alias to remove',
                prompt:'1–64 letters, digits, underscore or dash. The primary root cannot be removed.',
                validateInput:text=>/^[A-Za-z0-9_-]{1,64}$/.test(text)?undefined:'Enter a valid root alias.'});
            if(!alias)return;
            if(operation==='remove_root')return {operation,alias};
            const source=await choose('Select an additional source directory on this execution host',true);
            return source?{operation,alias,path:source}:undefined;
        }
        const tokenizer=await choose('Select a local tokenizer definition JSON');if(!tokenizer)return;
        const baseline=await choose('Select baseline logical-result JSON');if(!baseline)return;
        const returned=await choose('Select returned logical-result JSON');if(!returned)return;
        const snapshot=row.inspection?record(record(row.inspection).snapshot):{};
        const publication=snapshot.publication?record(snapshot.publication):{};
        const generation=await vscode.window.showInputBox({title:'AIRK calibration generation',prompt:'Explicit generation for these user-supplied comparable samples. Provider framing requires independent verification.',
            value:Number.isSafeInteger(publication.generation)?String(publication.generation):'',validateInput:text=>/^\d+$/.test(text)&&Number.isSafeInteger(Number(text))&&Number(text)>0?undefined:'Enter a positive indexed generation.'});
        if(generation===undefined)return;
        const recording=await vscode.window.showQuickPick([{label:'Calculate only',record:false},{label:'Record local aggregate',record:true}],{title:'AIRK calibration result',placeHolder:'Sample text stays out of control IPC and usage records; durable jobs retain file selectors and hashes'});
        return recording?{operation,tokenizer,baseline,returned,generation:Number(generation),content_kind:'mixed',record:recording.record}:undefined;
    }
    dispose():void{
        this.disposed=true;clearTimeout(this.timer);this.controller.dispose();this.output.dispose();this.stateChanged.dispose();this.listeners.forEach(listener=>listener.dispose());
    }
}
