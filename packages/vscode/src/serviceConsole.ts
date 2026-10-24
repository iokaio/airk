// SPDX-License-Identifier: Apache-2.0
import * as vscode from 'vscode';
import { BootstrapObservation, nativeRunner, RuntimeHost, RuntimeOptions } from './serviceBootstrap';
import { RecordValue, ServiceError } from './serviceClient';
import { ServiceConsoleController } from './serviceConsoleController';
import { consoleContent, renderConsole, ServiceConsoleState } from './serviceConsoleView';
export type ConsoleRuntime=Pick<RuntimeHost,'discover'|'binding'|'ensure'|'client'|'lifecycle'|'abandonLifecycle'>;

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
        const binding=this.runtime.binding(bootstrap.inspection,bootstrap.installDir);
        const result=await nativeRunner(bootstrap.bundle.main,['service','logs','--runtime-dir',binding.controlDir,'--limit','256']);
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
    dispose():void{
        this.disposed=true;clearTimeout(this.timer);this.controller.dispose();this.output.dispose();this.listeners.forEach(listener=>listener.dispose());
    }
}
