// SPDX-License-Identifier: Apache-2.0
import {NativeMetadataProjectionCache} from './nativeMetadata';
import * as vscode from 'vscode';
import * as path from 'node:path';
import { comparePublication, comparePublicationVector, type PublicationObservation, type PublicationRelation } from './publications';
import { randomBytes, randomUUID } from 'node:crypto';
import type { ExplorerHost } from './explorerController';
import type { LaunchSpec } from './model';
import type { SearchDraft } from './explorerDrafts';
import { combinedScopeSelectionIdentity, observedScopeSelectionIdentity, scopeSelectionIdentity } from './scopeSelection';
import { EvidenceClient, EvidenceFailure } from './explorerEvidence';
import { nativeSavedSourceState, readResultBody, verificationLabel } from './sourceVerification';
import { SharedCapabilityDisplayCache } from './sharedCapabilityFields';
import { explorerError } from './explorerErrors';
import { openSource, saveStudioInWorkspace } from './explorerWorkspaceIO';
import { assertOriginalPublicationForSymbolInputs, assertPortablePlanInputs, planForWorkbenchRerun, strategyFromPlan } from './workbenchStrategies';
import { newSession, SessionStore } from './workbenchSessions';
import { CurrentTurnProjections, combinedReadDetails, continuationCursor, continuationRequiredRecordBytes, continuations, discoveryFollowupPlan, evidenceCards, evidenceRows, renderExport } from './workbenchRender';
import { defaultWorkbenchFormDraft, normalizeWorkbenchFormDraft, object, parseWorkbenchMessage, Value, WorkbenchMessage, WorkbenchSession, WorkbenchState, WORKBENCH_HELP_ROUTE } from './workbenchContracts';

/** Separate saved histories for explicit scopes; native backing remains service.project. */
export function workbenchProjectKey(spec:LaunchSpec):string|undefined {
    return spec.combinedSession?combinedScopeSelectionIdentity(spec.combinedSession):spec.catalogSession?scopeSelectionIdentity(spec.catalogSession):spec.service?.project;
}

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
    private readonly metadata=new NativeMetadataProjectionCache();
    private logical(turn:{envelope?:Value}):Value {if(!turn.envelope)return {};const view=this.metadata.view(turn.envelope);if(!view.payload)throw new Error(view.error);return view.payload;}
    private readonly capabilityDisplay=new SharedCapabilityDisplayCache();
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
    private sessionEpoch=0;
    private session?:WorkbenchSession;
    private sessions?:SessionStore;
    private get store():SessionStore{return this.sessions??=new SessionStore(path.join(this.context.globalStorageUri.fsPath,'workbench'));}
    private readonly subscriptions:vscode.Disposable[]=[];
    private recent:WorkbenchState['recent']=[];
    private busy=false;
    private notice?:string;
    private sourceOpen?:{id:string;generation:number;project:string;observation:Value};
    private saving='';
    private status?:Value;
    private statusAuthority?:string;
    // Live object identity is intentional: loaded/forked saved turns have no authority proof.
    private readonly turnAuthorities=new WeakMap<object,string>();
    private publicationRelations():Record<string,PublicationRelation>{
        const observed=this.permitted()&&this.status&&this.statusAuthority&&this.statusAuthority===JSON.stringify(this.selectedSpec())?this.publicationObservation(this.status,this.statusAuthority):undefined;
        return Object.fromEntries((this.session?.turns??[]).map(turn=>{
            const authority=this.turnAuthorities.get(turn);
            if(this.selectedSpec()?.combinedSession){
                const current=this.permitted()&&this.status&&this.statusAuthority===JSON.stringify(this.selectedSpec())?this.combinedPublicationObservation(this.status,this.statusAuthority!):undefined;
                return [turn.id,comparePublicationVector(turn.envelope&&authority?this.combinedPublicationObservation(turn.envelope,authority):undefined,current)];
            }
            return [turn.id,comparePublication(turn.envelope&&authority?this.publicationObservation(turn.envelope,authority):undefined,observed)];
        }));
    }
    private combinedPublicationObservation(value:Value,captured:string):{authority:string;vector:unknown}|undefined {
        try{
            const spec=JSON.parse(captured) as LaunchSpec;if(!spec.combinedSession)return;
            const authority=JSON.stringify([spec.command,spec.service?.installDir,combinedScopeSelectionIdentity(spec.combinedSession)]);
            const vectors=[value.publication_vector,object(value.member_verification).publication_vector,
                ...combinedReadDetails(value).map(detail=>detail.result.publication_vector)].filter(vector=>vector!==undefined);
            if(!vectors.length)return;
            const first={authority,vector:vectors[0]};
            // Multiple step observations can be compared only when all retain
            // exactly the same member publications and authorization identity.
            if(vectors.some(vector=>comparePublicationVector(first,{authority,vector})!=='equal'))return;
            return first;
        }catch{return undefined;}
    }
    private publicationObservation(value:Value,authority:string):PublicationObservation|undefined{
        const collection=typeof value.collection==='string'?value.collection:typeof object(value.plan).collection==='string'?String(object(value.plan).collection):undefined;
        if(!collection)return undefined;
        try{
            const spec=JSON.parse(authority) as LaunchSpec;
            if(spec.catalogSession){authority=JSON.stringify([spec.command,spec.service?.installDir,observedScopeSelectionIdentity(spec.catalogSession,value.scope_observation)]);}
            return {authority,collection,publication:value.publication};
        }catch{return undefined;}
    }
    private observeStatus(value:Value,authority:string):void{this.status=value;this.statusAuthority=authority;}

    private operations?:Value;
    private saveQueue=Promise.resolve();
    private readonly queuedFormSaves=new WeakSet<WorkbenchSession>();
    private disposed=false;
    private pendingInput?:string;
    private pendingForm=false;
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
    private selectedSpec():LaunchSpec|undefined{return this.host.specs().find(spec=>workbenchProjectKey(spec)===this.session?.project);}
    /** Invalidate synchronously before refresh/list awaits, including hidden views. */
    private invalidateChangedClient():void{
        const spec=this.selectedSpec();
        if(!this.permitted()||(this.session&&!spec)||(this.client&&this.clientIdentity!==JSON.stringify(spec))){
            this.sessionEpoch++;this.stop();this.releaseClient();this.status=undefined;this.operations=undefined;
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
        return {projects:this.host.specs().flatMap(s=>{const id=workbenchProjectKey(s);return id?[{id,name:s.name}]:[];}),project:this.session?.project,session:this.session,sessionEpoch:this.sessionEpoch,recent:this.recent,
            ...this.display.project(this.session?.turns??[],turn=>this.metadata.view(turn.envelope??{}).payload??{}),metadataViews:Object.fromEntries((this.session?.turns??[]).filter(turn=>turn.envelope).map(turn=>[turn.id,this.metadata.view(turn.envelope!).payload??{}])),metadataIssues:Object.fromEntries((this.session?.turns??[]).filter(turn=>turn.envelope&&this.metadata.view(turn.envelope).error).map(turn=>[turn.id,this.metadata.view(turn.envelope!).error!])),capabilityDetails:this.capabilityDisplay.project(this.session?.turns??[]),busy:this.busy,notice:this.notice,sourceOpen:this.sourceOpen?.generation===this.generation&&this.sourceOpen.project===this.session?.project?{id:this.sourceOpen.id,observation:this.sourceOpen.observation}:undefined,save:this.saving,status:this.status,publicationRelations:this.publicationRelations(),persistence:this.persistence(),operations:this.operations};
    }
    private send():void{if(!this.disposed){void this.view?.webview.postMessage({type:'state',state:this.state()});}}
    private sendStatus():void{if(!this.disposed){void this.view?.webview.postMessage({type:'status',sessionId:this.session?.id,project:this.session?.project,status:this.status,publicationRelations:this.publicationRelations()});}}
    private sendSaved(session:WorkbenchSession):void{if(!this.disposed&&this.session===session){void this.view?.webview.postMessage({type:'saved',sessionId:session.id,project:session.project,revision:session.revision,recent:this.recent,save:this.saving});}}
    private sendFormDraft(session:WorkbenchSession):void{
        if(!this.disposed&&this.session===session){void this.view?.webview.postMessage({type:'formDraft',sessionId:session.id,project:session.project,epoch:this.sessionEpoch,formDraft:session.formDraft??defaultWorkbenchFormDraft(),save:this.saving});}
    }

    private fail(error:unknown):void{this.notice=error instanceof Error?error.message:String(error);this.send();}
    async refresh():Promise<void>{
        this.invalidateChangedClient();
        if(!this.permitted()){this.send();return;}
        const specs=this.host.specs();
        if(!this.session){const active=this.host.activeProject(),legacy=specs.find(spec=>!spec.catalogSession&&!spec.combinedSession&&spec.service?.project===active);const id=legacy?workbenchProjectKey(legacy):specs.length===1?workbenchProjectKey(specs[0]):undefined;if(id){await this.select(id);return;}}
        if(this.session&&!specs.some(s=>workbenchProjectKey(s)===this.session!.project)){this.stop();this.releaseClient();this.session=undefined;this.status=undefined;}
        this.send();
    }
    /** Host calls only after an explicit reviewed scope choice is exposed in specs. */
    async useScope(spec:LaunchSpec):Promise<void>{
        const id=workbenchProjectKey(spec);
        if((!spec.catalogSession&&!spec.combinedSession)||!id||!this.host.specs().some(value=>JSON.stringify(value)===JSON.stringify(spec)))throw new Error('Reviewed scope is not available in this workspace.');
        this.invalidateChangedClient();
        if(this.session?.project===id){await this.refresh();return;}
        await this.select(id);
    }
    private async select(id:string):Promise<void>{
        this.authorized();const spec=this.host.specs().find(value=>workbenchProjectKey(value)===id);if(!spec){throw new Error('Project is not registered in this workspace.');}
        const identity=JSON.stringify(spec);
        this.stop();
        if(this.client&&this.clientIdentity!==identity){this.releaseClient();}
        this.sessionEpoch++;this.status=undefined;this.session=newSession(id);this.recent=[];this.notice=undefined;
        const generation=this.generation,session=this.session;
        const current=()=>generation===this.generation&&session===this.session&&this.localAllowed()&&JSON.stringify(this.selectedSpec())===identity;
        this.send();
        if(this.permitted()){
            const client=this.evidence();
            // Readiness and local history have independent owners. A slow saved
            // session scan must not delay this transcript's first native status.
            void client.call('airk_status',{}, {timeout:STATUS_RESPONSE_TIMEOUT_MS}).then(result=>{if(current()&&client===this.client&&this.permitted()){this.observeStatus(result.payload,identity);this.sendStatus();}},error=>{if(current()&&client===this.client&&this.permitted()){this.fail(error);}});
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
            this.sessionEpoch++;this.stop();const accepted=this.generation;
            // The already authorized deletion keeps its exact project even if a
            // later selection supersedes it; it never restores that old selection.
            await this.saveQueue;await this.store.remove(selected);
            if(accepted===this.generation&&this.session===session&&this.localAllowed()&&JSON.stringify(this.selectedSpec())===identity){if(this.selectedSpec()){await this.select(selected);}else{this.session=undefined;this.recent=[];this.status=undefined;this.send();}}
        }
    }
    private save(session:WorkbenchSession,formOnly=false):void{
        if(!this.persistence()){this.saving='Session persistence off';if(!formOnly)this.send();return;}
        // Coalesce rapid inert input updates behind the same serialized writer.
        // No timer, second owner or extra inventory/worker observation is added.
        if(formOnly&&this.queuedFormSaves.has(session))return;
        if(formOnly)this.queuedFormSaves.add(session);
        this.saving='Saving…';const captured=formOnly?undefined:structuredClone(session),results=this.persistResults();
        this.saveQueue=this.saveQueue.then(async()=>{
            if(formOnly)this.queuedFormSaves.delete(session);
            const writing=captured??structuredClone(session);
            // A queued result snapshot must never roll newer form choices back.
            writing.formDraft=structuredClone(session.formDraft??defaultWorkbenchFormDraft());
            // Preserve revisions after earlier saves queued by the same window.
            writing.revision=session.revision;
            const recent=await this.store.save(writing,results);session.revision=writing.revision;
            if(this.session!==session){return;}
            const summaries=recent??(await this.store.list(session.project)).map(s=>({id:s.id,title:s.title,updatedAt:s.updatedAt,turns:s.turns.length}));
            // Empty saves retain the list fallback; a new transcript can arrive during it.
            if(this.session!==session){return;}
            this.recent=summaries;this.saving=results?'Auto-saved':'Inputs saved; results excluded';this.sendSaved(session);
        }).catch(error=>{this.saving='Save failed — export or retry';this.fail(error);});
    }
    private async loadRecent(id:string):Promise<void>{
        const session=this.session;if(!session){return;}
        this.sessionEpoch++;this.stop();const generation=this.generation,identity=JSON.stringify(this.selectedSpec());
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
        this.sessionEpoch++;this.stop();const generation=this.generation,identity=JSON.stringify(this.selectedSpec());
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
        // Validate before electing a client or stopping an outstanding request.
        // Form values stay saved, but literal IDs must retain their own fence.
        if(plan)assertOriginalPublicationForSymbolInputs(plan);
        this.stop();const session=this.session,client=this.evidence(),generation=this.generation;this.busy=true;this.notice=undefined;
        const turn={id:randomUUID(),at:Date.now(),input:(input||`${recipe??'Plan'} evidence`).slice(0,4096),format:'structured' as const,request:undefined as Value|undefined,envelope:undefined as Value|undefined,error:undefined as string|undefined,nativeFailure:undefined as Value|undefined};
        session.turns.push(turn);if(session.turns.length===1){session.title=turn.input.replaceAll('\n',' ').slice(0,80);}this.send();
        try{
            let remembered=false;
            if(!plan&&!recipe&&this.persistence()){
                const shortcut=await this.store.findShortcut(session.project,input,pathPrefix??'');
                if(shortcut){plan=shortcut;remembered=true;}
            }
            if(generation!==this.generation||session!==this.session||client!==this.client||this.clientIdentity!==JSON.stringify(this.selectedSpec())||!this.permitted()){return;}
            if(plan)assertOriginalPublicationForSymbolInputs(plan);
            if(remembered){this.notice='Using an explicitly remembered plan; all steps are validated against current evidence.';}
            const request:Value={schema:'airk.parse.request/1',mode:prepare?'prepare':'execute',...(pathPrefix?{path_prefix:pathPrefix}:{})};
            if(plan){request.plan=plan;if(this.selectedSpec()?.combinedSession&&plan.query_scope){const scope=object(plan.query_scope);for(const key of ['project_ids','index_ids','member_availability'])if(scope[key]!==undefined)request[key]=structuredClone(scope[key]);}}else if(recipe){request.recipe={id:recipe,parameters:input.trim()?{name:input.trim()}:{}};}else{request.text=input;}
            turn.request=structuredClone(request);
            const result=await client.call('airk_parse',request,{supersede:'workbench',timeout:35000});
            if(generation!==this.generation||session!==this.session||client!==this.client||this.clientIdentity!==JSON.stringify(this.selectedSpec())||!this.permitted()){return;}
            turn.envelope=result.rawPayload??result.payload;if(result.rawPayload)this.metadata.remember(result.rawPayload,result.payload);this.turnAuthorities.set(turn,this.clientIdentity!);
            if(result.payload.status==='prepared'){this.notice='Plan prepared. Review or edit it, then choose Run plan.';}
        }catch(error){if(generation===this.generation){turn.error=error instanceof Error?error.message:String(error);if(error instanceof EvidenceFailure&&error.nativeResponse){turn.error=explorerError(error.error.code).message;turn.nativeFailure=structuredClone(error.nativeResponse);}}}
        finally{if(generation===this.generation){this.busy=false;session.updatedAt=Date.now();this.save(session);this.send();}}
    }
    private async message(message:WorkbenchMessage):Promise<void>{
        if(message.type==='ready'){await this.refresh();if(this.pendingInput){void this.view?.webview.postMessage({type:'prefill',input:this.pendingInput});this.pendingInput=undefined;}if(this.pendingForm){void this.view?.webview.postMessage({type:'prefillForm'});this.pendingForm=false;}return;}
        if(message.type==='help'){await vscode.commands.executeCommand('airk.explorer.open',WORKBENCH_HELP_ROUTE);return;}
        this.authorized();
        switch(message.type){
            case 'project':await this.select(message.id);break;
            case 'refresh':{const session=this.session;await this.refresh();if(this.session&&this.session===session){const client=this.evidence(),generation=this.generation,identity=this.clientIdentity;const result=await client.call('airk_status',{}, {timeout:STATUS_RESPONSE_TIMEOUT_MS});if(generation===this.generation&&session===this.session&&client===this.client&&identity===this.clientIdentity&&identity===JSON.stringify(this.selectedSpec())&&this.permitted()){this.observeStatus(result.payload,identity!);this.sendStatus();}}break;}
            case 'new':await this.newSession();break;
            case 'stop':this.stop();if(this.session){this.save(this.session);}break;
            case 'clear':await this.clear();break;
            case 'manage':await vscode.commands.executeCommand('airk.explorer.open','manage/overview');break;
            case 'forms':{const generation=this.generation;const reply=await this.evidence().call('airk_capabilities',{});if(generation===this.generation&&this.permitted()){this.operations=object(object(reply.payload.workbench).operations);this.send();}break;}
            case 'retrySave':if(this.session){this.save(this.session);}break;
            case 'fork':if(this.session){this.sessionEpoch++;this.stop();const fork={...structuredClone(this.session),id:randomUUID(),revision:0,title:`${this.session.title.slice(0,70)} (copy)`,formDraft:defaultWorkbenchFormDraft()};this.session=fork;this.save(fork);this.send();}break;
            case 'formDraft':case 'resetForm':{
                const session=this.session,current=session&&normalizeWorkbenchFormDraft(session.formDraft);
                if(!session||!current||!this.selectedSpec()||message.sessionId!==session.id||message.project!==session.project||message.epoch!==this.sessionEpoch
                    ||!Number.isSafeInteger(message.revision)||message.revision<=current.revision)return;
                if(message.type==='resetForm'&&message.tool!==current.activeTool)return;
                const values={...current.values};
                if(message.type==='resetForm')delete values[message.tool];
                else if(message.values!==undefined)values[message.tool]={...message.values};
                const next=normalizeWorkbenchFormDraft({...current,activeTool:message.tool,values,revision:message.revision,
                    ...(message.type==='formDraft'&&message.scope!==undefined?{scope:message.scope}:{}),
                    ...(message.type==='formDraft'&&message.recipe!==undefined?{recipe:message.recipe}:{})});
                if(!next)throw new Error('Workbench form choices are invalid or exceed the saved-input limits.');
                session.formDraft=next;session.updatedAt=Date.now();this.save(session,true);this.sendFormDraft(session);break;
            }
            case 'recent':await this.loadRecent(message.id);break;
            case 'delete':await this.deleteRecent(message.id);break;
            case 'rename':if(this.session){this.session.title=message.title;this.save(this.session);this.send();}break;
            case 'run':await this.run(message.input,message.prepare,message.recipe,message.plan,message.pathPrefix);break;
            case 'rerun':{const turn=this.session?.turns.find(t=>t.id===message.turn);if(turn){
                // Parsed text can also have produced an exact native symbol ID;
                // inspect its retained plan before discarding any revision fence.
                assertPortablePlanInputs(object(object(turn.envelope).plan));
                const request=object(turn.request),plan=request.plan?planForWorkbenchRerun(object(request.plan)):undefined;
                await this.run(turn.input,false,typeof object(request.recipe).id==='string'?String(object(request.recipe).id):undefined,plan,typeof request.path_prefix==='string'?request.path_prefix:undefined);
            }break;}
            case 'saveStrategy':{
                const turn=this.session?.turns.find(t=>t.id===message.turn);if(!turn?.envelope){throw new Error('Collect or prepare evidence first.');}
                const generation=this.generation,spec=this.evidence().spec,document=strategyFromPlan(object(turn.envelope.plan),turn.input,turn.input);
                const name=await vscode.window.showInputBox({title:'Save Workbench inputs to Strategies',prompt:'Name using lowercase letters, numbers and hyphens. The Strategy uses its own disclosed evidence budgets; cached evidence is excluded.'});
                if(name){const saved=await saveStudioInWorkspace(document,name,spec.root,undefined,()=>this.host.trusted()&&this.generation===generation);this.notice=saved.saved?'Saved inputs to the guarded Strategies library.':saved.reason;this.send();}break;
            }
            case 'remember':{const turn=this.session?.turns.find(t=>t.id===message.turn);if(!this.session||!turn?.envelope||!this.persistence()){throw new Error('Prepare a plan and enable session persistence before remembering it.');}await this.store.remember(this.session.project,turn.input,object(turn.envelope.plan));this.notice='Remembered the accepted inputs for this project and scope. Evidence and revision handles were excluded.';this.send();break;}
            case 'continue':{
                const turn=this.session?.turns.find(t=>t.id===message.turn),envelope=turn?.envelope?this.logical(turn):undefined;
                if(!envelope||!continuations(envelope).some(c=>c.id===message.step)){throw new Error('No bounded continuation remains for this result.');}
                const index=Number(message.step.slice(5)),report=object((envelope.steps as unknown[])[index]),result=object(report.result),prior=object(envelope.plan),budgets=object(prior.budgets);
                const remaining=Number(budgets.max_bytes??524288)-Number(envelope.payload_bytes??0);if(remaining<1024){throw new Error('The original evidence byte budget is exhausted; start a scoped new request.');}
                const args:Value={...object(report.arguments),cursor:continuationCursor(result)},required=continuationRequiredRecordBytes(result);
                if(required!==undefined&&required>Number(args.max_bytes??8192)){
                    if(required>65536||required+512>remaining)throw new Error('The next record cannot fit the remaining evidence budget. Start a scoped request with a larger budget or read its source range.');
                    args.max_bytes=required;
                }
                const plan={schema:'airk.parse.plan/1',...(prior.query_scope?{query_scope:prior.query_scope,scope_identity:prior.scope_identity}:{publication:envelope.publication}),collection:prior.collection,path_prefix:prior.path_prefix,budgets:{...budgets,calls:Number(budgets.calls??12)-Number(envelope.calls??0),max_bytes:remaining},steps:[{id:'continuation',tool:report.tool,args}]};
                await this.run(`Continue ${String(report.tool)} from ${turn!.input}`,false,undefined,plan);break;
            }
            case 'followTarget':case 'followDefinition':{
                const turn=this.session?.turns.find(value=>value.id===message.turn);
                if(!turn?.envelope)throw new Error('The discovery result is no longer retained.');
                const target=message.type==='followTarget'?message.target:message.name;
                const plan=discoveryFollowupPlan(this.logical(turn),message.step,target,message.type==='followDefinition'?message.path:undefined);
                await this.run(`${message.type==='followTarget'?'Call candidates for':'Definition of'} ${target}`,false,undefined,plan);break;
            }
            case 'open':case 'context':{
                const turn=this.session?.turns.find(t=>t.id===message.turn),card=turn&&evidenceCards(this.logical(turn)).find(c=>c.id===message.card);
                if(!card){throw new Error('Evidence handle expired.');}
                const selected=card.indexId&&card.projectIds?.length?{project_ids:card.projectIds,index_ids:[card.indexId]}:undefined;
                if(this.selectedSpec()?.combinedSession&&!selected)throw new Error('Choose an originating index from qualified evidence before following this location.');
                if(message.type==='context'){
                    const plan={schema:'airk.parse.plan/1',...(selected?{query_scope:selected}:{publication:card.publication}),steps:[{id:'context',tool:'context',args:{path:card.path,line:card.start,known_hash:card.hash,before:3,after:3,...(selected?{revision:card.publication}:{})}}]};
                    await this.run(`Context ${card.path}:${card.start}`,false,undefined,plan);
                }else{
                    const generation=this.generation,native=await this.evidence().call('airk_file_info',{path:card.path,response_mode:'full',...(selected?{...selected,revision:card.publication,known_hash:card.hash}:{})}),payload=native.payload;
                    const members=Array.isArray(payload.members)?payload.members.map(object):[];
                    const origin=selected?members.filter(member=>member.index_id===card.indexId&&member.outcome==='complete'):[];
                    if(selected&&origin.length!==1)throw new Error('The originating index did not return a complete source location.');
                    const info=selected?readResultBody(origin[0].result):payload,source=object(info.source);
                    if(selected){
                        const returned=evidenceRows(payload).map(row=>object(row.origin)).filter(value=>value.index_id===card.indexId);
                        if(!card.sourceRootId||!card.sourceRelativePath||returned.length!==1||returned[0].source_root_id!==card.sourceRootId
                            ||returned[0].source_relative_path!==card.sourceRelativePath||source.relative_path!==card.sourceRelativePath)throw new Error('The originating source identity changed before opening this location.');
                    }
                    if(generation!==this.generation){throw new Error('Project changed before opening the source.');}this.authorized();
                    if(typeof source.root!=='string'||typeof source.relative_path!=='string'){throw new Error('AIRK did not report the original source location.');}
                    const result=await openSource({root:source.root,relative:source.relative_path,contentHash:card.hash,startLine:card.start,lineBasis:typeof info.line_basis==='string'?info.line_basis:undefined,
                        savedState:nativeSavedSourceState(info,card.hash,selected?card.publication:native.publication),nativeResult:native.rawPayload??native.payload,
                        nativeMessage:info.content_hash!==card.hash?'The indexed hash changed since this citation; a saved-source comparison for that older hash is unavailable.':verificationLabel(info.source_verification)},vscode.ViewColumn.Active);
                    if(generation!==this.generation)throw new Error('Project changed while opening the source.');this.authorized();
                    this.sourceOpen={id:randomUUID().replaceAll('-',''),generation,project:this.session!.project,observation:{path:card.path,expected_hash:card.hash,response:result}};this.notice=result.message;this.send();
                }break;
            }
            case 'exportSourceOpen':{
                const captured=this.sourceOpen,current=()=>{this.authorized();if(!captured||captured.id!==message.id||captured.generation!==this.generation||captured.project!==this.session?.project)throw new Error('The source-open observation is no longer selected.');};current();
                const text=JSON.stringify(captured!.observation,null,2)+'\n';
                if(message.format==='copy'){await vscode.env.clipboard.writeText(text);current();break;}
                const uri=await vscode.window.showSaveDialog({title:'Export dated source-open observation',filters:{JSON:['json']}});if(!uri)break;current();await vscode.workspace.fs.writeFile(uri,new TextEncoder().encode(text));break;
            }
            case 'copy':case 'export':{
                if(!this.session){return;}
                const text=renderExport(this.session,message.format,message.turn,message.cards,turn=>this.metadata.view(turn.envelope??{}).payload??{}),bytes=Buffer.byteLength(text,'utf8');
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
    dispose():void{this.visibilityChanged.dispose();this.stop();this.disposed=true;this.display.clear();this.metadata.clear();this.capabilityDisplay.clear();this.releaseClient();for(const sub of this.subscriptions){sub.dispose();}}
    async removeProject(project:string):Promise<void>{
        if(this.session?.project===project){this.sessionEpoch++;this.stop();this.releaseClient();this.session=undefined;this.status=undefined;}
        await this.saveQueue;await this.store.remove(project);this.send();
    }
    async focus(input?:string):Promise<void>{
        this.pendingInput=input;
        await vscode.commands.executeCommand('workbench.view.extension.airk-workbench');
        this.view?.show(false);
        if(this.view&&input){void this.view.webview.postMessage({type:'prefill',input});this.pendingInput=undefined;}
    }
    /** Explicit Inbox Apply only: stores editable inputs, never prepares or runs
     * evidence and never restores a grant or another window's saved defaults. */
    async prefillSearchForm(spec:LaunchSpec,search:SearchDraft,reviewedSubset=false):Promise<void>{
        this.authorized();const key=workbenchProjectKey(spec);
        if(!spec.combinedSession||!key||!this.host.specs().some(value=>JSON.stringify(value)===JSON.stringify(spec)))throw new Error('The combined scope changed before applying the search draft.');
        if(reviewedSubset){
            if(!search.project_ids?.length||!search.index_ids?.length||search.project_ids.some(id=>!spec.combinedSession!.project_ids.includes(id))
                ||spec.combinedSession.index_ids&&search.index_ids.some(id=>!spec.combinedSession!.index_ids!.includes(id)))throw new Error('The recommended scope is outside the frozen connection.');
        }else if(!search.project_ids||JSON.stringify([...search.project_ids].sort())!==JSON.stringify(spec.combinedSession.project_ids)
            ||JSON.stringify(search.index_ids?[...search.index_ids].sort():null)!==JSON.stringify(spec.combinedSession.index_ids??null))throw new Error('The search draft does not match the frozen combined scope.');
        if(!this.session||this.session.project!==key){this.stop();this.releaseClient();this.sessionEpoch++;this.session=newSession(key);this.recent=[];this.status=undefined;this.operations=undefined;}
        const session=this.session,prior=normalizeWorkbenchFormDraft(session.formDraft)??defaultWorkbenchFormDraft();
        const scope=search as SearchDraft&{project_ids?:string[];index_ids?:string[];member_availability?:string};
        const selectors={...(scope.project_ids?{project_ids:JSON.stringify(scope.project_ids)}:{}),...(scope.index_ids?{index_ids:JSON.stringify(scope.index_ids)}:{})};
        const values:Record<string,string>=reviewedSubset?{
            ...(prior.values.search??{}),...selectors,
            ...(!prior.values.search?.member_availability&&scope.member_availability?{member_availability:scope.member_availability}:{}),
        }:{query:search.query,limit:String(search.limit),max_bytes:String(search.maxBytes),full_chunks:String(search.fullChunks),
            ...(search.mode!=='policy'?{mode:search.mode}:{}),...(search.pathPrefix?{path_prefix:search.pathPrefix}:{}),...selectors,
            ...(scope.member_availability?{member_availability:scope.member_availability}:{})};
        const draft=normalizeWorkbenchFormDraft({...prior,activeTool:'search',values:{...prior.values,search:values},revision:prior.revision+1});
        if(!draft)throw new Error('The search draft exceeds the saved form limits.');
        session.formDraft=draft;session.updatedAt=Date.now();this.notice='Search draft applied to this combined scope. Review it in Tool forms, then choose Prepare or Run.';
        this.pendingForm=true;this.save(session,true);this.send();await this.focus();
        if(this.view){void this.view.webview.postMessage({type:'prefillForm'});this.pendingForm=false;}
    }
}
