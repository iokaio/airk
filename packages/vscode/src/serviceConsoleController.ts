// SPDX-License-Identifier: Apache-2.0
import { randomUUID } from 'node:crypto';
import { BootstrapObservation, LegacyStartupReview, LegacyStartupTarget, RuntimeBinding, RuntimeHost } from './serviceBootstrap';
import { count, identifier, parseConsolePage, record, RecordValue, ServiceClient, ServiceError } from './serviceClient';
import { ConsoleIntent, IntentJournal, IntentStorage, LifecycleIntent } from './serviceIntents';
import { consoleAction, ConsoleObservation, diagnosticPreview, ServiceConsoleState } from './serviceConsoleView';

export interface ConsoleHost {
    version:string; authority:string; storage:IntentStorage;
    runtime:Pick<RuntimeHost,'discover'|'binding'|'ensure'|'client'|'snapshot'|'repair'|'prepareConfiguration'|'reviewTransition'|'transition'>;
    trusted():boolean;
    changed(state:ServiceConsoleState):void;
    confirm(message:string,acceptLabel?:string):Promise<boolean>;
    copy(text:string):Promise<void>;
    openReport?(text:string):Promise<void>;
    saveReport?(text:string):Promise<void>;
    exportLogs?(observation?:BootstrapObservation):Promise<void>;
    rawLogs?(save:boolean,observation?:BootstrapObservation):Promise<void|'opened'|'saved'|'canceled'>;
    diagnosticFailure?(operation:string,error:unknown,observation?:BootstrapObservation):Promise<void>;
    logs(observation?:BootstrapObservation):Promise<void>;
    resources(current:RecordValue):Promise<RecordValue|undefined>;
    configuration?(row:RecordValue,operation?:'detail'|'advanced'):Promise<RecordValue|undefined>;
    lifecycle(binding:RuntimeBinding,intent:LifecycleIntent,force?:boolean):Promise<RecordValue>;
    abandonLifecycle?(binding:RuntimeBinding,intent:LifecycleIntent,id:string):Promise<RecordValue>;
    responseMode?():'terse'|'full';
    chooseResponseMode?():Promise<void>;
    registerWorkspace?():Promise<void>;
    projectRemoved?(project:string):Promise<void>;
    currentProjects?():ReadonlySet<string>;
    legacy?:{
        choose():Promise<LegacyStartupTarget|undefined>;
        review(binding:RuntimeBinding,target:LegacyStartupTarget):Promise<LegacyStartupReview>;
        load(binding:RuntimeBinding,approval:string):Promise<LegacyStartupReview>;
        list(binding:RuntimeBinding):Promise<RecordValue>;
        remove(binding:RuntimeBinding,review:LegacyStartupReview):Promise<RecordValue>;
    };
}
const empty=():ConsoleObservation=>({state:'unavailable',checkedAt:Date.now()});
export const LOCAL_LOG_ACTIONS=['logs','exportLogs','openRawLogs','saveRawLogs'];
function objOutcomeUnknown(row:RecordValue):boolean{return record(record(row.configuration??{}).outcome??{}).code==='configuration_outcome_unknown_reinspect';}
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
    private legacyReview?:LegacyStartupReview;
    private readonly idle=new Set<(available:boolean)=>void>();
    private logRequest?:{action:string;work:Promise<void>};
    private pollDeadline?:number;
    private lastPollFailure?:string;
    private readonly receiptDiagnostics=new Set<string>();
    private readonly retainedOutcomes=new WeakSet<ServiceError>();
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
        this.state.registrationAvailable=!!this.host.registerWorkspace;
        this.state.legacyAvailable=!!this.host.legacy;
        this.state.currentProjects=new Set(this.host.currentProjects?.()??[]);
        this.host.changed(this.state);
    }
    private async rpc(method:unknown,expected?:string):Promise<RecordValue>{
        this.pollActive();
        this.authorized();if(!this.client)throw new ServiceError('console_not_connected');
        const value=await this.observation(()=>this.client!.rpc(method,expected),typeof method==='string'?method:Object.keys(record(method))[0]);this.authorized();return value;
    }
    private pollActive():void{
        if(this.pollDeadline!==undefined&&Date.now()>=this.pollDeadline)throw new ServiceError('status_poll_timed_out');
    }
    /** One deadline covers the whole poll, rather than a new deadline for each RPC. */
    private async observation<T>(read:()=>Promise<T>,operation='bootstrap'):Promise<T>{
        this.pollActive();if(this.pollDeadline===undefined)return read();
        let timer:ReturnType<typeof setTimeout>|undefined;
        const expired=new Promise<never>((_resolve,reject)=>{timer=setTimeout(()=>{
            this.client?.dispose();this.client=undefined;
            const failure=new ServiceError('status_poll_timed_out');failure.cause=new Error(`Status poll exceeded its shared deadline while reading ${operation}`);reject(failure);
        },Math.max(1,this.pollDeadline!-Date.now()));});
        try{return await Promise.race([read(),expired]);}finally{if(timer)clearTimeout(timer);}
    }
    private async discover():Promise<void>{
        this.authorized();
        this.bootstrap=await this.observation(()=>this.host.runtime.discover());this.authorized();
        this.state.bootstrap={state:'live',checkedAt:Date.now(),value:{
            installed:this.bootstrap.installed,installDir:this.bootstrap.installDir,
            bundleDigest:this.bootstrap.bundle.digest,platform:this.bootstrap.bundle.platform,
            error:this.bootstrap.error,diagnosis:this.bootstrap.diagnosis,
            lifecycle:this.bootstrap.inspection?.lifecycle,
        }};
        this.client?.dispose();this.client=undefined;this.binding=undefined;this.disconnected();
        if(this.bootstrap.inspection){
            if(typeof this.bootstrap.inspection.control_dir!=='string'){
                this.cachedSnapshot(this.bootstrap.inspection);return;
            }
            this.binding=this.host.runtime.binding(this.bootstrap.inspection,this.bootstrap.installDir);
            if(this.binding.payloadVerified!==false&&this.binding.version===this.host.version)this.client=this.host.runtime.client(this.binding);
            this.state.lifecycle=this.binding.lifecycle;
            if(typeof this.bootstrap.inspection.registry_namespace==='string')this.restoreJournal(this.bootstrap.inspection.registry_namespace);
            this.cachedSnapshot(this.bootstrap.inspection);
            if(this.binding.version!==this.host.version){
                try{const snapshot=await this.observation(()=>this.host.runtime.snapshot(this.binding!));this.authorized();this.cachedSnapshot(snapshot);}catch{/* Installer observations remain usable without readable cached project metadata. */}
                this.state.message='Selected native runtime differs from this extension. Review the bundled upgrade or use the matching extension; the pin is preserved.';
            }
        }
    }
    private cachedSnapshot(snapshot:RecordValue):void{
        if(this.bootstrap)this.bootstrap.inspection={...this.bootstrap.inspection,...snapshot};
        this.state.bootstrap={...this.state.bootstrap,value:{...this.state.bootstrap.value,lifecycle:snapshot.lifecycle,
            payloadVerified:snapshot.payload_verified,registryError:snapshot.registry_error,startupManager:snapshot.startup_manager,payloadRepair:snapshot.payload_repair,
            selectedRuntime:snapshot.selection?record(snapshot.selection).active:undefined,
            selectedVersion:snapshot.approval?record(snapshot.approval).version:undefined,
            verificationBasis:snapshot.approval?record(snapshot.approval).verification_basis:undefined}};
        if(snapshot.console){this.state.page=parseConsolePage(snapshot.console);this.state.activePage=this.state.page;this.state.inactiveProjects=false;}
        this.state.configurationResultsOmitted=snapshot.configuration_results_omitted===true;
        if(snapshot.configuration_results!==undefined){
            const items=snapshot.configuration_results;
            if(!Array.isArray(items)||items.length>32||Buffer.byteLength(JSON.stringify(items))>65536)throw new ServiceError('invalid_saved_configuration_results');
            const visible=new Map((this.state.page?.projects??[]).filter(row=>row.configuration).map(row=>[
                String(record(record(row.project).registration).binding),record(row.configuration)]));
            const results:NonNullable<ServiceConsoleState['configurationResults']>=new Map();
            for(const [project,job] of visible){const prior=this.state.configurationResults?.get(project);if(prior&&prior.job===job.id)results.set(project,prior);}
            for(const value of items){const item=record(value),project=identifier(item.project,32),job=identifier(item.job,32),observed=visible.get(project);
                if(observed?.id!==job)throw new ServiceError('crossed_configuration_result');
                results.set(project,{job,jobState:String(observed.state),checkedAt:Date.now(),value:item.result===null?undefined:record(item.result),
                    ...(item.unavailable===true?{code:'configuration_result_unavailable'}:{})});
            }
            this.state.configurationResults=results;
        }
        if(snapshot.logs)this.state.serviceEvents={state:'cached',checkedAt:Date.now(),value:record(snapshot.logs)};
    }
    private async savedSnapshot(after:string|null=null,revision:number|null=null):Promise<void>{
        if(!this.binding)return;
        const snapshot=await this.observation(()=>this.host.runtime.snapshot(this.binding!,after,revision));this.authorized();
        if(snapshot.registry_namespace)this.restoreJournal(identifier(snapshot.registry_namespace,16));
        this.cachedSnapshot(snapshot);this.pending();
        if(snapshot.registry_error)throw new ServiceError('cached_registry_unavailable_or_cursor_expired');
    }
    private disconnected():void{
        this.state.connected=false;
        if(this.state.health.state==='live')this.state.health={...this.state.health,state:'cached'};
        for(const name of ['resources','model','sources'] as const){
            if(this.state[name].value)this.state[name]={...this.state[name],state:'cached'};
        }
    }
    async refresh():Promise<void>{
        if(this.disposed||this.state.busy)return;
        this.pollDeadline=Date.now()+15000;this.state.busy=true;this.publish();
        try{this.authorized();if(!this.client)await this.discover();await this.observe();this.lastPollFailure=undefined;}
        catch(error){
            if(this.lastPollFailure!==code(error)){
                this.lastPollFailure=code(error);void this.host.diagnosticFailure?.('status_poll',error,this.bootstrap).catch(()=>{});
            }
            this.state.message=code(error);
            if(code(error)==='status_poll_timed_out'){
                this.disconnected();this.state.health={...this.state.health,state:'failed',code:'status_poll_timed_out'};
                this.state.message='Status check timed out. Reconnecting automatically; current indexing progress is unconfirmed.';
            }
            if(!this.client)this.state.bootstrap={...this.state.bootstrap,state:'failed',code:code(error),checkedAt:Date.now()};
        }
        finally{this.pollDeadline=undefined;this.state.busy=false;this.publish();this.releaseIdle();}
    }
    private async observe():Promise<void>{
        if(!this.client){await this.savedSnapshot(this.pageStart.after,this.pageStart.revision);return;}
        try{
            const value=await this.rpc('health','health');
            this.state.health={state:'live',value,checkedAt:Date.now()};this.state.connected=true;
        }catch(error){
            this.state.health={...this.state.health,state:'failed',code:code(error),checkedAt:Date.now()};
            this.disconnected();this.client?.dispose();this.client=undefined;
            try{await this.savedSnapshot(this.pageStart.after,this.pageStart.revision);}catch{/* Retain the original health failure. */}
            throw error;
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
            this.journal=journal;this.state.selected.clear();this.eventCursor=null;this.state.events=[];this.state.page=undefined;this.state.activePage=undefined;
            this.legacyReview=undefined;this.state.legacyReview=undefined;this.state.legacyReceipts=undefined;
            this.state.preview=undefined;
            this.state.configurationResults=undefined;
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
        const prior=this.state.page;
        const start=first?{after:null,revision:null}:next?{after:prior?.after??null,revision:prior?.revision??null}:this.pageStart;
        if(!this.client){await this.savedSnapshot(start.after,start.after?start.revision:null);this.pageStart=start;return;}
        if(this.journal?.list().length){
            const client=this.client;
            const current=()=>{this.authorized();if(this.client!==client)throw new ServiceError('stale_receipt_observation');};
            try{
                await this.observation(()=>this.journal!.reconcile(client!,response=>{current();return this.receipt(response);},async project=>{current();await this.host.projectRemoved?.(project);},(id,error)=>{this.logReceipt(id,error);current();}));
            }catch(error){
                this.logReceipt('reconciliation',error);
                // Receipt bookkeeping must not prevent loading current project status.
                this.pollActive();this.authorized();
            }
            this.pending();
            for(const entry of this.journal.list())this.logReceipt(entry.id,new ServiceError('request_outcome_unconfirmed'));
        }
        const page=await this.observation(()=>this.client!.projects(start.after,start.after?start.revision:null,16,this.state.inactiveProjects===true));
        this.pageStart=start;
        this.authorized();this.state.page=page;
        if(this.state.inactiveProjects)for(const row of page.projects){await this.host.projectRemoved?.(identifier(record(record(row.project).registration).binding,32));this.authorized();}
        // Refresh active registrations even while Management browses removed ones.
        this.state.activePage=this.state.inactiveProjects
            ? await this.observation(()=>this.client!.projects(null,null,16,false)) : page;
        this.state.workers=undefined;
        this.state.workers=await this.observation(()=>this.client!.workers(page.projects.map(row=>String(record(record(row.project).registration).binding))));
        const results:NonNullable<ServiceConsoleState['configurationResults']>=new Map();
        for(const row of page.projects){
            if(!row.configuration)continue;
            const job=record(row.configuration),project=identifier(record(record(row.project).registration).binding,32),id=identifier(job.id,32);
            const prior=this.state.configurationResults?.get(project);
            if(prior?.job===id&&prior.jobState===job.state&&!prior.code){results.set(project,prior);continue;}
            if(job.state==='pending'||job.state==='running')continue;
            try{
                const response=await this.rpc({configuration_result:{id}},'configuration'),observed=record(response.job);
                if(observed.id!==id||observed.project!==project||observed.kind!=='configure')throw new ServiceError('crossed_configuration_result');
                const value=response.result===null?undefined:record(response.result);
                if(value&&Buffer.byteLength(JSON.stringify(value))>16384)throw new ServiceError('configuration_result_exceeds_budget');
                results.set(project,{job:id,jobState:String(observed.state),value,checkedAt:Date.now()});
            }catch(error){results.set(project,{job:id,jobState:String(job.state),code:code(error),checkedAt:Date.now()});}
        }
        this.state.configurationResults=results;
        this.state.configurationResultsOmitted=false;
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
        }else if(!more&&current.items.length&&current.value.phase===value.phase
            &&JSON.stringify(current.value.counts)===JSON.stringify(value.counts)
            &&!['planning','running','canceling'].includes(String(value.phase))){current.value=value;return;}
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
        if(response.status==='lifecycle'){
            const saved=record(response.record);
            if(saved.phase==='abandoned'||saved.phase==='complete'&&record(saved.request).action==='stop')this.stopPreference(true);
        }
        if(response.status==='lifecycle'&&record(response.record).phase==='abandoned')this.state.message='Lifecycle request abandoned. Automatic starts remain disabled until Start service.';
        if(response.status==='plan'){
            const value=record(response.plan),id=identifier(value.id,32);
            await this.host.storage.update(`airk.service.plan.${this.journal!.scope}`,id);
            this.state.plan={value,items:[],after:null};
        }
    }
    private stopPreference(stopped:boolean):void{
        if(!this.bootstrap?.inspection)return;
        const prior=this.bootstrap.inspection.lifecycle?record(this.bootstrap.inspection.lifecycle):{};
        const lifecycle={...prior,explicitly_stopped:stopped,pending:null};
        this.bootstrap.inspection.lifecycle=lifecycle;
        this.state.bootstrap={...this.state.bootstrap,value:{...this.state.bootstrap.value,lifecycle}};
    }
    private async mutation(title:string,intent:ConsoleIntent):Promise<RecordValue>{
        if(!this.journal||(!this.client&&!('lifecycle' in intent)))throw new ServiceError('console_not_connected');
        const id=await this.journal.prepare(title,intent);this.pending();this.publish();
        this.authorized();
        try{const response=await this.submit(id);this.pending();return response;}
        catch(error){
            this.logReceipt(id,error);
            if(error instanceof ServiceError&&error.outcomeUnknown)this.retainedOutcomes.add(error);
            throw error;
        }
    }
    private async submit(id:string,force=false):Promise<RecordValue>{
        if(!this.journal)throw new ServiceError('console_journal_not_observed');
        return this.journal.submit(this.client,id,response=>this.receipt(response),async intent=>{
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
        const action=consoleAction(message,this.state);if(!action||this.disposed)return;
        if(LOCAL_LOG_ACTIONS.includes(action.action)){await this.localLogs(action.action);return;}
        if(this.state.busy)return;
        const requestedPreview=this.state.preview;
        this.state.busy=true;this.state.message=undefined;this.publish();
        try{
            if(['reviewLegacyStartup','legacyStartupReceipts','resumeLegacyStartup','removeLegacyStartup','copyLegacyStartup'].includes(action.action)){
                await this.legacyAction(action);return;
            }
            if(action.action==='registerWorkspace'){
                this.authorized();if(!this.host.registerWorkspace)throw new ServiceError('workspace_registration_unavailable');
                await this.host.registerWorkspace();this.authorized();
                this.client?.dispose();this.client=undefined;await this.discover();await this.observe();return;
            }
            if(action.action==='previewReport'){
                this.state.includePaths=action.includePaths!;this.state.preview=diagnosticPreview(this.state,this.state.includePaths);return;
            }
            if(action.action==='copyReport'){if(requestedPreview)await this.host.copy(requestedPreview);this.state.preview=requestedPreview;return;}
            if(action.action==='openReport'||action.action==='saveReport'){
                if(!requestedPreview)throw new ServiceError('generate_diagnostic_report_first');
                const operation=action.action==='openReport'?this.host.openReport:this.host.saveReport;
                if(!operation)throw new ServiceError('diagnostic_export_unavailable');
                await operation(requestedPreview);this.state.preview=requestedPreview;return;
            }
            if(action.action==='copyConfiguration'){
                const row=this.state.page?.projects.find(row=>record(record(row.project).registration).binding===action.project);
                const saved=this.state.configurationResults?.get(action.project!);
                if(!saved?.value||!row?.configuration||record(row.configuration).id!==saved.job)throw new ServiceError('configuration_result_not_observed');
                await this.host.copy(JSON.stringify(saved.value,null,2));return;
            }
            this.authorized();
            if(action.action==='responseMode'){await this.host.chooseResponseMode?.();return;}
            if(['upgradeRuntime','rollbackRuntime','resumeActivation'].includes(action.action)){
                if(!this.binding)throw new ServiceError('runtime_binding_not_observed');
                const kind=action.action==='upgradeRuntime'?'upgrade':action.action==='rollbackRuntime'?'rollback':'resume';
                const review=await this.host.runtime.reviewTransition(this.binding,kind);this.authorized();
                if(!await this.host.confirm(`${kind==='upgrade'?'Use the bundled runtime':kind==='rollback'?'Roll back the runtime':'Resume the saved native activation'} on ${this.host.authority}? Reviewed selection ${review.expectedActive}; target ${review.target}, version ${review.version}. Native activation drains affected work and starts the selected service. Requests can disconnect. Other clients have not been counted. A changed selection invalidates this review.`))return;
                this.authorized();this.client?.dispose();this.client=undefined;this.disconnected();
                try{await this.host.runtime.transition(this.binding,review);this.authorized();}
                catch(error){try{await this.discover();}catch{/* Preserve the native intent for explicit recovery. */}throw error;}
                await this.discover();await this.observe();return;
            }
            if(action.action==='repairRuntime'){
                if(!this.binding)throw new ServiceError('runtime_binding_not_observed');
                const lifecycle=this.bootstrap?.inspection?.lifecycle;
                if(!lifecycle||record(lifecycle).explicitly_stopped!==true)throw new ServiceError('stop_service_before_payload_repair');
                const preview=await this.host.runtime.repair(this.binding);this.authorized();
                if(!await this.host.confirm(`Repair the selected runtime on ${this.host.authority}? Preserve ${preview.preserved_files} files (${preview.preserved_bytes} bytes) at ${preview.preserved_directory}, then copy the exact approved distribution. One free runtime slot and replacement disk space are required. The service stays stopped. Review digest: ${preview.preview}`))return;
                this.authorized();this.client?.dispose();this.client=undefined;this.disconnected();
                await this.host.runtime.repair(this.binding,identifier(preview.preview,32));this.authorized();
                await this.discover();this.state.message='Runtime payload repaired; the damaged slot is preserved. Use Start service when ready.';return;
            }
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
                this.client?.dispose();this.client=undefined;this.disconnected();return;
            }
            if(action.action==='retryIntent'||action.action==='forceIntent'){
                if(!this.journal)await this.discover();
                if(action.action==='forceIntent'){
                    if(!this.state.pending.some(intent=>intent.id===action.id&&intent.lifecycle&&intent.forceAvailable))throw new ServiceError('lifecycle_force_stop_ownership_unavailable');
                    if(!await this.host.confirm(`Force-stop the verified OS-managed AIRK service on ${this.host.authority}? This can terminate its owned processes and interrupt active requests or publication. Native ownership is rechecked, and the saved request key is retained for recovery.`))return;
                }
                await this.submit(action.id!,action.action==='forceIntent');this.client?.dispose();this.client=undefined;this.disconnected();
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
                this.stopPreference(false);
                this.client?.dispose();this.client=this.host.runtime.client(this.binding);
            }else if(action.action==='connect'){
                this.client?.dispose();this.client=undefined;await this.discover();
            }else if(action.action==='refresh'){
                if(!this.client)await this.discover();
            }else if(action.action==='stop'||action.action==='restart'){
                if(!this.binding||this.binding.lifecycle==='external')throw new ServiceError('lifecycle_managed_externally');
                if(action.action==='restart'&&this.binding.payloadVerified===false)throw new ServiceError('selected_runtime_payload_unverified');
                const saved=this.bootstrap?.inspection?.lifecycle?record(this.bootstrap.inspection.lifecycle):{};
                const health=this.state.connected?record(this.state.health.value?.observation):{};
                const instance=identifier(health.instance??saved.recorded_instance,16);
                if(!await this.host.confirm(`${action.action==='stop'?'Stop':'Restart'} AIRK on ${this.host.authority}? Instance ${instance}; ${this.state.page?.projects.length??'unknown'} projects on the displayed page. Active requests may disconnect; native work will drain. Other connected clients have not been counted.`))return;
                this.authorized();
                const intent:LifecycleIntent={lifecycle:{action:action.action,expected_instance:instance,runtime:this.binding.runtimeSha256,
                    caller:randomUUID().replaceAll('-',''),key:this.client?await this.client.key():{
                        namespace:identifier(saved.namespace,16),epoch:count(saved.epoch),nonce:randomUUID().replaceAll('-','')}}};
                await this.mutation(`${action.action} service ${instance}`,intent);
                this.client?.dispose();this.client=undefined;this.disconnected();
                if(action.action==='stop')return;
                await this.discover();
            }else if(action.action==='firstPage'||action.action==='nextPage'){
                await this.page(action.action==='nextPage',action.action==='firstPage');return;
            }else{
                if(!this.client||!this.state.connected)throw new ServiceError('console_not_connected');
                await this.connectedAction(action);
                if(['firstPage','nextPage','morePlan','refreshPlan'].includes(action.action))return;
            }
            await this.observe();
        }catch(error){
            if(!this.retainedOutcome(error))this.state.message=code(error);
        }
        finally{
            try{this.pending();}catch(error){this.state.message=code(error);}
            this.state.busy=false;
            // Copy always uses the exact captured preview, even while observations refresh.
            this.publish();
            this.releaseIdle();
        }
    }
    private logReceipt(identity:string,error:unknown):void{
        const key=`${this.journal?.scope}:${identity}:${code(error)}`;
        if(this.receiptDiagnostics.has(key))return;
        this.receiptDiagnostics.add(key);
        void this.host.diagnosticFailure?.('request_receipt',new Error(`${key}: ${String(error)}`,{cause:error}),this.bootstrap).catch(()=>{});
    }
    private retainedOutcome(error:unknown):boolean{
        return error instanceof ServiceError&&this.retainedOutcomes.has(error);
    }
    /** Local file diagnostics must not wait behind an unrelated status/RPC poll. */
    private async localLogs(action:string):Promise<void>{
        if(this.logRequest){
            const current=this.logRequest;await current.work;
            if(current.action===action)return;
        }
        const work=(async()=>{
            try{
                this.authorized();this.state.message='Reading local AIRK logs…';this.publish();
                if(action==='openRawLogs'||action==='saveRawLogs'){
                    if(!this.host.rawLogs)throw new ServiceError('diagnostic_export_unavailable');
                    const result=await this.host.rawLogs(action==='saveRawLogs',this.bootstrap);this.authorized();
                    this.state.message=result==='saved'?'Raw job error log saved.':result==='opened'?'Raw job error log opened.':result==='canceled'?'Log save canceled.':'Log request finished.';
                }else if(action==='exportLogs'){
                    if(!this.host.exportLogs)throw new ServiceError('diagnostic_export_unavailable');
                    await this.host.exportLogs(this.bootstrap);this.authorized();this.state.message='Service log export finished.';
                }else{await this.host.logs(this.bootstrap);this.authorized();this.state.message='Service logs opened in AIRK Service output.';}
            }catch(error){
                this.state.message=code(error);
                void this.host.diagnosticFailure?.(action.replace(/[A-Z]/g,letter=>'_'+letter.toLowerCase()),error,this.bootstrap).catch(()=>{});
            }
            finally{this.publish();}
        })();
        this.logRequest={action,work};
        try{await work;}finally{if(this.logRequest?.work===work)this.logRequest=undefined;}
    }
    private async legacyAction(action:NonNullable<ReturnType<typeof consoleAction>>):Promise<void>{
        this.authorized();const legacy=this.host.legacy;if(!legacy)throw new ServiceError('legacy_review_unavailable');
        if(action.action==='copyLegacyStartup'){if(this.state.legacyReview)await this.host.copy(JSON.stringify(this.state.legacyReview,null,2));return;}
        if(!this.binding)await this.discover();this.authorized();const binding=this.binding;
        if(!binding)throw new ServiceError('initialized_native_control_required');
        if(action.action==='legacyStartupReceipts'){
            const value=await legacy.list(binding);this.authorized();
            if(!Array.isArray(value.items)||value.items.length>16)throw new ServiceError('invalid_legacy_receipt_list');
            for(const item of value.items){const row=record(item);identifier(row.approval,32);if(!['planned','disabled','definition_removed','receipt_removed','complete'].includes(String(row.phase)))throw new ServiceError('invalid_legacy_receipt_phase');}
            this.state.legacyReceipts=value;return;
        }
        if(action.action==='reviewLegacyStartup'){
            const target=await legacy.choose();this.authorized();if(!target)return;
            this.legacyReview=await legacy.review(binding,target);this.authorized();
            this.state.legacyReview={value:this.legacyReview.value,phase:this.legacyReview.phase,checkedAt:Date.now()};this.state.legacyResult=undefined;return;
        }
        if(action.action==='resumeLegacyStartup'){
            this.legacyReview=await legacy.load(binding,identifier(action.id,32));this.authorized();
            this.state.legacyReview={value:this.legacyReview.value,phase:this.legacyReview.phase,checkedAt:Date.now()};this.state.legacyResult=undefined;
            if(this.legacyReview.phase==='complete'){this.state.message='Showing the original completed legacy removal receipt.';return;}
        }
        const review=this.legacyReview;
        if(!review?.approval||review.phase==='complete')throw new ServiceError('legacy_startup_review_required');
        const spec=record(record(review.value.preview).spec),target=review.target;
        if(!await this.host.confirm(`${review.phase?'Resume removal of':'Remove'} reviewed old AIRK startup ${String(spec.id)} on ${this.host.authority}? Source: ${target.root}. Old data: ${target.data_dir}. Collection: ${target.collection}. Configured executable: ${target.executable}. This stops that OS-managed old worker and removes its login registration. Active old indexing may be interrupted. Original sources, project settings and old indexes stay in place; unmanaged old processes remain under their own controls. Approval: ${review.approval}.`))return;
        this.authorized();
        const result=await legacy.remove(binding,review);this.authorized();this.state.legacyResult=result;
        this.state.legacyReview={value:review.value,phase:'complete',checkedAt:Date.now()};this.legacyReview=undefined;
        if(Array.isArray(this.state.legacyReceipts?.items))this.state.legacyReceipts={...this.state.legacyReceipts,items:this.state.legacyReceipts.items.map(value=>record(value).approval===review.approval?{...record(value),phase:'complete'}:value)};
        this.state.message='The reviewed old startup was removed. Sources, project settings and old indexes were retained.';
    }
    /** Trusted host command: capture a native repair preview; execution still needs its separate review. */
    async projectConfiguration(project:string,draft:RecordValue,valid:()=>boolean=()=>true):Promise<void>{
        if(!await this.whenIdle())throw new ServiceError('console_not_connected');
        const check=()=>{this.authorized();if(!valid())throw new ServiceError('project_configuration_selection_changed');};
        this.state.busy=true;this.state.message=undefined;this.publish();
        try{
            check();identifier(project,32);if(!this.client)await this.discover();check();
            if(!this.client||!this.binding)throw new ServiceError('console_not_connected');
            await this.observe();check();
            if(['vectors','detail'].includes(String(draft.operation))){
                const intent=await this.client.preparePreference(project,draft);check();
                await this.mutation(`Set ${String(draft.operation)} ${project}`,intent);
                this.state.message='Settings request accepted. Native preparation and indexing continue when this window closes.';return;
            }
            const wait=(response:RecordValue,kind:string)=>this.waitForProjectJob(response,project,kind,check);
            await wait(await this.mutation(`Inspect configuration ${project}`,{enqueue:await this.client.prepareJob(project,'inspect')}),'inspect');check();
            const request=await this.host.runtime.prepareConfiguration(this.binding,project,draft);check();
            const id=await wait(await this.mutation(`Configure ${String(request.configuration.operation)} ${project}`,await this.client.prepareConfiguration(project,request)),'configure');
            const result=await this.rpc({configuration_result:{id}},'configuration');check();
            if(record(result.job).id!==id||record(result.job).project!==project||result.result===null)throw new ServiceError('configuration_result_unavailable');
            await this.observe();
        }catch(error){
            if(!this.retainedOutcome(error))this.state.message=code(error);
            throw error;
        }
        finally{try{this.pending();}catch(error){this.state.message=code(error);}this.state.busy=false;this.publish();this.releaseIdle();}
    }
    async projectPlan(project:string,operation:import('./serviceClient').RepairOperation):Promise<void>{
        if(!await this.whenIdle())return;
        this.state.busy=true;this.state.message=undefined;this.publish();
        try{
            this.authorized();identifier(project,32);if(!this.client)await this.discover();
            if(!this.client)throw new ServiceError('console_not_connected');
            await this.observe();
            await this.mutation(`Plan ${operation} ${project}`,await this.client.preparePlan([project],operation));
            await this.observe();
        }catch(error){
            if(!this.retainedOutcome(error))this.state.message=code(error);
        }
        finally{try{this.pending();}catch(error){this.state.message=code(error);}this.state.busy=false;this.publish();this.releaseIdle();}
    }
    private async waitForProjectJob(response:RecordValue,project:string,kind:string,check:()=>void=()=>this.authorized()):Promise<string>{
        const accepted=record(response.job),id=identifier(accepted.id,32),deadline=Date.now()+90000;
        if(accepted.project!==project||accepted.kind!==kind)throw new ServiceError('crossed_configuration_job');
        while(true){
            check();const observed=record((await this.rpc({job:{id}},'job')).job);check();
            if(observed.id!==id||observed.project!==project||observed.kind!==kind)throw new ServiceError('crossed_configuration_job');
            if(observed.outcome){
                if(record(observed.outcome).state!=='succeeded')throw new ServiceError('configuration_job_failed');
                return id;
            }
            if(Date.now()>=deadline)throw new ServiceError(kind==='inspect'?'Prerequisite inspection has not finished. The follow-up change has not been submitted; repeat it after inspection completes.':'The native change was accepted but has not finished. Follow its saved outcome in Recent activity.');
            await new Promise(resolve=>setTimeout(resolve,100));
        }
    }
    private async connectedAction(action:NonNullable<ReturnType<typeof consoleAction>>):Promise<void>{
        const project=action.project!,id=action.id!;
        switch(action.action){
            case 'unregisterProject':{
                const row=this.state.page?.projects.find(row=>record(record(row.project).registration).binding===project);
                if(!row||!this.binding||this.state.inactiveProjects)throw new ServiceError('project_configuration_unavailable');
                // Native inspection can use retained settings when the source folder is gone.
                // Complete preparation before asking the user to approve the exact removal.
                await this.waitForProjectJob(await this.mutation(`Inspect removal ${project}`,{enqueue:await this.client!.prepareJob(project,'inspect')}),project,'inspect');
                const request=await this.host.runtime.prepareConfiguration(this.binding,project,{operation:'unregister'});this.authorized();
                if(request.configuration.operation!=='unregister')throw new ServiceError('unexpected_removal_configuration');
                const registration=record(record(row.project).registration);
                if(!await this.host.confirm(`Remove ${String(registration.collection)} from AIRK on ${this.host.authority}? Source: ${String(registration.root)}. Data: ${String(registration.data_root)}. This stops all project work, drains its worker and permanently deletes its AIRK index data and saved settings, including when the source folder has been deleted. Source files are untouched. Only a minimal removal history remains.`))return;
                this.authorized();
                await this.waitForProjectJob(await this.mutation(`Unregister ${project}`,await this.client!.prepareConfiguration(project,request)),project,'configure');
                await this.host.projectRemoved?.(project);this.authorized();
                this.state.inactiveProjects=true;this.state.selected.clear();
                await this.page(false,true);
                this.state.message='Project indexing stopped. AIRK is deleting its index data and saved settings; removal continues without this window.';
                return;
            }
            case 'registeredProjects':case 'inactiveProjects':{
                this.state.inactiveProjects=action.action==='inactiveProjects';this.state.selected.clear();
                await this.page(false,true);return;
            }
            case 'previewCleanup':case 'deleteDerived':{
                if(!this.state.inactiveProjects||!this.binding)throw new ServiceError('cleanup_requires_inactive_project');
                const row=this.state.page?.projects.find(row=>record(record(row.project).registration).binding===project);
                if(!row)throw new ServiceError('cleanup_project_not_observed');
                let draft:RecordValue={operation:'prepare_derived_cleanup'};
                if(action.action==='deleteDerived'){
                    const result=this.state.configurationResults?.get(project),preview=result?.value;
                    if(!preview||result?.jobState!=='succeeded'||typeof preview.active!=='string'||typeof preview.guard!=='string')throw new ServiceError('cleanup_preview_required');
                    identifier(preview.guard,32);
                    const registration=record(record(row.project).registration);
                    if(!await this.host.confirm(`Delete the selected owned index state for ${String(registration.collection)} on ${this.host.authority}? Source: ${String(registration.root)}. Data: ${String(registration.data_root)}. Preview ${preview.guard}: ${String(preview.owned_files)} known files, ${String(preview.owned_bytes)} bytes${preview.inventory_complete===true?'':' (bounded lower bounds)'}. Saved settings and original sources are preserved. Unknown files and links are refused. This cannot restore the removed index; it must be indexed again. Interrupted cleanup must finish before re-registration.`))return;
                    this.authorized();draft={operation:'delete_derived',preview:preview.guard};
                }
                const request=await this.host.runtime.prepareConfiguration(this.binding,project,draft);this.authorized();
                await this.mutation(`${action.action} ${project}`,await this.client!.prepareConfiguration(project,request));break;
            }
            case 'enableSemantic':case 'disableSemantic':case 'configureDetail':case 'configureProject':{
                const row=this.state.page?.projects.find(row=>record(record(row.project).registration).binding===project);
                if(!row||!this.binding||['configureProject','configureDetail'].includes(action.action)&&!this.host.configuration)throw new ServiceError('project_configuration_unavailable');
                if(row.configuration&&['pending','running'].includes(String(record(row.configuration).state))){
                    this.state.message='A settings change is already queued or applying. Check its activity before submitting another change. Published semantic coverage still describes the earlier index.';return;
                }
                const draft=['configureProject','configureDetail'].includes(action.action)?await this.host.configuration!(row,action.action==='configureDetail'?'detail':'advanced'):{operation:'vectors',enabled:action.action==='enableSemantic'};this.authorized();if(!draft)return;
                if(['vectors','detail'].includes(String(draft.operation))){
                    const accepted=await this.mutation(`Set ${String(draft.operation)} ${project}`,await this.client!.preparePreference(project,draft));
                    this.state.message=`Settings request ${String(record(accepted.job).state??'queued')}. AIRK will check and apply your choice, then update the index. This continues when VS Code closes.`;break;
                }
                if(!record(record(row.inspection??{}).snapshot??{}).settings_digest || objOutcomeUnknown(row)){
                    this.state.message='Checking current settings before applying your change…';this.publish();
                    await this.waitForProjectJob(await this.mutation(`Inspect settings ${project}`,{enqueue:await this.client!.prepareJob(project,'inspect')}),project,'inspect');
                }
                this.authorized();
                const request=await this.host.runtime.prepareConfiguration(this.binding,project,draft);this.authorized();
                if(request.configuration.operation==='unregister'){
                    const registration=record(record(row.project).registration);
                    if(!await this.host.confirm(`Unregister ${String(registration.collection)} from AIRK on ${this.host.authority}? Source: ${String(registration.root)}. Data: ${String(registration.data_root)}. This stops automatic work and new connections for this project; its owned worker will drain and existing clients may disconnect. Removal deletes its AIRK index data and saved settings. Source files are untouched. Only minimal removal history remains.`))return;
                    this.authorized();
                }
                const accepted=await this.mutation(`Configure ${String(request.configuration.operation)} ${project}`,await this.client!.prepareConfiguration(project,request));
                const acceptedJob=record(accepted.job);
                this.state.message=`Settings request accepted: ${String(acceptedJob.state??'queued')}. Acceptance is separate from applying settings and published coverage. Existing work runs first; semantic setup is followed by source publication and vector processing. Recent activity shows its saved outcome.`;break;
            }
            case 'firstPage':await this.page(false,true);return;
            case 'nextPage':if(this.state.page?.after)await this.page(true);return;
            case 'morePlan':await this.plan(true);return;
            case 'refreshPlan':if(this.state.plan)this.state.plan.items=[];await this.plan(false);return;
            case 'resources':{
                const configuration=await this.host.resources(record(record(this.state.resources.value?.observation).configuration));
                this.authorized();if(configuration)await this.rpc({configure_resources:{configuration}},'resources');break;
            }
            case 'retryRecovery':{
                const row=this.state.page?.projects.find(row=>record(record(row.project).registration).binding===project);
                if(!row||!Array.isArray(row.recovery))throw new ServiceError('recovery_not_observed');
                const recoveries=row.recovery.map(record);
                if(recoveries.some(r=>r.reason==='helper_unavailable')){
                    const model=await this.rpc('embedding_status','embedding_status');this.authorized();
                    if(model.available===false)await this.rpc('start_embedding','embedding_starting');
                }
                const failed=recoveries.find(r=>r.state==='needs_attention'&&typeof r.job==='string');
                if(failed)await this.mutation(`Retry recovery ${project}`,await this.client!.prepareRetry(identifier(failed.job,32)));
                this.state.message='Recovery retry requested. Confirm progress and published readiness in this view.';break;
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
                const row=this.state.page?.projects.find(row=>record(record(row.project).registration).binding===project);
                const current=record(record(row?.progress??{}).job??{}),queued=record(row?.pending_job??{});
                const matches=(job:RecordValue)=>['pending','running'].includes(String(job.state)) && (job.kind===kind || kind==='index'&&job.kind==='reconcile');
                const existing=matches(queued)?queued:matches(current)?current:undefined;
                if(existing){this.state.message=`Already ${String(existing.state)}: ${String(existing.kind)} (${String(existing.id).slice(0,12)}). Following the existing request.`;return;}
                if(kind==='model_setup'&&row?.inspection&&record(record(row.inspection).snapshot).vectors_enabled===false){
                    this.state.message='Semantic search is turned off. Use Enable semantic search to prepare the model and enable indexing together.';return;
                }
                const accepted=await this.mutation(`${action.action} ${project}`,{enqueue:await this.client!.prepareJob(project,kind)});
                const job=record(accepted.job);
                this.state.message=`${kind==='inspect'?'Index/settings check':kind==='index'?'Source indexing':'Selected-model preparation'} accepted: ${String(job.state??'queued')} (job ${String(job.id).slice(0,12)}). Acceptance does not mean completion. Recent activity records the outcome; work can wait behind an existing job or project pause.`;break;
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
                const labels:Record<string,string>={reconcile:'Update the index from saved source files',recover:'Recover captured index changes',verify:'Verify retained index data',vectors:'Regenerate semantic vectors',rebuild:'Rebuild the index from sources'};
                const actions=this.state.plan!.items.map(item=>{
                    const row=this.state.page?.projects.find(row=>record(record(row.project).registration).binding===item.project);
                    const registration=record(record(row?.project).registration);
                    const name=String(registration.root??'').split(/[\\/]/).filter(Boolean).at(-1)??String(item.project??'project');
                    return `${name}: ${labels[String(item.action)]??String(item.action??'Check')}`;
                }).join('\n');
                if(!await this.host.confirm(`Start the reviewed repair?\n${actions}\nUnavailable projects retain their failure or skipped receipts.`,'Start repair'))return;
                await this.rpc({plan_execute:{id:identifier(plan.id,32),preview:identifier(plan.preview_digest,32),allow_fallback:action.allowFallback}},'plan');
                this.state.message='Repair accepted. Follow Indexing for current work; acceptance does not mean the repair has finished.';break;
            }
            case 'pausePlan':case 'resumePlan':
                await this.rpc({plan_pause:{id:identifier(this.state.plan?.value.id,32),paused:action.action==='pausePlan'}},'plan');break;
            case 'cancelPlan':await this.rpc({plan_cancel:{id:identifier(this.state.plan?.value.id,32)}},'plan');break;
            default:throw new ServiceError('unsupported_console_action');
        }
    }
    dispose():void{this.disposed=true;this.client?.dispose();this.client=undefined;this.releaseIdle();}
}
