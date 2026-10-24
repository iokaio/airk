// SPDX-License-Identifier: Apache-2.0
import * as vscode from 'vscode';
import * as path from 'node:path';
import { BootstrapObservation, nativeRunner, RuntimeHost, RuntimeOptions } from './serviceBootstrap';
import { RecordValue, ServiceError, record } from './serviceClient';
import { ServiceConsoleController } from './serviceConsoleController';
import { consoleContent, renderConsole, ServiceConsoleState } from './serviceConsoleView';
export type ConsoleRuntime=Pick<RuntimeHost,'discover'|'binding'|'ensure'|'client'|'snapshot'|'repair'|'prepareConfiguration'|'lifecycle'|'abandonLifecycle'>;

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
export class ServiceConsole implements vscode.WebviewViewProvider,vscode.Disposable {
    controller:ServiceConsoleController;
    private view?:vscode.WebviewView;
    private timer?:NodeJS.Timeout;
    private disposed=false;
    private failures=0;
    private readonly output=vscode.window.createOutputChannel('AIRK Service');
    private readonly listeners:vscode.Disposable[]=[];
    constructor(private readonly context:vscode.ExtensionContext,readonly runtime:ConsoleRuntime){
        this.controller=this.createController();
    }
    private createController():ServiceConsoleController{
        return new ServiceConsoleController({
            version:this.context.extension.packageJSON.version,authority:vscode.env.remoteName?`Remote execution host (${vscode.env.remoteName})`:'Local execution host',
            storage:this.context.globalState,runtime:this.runtime,trusted:()=>vscode.workspace.isTrusted&&!this.disposed,
            changed:state=>this.changed(state),confirm:async message=>(await vscode.window.showWarningMessage(message,{modal:true},'Continue'))==='Continue',
            copy:async text=>vscode.env.clipboard.writeText(text),logs:observation=>this.logs(observation),resources:current=>this.resources(current),
            configuration:row=>this.configuration(row),
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
        });
    }
    reset():void{
        this.controller.dispose();this.controller=this.createController();
        if(this.view?.visible){void this.controller.refresh();this.schedule();}
    }
    resolveWebviewView(view:vscode.WebviewView):void{
        this.view=view;view.webview.options={enableScripts:true,localResourceRoots:[]};
        view.webview.html=renderConsole(this.controller.state);
        this.listeners.push(view.webview.onDidReceiveMessage(message=>{void this.controller.action(message);}),
            view.onDidChangeVisibility(()=>{if(view.visible){void this.controller.refresh();this.schedule();}else clearTimeout(this.timer);}),
            view.onDidDispose(()=>{this.view=undefined;clearTimeout(this.timer);}));
        if(view.visible){void this.controller.refresh();this.schedule();}
    }
    private changed(state:ServiceConsoleState):void{
        if(!this.view||this.disposed)return;
        void this.view.webview.postMessage({type:'render',revision:state.revision,html:consoleContent(state)});
    }
    private schedule():void{
        clearTimeout(this.timer);if(!this.view?.visible||this.disposed)return;
        this.failures=this.controller.state.connected?0:Math.min(this.failures+1,5);
        this.timer=setTimeout(()=>{
            void this.controller.refresh().finally(()=>this.schedule());
        },this.failures?Math.min(30000,1000*2**this.failures):5000);
    }
    async command(action:string):Promise<void>{
        await vscode.commands.executeCommand('airk.service.focus');
        const controller=this.controller;
        if(!await controller.whenIdle())return;
        await controller.action({type:'service',revision:controller.state.revision,action,
            ...(action==='previewReport'?{includePaths:false}:{})});
    }
    private async logs(observation?:BootstrapObservation):Promise<void>{
        if(!vscode.workspace.isTrusted)throw new ServiceError('runtime_client_no_longer_authorized');
        const bootstrap=observation??await this.runtime.discover();
        if(!bootstrap.inspection){this.output.appendLine('No selected runtime is available. Inspect installation settings and bootstrap diagnostics.');this.output.show(true);return;}
        const control=bootstrap.inspection.log_control_dir??bootstrap.inspection.control_dir;
        if(typeof control!=='string'||!path.isAbsolute(control)||control.length>4096||/[\u0000-\u001f\u007f]/.test(control))throw new ServiceError('log_control_binding_unavailable');
        const result=await nativeRunner(bootstrap.bundle.main,['service','logs','--runtime-dir',control,'--limit','256']);
        if(!vscode.workspace.isTrusted||this.disposed)return;
        this.output.appendLine(JSON.stringify(result,null,2));this.output.show(true);
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
    private async configuration(row:RecordValue):Promise<RecordValue|undefined>{
        const registration=record(record(row.project).registration);
        const selection=await vscode.window.showQuickPick([
            {label:'Vector search',operation:'vectors',description:'Enable or disable semantic indexing for this project'},
            {label:'Index detail',operation:'detail',description:'Choose evidence detail for the next source pass'},
            {label:'Vector eligibility',operation:'vector_selection',description:'Set bounded inclusion/exclusion policy JSON'},
            {label:'Add source root',operation:'add_root',description:'Register an additional source directory under an alias'},
            {label:'Remove source root',operation:'remove_root',description:'Remove a saved alias; source files are preserved'},
            {label:'Select custom offline model',operation:'install_model',description:'Select and pin model.json from a local model directory'},
            {label:'Select bundled MiniLM',operation:'bundled_model',description:'Replace the current selection with the paired offline default'},
            {label:'Usage recording',operation:'usage',description:'Choose disabled, aggregates or diagnostic events'},
            {label:'Calibrate token counts',operation:'calibrate',description:'Count explicit logical JSON samples with a local tokenizer'},
        ],{title:`AIRK project settings: ${String(registration.collection)}`,placeHolder:'Changes become durable native jobs; source files stay unchanged'});
        if(!selection)return;
        const operation=selection.operation;
        if(operation==='bundled_model')return {operation};
        if(operation==='vectors'){
            const enabled=await vscode.window.showQuickPick([{label:'Enable vector search',value:true},{label:'Disable vector search',value:false}],{title:'AIRK vector search'});
            return enabled?{operation,enabled:enabled.value}:undefined;
        }
        if(operation==='detail'){
            const level=await vscode.window.showQuickPick(['essential','agent_lean','balanced','detailed'],{title:'AIRK evidence detail'});
            return level?{operation,level}:undefined;
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
        this.disposed=true;clearTimeout(this.timer);this.controller.dispose();this.output.dispose();this.listeners.forEach(listener=>listener.dispose());
    }
}
