// SPDX-License-Identifier: Apache-2.0
// Service console state, message validation and the bounded diagnostic report. Explorer › Management
// renders this state; the separate Service Console webview was removed in 1.5.0.
import { ConsolePage, RecordValue, RepairOperation, identifier, record } from './serviceClient';
export interface ConsoleObservation { state: 'live'|'cached'|'failed'|'unavailable'; value?:RecordValue; code?:string; checkedAt:number }
export interface ServiceConsoleState {
    revision:string; trusted:boolean; busy:boolean; authority:string; version:string; connected:boolean;
    lifecycle?:'on-demand'|'external'|'user-on-demand';
    responseMode?:'terse'|'full';
    registrationAvailable?:boolean; currentProjects?:ReadonlySet<string>;
    inactiveProjects?:boolean;
    legacyAvailable?:boolean;legacyReview?:{value:RecordValue;phase?:string;checkedAt:number};legacyReceipts?:RecordValue;legacyResult?:RecordValue;
    health:ConsoleObservation; resources:ConsoleObservation; model:ConsoleObservation; sources:ConsoleObservation;
    bootstrap:ConsoleObservation; page?:ConsolePage; workers?:RecordValue; events:RecordValue[]; historyOmitted:boolean;
    serviceEvents?:ConsoleObservation;
    configurationResults?:Map<string,{job:string;jobState:string;value?:RecordValue;code?:string;checkedAt:number}>;
    configurationResultsOmitted?:boolean;
    selected:Set<string>; plan?:{value:RecordValue;items:RecordValue[];after:string|null};
    pending:{id:string;title:string;unknown:boolean;lifecycle?:boolean;forceAvailable?:boolean}[]; preview?:string; includePaths:boolean; message?:string;
}
export type ConsoleAction = {type:'service';revision:string;action:string;project?:string;id?:string;selected?:boolean;scope?:'one'|'selected'|'all';operation?:RepairOperation;includePaths?:boolean;allowFallback?:boolean};
export const CONSOLE_SIMPLE_ACTIONS=['reviewLegacyStartup','legacyStartupReceipts','removeLegacyStartup','copyLegacyStartup','registeredProjects','inactiveProjects','registerWorkspace','refresh','connect','start','stop','restart','repairRuntime','upgradeRuntime','rollbackRuntime','resumeActivation','logs','resources','responseMode','recoverLifecycle','abandonLifecycle','startEmbedding','stopEmbedding','forceEmbedding','firstPage','nextPage','morePlan','refreshPlan','pausePlan','resumePlan','cancelPlan','approvePlan','previewReport','copyReport'];
export const CONSOLE_PROJECT_ACTIONS=['unregisterProject','previewCleanup','deleteDerived','inspect','index','pause','resume','restartProject','watch','modelSetup','configureProject','copyConfiguration'];
export const CONSOLE_OPERATIONS=['auto','verify','recover','reconcile','vectors','rebuild'];
/** Messages select fixed host-owned operations; no paths, commands, tokens or RPC bodies are accepted. */
export function consoleAction(value:unknown,current:ServiceConsoleState):ConsoleAction|undefined {
    try {
        const message=record(value);
        if(message.type!=='service'||message.revision!==current.revision||typeof message.action!=='string')return;
        const keys=new Set(['type','revision','action']);const action=message.action;
        if(CONSOLE_SIMPLE_ACTIONS.includes(action)){
            if(['reviewLegacyStartup','legacyStartupReceipts','removeLegacyStartup','copyLegacyStartup'].includes(action)&&!current.legacyAvailable)return;
            if(action==='removeLegacyStartup'&&(!current.legacyReview?.value.approval||current.legacyReview.phase==='complete'))return;
            if(action==='copyLegacyStartup'&&!current.legacyReview)return;
            if(action==='previewReport'){keys.add('includePaths');if(typeof message.includePaths!=='boolean')return;}
            if(action==='approvePlan'){keys.add('allowFallback');if(typeof message.allowFallback!=='boolean')return;}
        }else if(CONSOLE_PROJECT_ACTIONS.includes(action)||action==='select'){
            keys.add('project');const project=identifier(message.project,32);
            if(!current.page?.projects.some(row=>record(record(row.project).registration).binding===project))return;
            if(['previewCleanup','deleteDerived'].includes(action)&&!current.inactiveProjects)return;
            if(current.inactiveProjects&&!['previewCleanup','deleteDerived','copyConfiguration'].includes(action))return;
            if(action==='watch'||action==='select'){keys.add('selected');if(typeof message.selected!=='boolean')return;}
        }else if(action==='repair'){
            if(current.inactiveProjects)return;
            keys.add('scope');keys.add('operation');if(!['one','selected','all'].includes(String(message.scope))||!CONSOLE_OPERATIONS.includes(String(message.operation)))return;
            if(message.scope==='one'){keys.add('project');const project=identifier(message.project,32);
                if(!current.page?.projects.some(row=>record(record(row.project).registration).binding===project))return;}
        }else if(action==='resumeLegacyStartup'){
            keys.add('id');const id=identifier(message.id,32);
            if(!current.legacyAvailable||!Array.isArray(current.legacyReceipts?.items)||!current.legacyReceipts.items.some(value=>record(value).approval===id))return;
        }else if(['retryJob','cancelJob'].includes(action)){
            keys.add('id');const id=identifier(message.id,32);
            const jobs=new Set(current.events.map(event=>event.job));
            for(const row of current.page?.projects??[]){
                const progress=row.progress?record(row.progress):undefined;if(progress?.job)jobs.add(record(progress.job).id);
                if(action==='cancelJob'&&row.pending_job)jobs.add(record(row.pending_job).id);
            }
            if(!jobs.has(id))return;
        }else if(action==='retryIntent'||action==='forgetIntent'||action==='forceIntent'){
            keys.add('id');const id=identifier(message.id,16);if(!current.pending.some(intent=>intent.id===id))return;
            if(action==='forceIntent'&&!current.pending.some(intent=>intent.id===id&&intent.lifecycle&&intent.forceAvailable))return;
        }else return;
        if(Object.keys(message).some(key=>!keys.has(key))||Object.keys(message).length!==keys.size)return;
        return message as unknown as ConsoleAction;
    }catch{return;}
}
const obj=(value:unknown):RecordValue=>value&&typeof value==='object'&&!Array.isArray(value)?value as RecordValue:{};
export function waitLabel(reason:unknown):string {
    const labels:Record<string,string>={worker_starting:'Worker is starting',worker_role_change:'Worker role is changing',
        resident_capacity:'Waiting for a resident worker slot',memory_budget:'Waiting for the memory budget',
        memory_pressure:'Waiting for observed memory pressure to ease',extraction_capacity:'Waiting for extraction capacity',
        scheduler:'Waiting for the scheduler turn',resource_policy:'Waiting for valid resource policy',
        provider_drain:'Waiting for previous provider processes to drain',embedding_recovery:'Waiting for the embedding helper',
        index_capacity:'Waiting for indexing capacity',reader_lease:'Waiting for active readers',publication:'Waiting for publication',
        collection_selection:'Waiting for collection selection',native_lock:'Waiting for a native ownership lock'};
    return typeof reason==='string'&&Object.hasOwn(labels,reason)?labels[reason]:'Unrecognized native wait observation';
}
export function diagnosticPreview(state:ServiceConsoleState,includePaths:boolean):string {
    const fields=(value:RecordValue,keys:string[])=>Object.fromEntries(keys.filter(key=>['string','number','boolean'].includes(typeof value[key])||value[key]===null).map(key=>[key,typeof value[key]==='string'?String(value[key]).slice(0,512):value[key]]));
    const health=obj(state.health.value?.observation),resources=obj(state.resources.value?.observation),model=obj(state.model.value);
    const report:RecordValue={schemaVersion:1,capturedAt:new Date().toISOString(),authority:includePaths?state.authority:'selected execution host',extensionVersion:state.version,
        observations:Object.fromEntries(['health','resources','model','sources','bootstrap'].map(name=>{const observation=state[name as 'health'];return [name,{state:observation.state,checkedAt:observation.checkedAt,code:observation.code}];})),
        service:fields(health,['instance','version','protocol','process_id','executable_sha256','runtime_sha256','uptime_ms',...(includePaths?['user']:[])]),
        bootstrap:fields(obj(state.bootstrap.value),['installed','payloadVerified','registryError','error','platform','bundleDigest','selectedRuntime','selectedVersion','verificationBasis',...(includePaths?['installDir']:[])]),
        startupManager:fields(obj(state.bootstrap.value?.startupManager),['mode','id','platform','installed','loaded','running','registered_definition_matches','verification_failed']),
        runtimeRepair:fields(obj(state.bootstrap.value?.payloadRepair),['phase','runtime','preview',...(includePaths?['preserved_directory']:[])]),
        runtimeActivation:fields(obj(obj(state.bootstrap.value?.diagnosis).pending_activation),['candidate','phase']),
        runtimeActivationResult:fields(obj(obj(state.bootstrap.value?.diagnosis).activation_result),['candidate','preceding','outcome','observed_unix_ms']),
        runtimeInstaller:{pendingCopies:Array.isArray(obj(state.bootstrap.value?.diagnosis).pending_stages)?(obj(state.bootstrap.value?.diagnosis).pending_stages as unknown[]).length:null,
            stagingFailed:typeof obj(state.bootstrap.value?.diagnosis).staging_error==='string',activationFailed:typeof obj(state.bootstrap.value?.diagnosis).activation_error==='string'},
        lifecycle:fields(obj(state.bootstrap.value?.lifecycle),['explicitly_stopped','recorded_instance']),
        resources:fields(resources,['resident_workers','resident_limit','running_jobs','waiting_projects','logical_cpus','extraction_threads_limit','extraction_threads_reserved','inference_threads_limit','memory_limit_bytes','memory_reserved_bytes','query_headroom_bytes','background_io_limit_bytes_per_second','background_io_reserved_bytes_per_second','observed_rss_bytes','available_memory_bytes','telemetry_complete','pressure','pending_provider_drains','provider_drain_error']),
        resourceConfiguration:fields(obj(resources.configuration),['preset','extraction_threads','inference_threads','memory_bytes','background_io_bytes_per_second']),
        inference:fields(model,['available','worker','loads','reuses','sessions','observation_age_ms']),
        projects:(state.page?.projects??[]).map(row=>{const project=obj(row.project),registration=obj(project.registration),inspection=obj(row.inspection),snapshot=obj(inspection.snapshot),progress=obj(row.progress),work=obj(progress.progress),job=obj(progress.job);return {
            project:registration.binding,collection:includePaths?registration.collection:'[redacted]',root:includePaths?registration.root:'[redacted]',dataRoot:includePaths?registration.data_root:'[redacted]',
            fencingGeneration:project.fencing_generation,paused:project.paused,inspectionObservedAt:inspection.observed_unix_ms,
            inspection:fields(snapshot,['incarnation','publication','index_state','source_state','recovery_required','indexed_files','indexed_vectors','vectors_enabled','failure']),
            progress:fields(work,['phase','paused','stalled','elapsed_ms','phase_elapsed_ms','last_advance_age_ms','unpaused_idle_ms','captured_files','extracted_files','prepared_changes','committed_changes','publications','total_files','completed_semantic_units','publication']),
            job:fields(job,['id','kind','state','attempts','fencing_generation']),
            configuration:fields(obj(row.configuration),['id','kind','state','attempts','fencing_generation']),
            configurationResult:fields(obj(state.configurationResults?.get(String(registration.binding))),['job','jobState','code','checkedAt']),
            failedObservations:row.failed_observations};}),
        events:state.events.slice(-32).map(event=>fields(event,['sequence','observed_unix_ms','project','kind','job','job_kind','fencing_generation','attempts','code'])),
        processLogs:{state:state.serviceEvents?.state??'unavailable',checkedAt:state.serviceEvents?.checkedAt,
            malformedRecords:state.serviceEvents?.value?.malformed_records,tailTruncated:state.serviceEvents?.value?.tail_truncated,
            records:(Array.isArray(state.serviceEvents?.value?.records)?state.serviceEvents!.value!.records:[]).slice(-32).map(value=>fields(obj(value),['instance','kind','project','job','count','observed_unix_ms']))},
        historyOmitted:state.historyOmitted,configurationResultsOmitted:state.configurationResultsOmitted??false,moreProjects:!!state.page?.after,pendingRequestCount:state.pending.length,pathsRedacted:!includePaths,truncated:false};
    let text=JSON.stringify(report,null,2);
    while(Buffer.byteLength(text)>65536){report.truncated=true;const projects=report.projects as unknown[],events=report.events as unknown[];
        if(events.length)events.shift();else if(projects.length)projects.pop();else break;text=JSON.stringify(report,null,2);}
    return text;
}
