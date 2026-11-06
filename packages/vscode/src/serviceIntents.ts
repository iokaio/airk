// SPDX-License-Identifier: Apache-2.0
import { SplitIntent, SPLIT_JOURNAL_BYTES, splitIntentKey, validateSplitApply, validateSplitRecover, validateSplitRecoveryReceipt, validateSplitSnapshot } from './indexingSplit';
import { createHash } from 'node:crypto';
import { isDeepStrictEqual } from 'node:util';
import { identifier, MutationIntent, record, RequestKey, ServiceClient, ServiceError, validateMutationIntent, count } from './serviceClient';
import { CatalogApplyIntent, IndexSetupApplyIntent, SourceResolveIntent, PolicyResumeIntent, validatePolicyResumeIntent, catalogFenceObservation, indexSetupObservation, sourceChangeObservation, validateCatalogApply, validateIndexSetupApply, validateSourceResolveIntent } from './indexingCatalog';

export interface IntentStorage {
    keys(): readonly string[];
    get<T>(key: string): T | undefined;
    update(key: string, value: unknown): Thenable<void>;
}
export interface LifecycleIntent { lifecycle:{caller:string;key:RequestKey;action:'stop'|'restart';expected_instance:string;runtime:string} }
export type ConsoleIntent=MutationIntent|LifecycleIntent|CatalogApplyIntent|IndexSetupApplyIntent|SourceResolveIntent|PolicyResumeIntent|SplitIntent;
export interface SavedIntent { schema: 1; scope: string; title: string; intent: ConsoleIntent; nativeObservation?:Record<string,unknown>; nativeFenceObservation?:Record<string,unknown>; nativeSetupObservation?:Record<string,unknown>; nativeSourceObservation?:Record<string,unknown>; localDispatchState?:'submitted'|'known_receipt'; reservedObservationBytes?:number; nativeSplitReceipt?:Record<string,unknown>; nativeSplitSnapshot?:Record<string,unknown> }
function validate(input:unknown):ConsoleIntent {
    const object=record(input);
    if('catalog_split_apply' in object)return validateSplitApply(input);
    if('catalog_split_recover' in object)return validateSplitRecover(input);
    if('catalog_policy_resume' in object)return validatePolicyResumeIntent(input);
    if('catalog_source_change_resolve' in object)return validateSourceResolveIntent(input);
    if('catalog_index_setup_apply' in object)return validateIndexSetupApply(input);
    if('catalog_apply' in object)return validateCatalogApply(input);
    if(!('lifecycle' in object))return validateMutationIntent(input);
    if(Object.keys(object).join(',')!=='lifecycle')throw new ServiceError('invalid_lifecycle_intent');
    const value=record(object.lifecycle),key=record(value.key);
    if(Object.keys(value).sort().join(',')!=='action,caller,expected_instance,key,runtime'
        ||!['stop','restart'].includes(String(value.action))||Object.keys(key).sort().join(',')!=='epoch,namespace,nonce')throw new ServiceError('invalid_lifecycle_intent');
    identifier(value.caller,16);identifier(value.expected_instance,16);identifier(value.runtime,32);
    identifier(key.namespace,16);identifier(key.nonce,16);count(key.epoch);
    return input as LifecycleIntent;
}
function identity(intent:ConsoleIntent):{namespace:string;id:string}{
    if('catalog_split_apply' in intent||'catalog_split_recover' in intent){const key=splitIntentKey(intent);return {namespace:key.namespace,id:key.nonce};}
    if('catalog_policy_resume' in intent)return {namespace:intent.catalog_policy_resume.key.namespace,id:intent.catalog_policy_resume.key.nonce};
    if('catalog_source_change_resolve' in intent)return {namespace:intent.namespace,id:intent.catalog_source_change_resolve.begin_operation};
    if('catalog_index_setup_apply' in intent){const key=record(record(intent.catalog_index_setup_apply.draft.request).key);return {namespace:identifier(key.namespace,16),id:identifier(key.nonce,16)};}
    const request='catalog_apply' in intent?intent.catalog_apply.request:'enqueue' in intent?intent.enqueue:'retry' in intent?intent.retry:'plan_create' in intent?intent.plan_create:'configure_project' in intent?intent.configure_project:intent.lifecycle;
    return {namespace:request.key.namespace,id:request.key.nonce};
}
const PREFIX = 'airk.service.intent.1.';
const LIMIT = 64;
const MAX_BYTES = 65536;
const MAX_TOTAL_BYTES = 64 * 32768;
function splitIntent(value:unknown):boolean {const intent=record(value);return 'catalog_split_apply' in intent||'catalog_split_recover' in intent;}
function chargedRecordBytes(value:unknown):number {const saved=record(value),bytes=Buffer.byteLength(JSON.stringify(saved));return saved.reservedObservationBytes===SPLIT_JOURNAL_BYTES?Math.max(bytes,SPLIT_JOURNAL_BYTES):bytes;}
/** Each immutable request has its own storage key: windows never rewrite a shared list. */
export class IntentJournal {
    readonly scope: string;
    private tail: Promise<unknown> = Promise.resolve();
    private reconcileAfter?:string;
    constructor(private readonly storage: IntentStorage, control: string, private readonly namespace: string) {
        identifier(namespace,16);
        this.scope=createHash('sha256').update(JSON.stringify([control,namespace])).digest('hex');
    }
    private key(id:string):string { return `${PREFIX}${this.scope}.${identifier(id,16)}`; }
    list(): {id:string;value:SavedIntent}[] {
        const keys=this.storage.keys().filter(key=>key.startsWith(PREFIX));
        // Memento does not provide a cross-window count transaction. Racing
        // windows can exceed the admission count; page those records instead
        // of poisoning the journal or discarding an unknown outcome.
        return keys.filter(key=>key.startsWith(`${PREFIX}${this.scope}.`)).sort().slice(0,LIMIT).map(key=>{
            const value=record(this.storage.get(key));
            if(Buffer.byteLength(JSON.stringify(value))>(splitIntent(value.intent)?SPLIT_JOURNAL_BYTES:MAX_BYTES)||value.schema!==1||value.scope!==this.scope
                ||typeof value.title!=='string'||value.title.length>160||value.localDispatchState!==undefined&&!['submitted','known_receipt'].includes(String(value.localDispatchState)))throw new ServiceError('local_intent_record_invalid');
            const intent=validate(value.intent);
            if(splitIntent(intent)?value.reservedObservationBytes!==SPLIT_JOURNAL_BYTES:value.reservedObservationBytes!==undefined)throw new ServiceError('local_intent_terminal_reservation_invalid');
            const requestKey=identity(intent);
            if(requestKey.namespace!==this.namespace)throw new ServiceError('local_intent_scope_mismatch');
            const id=identifier(key.slice(key.lastIndexOf('.')+1),16);
            if(requestKey.id!==id)throw new ServiceError('local_intent_identity_mismatch');
            return {id,value:value as unknown as SavedIntent};
        });
    }
    private serialize<T>(action:()=>Promise<T>):Promise<T> {
        const next=this.tail.then(action);this.tail=next.catch(()=>undefined);return next;
    }
    prepare(title:string,intent:ConsoleIntent,nativeObservation?:Record<string,unknown>):Promise<string> {
        // Copy before any asynchronous work; the exact persisted bytes are submitted.
        const copied=validate(JSON.parse(JSON.stringify(intent)));
        const key=identity(copied);
        const id=identifier(key.id,16);
        if(key.namespace!==this.namespace)throw new ServiceError('local_intent_scope_mismatch');
        const value:SavedIntent={schema:1,scope:this.scope,title,intent:copied,...(splitIntent(copied)?{reservedObservationBytes:SPLIT_JOURNAL_BYTES,localDispatchState:'submitted' as const}:{})};
        if('catalog_source_change_resolve' in copied){
            if(!nativeObservation)throw new ServiceError('native_source_begin_observation_required');const observed=sourceChangeObservation(nativeObservation);if(!observed)throw new ServiceError('native_source_begin_observation_required');
            const begin=record(observed.begin),metadata=record(begin.metadata),request=record(begin.request);
            if(metadata.operation_id!==copied.catalog_source_change_resolve.begin_operation||record(request.key).namespace!==copied.namespace)throw new ServiceError('crossed_local_source_begin');
            value.nativeObservation=structuredClone(nativeObservation);
        }else if('catalog_policy_resume' in copied){
            if(!nativeObservation)throw new ServiceError('native_policy_observation_required');
            const snapshot=record(nativeObservation.snapshot),policy=record(snapshot.policy);
            if(nativeObservation.status!=='catalog_policy_status'||nativeObservation.index_id!==copied.catalog_policy_resume.index_id||snapshot.operation_id!==copied.catalog_policy_resume.operation_id||policy.owner_binding!==copied.project)throw new ServiceError('crossed_local_policy_resume');
            value.nativeObservation=structuredClone(nativeObservation);
        }else if(nativeObservation!==undefined)throw new ServiceError('unexpected_local_native_observation');
        return this.serialize(async()=>{
            if(!title||title.length>160||Buffer.byteLength(JSON.stringify(value))>(splitIntent(copied)?SPLIT_JOURNAL_BYTES:MAX_BYTES))throw new ServiceError('local_intent_record_oversize');
            this.list();
            const existing=this.storage.get(this.key(id));
            if(existing!==undefined){
                const saved=record(existing);
                if(saved.schema!==1||saved.scope!==this.scope||!isDeepStrictEqual(saved.intent,copied))throw new ServiceError('local_intent_key_conflict');
                return id;
            }
            const savedKeys=this.storage.keys().filter(key=>key.startsWith(PREFIX));
            if(savedKeys.length>=LIMIT)throw new ServiceError('local_intent_capacity_exceeded');
            const totalBytes=savedKeys.reduce((sum,key)=>{const saved=this.storage.get(key);return sum+(saved===undefined?0:chargedRecordBytes(saved));},0);
            if(totalBytes+chargedRecordBytes(value)>MAX_TOTAL_BYTES)throw new ServiceError('local_intent_byte_capacity_exceeded');
            await this.storage.update(this.key(id),value);
            return id;
        });
    }
    /** Preserve the full authenticated native outcome without changing the immutable request. */
    observeCatalog(id:string,response:Record<string,unknown>):Promise<void>{
        const copied=structuredClone(response);
        return this.serialize(async()=>{
            const saved=this.list().find(entry=>entry.id===id);if(!saved||!('catalog_apply' in saved.value.intent)&&!('catalog_index_setup_apply' in saved.value.intent)&&!('catalog_source_change_resolve' in saved.value.intent)&&!('catalog_policy_resume' in saved.value.intent)&&!('catalog_split_apply' in saved.value.intent)&&!('catalog_split_recover' in saved.value.intent))throw new ServiceError('local_catalog_intent_not_found',true);
            let splitSnapshot:Record<string,unknown>|undefined,splitReceipt:Record<string,unknown>|undefined;
            if('catalog_split_apply'in saved.value.intent||'catalog_split_recover'in saved.value.intent){
                const intent=saved.value.intent;
                if(copied.snapshot!=null){try{const candidate=validateSplitSnapshot(copied.snapshot,'catalog_split_apply'in intent?intent.catalog_split_apply.draft:undefined);if('catalog_split_recover'in intent&&(!isDeepStrictEqual(candidate.request,intent.reviewed.request)||!isDeepStrictEqual(candidate.baseline,intent.reviewed.baseline)||!isDeepStrictEqual(candidate.plan,intent.reviewed.plan)))throw new Error('crossed stored split status');splitSnapshot=candidate;}catch{/* Retain invalid native data for diagnosis without treating it as accepted proof. */}}
                if('catalog_split_recover'in intent&&copied.receipt!=null){try{splitReceipt=validateSplitRecoveryReceipt(copied.receipt,intent);}catch{/* An invalid acknowledgment cannot replace historical accepted proof. */}}
            }
            const knownSplit='catalog_split_apply'in saved.value.intent?splitSnapshot!==undefined:splitReceipt!==undefined&&record(splitReceipt.outcome).outcome!=='unknown';
            const typedFence=['catalog_applied','catalog_receipt','catalog_outcome_unknown'].includes(String(copied.status));
            const priorFence=saved.value.nativeFenceObservation,observed=typedFence?catalogFenceObservation(copied):undefined,prior=priorFence?catalogFenceObservation(priorFence):undefined,observedFence=typedFence&&(copied.fenced_receipt!=null||observed!==undefined);
            const wasApplied=!!priorFence&&(priorFence.fenced_receipt!=null||prior?.state==='applied');
            const isApplied=copied.fenced_receipt!=null||observed?.state==='applied';
            const setup='catalog_index_setup_apply' in saved.value.intent&&indexSetupObservation(copied);
            const source='catalog_source_change_resolve' in saved.value.intent,knownSource=sourceChangeObservation(saved.value.nativeObservation??{})?saved.value.nativeObservation:saved.value.nativeSourceObservation;
            const value:SavedIntent={...saved.value,nativeObservation:copied,...(splitIntent(saved.value.intent)&&knownSplit?{localDispatchState:'known_receipt' as const}:{}),...(splitSnapshot?{nativeSplitSnapshot:structuredClone(splitSnapshot)}:{}),...(splitReceipt&&record(splitReceipt.outcome).outcome!=='unknown'?{nativeSplitReceipt:structuredClone(splitReceipt)}:{}),...(observedFence&&(!wasApplied||isApplied)?{nativeFenceObservation:copied}:{}),...(setup?{nativeSetupObservation:copied}:{}),...(source?{nativeSourceObservation:sourceChangeObservation(copied)?undefined:knownSource}:{}),...('catalog_index_setup_apply' in saved.value.intent?{localDispatchState:copied.status==='catalog_index_setup_receipt'&&copied.receipt!=null?'known_receipt' as const:'submitted' as const}:{})},bytes=Buffer.byteLength(JSON.stringify(value));
            if(bytes>(splitIntent(saved.value.intent)?SPLIT_JOURNAL_BYTES:MAX_BYTES))throw new ServiceError('local_intent_record_oversize',true);
            const total=this.storage.keys().filter(key=>key.startsWith(PREFIX)&&key!==this.key(id)).reduce((sum,key)=>{const item=this.storage.get(key);return sum+(item===undefined?0:chargedRecordBytes(item));},chargedRecordBytes(value));
            if(total>MAX_TOTAL_BYTES)throw new ServiceError('local_intent_byte_capacity_exceeded',true);
            await this.storage.update(this.key(id),value);
        });
    }
    markSubmitted(id:string):Promise<void>{
        return this.serialize(async()=>{
            const saved=this.list().find(entry=>entry.id===id);if(!saved)throw new ServiceError('local_intent_not_found');
            const value:SavedIntent={...saved.value,localDispatchState:'submitted'},bytes=Buffer.byteLength(JSON.stringify(value));
            if(bytes>(splitIntent(saved.value.intent)?SPLIT_JOURNAL_BYTES:MAX_BYTES))throw new ServiceError('local_intent_record_oversize');
            const total=this.storage.keys().filter(key=>key.startsWith(PREFIX)&&key!==this.key(id)).reduce((sum,key)=>{const item=this.storage.get(key);return sum+(item===undefined?0:chargedRecordBytes(item));},chargedRecordBytes(value));
            if(total>MAX_TOTAL_BYTES)throw new ServiceError('local_intent_byte_capacity_exceeded');await this.storage.update(this.key(id),value);
        });
    }
    async reconcile(client:ServiceClient,onReceipt:(response:Record<string,unknown>)=>Promise<void>,onRemoved?:(project:string)=>Promise<void>,onFailure?:(id:string,error:unknown)=>void):Promise<void>{
        const candidates=this.list().filter(saved=>!('lifecycle' in saved.value.intent)&&!('plan_create' in saved.value.intent)&&!('catalog_apply' in saved.value.intent)&&!('catalog_index_setup_apply' in saved.value.intent)&&!('catalog_source_change_resolve' in saved.value.intent)&&!('catalog_policy_resume' in saved.value.intent)&&!('catalog_split_apply' in saved.value.intent)&&!('catalog_split_recover' in saved.value.intent));
        const after=this.reconcileAfter??'';
        const batch=[...candidates.filter(saved=>saved.id>after),...candidates.filter(saved=>saved.id<=after)].slice(0,8);
        for(const saved of batch){
            this.reconcileAfter=saved.id;
            try{
                const intent=saved.value.intent;
                if('lifecycle' in intent||'plan_create' in intent||'catalog_index_setup_apply' in intent||'catalog_source_change_resolve' in intent||'catalog_policy_resume' in intent||'catalog_split_apply' in intent||'catalog_split_recover' in intent)continue;
                if('catalog_apply' in intent){
                    const request=intent.catalog_apply.request;
                    const result=await client.rpc({catalog_receipt:{caller:request.caller,key:request.key}},'catalog_receipt');
                    const complete=client.validateCatalogObservation(result,request,intent.request_digest,intent.catalog_apply.preview_digest);
                    if(saved.value.nativeFenceObservation)client.validateCatalogFenceContinuity(saved.value.nativeFenceObservation,result);
                    await this.observeCatalog(saved.id,result);if(!complete)continue;
                    await onReceipt(result);await this.forget(saved.id);continue;
                }
                const request='enqueue' in intent?intent.enqueue:'configure_project' in intent?intent.configure_project:intent.retry;
                const project='project' in request?request.project:undefined;
                const result=await client.rpc({request_receipt:{caller:request.caller,key:request.key,...(project?{project}:{})}},'request_receipt');
                if(result.removed_project!==undefined){
                    if(!project||identifier(result.removed_project,32)!==project)throw new ServiceError('crossed_removed_project_receipt');
                    await onRemoved?.(project);await this.forget(saved.id);continue;
                }
                if(result.job===null)continue; // Missing/expired receipts never authorize another write.
                const job=record(result.job);identifier(job.id,32);
                if('project' in request&&job.project!==request.project)throw new ServiceError('crossed_job_receipt');
                await onReceipt({status:'job',job});await this.forget(saved.id);
            }catch(error){if(!onFailure)throw error;onFailure(saved.id,error);}
        }
    }
    async submit(client:ServiceClient|undefined,id:string,onReceipt?:(response:Record<string,unknown>)=>Promise<void|boolean>,
        lifecycle?:(intent:LifecycleIntent)=>Promise<Record<string,unknown>>):Promise<Record<string,unknown>> {
        const saved=this.list().find(entry=>entry.id===id);
        if(!saved)throw new ServiceError('local_intent_not_found');
        // Errors, including a lost acknowledgement, preserve the immutable record.
        const intent=saved.value.intent;
        // Catalog applies are never replayed from a saved journal entry. Only receipt reads may settle them.
        if('catalog_split_apply' in intent||'catalog_split_recover' in intent)throw new ServiceError('split_requires_exact_native_receipt_reconciliation');
        if('catalog_apply' in intent)throw new ServiceError('catalog_apply_requires_receipt_reconciliation');
        if('catalog_index_setup_apply' in intent)throw new ServiceError('index_setup_requires_receipt_reconciliation');
        if('catalog_source_change_resolve' in intent)throw new ServiceError('source_resolution_requires_observation');
        if('catalog_policy_resume' in intent)throw new ServiceError('policy_resume_requires_receipt_reconciliation');
        if('lifecycle' in intent&&!lifecycle)throw new ServiceError('native_lifecycle_transport_required');
        if(!('lifecycle' in intent)&&!client)throw new ServiceError('console_not_connected');
        const response='lifecycle' in intent?await lifecycle!(intent):await client!.submitMutation(intent);
        const retire=await onReceipt?.(response);
        if(retire!==false)await this.forget(id);
        return response;
    }
    forgetUnchanged(id:string,expected:unknown,valid:()=>boolean):Promise<void>{
        return this.serialize(async()=>{
            if(!valid()||!isDeepStrictEqual(this.list().find(entry=>entry.id===id)?.value,expected))throw new ServiceError('stale_local_tracking_review');
            await this.storage.update(this.key(id),undefined);
        });
    }
    forget(id:string):Promise<void> {
        return this.serialize(async()=>{await this.storage.update(this.key(id),undefined);});
    }
}
