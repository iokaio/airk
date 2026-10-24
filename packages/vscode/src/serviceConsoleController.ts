// SPDX-License-Identifier: Apache-2.0
import { randomUUID } from 'node:crypto';
import { BootstrapObservation, RuntimeBinding, RuntimeHost } from './serviceBootstrap';
import { count, identifier, record, RecordValue, ServiceClient, ServiceError } from './serviceClient';
import { ConsoleIntent, IntentJournal, IntentStorage, LifecycleIntent } from './serviceIntents';
import { consoleAction, ConsoleObservation, diagnosticPreview, ServiceConsoleState } from './serviceConsoleView';

export interface ConsoleHost {
    version:string; authority:string; storage:IntentStorage;
    runtime:Pick<RuntimeHost,'discover'|'binding'|'ensure'|'client'>;
    trusted():boolean;
    changed(state:ServiceConsoleState):void;
    confirm(message:string):Promise<boolean>;
    copy(text:string):Promise<void>;
    logs(observation?:BootstrapObservation):Promise<void>;
    resources(current:RecordValue):Promise<RecordValue|undefined>;
    lifecycle(binding:RuntimeBinding,intent:LifecycleIntent,force?:boolean):Promise<RecordValue>;
    abandonLifecycle?(binding:RuntimeBinding,intent:LifecycleIntent,id:string):Promise<RecordValue>;
    responseMode?():'terse'|'full';
    chooseResponseMode?():Promise<void>;
}
const empty=():ConsoleObservation=>({state:'unavailable',checkedAt:Date.now()});
function code(error:unknown):string{return error instanceof ServiceError?error.code:'console_operation_failed';}
export class ServiceConsoleController {
    readonly state:ServiceConsoleState;
    private binding?:RuntimeBinding;
    private bootstrap?:BootstrapObservation;
    private client?:ServiceClient;
    private journal?:IntentJournal;
    private eventCursor:unknown=null;
    private pageStart:{after:string|null;revision:number|null}={after:null,revision:null};
    private disposed=false;
    private readonly idle=new Set<(available:boolean)=>void>();
    constructor(private readonly host:ConsoleHost){
        this.state={revision:'',trusted:host.trusted(),busy:false,authority:host.authority,version:host.version,connected:false,
            health:empty(),resources:empty(),model:empty(),sources:empty(),bootstrap:empty(),events:[],historyOmitted:false,
            selected:new Set(),pending:[],includePaths:false};
        this.publish();
    }
    private authorized():void{if(this.disposed||!this.host.trusted())throw new ServiceError('runtime_client_no_longer_authorized');}
    whenIdle():Promise<boolean>{
        if(this.disposed)return Promise.resolve(false);
        if(!this.state.busy)return Promise.resolve(true);
        return new Promise(resolve=>this.idle.add(resolve));
    }
    private releaseIdle():void{for(const resolve of this.idle)resolve(!this.disposed);this.idle.clear();}
    private publish():void{
        if(this.disposed)return;this.state.trusted=this.host.trusted();this.state.revision=randomUUID();
        this.state.responseMode=this.host.responseMode?.()??'terse';
        this.host.changed(this.state);
    }
    private async rpc(method:unknown,expected?:string):Promise<RecordValue>{
        this.authorized();if(!this.client)throw new ServiceError('console_not_connected');
        const value=await this.client.rpc(method,expected);this.authorized();return value;
    }
    private async discover():Promise<void>{
        this.authorized();
        this.bootstrap=await this.host.runtime.discover();this.authorized();
        this.state.bootstrap={state:'live',checkedAt:Date.now(),value:{
            installed:this.bootstrap.installed,installDir:this.bootstrap.installDir,
            bundleDigest:this.bootstrap.bundle.digest,platform:this.bootstrap.bundle.platform,
            error:this.bootstrap.error,diagnosis:this.bootstrap.diagnosis,
            lifecycle:this.bootstrap.inspection?.lifecycle,
        }};
        if(this.bootstrap.inspection){
            this.binding=this.host.runtime.binding(this.bootstrap.inspection,this.bootstrap.installDir);
            this.client?.dispose();this.client=this.host.runtime.client(this.binding);
            this.state.lifecycle=this.binding.lifecycle;
            if(typeof this.bootstrap.inspection.registry_namespace==='string')this.restoreJournal(this.bootstrap.inspection.registry_namespace);
        }
    }
    async refresh():Promise<void>{
        if(this.disposed||this.state.busy)return;
        this.state.busy=true;this.publish();
        try{this.authorized();if(!this.client)await this.discover();await this.observe();}
        catch(error){this.state.message=code(error);if(!this.client)this.state.bootstrap={...this.state.bootstrap,state:'failed',code:code(error),checkedAt:Date.now()};}
        finally{this.state.busy=false;this.publish();this.releaseIdle();}
    }
    private async observe():Promise<void>{
        if(!this.client)return;
        try{
            const value=await this.rpc('health','health');
            this.state.health={state:'live',value,checkedAt:Date.now()};this.state.connected=true;
        }catch(error){
            this.state.health={...this.state.health,state:'failed',code:code(error),checkedAt:Date.now()};
            this.state.connected=false;this.client.dispose();this.client=undefined;throw error;
        }
        for(const [name,method,expected] of [['resources','resources','resources'],['model','embedding_status','embedding_status'],['sources','source_status','source_status']] as const){
            try{this.state[name]={state:'live',value:await this.rpc(method,expected),checkedAt:Date.now()};}
            catch(error){this.state[name]={...this.state[name],state:'failed',code:code(error),checkedAt:Date.now()};}
        }
        try{
            const page=record((await this.rpc({logs:{limit:32}},'logs')).page);
            if(page.schema!==1||!Array.isArray(page.records)||page.records.length>32)throw new ServiceError('invalid_process_log_page');
            this.state.serviceEvents={state:'cached',value:page,checkedAt:Date.now()};
        }catch(error){this.state.serviceEvents={...this.state.serviceEvents,state:'failed',code:code(error),checkedAt:Date.now()};}
        const epoch=await this.rpc('request_epoch','request_epoch');
        this.restoreJournal(identifier(epoch.namespace,16));
        this.pending();
        try{await this.page(false);}catch(error){this.state.message=code(error);}
        try{await this.events();}catch(error){
            this.state.message=code(error);
            if(error instanceof ServiceError&&['event_cursor_expired','event_scope_changed'].includes(error.code)){
                this.eventCursor=null;this.state.historyOmitted=true;
            }
        }
        if(this.state.plan)await this.plan(false);
    }
    private restoreJournal(namespace:string):void{
        const journal=new IntentJournal(this.host.storage,this.binding!.controlDir,identifier(namespace,16));
        if(this.journal?.scope!==journal.scope){
            this.journal=journal;this.state.selected.clear();this.eventCursor=null;this.state.events=[];this.state.page=undefined;
            this.state.preview=undefined;
            this.pageStart={after:null,revision:null};
            const saved=this.host.storage.get<string>(`airk.service.plan.${journal.scope}`);
            this.state.plan=saved?{value:{id:identifier(saved,32)},items:[],after:null}:undefined;
        }
        this.pending();
    }
    private pending():void{
        const lifecycle=this.bootstrap?.inspection?.lifecycle;
        const pending=lifecycle?record(lifecycle).pending:undefined;
        const request=pending?record(record(pending).request):undefined;
        this.state.pending=this.journal?.list().map(entry=>({id:entry.id,title:entry.value.title,unknown:true,
            lifecycle:'lifecycle' in entry.value.intent,
            forceAvailable:request?.manager===true&&record(request.key).nonce===entry.id}))??[];
    }
    private async page(next:boolean,first=false):Promise<void>{
        if(!this.client)return;
        const prior=this.state.page;
        const start=first?{after:null,revision:null}:next?{after:prior?.after??null,revision:prior?.revision??null}:this.pageStart;
        const page=await this.client.projects(start.after,start.after?start.revision:null);
        this.pageStart=start;
        this.authorized();this.state.page=page;
        this.state.workers=await this.client.workers(page.projects.map(row=>String(record(record(row.project).registration).binding)));
    }
    private async events():Promise<void>{
        const page=record((await this.rpc({events:{project:null,cursor:this.eventCursor,limit:32}},'events')).page);
        if(!Array.isArray(page.events)||page.events.length>32)throw new ServiceError('invalid_event_page');
        const events=page.events.map(event=>record(event));
        const merged=new Map(this.state.events.map(event=>[event.sequence,event]));
        events.forEach(event=>{count(event.sequence);merged.set(event.sequence,event);});
        this.state.historyOmitted||=page.retained_prefix_lost===true||merged.size>128;
        this.state.events=[...merged.values()].slice(-128);
        // Keep an unfinished snapshot cursor; never advance straight to its tail.
        this.eventCursor=page.next??page.tail;
    }
    private async plan(more:boolean):Promise<void>{
        const current=this.state.plan;if(!current)return;
        const id=identifier(current.value.id,32);
        const value=record((await this.rpc({plan:{id}},'plan')).plan);
        if(value.id!==id)throw new ServiceError('crossed_plan_receipt');
        if(current.items.length&&current.value.preview_digest!==value.preview_digest){
            more=false;this.state.message='Repair preview changed; review the current pages before approval.';
        }else if(!more&&current.items.length){current.value=value;return;}
        const page=record((await this.rpc({plan_items:{id,after:more?current.after:null,limit:32}},'plan_items')).page);
        if(!Array.isArray(page.items)||page.items.length>32)throw new ServiceError('invalid_plan_page');
        const items=page.items.map(item=>record(item));
        const snapshot=record(page.plan);
        if(snapshot.id!==id)throw new ServiceError('crossed_plan_receipt');
        if(more&&snapshot.preview_digest!==value.preview_digest){
            this.state.plan={value:snapshot,items:[],after:null};throw new ServiceError('repair_preview_changed_refresh_rows');
        }
        this.state.plan={value:snapshot,items:more?[...current.items,...items].slice(-1024):items,after:page.after===null?null:identifier(page.after,32)};
    }
    private async receipt(response:RecordValue):Promise<void>{
        if(response.status==='lifecycle'&&record(response.record).phase==='abandoned')this.state.message='Lifecycle request abandoned. Automatic starts remain disabled until Start service.';
        if(response.status==='plan'){
            const value=record(response.plan),id=identifier(value.id,32);
            await this.host.storage.update(`airk.service.plan.${this.journal!.scope}`,id);
            this.state.plan={value,items:[],after:null};
        }
    }
    private async mutation(title:string,intent:ConsoleIntent):Promise<void>{
        if(!this.journal||(!this.client&&!('lifecycle' in intent)))throw new ServiceError('console_not_connected');
        const id=await this.journal.prepare(title,intent);this.pending();this.publish();
        this.authorized();await this.submit(id);this.pending();
    }
    private async submit(id:string,force=false):Promise<void>{
        if(!this.journal)throw new ServiceError('console_journal_not_observed');
        await this.journal.submit(this.client,id,response=>this.receipt(response),async intent=>{
            this.authorized();if(!this.binding)throw new ServiceError('runtime_binding_not_observed');
            try{
                const result=await this.host.lifecycle(this.binding,intent,force);this.authorized();return result;
            }catch(error){
                // Discovery only: expose the saved native phase/force ownership
                // without replaying the uncertain mutation or starting a service.
                try{await this.discover();}catch{/* Keep the original outcome and request. */}
                throw error;
            }
        });
    }
    async action(message:unknown):Promise<void>{
        const action=consoleAction(message,this.state);if(!action||this.state.busy||this.disposed)return;
        const requestedPreview=this.state.preview;
        this.state.busy=true;this.state.message=undefined;this.publish();
        try{
            if(action.action==='previewReport'){
                this.state.includePaths=action.includePaths!;this.state.preview=diagnosticPreview(this.state,this.state.includePaths);return;
            }
            if(action.action==='copyReport'){if(requestedPreview)await this.host.copy(requestedPreview);this.state.preview=requestedPreview;return;}
            if(action.action==='logs'){await this.host.logs(this.bootstrap);return;}
            this.authorized();
            if(action.action==='responseMode'){await this.host.chooseResponseMode?.();return;}
            if(action.action==='recoverLifecycle'||action.action==='abandonLifecycle'){
                if(!this.binding||!this.journal||!this.bootstrap?.inspection?.lifecycle)throw new ServiceError('native_lifecycle_not_observed');
                const pending=record(record(this.bootstrap.inspection.lifecycle).pending),request=record(pending.request),key=record(request.key);
                const kind=request.action;
                if(kind!=='stop'&&kind!=='restart')throw new ServiceError('invalid_lifecycle_intent');
                const intent:LifecycleIntent={lifecycle:{action:kind,caller:identifier(request.caller,16),expected_instance:identifier(request.expected_instance,16),runtime:identifier(request.runtime,32),
                    key:{namespace:identifier(key.namespace,16),epoch:count(key.epoch),nonce:identifier(key.nonce,16)}}};
                if(action.action==='recoverLifecycle')await this.mutation(`Resume native ${kind}`,intent);
                else{
                    if(!await this.host.confirm('Abandon this saved stop/restart intent? This leaves all processes as they are. Automatic starts remain disabled until you explicitly start the service.'))return;
                    this.authorized();if(!this.host.abandonLifecycle)throw new ServiceError('unsupported_console_action');
                    await this.host.abandonLifecycle(this.binding,intent,identifier(pending.id,32));
                    await this.journal.forget(intent.lifecycle.key.nonce);
                    this.state.message='Native lifecycle intent abandoned. Automatic starts remain disabled until Start service.';
                }
                this.client?.dispose();this.client=undefined;this.state.connected=false;return;
            }
            if(action.action==='retryIntent'||action.action==='forceIntent'){
                if(!this.journal)await this.discover();
                if(action.action==='forceIntent'){
                    if(!this.state.pending.some(intent=>intent.id===action.id&&intent.lifecycle&&intent.forceAvailable))throw new ServiceError('lifecycle_force_stop_ownership_unavailable');
                    if(!await this.host.confirm(`Force-stop the verified OS-managed AIRK service on ${this.host.authority}? This can terminate its owned processes and interrupt active requests or publication. Native ownership is rechecked, and the saved request key is retained for recovery.`))return;
                }
                await this.submit(action.id!,action.action==='forceIntent');this.client?.dispose();this.client=undefined;this.state.connected=false;
                return;
            }
            if(action.action==='forgetIntent'){
                if(await this.host.confirm('Forget this local request key? Native work and native lifecycle intent continue. Its outcome may remain unknown.'))await this.journal?.forget(action.id!);
                return;
            }
            if(action.action==='select'){
                if(action.selected){if(this.state.selected.size>=256)throw new ServiceError('repair_selection_full');this.state.selected.add(action.project!);}
                else this.state.selected.delete(action.project!);return;
            }
            if(action.action==='start'){
                if(this.binding?.lifecycle==='external')throw new ServiceError('lifecycle_managed_externally');
                this.binding=await this.host.runtime.ensure(true);this.authorized();this.state.lifecycle=this.binding.lifecycle;
                this.client?.dispose();this.client=this.host.runtime.client(this.binding);
            }else if(action.action==='connect'){
                this.client?.dispose();this.client=undefined;await this.discover();
            }else if(action.action==='refresh'){
                if(!this.client)await this.discover();
            }else if(action.action==='stop'||action.action==='restart'){
                if(!this.binding||this.binding.lifecycle==='external')throw new ServiceError('lifecycle_managed_externally');
                const health=record(this.state.health.value?.observation),instance=identifier(health.instance,16);
                if(!await this.host.confirm(`${action.action==='stop'?'Stop':'Restart'} AIRK on ${this.host.authority}? Instance ${instance}; ${this.state.page?.projects.length??'unknown'} projects on the displayed page. Active requests may disconnect; native work will drain. Other connected clients have not been counted.`))return;
                this.authorized();
                if(!this.client)throw new ServiceError('console_not_connected');
                const intent:LifecycleIntent={lifecycle:{action:action.action,expected_instance:instance,runtime:this.binding.runtimeSha256,
                    caller:randomUUID().replaceAll('-',''),key:await this.client.key()}};
                await this.mutation(`${action.action} service ${instance}`,intent);
                this.client?.dispose();this.client=undefined;this.state.connected=false;
                if(action.action==='stop')return;
                await this.discover();
            }else{
                if(!this.client||!this.state.connected)throw new ServiceError('console_not_connected');
                await this.connectedAction(action);
                if(['firstPage','nextPage','morePlan','refreshPlan'].includes(action.action))return;
            }
            await this.observe();
        }catch(error){this.state.message=code(error);}
        finally{
            try{this.pending();}catch(error){this.state.message=code(error);}
            this.state.busy=false;
            // Copy always uses the exact captured preview, even while observations refresh.
            this.publish();
            this.releaseIdle();
        }
    }
    private async connectedAction(action:NonNullable<ReturnType<typeof consoleAction>>):Promise<void>{
        const project=action.project!,id=action.id!;
        switch(action.action){
            case 'firstPage':await this.page(false,true);return;
            case 'nextPage':if(this.state.page?.after)await this.page(true);return;
            case 'morePlan':await this.plan(true);return;
            case 'refreshPlan':if(this.state.plan)this.state.plan.items=[];await this.plan(false);return;
            case 'resources':{
                const configuration=await this.host.resources(record(record(this.state.resources.value?.observation).configuration));
                this.authorized();if(configuration)await this.rpc({configure_resources:{configuration}},'resources');break;
            }
            case 'startEmbedding':await this.rpc('start_embedding','embedding_starting');break;
            case 'stopEmbedding':case 'forceEmbedding':{
                const worker=identifier(this.state.model.value?.worker,16),force=action.action==='forceEmbedding';
                if(!await this.host.confirm(`${force?'Force-stop':'Stop'} inference helper ${worker} for this host? Active semantic requests may fail and require explicit retry. Lexical evidence and independent source work remain available.`))return;
                await this.rpc({stop_embedding:{worker,force}},'embedding_stopped');break;
            }
            case 'pause':case 'resume':await this.rpc({pause:{project,paused:action.action==='pause'}});break;
            case 'watch':await this.rpc({configure_watch:{project,enabled:action.selected}});break;
            case 'restartProject':{
                const workers=this.state.workers?.observations;
                const observation=Array.isArray(workers)?workers.map(value=>record(value)).find(value=>value.project===project):undefined;
                if(!observation)throw new ServiceError('worker_identity_not_observed');
                if(!await this.host.confirm(`Restart only project worker ${project}? Its active requests may disconnect; other projects continue.`))return;
                await this.rpc({restart_project:{project,expected:observation.identity??null}},'project_restarting');break;
            }
            case 'inspect':case 'index':case 'modelSetup':{
                const kind=action.action==='inspect'?'inspect':action.action==='index'?'index':'model_setup';
                await this.mutation(`${action.action} ${project}`,{enqueue:await this.client!.prepareJob(project,kind)});break;
            }
            case 'retryJob':await this.mutation(`Retry ${id}`,await this.client!.prepareRetry(id));break;
            case 'cancelJob':await this.rpc({cancel:{id}},'job');break;
            case 'repair':{
                const selection=action.scope==='all'?'all':action.scope==='one'?[project]:[...this.state.selected];
                await this.mutation(`Plan ${action.operation} (${action.scope})`,await this.client!.preparePlan(selection,action.operation!));break;
            }
            case 'approvePlan':{
                const plan=this.state.plan?.value;if(!plan)throw new ServiceError('repair_plan_not_observed');
                if(this.state.plan?.after||this.state.plan?.items.length!==record(plan.counts).selected)throw new ServiceError('finish_repair_preview_pages_before_approval');
                if(!await this.host.confirm('Execute this exact native repair preview? Unavailable projects retain their failure or skipped receipts.'))return;
                await this.rpc({plan_execute:{id:identifier(plan.id,32),preview:identifier(plan.preview_digest,32),allow_fallback:action.allowFallback}},'plan');break;
            }
            case 'pausePlan':case 'resumePlan':
                await this.rpc({plan_pause:{id:identifier(this.state.plan?.value.id,32),paused:action.action==='pausePlan'}},'plan');break;
            case 'cancelPlan':await this.rpc({plan_cancel:{id:identifier(this.state.plan?.value.id,32)}},'plan');break;
            default:throw new ServiceError('unsupported_console_action');
        }
    }
    dispose():void{this.disposed=true;this.client?.dispose();this.client=undefined;this.releaseIdle();}
}
