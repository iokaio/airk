// SPDX-License-Identifier: Apache-2.0
import * as vscode from 'vscode';
import * as path from 'node:path';
import { randomBytes, randomUUID } from 'node:crypto';
import type { ExplorerHost } from './explorerController';
import type { LaunchSpec } from './model';
import { EvidenceClient } from './explorerEvidence';
import { openSource, saveStudioInWorkspace } from './explorerWorkspaceIO';
import { strategyFromPlan } from './workbenchStrategies';
import { newSession, SessionStore } from './workbenchSessions';
import { CurrentTurnProjections, continuations, evidenceCards, renderExport } from './workbenchRender';
import { object, parseWorkbenchMessage, Value, WorkbenchMessage, WorkbenchSession, WorkbenchState, WORKBENCH_HELP_ROUTE } from './workbenchContracts';

// Cold native setup (10 s) plus scalar observation (2 s), with transport slack.
export const STATUS_RESPONSE_TIMEOUT_MS=15000;
export function workbenchHtml(webview:vscode.Webview, extension:vscode.Uri, nonce:string):string {
    const script=webview.asWebviewUri(vscode.Uri.joinPath(extension,'out','workbenchWebview.js'));
    const style=webview.asWebviewUri(vscode.Uri.joinPath(extension,'out','workbenchWebview.css'));
    return `<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src ${webview.cspSource}; script-src 'nonce-${nonce}';">
<title>AIRK Workbench</title><link rel="stylesheet" href="${style}"></head><body><div id="root"></div><script nonce="${nonce}" src="${script}"></script></body></html>`;
}

/** Host owns sessions, evidence and native transport; webview actions carry opaque handles. */
export class WorkbenchView implements vscode.WebviewViewProvider, vscode.Disposable {
    private view?:vscode.WebviewView;
    private readonly display=new CurrentTurnProjections();
    private readonly visibilityChanged=new vscode.EventEmitter<void>();
    readonly onDidChangeVisibility=this.visibilityChanged.event;
    get visible():boolean{return this.view?.visible===true;}
    async toggle():Promise<void>{
        if(this.visible)await vscode.commands.executeCommand('workbench.action.closeAuxiliaryBar');
        else await this.focus();
    }
    private client?:EvidenceClient;
    private clientIdentity?:string;
    private generation=0;
    private session?:WorkbenchSession;
    private sessions?:SessionStore;
    private get store():SessionStore{return this.sessions??=new SessionStore(path.join(this.context.globalStorageUri.fsPath,'workbench'));}
    private readonly subscriptions:vscode.Disposable[]=[];
    private recent:WorkbenchState['recent']=[];
    private busy=false;
    private notice?:string;
    private saving='';
    private status?:Value;
    private operations?:Value;
    private saveQueue=Promise.resolve();
    private disposed=false;
    private pendingInput?:string;
    constructor(private readonly context:vscode.ExtensionContext, private readonly host:ExplorerHost){
        this.subscriptions.push(host.onDidChangeSpecs(()=>{this.invalidateChangedClient();if(this.view){void this.refresh().catch(error=>this.fail(error));}}),vscode.workspace.onDidChangeConfiguration(event=>{
            if(event.affectsConfiguration('airk.runtime.nativeExecution')){this.invalidateChangedClient();if(this.view){void this.refresh().catch(error=>this.fail(error));}}
            if(event.affectsConfiguration('airk.workbench')){void this.privacyChanged().catch(error=>this.fail(error));}
        }));
    }
    private persistence():boolean{return vscode.workspace.getConfiguration('airk').get('workbench.persistSessions',true);}
    private persistResults():boolean{return vscode.workspace.getConfiguration('airk').get('workbench.persistResults',true);}
    private nativeAllowed():boolean{return vscode.workspace.getConfiguration('airk').inspect<boolean>('runtime.nativeExecution')?.globalValue!==false;}
    private localAllowed():boolean{return this.host.trusted()&&!this.disposed;}
    private permitted():boolean{return this.localAllowed()&&this.nativeAllowed();}
    private authorized():void{if(!this.host.trusted()||this.disposed){throw new Error('A trusted workspace is required.');}}
    private releaseClient():void{this.client?.dispose();this.client=undefined;this.clientIdentity=undefined;}
    private selectedSpec():LaunchSpec|undefined{return this.host.specs().find(spec=>spec.service?.project===this.session?.project);}
    /** Invalidate synchronously before refresh/list awaits, including hidden views. */
    private invalidateChangedClient():void{
        const spec=this.selectedSpec();
        if(!this.permitted()||(this.session&&!spec)||(this.client&&this.clientIdentity!==JSON.stringify(spec))){
            this.stop();this.releaseClient();this.status=undefined;this.operations=undefined;
        }
    }
    resolveWebviewView(view:vscode.WebviewView):void{
        this.view=view;
        const visibility=view.onDidChangeVisibility(()=>{
            this.visibilityChanged.fire();
            // Hidden webviews can miss messages; reconcile without replacing the DOM.
            if(view.visible){this.send();}
        });
        this.visibilityChanged.fire();
        view.webview.options={enableScripts:true,enableCommandUris:false,localResourceRoots:[vscode.Uri.joinPath(this.context.extensionUri,'out')]};
        view.webview.html=workbenchHtml(view.webview,this.context.extensionUri,randomBytes(24).toString('base64url'));
        const received=view.webview.onDidReceiveMessage(raw=>{const message=parseWorkbenchMessage(raw);if(message){void this.message(message).catch(error=>this.fail(error));}else{this.fail(new Error('Workbench request is invalid or exceeds its size limit.'));}});
        const release=view.onDidDispose(()=>{received.dispose();visibility.dispose();release.dispose();if(this.view===view){this.view=undefined;this.display.clear();this.visibilityChanged.fire();this.stop();this.releaseClient();}});
        void this.refresh().catch(error=>this.fail(error));
    }
    private state():WorkbenchState{
        return {projects:this.host.specs().flatMap(s=>s.service?[{id:s.service.project,name:s.name}]:[]),project:this.session?.project,session:this.session,recent:this.recent,
            ...this.display.project(this.session?.turns??[]),busy:this.busy,notice:this.notice,save:this.saving,status:this.status,persistence:this.persistence(),operations:this.operations};
    }
    private send():void{if(!this.disposed){void this.view?.webview.postMessage({type:'state',state:this.state()});}}
    private sendStatus():void{if(!this.disposed){void this.view?.webview.postMessage({type:'status',sessionId:this.session?.id,project:this.session?.project,status:this.status});}}
    private sendSaved(session:WorkbenchSession):void{if(!this.disposed&&this.session===session){void this.view?.webview.postMessage({type:'saved',sessionId:session.id,project:session.project,revision:session.revision,recent:this.recent,save:this.saving});}}

    private fail(error:unknown):void{this.notice=error instanceof Error?error.message:String(error);this.send();}
    async refresh():Promise<void>{
        this.invalidateChangedClient();
        if(!this.permitted()){this.send();return;}
        const specs=this.host.specs();
        if(!this.session){const id=this.host.activeProject()??(specs.length===1?specs[0].service?.project:undefined);if(id){await this.select(id);return;}}
        if(this.session&&!specs.some(s=>s.service?.project===this.session!.project)){this.stop();this.releaseClient();this.session=undefined;this.status=undefined;}
        this.send();
    }
    private async select(id:string):Promise<void>{
        this.authorized();const spec=this.host.specs().find(value=>value.service?.project===id);if(!spec){throw new Error('Project is not registered in this workspace.');}
        const identity=JSON.stringify(spec);
        this.stop();
        if(this.client&&this.clientIdentity!==identity){this.releaseClient();}
        this.status=undefined;this.session=newSession(id);this.recent=[];this.notice=undefined;
        const generation=this.generation,session=this.session;
        const current=()=>generation===this.generation&&session===this.session&&this.localAllowed()&&JSON.stringify(this.selectedSpec())===identity;
        this.send();
        if(this.permitted()){
            const client=this.evidence();
            // Readiness and local history have independent owners. A slow saved
            // session scan must not delay this transcript's first native status.
            void client.call('airk_status',{}, {timeout:STATUS_RESPONSE_TIMEOUT_MS}).then(result=>{if(current()&&client===this.client&&this.permitted()){this.status=result.payload;this.sendStatus();}},error=>{if(current()&&client===this.client&&this.permitted()){this.fail(error);}});
        }
        let recent:WorkbenchState['recent'];
        try{recent=this.persistence()?(await this.store.list(id)).map(value=>({id:value.id,title:value.title,updatedAt:value.updatedAt,turns:value.turns.length})):[];}
        catch(error){if(!current()){return;}throw error;}
        if(!current()){return;}
        this.recent=recent;this.send();
    }
    private evidence():EvidenceClient{
        this.authorized();if(!this.nativeAllowed()){throw new Error('Native execution is disabled.');}const spec=this.selectedSpec();
        if(!spec){throw new Error('Choose a registered project.');}
        const identity=JSON.stringify(spec);
        if(this.client&&this.clientIdentity!==identity){this.stop();this.releaseClient();this.status=undefined;this.operations=undefined;}
        if(!this.client){this.client=new EvidenceClient(spec,this.host.version,{workbench:true,clientName:'airk-workbench',timeout:35000});this.clientIdentity=identity;}
        return this.client;
    }
    stop():void{this.generation++;if(this.busy){const turn=this.session?.turns.at(-1);if(turn&&!turn.envelope){turn.error='Request stopped. Already admitted native reads may still finish; late replies are discarded.';}}this.busy=false;this.client?.cancel('workbench');this.send();}
    async newSession():Promise<void>{if(this.session){await this.select(this.session.project);}else{await this.refresh();}}
    async clear():Promise<void>{
        this.authorized();if(!this.session){return;}
        const session=this.session,selected=session.project,generation=this.generation,identity=JSON.stringify(this.selectedSpec());
        const choice=await vscode.window.showWarningMessage('Delete saved Workbench sessions for this project?',{modal:true},'Delete sessions');
        if(choice==='Delete sessions'&&this.session===session&&generation===this.generation&&this.localAllowed()&&JSON.stringify(this.selectedSpec())===identity){
            this.stop();const accepted=this.generation;
            // The already authorized deletion keeps its exact project even if a
            // later selection supersedes it; it never restores that old selection.
            await this.saveQueue;await this.store.remove(selected);
            if(accepted===this.generation&&this.session===session&&this.localAllowed()&&JSON.stringify(this.selectedSpec())===identity){if(this.selectedSpec()){await this.select(selected);}else{this.session=undefined;this.recent=[];this.status=undefined;this.send();}}
        }
    }
    private save(session:WorkbenchSession):void{
        if(!this.persistence()){this.saving='Session persistence off';this.send();return;}
        this.saving='Saving…';const captured=structuredClone(session),results=this.persistResults();
        this.saveQueue=this.saveQueue.then(async()=>{
            // Preserve revisions after earlier saves queued by the same window.
            captured.revision=session.revision;
            const recent=await this.store.save(captured,results);session.revision=captured.revision;
            if(this.session!==session){return;}
            const summaries=recent??(await this.store.list(session.project)).map(s=>({id:s.id,title:s.title,updatedAt:s.updatedAt,turns:s.turns.length}));
            // Empty saves retain the list fallback; a new transcript can arrive during it.
            if(this.session!==session){return;}
            this.recent=summaries;this.saving=results?'Auto-saved':'Inputs saved; results excluded';this.sendSaved(session);
        }).catch(error=>{this.saving='Save failed — export or retry';this.fail(error);});
    }
    private async loadRecent(id:string):Promise<void>{
        const session=this.session;if(!session){return;}
        this.stop();const generation=this.generation,identity=JSON.stringify(this.selectedSpec());
        const current=()=>generation===this.generation&&this.session===session&&this.localAllowed()&&JSON.stringify(this.selectedSpec())===identity;
        await this.saveQueue;if(!current()){return;}
        let loaded:WorkbenchSession|undefined;
        try{loaded=await this.store.load(session.project,id);}
        catch(error){if(!current()){return;}throw error;}
        if(!current()){return;}
        this.session=loaded;this.send();
    }
    private async deleteRecent(id:string):Promise<void>{
        const session=this.session;if(!session){return;}
        this.stop();const generation=this.generation,identity=JSON.stringify(this.selectedSpec());
        // Deletion was explicitly accepted for this project/id. Finish that
        // operation, but never redirect it or reset a superseding transcript.
        await this.saveQueue;await this.store.remove(session.project,id);
        if(generation===this.generation&&this.session===session&&this.localAllowed()&&JSON.stringify(this.selectedSpec())===identity){await this.newSession();}
    }
    private async privacyChanged():Promise<void>{
        await this.saveQueue;
        // Turning persistence off applies to every saved project, not only the selected one.
        if(!this.persistence()||!this.persistResults()){
            for(const project of await this.store.projects()){
                if(!this.persistence()){await this.store.remove(project);}else{await this.store.clearResults(project);}
            }
            if(this.session&&!this.persistence()){this.recent=[];}
        }
        this.send();
    }
    private async run(input:string,prepare=false,recipe?:string,plan?:Value,pathPrefix?:string):Promise<void>{
        this.authorized();if(!this.session){throw new Error('Choose a project first.');}
        this.stop();const session=this.session,client=this.evidence(),generation=this.generation;this.busy=true;this.notice=undefined;
        const turn={id:randomUUID(),at:Date.now(),input:(input||`${recipe??'Plan'} evidence`).slice(0,4096),format:'structured' as const,request:undefined as Value|undefined,envelope:undefined as Value|undefined,error:undefined as string|undefined};
        session.turns.push(turn);if(session.turns.length===1){session.title=turn.input.replaceAll('\n',' ').slice(0,80);}this.send();
        try{
            let remembered=false;
            if(!plan&&!recipe&&this.persistence()){
                const shortcut=await this.store.findShortcut(session.project,input,pathPrefix??'');
                if(shortcut){plan=shortcut;remembered=true;}
            }
            if(generation!==this.generation||session!==this.session||client!==this.client||this.clientIdentity!==JSON.stringify(this.selectedSpec())||!this.permitted()){return;}
            if(remembered){this.notice='Using an explicitly remembered plan; all steps are validated against current evidence.';}
            const request:Value={schema:'airk.parse.request/1',mode:prepare?'prepare':'execute',...(pathPrefix?{path_prefix:pathPrefix}:{})};
            if(plan){request.plan=plan;}else if(recipe){request.recipe={id:recipe,parameters:input.trim()?{name:input.trim()}:{}};}else{request.text=input;}
            turn.request=structuredClone(request);
            const result=await client.call('airk_parse',request,{supersede:'workbench',timeout:35000});
            if(generation!==this.generation||session!==this.session||client!==this.client||this.clientIdentity!==JSON.stringify(this.selectedSpec())||!this.permitted()){return;}
            turn.envelope=result.payload;
            if(result.payload.status==='prepared'){this.notice='Plan prepared. Review or edit it, then choose Run plan.';}
        }catch(error){if(generation===this.generation){turn.error=error instanceof Error?error.message:String(error);}}
        finally{if(generation===this.generation){this.busy=false;session.updatedAt=Date.now();this.save(session);this.send();}}
    }
    private async message(message:WorkbenchMessage):Promise<void>{
        if(message.type==='ready'){await this.refresh();if(this.pendingInput){void this.view?.webview.postMessage({type:'prefill',input:this.pendingInput});this.pendingInput=undefined;}return;}
        if(message.type==='help'){await vscode.commands.executeCommand('airk.explorer.open',WORKBENCH_HELP_ROUTE);return;}
        this.authorized();
        switch(message.type){
            case 'project':await this.select(message.id);break;
            case 'refresh':{const session=this.session;await this.refresh();if(this.session&&this.session===session){const generation=this.generation;const result=await this.evidence().call('airk_status',{}, {timeout:STATUS_RESPONSE_TIMEOUT_MS});if(generation===this.generation&&this.permitted()){this.status=result.payload;this.sendStatus();}}break;}
            case 'new':await this.newSession();break;
            case 'stop':this.stop();if(this.session){this.save(this.session);}break;
            case 'clear':await this.clear();break;
            case 'manage':await vscode.commands.executeCommand('airk.explorer.open','manage/overview');break;
            case 'forms':{const generation=this.generation;const reply=await this.evidence().call('airk_capabilities',{});if(generation===this.generation&&this.permitted()){this.operations=object(object(reply.payload.workbench).operations);this.send();}break;}
            case 'retrySave':if(this.session){this.save(this.session);}break;
            case 'fork':if(this.session){this.stop();const fork={...structuredClone(this.session),id:randomUUID(),revision:0,title:`${this.session.title.slice(0,70)} (copy)`};this.session=fork;this.save(fork);this.send();}break;
            case 'recent':await this.loadRecent(message.id);break;
            case 'delete':await this.deleteRecent(message.id);break;
            case 'rename':if(this.session){this.session.title=message.title;this.save(this.session);this.send();}break;
            case 'run':await this.run(message.input,message.prepare,message.recipe,message.plan,message.pathPrefix);break;
            case 'rerun':{const turn=this.session?.turns.find(t=>t.id===message.turn);if(turn){const request=object(turn.request),plan=request.plan?structuredClone(object(request.plan)):undefined;if(plan){delete plan.digest;delete plan.publication;delete plan.catalog;delete plan.collection;for(const raw of Array.isArray(plan.steps)?plan.steps:[]){const args=object(object(raw).args);for(const key of ['known_hash','revision','cursor']){delete args[key];}}}await this.run(turn.input,false,typeof object(request.recipe).id==='string'?String(object(request.recipe).id):undefined,plan,typeof request.path_prefix==='string'?request.path_prefix:undefined);}break;}
            case 'saveStrategy':{
                const turn=this.session?.turns.find(t=>t.id===message.turn);if(!turn?.envelope){throw new Error('Collect or prepare evidence first.');}
                const generation=this.generation,spec=this.evidence().spec,document=strategyFromPlan(object(turn.envelope.plan),turn.input,turn.input);
                const name=await vscode.window.showInputBox({title:'Save Workbench inputs to Strategies',prompt:'Name using lowercase letters, numbers and hyphens. The Strategy uses its own disclosed evidence budgets; cached evidence is excluded.'});
                if(name){const saved=await saveStudioInWorkspace(document,name,spec.root,undefined,()=>this.host.trusted()&&this.generation===generation);this.notice=saved.saved?'Saved inputs to the guarded Strategies library.':saved.reason;this.send();}break;
            }
            case 'remember':{const turn=this.session?.turns.find(t=>t.id===message.turn);if(!this.session||!turn?.envelope||!this.persistence()){throw new Error('Prepare a plan and enable session persistence before remembering it.');}await this.store.remember(this.session.project,turn.input,object(turn.envelope.plan));this.notice='Remembered the accepted inputs for this project and scope. Evidence and revision handles were excluded.';this.send();break;}
            case 'continue':{
                const turn=this.session?.turns.find(t=>t.id===message.turn),envelope=turn?.envelope;
                if(!envelope||!continuations(envelope).some(c=>c.id===message.step)){throw new Error('No bounded continuation remains for this result.');}
                const index=Number(message.step.slice(5)),report=object((envelope.steps as unknown[])[index]),result=object(report.result),prior=object(envelope.plan),budgets=object(prior.budgets);
                const remaining=Number(budgets.max_bytes??524288)-Number(envelope.payload_bytes??0);if(remaining<1024){throw new Error('The original evidence byte budget is exhausted; start a scoped new request.');}
                const plan={schema:'airk.parse.plan/1',publication:envelope.publication,collection:prior.collection,path_prefix:prior.path_prefix,budgets:{...budgets,calls:Number(budgets.calls??12)-Number(envelope.calls??0),max_bytes:remaining},steps:[{id:'continuation',tool:report.tool,args:{...object(report.arguments),cursor:result.cursor??object(result.content).cursor}}]};
                await this.run(`Continue ${String(report.tool)} from ${turn!.input}`,false,undefined,plan);break;
            }
            case 'open':case 'context':{
                const turn=this.session?.turns.find(t=>t.id===message.turn),card=turn&&evidenceCards(turn.envelope??{}).find(c=>c.id===message.card);
                if(!card){throw new Error('Evidence handle expired.');}
                if(message.type==='context'){
                    const plan={schema:'airk.parse.plan/1',publication:card.publication,steps:[{id:'context',tool:'context',args:{path:card.path,line:card.start,known_hash:card.hash,before:3,after:3}}]};
                    await this.run(`Context ${card.path}:${card.start}`,false,undefined,plan);
                }else{
                    const generation=this.generation,info=(await this.evidence().call('airk_file_info',{path:card.path,response_mode:'full'})).payload,source=object(info.source);
                    if(generation!==this.generation){throw new Error('Project changed before opening the source.');}this.authorized();
                    if(typeof source.root!=='string'||typeof source.relative_path!=='string'){throw new Error('AIRK did not report the original source location.');}
                    const result=await openSource({root:source.root,relative:source.relative_path,contentHash:card.hash,startLine:card.start,lineBasis:typeof info.line_basis==='string'?info.line_basis:undefined},vscode.ViewColumn.Active);this.notice=result.message;this.send();
                }break;
            }
            case 'copy':case 'export':{
                if(!this.session){return;}
                const text=renderExport(this.session,message.format,message.turn,message.cards),bytes=Buffer.byteLength(text,'utf8');
                if(message.type==='copy'){
                    if(bytes>1048576){throw new Error('Copy exceeds 1 MiB. Select fewer records or use Save export.');}
                    await vscode.env.clipboard.writeText(text);this.notice=`Copied ${message.format} · ${bytes.toLocaleString()} bytes`;
                }else{
                    const uri=await vscode.window.showSaveDialog({defaultUri:vscode.Uri.file(`airk-workbench.${message.format==='json'?'json':'md'}`),filters:message.format==='json'?{JSON:['json']}:{Markdown:['md']}});
                    if(uri){await vscode.workspace.fs.writeFile(uri,Buffer.from(text));this.notice='Export saved.';}
                }
                this.send();break;
            }
        }
    }
    dispose():void{this.visibilityChanged.dispose();this.stop();this.disposed=true;this.display.clear();this.releaseClient();for(const sub of this.subscriptions){sub.dispose();}}
    async removeProject(project:string):Promise<void>{
        if(this.session?.project===project){this.stop();this.releaseClient();this.session=undefined;this.status=undefined;}
        await this.saveQueue;await this.store.remove(project);this.send();
    }
    async focus(input?:string):Promise<void>{
        this.pendingInput=input;
        await vscode.commands.executeCommand('workbench.view.extension.airk-workbench');
        this.view?.show(false);
        if(this.view&&input){void this.view.webview.postMessage({type:'prefill',input});this.pendingInput=undefined;}
    }
}
