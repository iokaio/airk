// SPDX-License-Identifier: Apache-2.0
import * as vscode from 'vscode';
import * as path from 'node:path';
import { randomBytes, randomUUID } from 'node:crypto';
import type { ExplorerHost } from './explorerController';
import { EvidenceClient } from './explorerEvidence';
import { openSource, saveStudioInWorkspace } from './explorerWorkspaceIO';
import { strategyFromPlan } from './workbenchStrategies';
import { newSession, SessionStore } from './workbenchSessions';
import { continuations, evidenceCards, renderExport } from './workbenchRender';
import { object, parseWorkbenchMessage, Value, WorkbenchMessage, WorkbenchSession, WorkbenchState, WORKBENCH_HELP_ROUTE } from './workbenchContracts';

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
    private readonly visibilityChanged=new vscode.EventEmitter<void>();
    readonly onDidChangeVisibility=this.visibilityChanged.event;
    get visible():boolean{return this.view?.visible===true;}
    async toggle():Promise<void>{
        if(this.visible)await vscode.commands.executeCommand('workbench.action.closeAuxiliaryBar');
        else await this.focus();
    }
    private client?:EvidenceClient;
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
        this.subscriptions.push(host.onDidChangeSpecs(()=>{if(this.view){void this.refresh().catch(error=>this.fail(error));}}),vscode.workspace.onDidChangeConfiguration(event=>{
            if(event.affectsConfiguration('airk.workbench')){void this.privacyChanged().catch(error=>this.fail(error));}
        }));
    }
    private persistence():boolean{return vscode.workspace.getConfiguration('airk').get('workbench.persistSessions',true);}
    private persistResults():boolean{return vscode.workspace.getConfiguration('airk').get('workbench.persistResults',true);}
    private authorized():void{if(!this.host.trusted()||this.disposed){throw new Error('A trusted workspace is required.');}}
    resolveWebviewView(view:vscode.WebviewView):void{
        this.view=view;
        const visibility=view.onDidChangeVisibility(()=>this.visibilityChanged.fire());
        this.visibilityChanged.fire();
        view.webview.options={enableScripts:true,enableCommandUris:false,localResourceRoots:[vscode.Uri.joinPath(this.context.extensionUri,'out')]};
        view.webview.html=workbenchHtml(view.webview,this.context.extensionUri,randomBytes(24).toString('base64url'));
        const received=view.webview.onDidReceiveMessage(raw=>{const message=parseWorkbenchMessage(raw);if(message){void this.message(message).catch(error=>this.fail(error));}else{this.fail(new Error('Workbench request is invalid or exceeds its size limit.'));}});
        const release=view.onDidDispose(()=>{received.dispose();visibility.dispose();release.dispose();if(this.view===view){this.view=undefined;this.visibilityChanged.fire();this.stop();this.client?.dispose();this.client=undefined;}});
        void this.refresh().catch(error=>this.fail(error));
    }
    private state():WorkbenchState{
        return {projects:this.host.specs().flatMap(s=>s.service?[{id:s.service.project,name:s.name}]:[]),project:this.session?.project,session:this.session,recent:this.recent,
            cards:Object.fromEntries((this.session?.turns??[]).map(t=>[t.id,evidenceCards(t.envelope??{})])),continuations:Object.fromEntries((this.session?.turns??[]).map(t=>[t.id,continuations(t.envelope??{})])),busy:this.busy,notice:this.notice,save:this.saving,status:this.status,persistence:this.persistence(),operations:this.operations};
    }
    private send():void{if(!this.disposed){void this.view?.webview.postMessage({type:'state',state:this.state()});}}
    private fail(error:unknown):void{this.notice=error instanceof Error?error.message:String(error);this.send();}
    async refresh():Promise<void>{
        if(!this.host.trusted()){this.stop();this.client?.dispose();this.client=undefined;this.send();return;}
        const specs=this.host.specs();
        if(!this.session){const id=this.host.activeProject()??(specs.length===1?specs[0].service?.project:undefined);if(id){await this.select(id);return;}}
        if(this.session&&!specs.some(s=>s.service?.project===this.session!.project)){this.stop();this.client?.dispose();this.client=undefined;this.session=undefined;this.status=undefined;}
        this.send();
    }
    private async select(id:string):Promise<void>{
        this.authorized();if(!this.host.specs().some(s=>s.service?.project===id)){throw new Error('Project is not registered in this workspace.');}
        this.stop();this.client?.dispose();this.client=undefined;this.status=undefined;
        this.session=newSession(id);this.notice=undefined;
        this.recent=this.persistence()?(await this.store.list(id)).map(s=>({id:s.id,title:s.title,updatedAt:s.updatedAt,turns:s.turns.length})):[];this.send();
        const generation=this.generation;
        void this.evidence().call('airk_status',{}, {timeout:10000}).then(result=>{if(generation===this.generation&&this.host.trusted()){this.status=result.payload;this.send();}},error=>{if(generation===this.generation){this.fail(error);}});
    }
    private evidence():EvidenceClient{
        this.authorized();const spec=this.host.specs().find(s=>s.service?.project===this.session?.project);
        if(!spec){throw new Error('Choose a registered project.');}
        if(this.client&&JSON.stringify(this.client.spec)!==JSON.stringify(spec)){this.client.dispose();this.client=undefined;}
        return this.client??=new EvidenceClient(spec,this.host.version,{workbench:true,clientName:'airk-workbench',timeout:35000});
    }
    stop():void{this.generation++;if(this.busy){const turn=this.session?.turns.at(-1);if(turn&&!turn.envelope){turn.error='Request stopped. Already admitted native reads may still finish; late replies are discarded.';}}this.busy=false;this.client?.cancel('workbench');this.send();}
    async newSession():Promise<void>{if(this.session){await this.select(this.session.project);}else{await this.refresh();}}
    async clear():Promise<void>{
        this.authorized();if(!this.session){return;}
        const selected=this.session.project;
        const choice=await vscode.window.showWarningMessage('Delete saved Workbench sessions for this project?',{modal:true},'Delete sessions');
        if(choice==='Delete sessions'&&this.session?.project===selected){this.stop();await this.saveQueue;await this.store.remove(selected);await this.select(selected);}
    }
    private save(session:WorkbenchSession):void{
        if(!this.persistence()){this.saving='Session persistence off';this.send();return;}
        this.saving='Saving…';const captured=structuredClone(session),results=this.persistResults();
        this.saveQueue=this.saveQueue.then(async()=>{
            // Preserve revisions after earlier saves queued by the same window.
            captured.revision=session.revision;
            await this.store.save(captured,results);session.revision=captured.revision;
            if(this.session===session){this.recent=(await this.store.list(session.project)).map(s=>({id:s.id,title:s.title,updatedAt:s.updatedAt,turns:s.turns.length}));this.saving=results?'Auto-saved':'Inputs saved; results excluded';this.send();}
        }).catch(error=>{this.saving='Save failed — export or retry';this.fail(error);});
    }
    private async privacyChanged():Promise<void>{
        await this.saveQueue;
        if(this.session&&(!this.persistence()||!this.persistResults())){
            if(!this.persistence()){await this.store.remove(this.session.project);}else{await this.store.clearResults(this.session.project);}
        }
        this.send();
    }
    private async run(input:string,prepare=false,recipe?:string,plan?:Value,pathPrefix?:string):Promise<void>{
        this.authorized();if(!this.session){throw new Error('Choose a project first.');}
        this.stop();const generation=this.generation,session=this.session,client=this.evidence();this.busy=true;this.notice=undefined;
        const turn={id:randomUUID(),at:Date.now(),input:(input||`${recipe??'Plan'} evidence`).slice(0,4096),format:'structured' as const,request:undefined as Value|undefined,envelope:undefined as Value|undefined,error:undefined as string|undefined};
        session.turns.push(turn);if(session.turns.length===1){session.title=turn.input.replaceAll('\n',' ').slice(0,80);}this.send();
        try{
            if(!plan&&!recipe&&this.persistence()){
                const shortcut=await this.store.findShortcut(session.project,input,pathPrefix??'');
                if(shortcut){plan=shortcut;this.notice='Using an explicitly remembered plan; all steps are validated against current evidence.';}
            }
            if(generation!==this.generation||session!==this.session||!this.host.trusted()){return;}
            const request:Value={schema:'airk.parse.request/1',mode:prepare?'prepare':'execute',...(pathPrefix?{path_prefix:pathPrefix}:{})};
            if(plan){request.plan=plan;}else if(recipe){request.recipe={id:recipe,parameters:input.trim()?{name:input.trim()}:{}};}else{request.text=input;}
            turn.request=structuredClone(request);
            const result=await client.call('airk_parse',request,{supersede:'workbench',timeout:35000});
            if(generation!==this.generation||session!==this.session||!this.host.trusted()){return;}
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
            case 'refresh':{await this.refresh();if(this.session){const generation=this.generation;const result=await this.evidence().call('airk_status',{});if(generation===this.generation){this.status=result.payload;this.send();}}break;}
            case 'new':await this.newSession();break;
            case 'stop':this.stop();if(this.session){this.save(this.session);}break;
            case 'clear':await this.clear();break;
            case 'manage':await vscode.commands.executeCommand('airk.explorer.open','manage/overview');break;
            case 'forms':{const generation=this.generation;const reply=await this.evidence().call('airk_capabilities',{});if(generation===this.generation){this.operations=object(object(reply.payload.workbench).operations);this.send();}break;}
            case 'retrySave':if(this.session){this.save(this.session);}break;
            case 'fork':if(this.session){this.stop();const fork={...structuredClone(this.session),id:randomUUID(),revision:0,title:`${this.session.title.slice(0,70)} (copy)`};this.session=fork;this.save(fork);this.send();}break;
            case 'recent':if(this.session){this.stop();await this.saveQueue;this.session=await this.store.load(this.session.project,message.id);this.send();}break;
            case 'delete':if(this.session){await this.saveQueue;await this.store.remove(this.session.project,message.id);await this.newSession();}break;
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
                    const result=await openSource({root:source.root,relative:source.relative_path,contentHash:card.hash,startLine:card.start,lineBasis:typeof info.line_basis==='string'?info.line_basis:undefined});this.notice=result.message;this.send();
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
    dispose():void{this.visibilityChanged.dispose();this.stop();this.disposed=true;this.client?.dispose();for(const sub of this.subscriptions){sub.dispose();}}
    async removeProject(project:string):Promise<void>{
        if(this.session?.project===project){this.stop();this.client?.dispose();this.client=undefined;this.session=undefined;this.status=undefined;}
        await this.saveQueue;await this.store.remove(project);this.send();
    }
    async focus(input?:string):Promise<void>{
        this.pendingInput=input;
        await vscode.commands.executeCommand('workbench.view.extension.airk-workbench');
        this.view?.show(false);
        if(this.view&&input){void this.view.webview.postMessage({type:'prefill',input});this.pendingInput=undefined;}
    }
}
