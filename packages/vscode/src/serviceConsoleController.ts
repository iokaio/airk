// SPDX-License-Identifier: Apache-2.0
import { randomUUID } from 'node:crypto';
import { isDeepStrictEqual } from 'node:util';
import { BootstrapObservation, LegacyStartupReview, LegacyStartupTarget, RuntimeBinding, RuntimeHost } from './serviceBootstrap';
import { count, identifier, parseConsolePage, record, RecordValue, ServiceClient, ServiceError } from './serviceClient';
import { ConsoleIntent, IntentJournal, IntentStorage, LifecycleIntent } from './serviceIntents';
import { CatalogObject, CatalogReview, IndexSetupReview, catalogBound, catalogHex, catalogObject, catalogRevision, sameCatalogValue, validateCatalogApply, validateCatalogOperation, validateGrantSelection, validateIndexSetupSelection, validateIndexSetupApply } from './indexingCatalog';
import { sourceChangeObservation, indexSetupObservation, validateSourceMutationSelection, validateSourceResolveIntent, CatalogRetirementReview } from './indexingCatalog';
import { consoleAction, ConsoleObservation, diagnosticPreview, ServiceConsoleState } from './serviceConsoleView';
import { SplitApplyIntent, SplitRecoverIntent, SplitReview, isSplitIntent, splitTerminal, validateSplitApply, validateSplitContinuity, validateSplitRecover, validateSplitSelection, validateSplitSnapshot } from './indexingSplit';

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
    resources(current:RecordValue,queryOnly?:boolean):Promise<RecordValue|undefined>;
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
    private observationToken=randomUUID();
    private observedInstance?:string;
    private observationBinding?:string;
    private registryObservationSuspended=false;
    /** Internal host ownership fence; never serialized into a webview/native schema. */
    get observationIdentity():string{return this.observationToken;}
    /** Already authenticated connection only; never discovers or starts a runtime. */
    get connectedRuntimeBinding():RuntimeBinding|undefined{
        return !this.disposed&&this.host.trusted()&&this.state.connected&&this.client?this.binding:undefined;
    }
    private binding?:RuntimeBinding;
    private bootstrap?:BootstrapObservation;
    private client?:ServiceClient;
    private journal?:IntentJournal;
    private registryNamespace?:string;
    private catalogApplyInFlight=false;
    private catalogHostIdentity?:{observation:string;host:string;key:string;revision:number};
    private catalogGrantCaller?:{observation:string;host:string;caller:string};
    private readonly nativeGrantReviews=new WeakMap<CatalogReview,{client:ServiceClient;identity:string;request:CatalogReview['request'];response:CatalogObject;previewDigest:string;requestDigest:string;contextKey:string}>();
    private readonly nativeSetupReviews=new WeakMap<IndexSetupReview,{client:ServiceClient;identity:string;review:IndexSetupReview}>();
    private readonly nativeSourceReviews=new WeakMap<CatalogReview,{client:ServiceClient;identity:string;review:CatalogReview}>();
    private readonly nativeSourceObservations=new WeakMap<CatalogObject,{client:ServiceClient;identity:string;response:CatalogObject;namespace:string}>();
    private readonly nativePolicyObservations=new WeakMap<CatalogObject,{client:ServiceClient;identity:string;response:CatalogObject;namespace:string}>();
    private readonly nativeSplitReviews=new WeakMap<SplitReview,{client:ServiceClient;identity:string;review:SplitReview}>();
    private readonly nativeSplitObservations=new WeakMap<CatalogObject,{client:ServiceClient;identity:string;response:CatalogObject;namespace:string}>();
    private nativeSourceCaller?:{identity:string;caller:string};
    private readonly setupResumeReceipts=new Map<string,{client:ServiceClient;identity:string;intent:ConsoleIntent;response:RecordValue}>();
    private readonly retirementReviews=new WeakMap<CatalogRetirementReview,{client:ServiceClient;identity:string;journal:IntentJournal;original:unknown;response:RecordValue}>();
    private eventCursor:unknown=null;
    private catalogReceiptCursor:string|undefined;
    private pageStart:{after:string|null;revision:number|null}={after:null,revision:null};
    private disposed=false;
    private legacyReview?:LegacyStartupReview;
    private readonly idle=new Set<(available:boolean)=>void>();
    private logRequest?:{action:string;work:Promise<void>};
    private pollDeadline?:number;
    private lastPollFailure?:string;
    private readonly receiptDiagnostics=new Set<string>();
    private readonly retainedOutcomes=new WeakSet<ServiceError>();
    private lifecycleReconnectAllowed=false;
    private lifecycleReconnectInFlight=false;
    constructor(private readonly host:ConsoleHost){
        this.state={revision:'',trusted:host.trusted(),busy:false,authority:host.authority,version:host.version,connected:false,
            health:empty(),resources:empty(),model:empty(),sources:empty(),bootstrap:empty(),events:[],historyOmitted:false,
            selected:new Set(),pending:[],includePaths:false};
        this.publish();
    }
    private authorized():void{if(this.disposed||!this.host.trusted()){this.clearLifecycleCapture();throw new ServiceError('runtime_client_no_longer_authorized');}}
    private clearLifecycleCapture():void{this.state.capturedLifecycle=undefined;this.state.capturedLifecycleUnavailable=undefined;this.lifecycleReconnectAllowed=false;this.lifecycleReconnectInFlight=false;}
    whenIdle():Promise<boolean>{
        if(this.disposed)return Promise.resolve(false);
        if(!this.state.busy)return Promise.resolve(true);
        return new Promise(resolve=>this.idle.add(resolve));
    }
    private releaseIdle():void{for(const resolve of this.idle)resolve(!this.disposed);this.idle.clear();}
    private publish():void{
        if(this.disposed)return;this.state.trusted=this.host.trusted();if(!this.state.trusted)this.clearLifecycleCapture();this.state.revision=randomUUID();
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
    /** Reuses the existing authenticated control client. Reads never discover, install or activate a catalog. */
    async catalogRead(method: CatalogObject): Promise<RecordValue> {
        const name=Object.keys(method);
        if(name.length!==1||!['catalog_page','catalog_detail','catalog_context','catalog_receipt','catalog_host_context','catalog_index_setup_receipt','catalog_legacy_binding','catalog_source_change_observation'].includes(name[0]))throw new ServiceError('invalid_catalog_read');
        this.authorized();const client=this.client,identity=this.observationIdentity;
        if(!client||!this.registryNamespace||this.registryObservationSuspended)throw new ServiceError('catalog_control_connection_unavailable');
        catalogBound(method);
        let response:RecordValue;
        if(name[0]==='catalog_host_context'){
            const args=catalogObject(method.catalog_host_context),revision=args.expected_catalog_revision??null;
            if(Object.keys(args).some(key=>!['key','expected_catalog_revision'].includes(key))||typeof args.key!=='string'||revision!==null&&typeof revision!=='number')throw new ServiceError('invalid_catalog_host_context_request');
            response=await client.catalogHostContext(args.key,revision);
        }else if(name[0]==='catalog_legacy_binding'){
            const args=catalogObject(method.catalog_legacy_binding),revision=args.expected_catalog_revision??null;
            if(Object.keys(args).some(key=>!['project','expected_catalog_revision'].includes(key))||typeof args.project!=='string'||revision!==null&&typeof revision!=='number')throw new ServiceError('invalid_catalog_legacy_binding_request');
            response=await client.catalogLegacyBinding(args.project,revision);
        }else response=await client.rpc(method,name[0]==='catalog_receipt'?undefined:name[0]==='catalog_context'?'catalog_detail':name[0]);
        this.authorized();if(client!==this.client||identity!==this.observationIdentity)throw new ServiceError('stale_catalog_client');
        catalogBound(response,65536);
        if(name[0]==='catalog_receipt'&&!['catalog_receipt','catalog_receipt_unprepared'].includes(String(response.status)))throw new ServiceError('unexpected_catalog_receipt_response',true,response);
        const body=response.page??response.detail??response.descriptor??(name[0]==='catalog_legacy_binding'?response.observation:undefined);
        if(body&&catalogObject(body).namespace!==this.registryNamespace)throw new ServiceError('catalog_namespace_changed_refresh');
        if(name[0]==='catalog_host_context'){
            const host=String(catalogObject(response.descriptor).host);
            if(this.catalogHostIdentity?.observation===identity&&this.catalogHostIdentity.host!==host)throw new ServiceError('stale_native_host_context');
            const descriptor=catalogObject(response.descriptor);
            this.catalogHostIdentity={observation:identity,host,key:String(descriptor.key),revision:Number(descriptor.catalog_revision)};
        }
        return response;
    }
    async catalogHostContext(key:string,expectedCatalogRevision:number|null=null):Promise<RecordValue>{
        return this.catalogRead({catalog_host_context:{key,expected_catalog_revision:expectedCatalogRevision}});
    }
    async catalogPrepare(operation: CatalogObject, revision: number): Promise<CatalogReview> {
        this.authorized();const client=this.client,identity=this.observationIdentity;
        if(!client||!this.journal||this.registryObservationSuspended)throw new ServiceError('catalog_control_connection_unavailable');
        validateCatalogOperation(operation);catalogRevision(revision);
        if(operation.operation==='create_grant')throw new ServiceError('native_grant_draft_required');
        if(['begin_index_source_change','retire_index','change_index_policy'].includes(String(operation.operation)))throw new ServiceError('native_source_draft_required');
        const request=await client.prepareCatalog(operation,revision);
        const current=()=>{this.authorized();if(client!==this.client||identity!==this.observationIdentity||request.key.namespace!==this.registryNamespace)throw new ServiceError('stale_catalog_client');};
        current();const response=await client.rpc({catalog_preview:{request}},'catalog_preview');current();catalogBound(response,65536);catalogBound(response.preview);
        const preview=catalogObject(response.preview);
        if(preview.catalog_revision!==revision||!isDeepStrictEqual(preview.operation,operation))throw new ServiceError('crossed_catalog_preview');
        const previewDigest=catalogHex(response.preview_digest,64);
        const requestDigest=catalogHex(preview.request_digest,64);
        return {request,response,previewDigest,requestDigest};
    }
    async catalogGrantDraft(contextKey:string,selection:CatalogObject,revision:number):Promise<CatalogReview>{
        this.authorized();const client=this.client,identity=this.observationIdentity,host=this.catalogHostIdentity;
        if(!client||!this.journal||this.registryObservationSuspended)throw new ServiceError('catalog_control_connection_unavailable');
        catalogHex(contextKey,64);catalogRevision(revision);const chosen=validateGrantSelection(selection);
        if(!host||host.observation!==identity||host.key!==contextKey||host.revision!==revision)throw new ServiceError('read_current_window_context_before_grant_review');
        const key=await client.key();
        const current=()=>{this.authorized();if(client!==this.client||identity!==this.observationIdentity||key.namespace!==this.registryNamespace||this.catalogHostIdentity!==host)throw new ServiceError('stale_catalog_client');};
        current();const review=await client.catalogGrantDraft(contextKey,{key,expected_catalog_revision:revision,...chosen});current();
        const grant=catalogObject(review.request.operation.grant);
        if(grant.host!==host.host)throw new ServiceError('crossed_native_grant_host');
        const caller=review.request.caller;
        if(this.catalogGrantCaller?.observation===identity&&this.catalogGrantCaller.host===host.host&&this.catalogGrantCaller.caller!==caller)throw new ServiceError('crossed_native_grant_caller');
        this.catalogGrantCaller={observation:identity,host:host.host,caller};
        this.nativeGrantReviews.set(review,{client,identity,request:structuredClone(review.request),response:structuredClone(review.response),previewDigest:review.previewDigest,requestDigest:review.requestDigest,contextKey});
        return review;
    }
    async catalogApply(review: CatalogReview, valid:()=>boolean): Promise<RecordValue> {
        this.authorized();const client=this.client,journal=this.journal,identity=this.observationIdentity;
        if(!client||!journal||this.registryObservationSuspended)throw new ServiceError('catalog_control_connection_unavailable');
        const check=()=>{this.authorized();if(!valid()||client!==this.client||journal!==this.journal||identity!==this.observationIdentity||review.request.key.namespace!==this.registryNamespace)throw new ServiceError('stale_catalog_review');};
        check();const intent=validateCatalogApply(structuredClone({catalog_apply:{request:review.request,preview_digest:review.previewDigest},request_digest:review.requestDigest}));
        if(this.catalogApplyInFlight||journal.list().some(entry=>entry.id===review.request.key.nonce))throw new ServiceError('catalog_apply_requires_receipt_reconciliation');
        if(review.request.operation.operation==='create_grant'){
            const frozen=this.nativeGrantReviews.get(review),host=this.catalogHostIdentity;
            if(!frozen||frozen.client!==client||frozen.identity!==identity||!sameCatalogValue(review.request,frozen.request)||!sameCatalogValue(review.response,frozen.response)||review.previewDigest!==frozen.previewDigest||review.requestDigest!==frozen.requestDigest||review.contextKey!==frozen.contextKey
                ||!host||host.observation!==identity||host.key!==frozen.contextKey||host.revision!==review.request.expected_catalog_revision)throw new ServiceError('stale_or_changed_native_grant_review');
            this.nativeGrantReviews.delete(review);
        }
        if(['begin_index_source_change','retire_index','change_index_policy'].includes(String(review.request.operation.operation))){
            if(journal.list().length&&(review.request.operation.operation!=='retire_index'||!this.retirementPendingAllowed(String(review.request.operation.id),review.request.key.namespace)))throw new ServiceError('catalog_outcome_unconfirmed_reconcile_first');
            const frozen=this.nativeSourceReviews.get(review);if(!frozen||frozen.client!==client||frozen.identity!==identity||!sameCatalogValue(review,frozen.review))throw new ServiceError('stale_or_changed_native_source_review');this.nativeSourceReviews.delete(review);
        }
        this.catalogApplyInFlight=true;
        // The exact reviewed request and digest are durable before the first and only dispatch.
        try{
            const id=await journal.prepare(`Indexing: ${String(review.request.operation.operation)}`,intent);
            try{check();}catch(error){await journal.forget(id);this.pending();throw error;}
            this.pending();this.publish();
            let received=false;
            try{const response=await client.submitCatalog(intent);received=true;await journal.observeCatalog(id,response);check();await journal.forget(id);this.pending();return response;}
            catch(error){
                if(error instanceof ServiceError&&error.nativeResponse){
                    try{await journal.observeCatalog(id,error.nativeResponse);}
                    catch(local){const observed=new ServiceError('local_catalog_observation_unavailable',true,error.nativeResponse);observed.cause=local;this.logReceipt(id,observed);throw observed;}
                }
                // A complete typed native refusal is a known no-write outcome. Transport uncertainty retains the intent.
                if(!received&&error instanceof ServiceError&&!error.outcomeUnknown){await journal.forget(id);this.pending();}
                this.logReceipt(id,error);throw error;
            }
        }finally{this.catalogApplyInFlight=false;}
    }
    async catalogIndexSetupDraft(selection:CatalogObject,revision:number):Promise<IndexSetupReview>{
        this.authorized();const client=this.client,identity=this.observationIdentity;
        if(!client||!this.journal||this.registryObservationSuspended)throw new ServiceError('catalog_control_connection_unavailable');
        const chosen=structuredClone(validateIndexSetupSelection(selection));catalogRevision(revision);const key=await client.key();
        const current=()=>{this.authorized();if(client!==this.client||identity!==this.observationIdentity||key.namespace!==this.registryNamespace)throw new ServiceError('stale_catalog_client');};
        current();const review=await client.catalogIndexSetupDraft({...chosen,key,expected_catalog_revision:revision});current();
        this.nativeSetupReviews.set(review,{client,identity,review:structuredClone(review)});return review;
    }
    async catalogSourceDraft(index:string,change:CatalogObject,revision:number):Promise<CatalogReview>{
        this.authorized();const client=this.client,identity=this.observationIdentity;
        if(!client||!this.journal||this.registryObservationSuspended)throw new ServiceError('catalog_control_connection_unavailable');
        const chosen=structuredClone(validateSourceMutationSelection(change));catalogRevision(revision);
        if(this.journal.list().length&&(chosen.operation!=='retire_index'||!this.retirementPendingAllowed(index,this.registryNamespace!)))throw new ServiceError('catalog_outcome_unconfirmed_reconcile_first');
        const key=await client.key();
        const current=()=>{this.authorized();if(client!==this.client||identity!==this.observationIdentity||key.namespace!==this.registryNamespace)throw new ServiceError('stale_catalog_client');};
        current();const review=await client.catalogSourceDraft({key,expected_catalog_revision:revision,index_id:index,change:chosen});current();
        if(this.nativeSourceCaller?.identity===identity&&this.nativeSourceCaller.caller!==review.request.caller)throw new ServiceError('crossed_native_source_caller');this.nativeSourceCaller={identity,caller:review.request.caller};
        this.nativeSourceReviews.set(review,{client,identity,review:structuredClone(review)});return review;
    }
    async catalogSourceObservation(beginOperation:string,namespace:string,prior?:CatalogObject):Promise<RecordValue>{
        this.authorized();const client=this.client,identity=this.observationIdentity;
        if(!client||namespace!==this.registryNamespace)throw new ServiceError('catalog_control_connection_unavailable');catalogHex(beginOperation);
        const response=await this.catalogRead({catalog_source_change_observation:{begin_operation:beginOperation}});
        this.authorized();if(client!==this.client||identity!==this.observationIdentity)throw new ServiceError('stale_catalog_client');
        client.validateSourceObservation(response,beginOperation,namespace,prior);this.nativeSourceObservations.set(response,{client,identity,response:structuredClone(response),namespace});return response;
    }
    async catalogPendingSource(index:string,namespace:string):Promise<RecordValue>{
        this.authorized();const client=this.client,identity=this.observationIdentity;
        if(!client||namespace!==this.registryNamespace||this.registryObservationSuspended)throw new ServiceError('catalog_control_connection_unavailable');
        const response=await client.catalogPendingSource(index,namespace);this.authorized();if(client!==this.client||identity!==this.observationIdentity||namespace!==this.registryNamespace||this.registryObservationSuspended)throw new ServiceError('stale_catalog_client');
        if(sourceChangeObservation(response))this.nativeSourceObservations.set(response,{client,identity,response:structuredClone(response),namespace});return response;
    }
    async catalogPolicyStatus(index:string):Promise<RecordValue>{
        this.authorized();const client=this.client,identity=this.observationIdentity,namespace=this.registryNamespace;
        if(!client||!namespace||this.registryObservationSuspended)throw new ServiceError('catalog_control_connection_unavailable');
        const response=await client.catalogPolicyStatus(index);this.authorized();
        if(client!==this.client||identity!==this.observationIdentity||namespace!==this.registryNamespace||this.registryObservationSuspended)throw new ServiceError('stale_catalog_client');
        this.nativePolicyObservations.set(response,{client,identity,response:structuredClone(response),namespace});return response;
    }
    async catalogSplitDraft(index:string,selection:CatalogObject,revision:number):Promise<SplitReview>{
        this.authorized();const client=this.client,identity=this.observationIdentity,namespace=this.registryNamespace;
        if(!client||!this.journal||!namespace||this.registryObservationSuspended)throw new ServiceError('catalog_control_connection_unavailable');
        const chosen=validateSplitSelection(structuredClone(selection));catalogRevision(revision);
        const key=await client.key();this.authorized();if(client!==this.client||identity!==this.observationIdentity||namespace!==this.registryNamespace||key.namespace!==namespace)throw new ServiceError('stale_split_review');
        const review=await client.catalogSplitDraft({...chosen,index_id:index,key,expected_catalog_revision:revision});this.authorized();
        if(client!==this.client||identity!==this.observationIdentity||namespace!==this.registryNamespace||this.registryObservationSuspended)throw new ServiceError('stale_split_review');
        this.nativeSplitReviews.set(review,{client,identity,review:structuredClone(review)});return review;
    }
    async catalogSplitSourceState(index:string,revision:number):Promise<RecordValue>{
        this.authorized();const client=this.client,identity=this.observationIdentity,namespace=this.registryNamespace;
        if(!client||!namespace||this.registryObservationSuspended)throw new ServiceError('catalog_control_connection_unavailable');
        const response=await client.catalogSplitSourceState(index,revision);this.authorized();
        if(client!==this.client||identity!==this.observationIdentity||namespace!==this.registryNamespace||this.registryObservationSuspended)throw new ServiceError('stale_split_source_state');return response;
    }
    async catalogSplitApply(review:SplitReview,valid:()=>boolean):Promise<RecordValue>{
        this.authorized();const client=this.client,journal=this.journal,identity=this.observationIdentity,frozen=this.nativeSplitReviews.get(review);
        if(!client||!journal||!frozen||frozen.client!==client||frozen.identity!==identity||!sameCatalogValue(review,frozen.review))throw new ServiceError('read_current_native_split_review');
        const intent=validateSplitApply({catalog_split_apply:{draft:structuredClone(review.draft)}}),request=catalogObject(review.draft.request);
        const check=()=>{this.authorized();if(!valid()||client!==this.client||journal!==this.journal||identity!==this.observationIdentity||catalogObject(request.key).namespace!==this.registryNamespace||this.registryObservationSuspended)throw new ServiceError('stale_split_review');};check();
        if(this.catalogApplyInFlight||this.catalogPending().length)throw new ServiceError('catalog_outcome_unconfirmed_reconcile_first');
        if(!await this.host.confirm(`Apply the complete reviewed split into ${String(request.destination_name)}? This builds independent storage, transfers the selected source scope, and applies only the reviewed Project memberships. Sources are read-only. Evidence grants and saved defaults are separate.`,'Apply reviewed split'))return review.response;
        check();this.catalogApplyInFlight=true;this.nativeSplitReviews.delete(review);
        try{return await this.submitSplitIntent(client,journal,intent,check,'Apply reviewed split');}finally{this.catalogApplyInFlight=false;}
    }
    async catalogSplitStatus(operation:string):Promise<RecordValue>{
        this.authorized();const client=this.client,identity=this.observationIdentity,namespace=this.registryNamespace;
        if(!client||!namespace||this.registryObservationSuspended)throw new ServiceError('catalog_control_connection_unavailable');
        const response=await client.catalogSplitStatus(operation);this.authorized();if(client!==this.client||identity!==this.observationIdentity||namespace!==this.registryNamespace||this.registryObservationSuspended)throw new ServiceError('stale_split_status');
        if(response.snapshot!==null)this.nativeSplitObservations.set(response,{client,identity,response:structuredClone(response),namespace});return response;
    }
    async catalogSplitRecover(observation:CatalogObject,action:'resume'|'cancel',valid:()=>boolean):Promise<RecordValue>{
        this.authorized();const client=this.client,journal=this.journal,identity=this.observationIdentity,frozen=this.nativeSplitObservations.get(observation);
        if(!client||!journal||!frozen||frozen.client!==client||frozen.identity!==identity||frozen.namespace!==this.registryNamespace||!sameCatalogValue(observation,frozen.response))throw new ServiceError('read_current_native_split_status');
        const snapshot=validateSplitSnapshot(frozen.response.snapshot),receipt=catalogObject(snapshot.receipt),postFence=['original_fenced','remainder_published','membership_applied','cleanup_pending'].includes(String(receipt.stage));
        if(this.catalogApplyInFlight||this.catalogPending().length)throw new ServiceError('catalog_outcome_unconfirmed_reconcile_first');
        const check=()=>{this.authorized();if(!valid()||client!==this.client||journal!==this.journal||identity!==this.observationIdentity||frozen.namespace!==this.registryNamespace||this.registryObservationSuspended)throw new ServiceError('stale_split_recovery');};check();
        const message=action==='cancel'?(postFence?'The original transfer fence is already active. Native cancellation cannot undo it; the recorded operation must finish with restrictions intact. Request the exact native cancellation outcome?':'Cancel the recorded split before its original transfer fence? The original stays accessible. Owned destination cleanup may remain pending.'):'Resume only the recorded native split operation and its exact pending phase? A canceled operation can resume destination cleanup only.';
        if(!await this.host.confirm(message,action==='cancel'?'Request native cancellation outcome':'Resume recorded split'))return observation;
        check();const intent=await client.prepareSplitRecovery(snapshot,action);check();this.catalogApplyInFlight=true;this.nativeSplitObservations.delete(observation);
        try{return await this.submitSplitIntent(client,journal,intent,check,`${action==='cancel'?'Cancel':'Resume'} recorded split`);}finally{this.catalogApplyInFlight=false;}
    }
    private async submitSplitIntent(client:ServiceClient,journal:IntentJournal,intent:SplitApplyIntent|SplitRecoverIntent,check:()=>void,title:string):Promise<RecordValue>{
        const id=await journal.prepare(title,intent);try{check();}catch(error){await journal.forget(id);throw error;}this.pending();this.publish();let received=false;
        try{const response='catalog_split_apply'in intent?await client.submitSplit(intent):await client.submitSplitRecovery(intent);received=true;await journal.observeCatalog(id,response);check();
            if(response.snapshot!==null&&splitTerminal(catalogObject(response.snapshot)))await journal.forget(id);
            if('catalog_split_recover'in intent&&response.receipt!=null&&(record(record(response.receipt).outcome).outcome==='restrictive_completion_required'||record(record(response.receipt).outcome).outcome==='already_complete'&&intent.reviewed.finalization==null))await journal.forget(id);
            this.pending();return response;
        }catch(error){if(error instanceof ServiceError&&error.nativeResponse)await journal.observeCatalog(id,error.nativeResponse);if(!received&&error instanceof ServiceError&&!error.outcomeUnknown)await journal.forget(id);this.pending();this.logReceipt(id,error);throw error;}
    }
    async catalogPolicyResume(observation:CatalogObject,valid:()=>boolean):Promise<RecordValue>{
        this.authorized();const client=this.client,journal=this.journal,identity=this.observationIdentity,frozen=this.nativePolicyObservations.get(observation);
        if(!client||!journal||!frozen||frozen.client!==client||frozen.identity!==identity||frozen.namespace!==this.registryNamespace||!sameCatalogValue(observation,frozen.response))throw new ServiceError('read_current_native_policy_observation');
        const snapshot=record(frozen.response.snapshot),policy=record(snapshot.policy),job=record(frozen.response.job);
        const application=snapshot.application===null?undefined:record(snapshot.application),cleanupPending=application?.physical_cleanup_pending===true;
        if(!['failed','canceled'].includes(String(job.state))&&!(job.state==='succeeded'&&cleanupPending))throw new ServiceError('native_failed_or_pending_policy_cleanup_required');
        if(this.catalogApplyInFlight||journal.list().length)throw new ServiceError('catalog_outcome_unconfirmed_reconcile_first');
        const check=()=>{this.authorized();if(!valid()||client!==this.client||journal!==this.journal||identity!==this.observationIdentity||frozen.namespace!==this.registryNamespace||this.registryObservationSuspended)throw new ServiceError('stale_policy_resume');};check();
        if(!await this.host.confirm(`Resume the recorded permanent-exclusion cleanup for index ${String(policy.index_id)}? The existing policy remains active. This retries only native operation ${String(snapshot.operation_id)}; source files are preserved.`,'Resume recorded cleanup'))return frozen.response;
        check();const intent=await client.preparePolicyResume(String(policy.index_id),String(snapshot.operation_id),String(policy.owner_binding));check();this.catalogApplyInFlight=true;
        try{
            const id=await journal.prepare('Resume permanent-exclusion cleanup',intent,frozen.response);
            try{check();}catch(error){await journal.forget(id);this.pending();throw error;}
            this.nativePolicyObservations.delete(observation);this.pending();this.publish();let received=false;
            try{const response=await client.submitPolicyResume(intent);received=true;await journal.observeCatalog(id,response);check();await journal.forget(id);this.pending();return response;}
            catch(error){
                if(error instanceof ServiceError&&error.nativeResponse?.status==='failure')await journal.observeCatalog(id,error.nativeResponse);
                if(!received&&error instanceof ServiceError&&!error.outcomeUnknown)await journal.forget(id);
                this.pending();this.logReceipt(id,error);throw error;
            }
        }finally{this.catalogApplyInFlight=false;}
    }
    async catalogSourceResolve(observation:CatalogObject,valid:()=>boolean):Promise<RecordValue>{
        this.authorized();const client=this.client,journal=this.journal,identity=this.observationIdentity,frozen=this.nativeSourceObservations.get(observation);
        const original=frozen?structuredClone(frozen.response):undefined,body=original?sourceChangeObservation(original):undefined;
        if(!client||!journal||!frozen||!original||frozen.client!==client||frozen.identity!==identity||!sameCatalogValue(observation,frozen.response)||!body||frozen.namespace!==this.registryNamespace)throw new ServiceError('read_current_native_source_observation');
        if(body.completion!==null||catalogObject(body.job).state!=='failed')throw new ServiceError('native_failed_source_job_required');
        const begin=catalogObject(body.begin),metadata=catalogObject(begin.metadata),intent=validateSourceResolveIntent({catalog_source_change_resolve:{begin_operation:metadata.operation_id},namespace:frozen.namespace});
        if(this.catalogApplyInFlight||journal.list().some(saved=>saved.id===metadata.operation_id))throw new ServiceError('source_resolution_requires_observation');
        const check=()=>{this.authorized();if(!valid()||client!==this.client||journal!==this.journal||identity!==this.observationIdentity||intent.namespace!==this.registryNamespace)throw new ServiceError('stale_source_resolution');};check();this.catalogApplyInFlight=true;this.nativeSourceObservations.delete(observation);
        try{
            const id=await journal.prepare('Indexing: resolve failed source metadata',intent,original);
            try{await journal.markSubmitted(id);check();}catch(error){await journal.forget(id);this.pending();throw error;}
            this.pending();this.publish();
            try{const response=await client.submitSourceResolve(intent,original);try{await journal.observeCatalog(id,response);}catch(local){const error=new ServiceError('local_source_observation_unavailable',true,response);error.cause=local;throw error;}check();if(client.validateSourceObservation(response,intent.catalog_source_change_resolve.begin_operation,intent.namespace,original))await journal.forget(id);this.pending();return response;}
            catch(error){
                if(error instanceof ServiceError&&error.nativeResponse){
                    if(error.nativeResponse.status==='failure')await journal.observeCatalog(id,error.nativeResponse);
                    else{try{client.validateSourceObservation(error.nativeResponse,intent.catalog_source_change_resolve.begin_operation,intent.namespace,original);await journal.observeCatalog(id,error.nativeResponse);}catch{/* Keep original proof when a crossed response is refused. */}}
                }
                if(error instanceof ServiceError&&!error.outcomeUnknown)await journal.forget(id);this.pending();this.logReceipt(id,error);throw error;
            }
        }finally{this.catalogApplyInFlight=false;}
    }
    async catalogIndexSetupApply(review:IndexSetupReview,valid:()=>boolean):Promise<RecordValue>{
        this.authorized();const client=this.client,journal=this.journal,identity=this.observationIdentity,frozen=this.nativeSetupReviews.get(review);
        if(!client||!journal||this.registryObservationSuspended)throw new ServiceError('catalog_control_connection_unavailable');
        const intent=validateIndexSetupApply(structuredClone({catalog_index_setup_apply:{draft:review.draft},request_digest:review.requestDigest})),request=catalogObject(intent.catalog_index_setup_apply.draft.request),key=catalogObject(request.key);
        const check=()=>{this.authorized();if(!valid()||client!==this.client||journal!==this.journal||identity!==this.observationIdentity||key.namespace!==this.registryNamespace)throw new ServiceError('stale_setup_review');};
        check();if(!frozen||frozen.client!==client||frozen.identity!==identity||!sameCatalogValue(review,frozen.review))throw new ServiceError('stale_or_changed_setup_review');
        if(this.catalogApplyInFlight||journal.list().some(entry=>entry.id===key.nonce))throw new ServiceError('index_setup_requires_receipt_reconciliation');
        this.nativeSetupReviews.delete(review);this.catalogApplyInFlight=true;
        try{
            const id=await journal.prepare('Indexing: explicit index setup',intent);
            // This exact new local intent has not crossed the native boundary yet.
            try{check();}catch(error){await journal.forget(id);this.pending();throw error;}
            this.pending();this.publish();let received=false;
            try{const response=await client.submitIndexSetup(intent);received=true;await journal.observeCatalog(id,response);check();if(client.validateIndexSetupObservation(response,intent.catalog_index_setup_apply.draft,intent.request_digest))await journal.forget(id);this.pending();return response;}
            catch(error){
                if(error instanceof ServiceError&&error.nativeResponse){
                    if(error.nativeResponse.status==='failure')await journal.observeCatalog(id,error.nativeResponse);
                    else{try{client.validateIndexSetupObservation(error.nativeResponse,intent.catalog_index_setup_apply.draft,intent.request_digest);await journal.observeCatalog(id,error.nativeResponse);}catch{/* Crossed results cannot replace the original native identity. */}}
                }
                if(!received&&error instanceof ServiceError&&!error.outcomeUnknown)await journal.forget(id);
                this.pending();this.logReceipt(id,error);throw error;
            }
        }finally{this.catalogApplyInFlight=false;}
    }
    private retirementTarget(id:string):{index:string;namespace:string;backing:string;revision:number;registration:CatalogObject}{
        const saved=this.journal?.list().find(entry=>entry.id===id),client=this.client;
        if(!saved||!client)throw new ServiceError('native_retirement_original_not_found');
        const intent=saved.value.intent;
        if('catalog_index_setup_apply' in intent){
            const known=saved.value.nativeSetupObservation??saved.value.nativeObservation;
            if(!known)throw new ServiceError('read_matching_native_setup_receipt_before_retirement');
            client.validateIndexSetupObservation(known,intent.catalog_index_setup_apply.draft,intent.request_digest);
            const receipt=indexSetupObservation(known),request=catalogObject(intent.catalog_index_setup_apply.draft.request);
            if(!receipt||!['reserved','prepared'].includes(String(receipt.stage)))throw new ServiceError('native_partial_setup_receipt_required');
            const registration=catalogObject(request.registration);return {index:String(receipt.index_id),namespace:String(catalogObject(request.key).namespace),backing:String(registration.binding),revision:catalogRevision(receipt.catalog_revision),registration};
        }
        if('catalog_source_change_resolve' in intent){
            const known=sourceChangeObservation(saved.value.nativeObservation??{})?saved.value.nativeObservation:saved.value.nativeSourceObservation;
            if(!known)throw new ServiceError('native_source_begin_proof_required');
            client.validateSourceObservation(known,intent.catalog_source_change_resolve.begin_operation,intent.namespace);
            const begin=catalogObject(sourceChangeObservation(known)!.begin),request=catalogObject(begin.request),operation=catalogObject(request.operation);
            const registration=catalogObject(catalogObject(operation.plan).registration);return {index:String(operation.id),namespace:intent.namespace,backing:String(registration.binding),revision:catalogRevision(catalogObject(begin.metadata).catalog_revision),registration};
        }
        throw new ServiceError('native_retirement_tracking_kind_unavailable');
    }
    catalogCanRetire(index:string,backing:string,namespace:string):boolean{
        try{this.authorized();return !!this.journal&&this.journal.list().every(saved=>{const target=this.retirementTarget(saved.id);return target.index===index&&target.backing===backing&&target.namespace===namespace;});}catch{return false;}
    }
    private retirementPendingAllowed(index:string,namespace:string):boolean{
        try{const rows=this.journal?.list();if(!rows)return false;let backing:string|undefined;return rows.every(saved=>{const target=this.retirementTarget(saved.id);backing??=target.backing;return target.index===index&&target.namespace===namespace&&target.backing===backing;});}catch{return false;}
    }
    async catalogRetirementReview(id:string):Promise<CatalogRetirementReview>{
        this.authorized();const client=this.client,journal=this.journal,identity=this.observationIdentity,target=this.retirementTarget(id),saved=journal?.list().find(entry=>entry.id===id);
        if(!client||!journal||!saved||target.namespace!==this.registryNamespace||this.registryObservationSuspended)throw new ServiceError('catalog_control_connection_unavailable');
        const original=structuredClone(saved.value),response=await client.catalogRetirementEvidence(target.index,target.namespace,target.backing,target.revision,target.registration);
        this.authorized();if(client!==this.client||journal!==this.journal||identity!==this.observationIdentity||this.registryObservationSuspended||target.namespace!==this.registryNamespace||!sameCatalogValue(original,journal.list().find(entry=>entry.id===id)?.value))throw new ServiceError('stale_retirement_evidence');
        const review={id,response};this.retirementReviews.set(review,{client,identity,journal,original,response:structuredClone(response)});return review;
    }
    async catalogStopTracking(review:CatalogRetirementReview,valid:()=>boolean):Promise<RecordValue>{
        try{this.authorized();}catch(error){this.retirementReviews.delete(review);throw error;}const frozen=this.retirementReviews.get(review),target=this.retirementTarget(review.id),journal=this.journal;
        if(!frozen||!journal||!valid()||frozen.client!==this.client||frozen.identity!==this.observationIdentity||frozen.journal!==journal||target.namespace!==this.registryNamespace||this.registryObservationSuspended||!sameCatalogValue(review.response,frozen.response)||!sameCatalogValue(frozen.original,journal.list().find(entry=>entry.id===review.id)?.value))throw new ServiceError('read_current_native_retirement_evidence');
        if(!frozen.client.validateRetirementEvidence(frozen.response,target.index,target.namespace,target.backing,target.revision,target.registration))throw new ServiceError('native_retirement_evidence_unavailable');
        this.retirementReviews.delete(review);await frozen.journal.forgetUnchanged(review.id,frozen.original,()=>valid()&&frozen.client===this.client&&frozen.identity===this.observationIdentity&&frozen.journal===this.journal&&!this.disposed&&this.host.trusted()&&target.namespace===this.registryNamespace&&!this.registryObservationSuspended);this.pending();this.publish();
        return {local_tracking:'stopped_after_native_retirement',original_intent_id:review.id,native_retirement_evidence:frozen.response};
    }
    async catalogSetupResume(id:string,valid:()=>boolean):Promise<RecordValue>{
        this.authorized();catalogHex(id);const client=this.client,journal=this.journal,identity=this.observationIdentity,known=this.setupResumeReceipts.get(id),saved=journal?.list().find(entry=>entry.id===id);
        if(!client||!journal||!known||known.client!==client||known.identity!==identity||!saved||!('catalog_index_setup_apply' in saved.value.intent)||saved.value.localDispatchState!=='known_receipt'||!sameCatalogValue(known.intent,saved.value.intent)||!sameCatalogValue(known.response,saved.value.nativeObservation))throw new ServiceError('read_matching_native_setup_receipt_before_resume');
        const intent=validateIndexSetupApply(structuredClone(saved.value.intent)),request=catalogObject(intent.catalog_index_setup_apply.draft.request),receipt=indexSetupObservation(known.response);
        client.validateIndexSetupObservation(known.response,intent.catalog_index_setup_apply.draft,intent.request_digest,saved.value.nativeSetupObservation);
        if(!receipt||!['reserved','prepared'].includes(String(receipt.stage))||this.catalogApplyInFlight)throw new ServiceError('native_setup_not_resumable');
        const check=()=>{this.authorized();if(!valid()||client!==this.client||journal!==this.journal||identity!==this.observationIdentity||catalogObject(request.key).namespace!==this.registryNamespace)throw new ServiceError('stale_setup_resume');};check();this.catalogApplyInFlight=true;this.setupResumeReceipts.delete(id);
        try{
            await journal.markSubmitted(id);check();this.pending();this.publish();
            try{const response=await client.submitIndexSetup(intent);await journal.observeCatalog(id,response);check();if(client.validateIndexSetupObservation(response,intent.catalog_index_setup_apply.draft,intent.request_digest,known.response))await journal.forget(id);this.pending();return response;}
            catch(error){
                if(error instanceof ServiceError&&error.nativeResponse){if(error.nativeResponse.status==='failure')await journal.observeCatalog(id,error.nativeResponse);else{try{client.validateIndexSetupObservation(error.nativeResponse,intent.catalog_index_setup_apply.draft,intent.request_digest,known.response);await journal.observeCatalog(id,error.nativeResponse);}catch{/* Preserve the last matching native receipt. */}}}
                this.pending();this.logReceipt(id,error);throw error; // The original setup was already accepted; a refused continuation cannot erase it.
            }
        }finally{this.catalogApplyInFlight=false;}
    }
    private pruneSetupResumeReceipts():void{
        const ids=new Set(this.journal?.list().map(entry=>entry.id)??[]);
        for(const [id,value] of this.setupResumeReceipts)if(!ids.has(id)||value.client!==this.client||value.identity!==this.observationIdentity)this.setupResumeReceipts.delete(id);
    }
    catalogPending(): readonly string[] { this.pruneSetupResumeReceipts();return this.journal?.list().filter(entry=>'catalog_apply' in entry.value.intent||'catalog_index_setup_apply' in entry.value.intent||'catalog_source_change_resolve' in entry.value.intent||'catalog_policy_resume' in entry.value.intent||isSplitIntent(entry.value.intent)).filter(entry=>!isSplitIntent(entry.value.intent)||entry.value.localDispatchState!=='known_receipt').map(entry=>entry.id)??[]; }
    catalogPendingDetails():readonly CatalogObject[]{return this.journal?.list().filter(entry=>'catalog_apply' in entry.value.intent||'catalog_index_setup_apply' in entry.value.intent||'catalog_source_change_resolve' in entry.value.intent||'catalog_policy_resume' in entry.value.intent||isSplitIntent(entry.value.intent)).map(entry=>({id:entry.id,intent:entry.value.intent,...(entry.value.nativeObservation?{native_observation:entry.value.nativeObservation}:{}),...(entry.value.nativeFenceObservation?{native_fence_observation:entry.value.nativeFenceObservation}:{}),...(entry.value.nativeSetupObservation?{native_setup_observation:entry.value.nativeSetupObservation}:{}),...(entry.value.nativeSourceObservation?{native_source_observation:entry.value.nativeSourceObservation}:{}),...(entry.value.nativeSplitReceipt?{historical_split_recovery_receipt:entry.value.nativeSplitReceipt}:{}),...(entry.value.nativeSplitSnapshot?{last_known_split_snapshot:entry.value.nativeSplitSnapshot}:{}),local_dispatch_state:entry.value.localDispatchState,can_resume_setup:this.setupResumeReceipts.get(entry.id)?.client===this.client&&this.setupResumeReceipts.get(entry.id)?.identity===this.observationIdentity&&entry.value.localDispatchState==='known_receipt'}))??[];}
    async catalogReconcile(): Promise<RecordValue[]> {
        this.pruneSetupResumeReceipts();
        this.authorized();const client=this.client,journal=this.journal,identity=this.observationIdentity;
        if(!client||!journal||this.registryObservationSuspended)throw new ServiceError('catalog_control_connection_unavailable');
        const results:RecordValue[]=[];
        const candidates=journal.list().filter(entry=>'catalog_apply' in entry.value.intent||'catalog_index_setup_apply' in entry.value.intent||'catalog_source_change_resolve' in entry.value.intent||'catalog_policy_resume' in entry.value.intent||isSplitIntent(entry.value.intent));
        const previous=candidates.findIndex(entry=>entry.id===this.catalogReceiptCursor),start=previous<0?0:(previous+1)%candidates.length;
        const batch=[...candidates.slice(start),...candidates.slice(0,start)].slice(0,8);
        for(const saved of batch){
            // Advance before the read so one unavailable operation cannot starve others.
            this.catalogReceiptCursor=saved.id;
            if(isSplitIntent(saved.value.intent)){
                const intent=saved.value.intent,response='catalog_split_apply'in intent?await client.catalogSplitReceipt(intent):await client.catalogSplitRecoveryReceipt(intent);
                this.authorized();if(client!==this.client||journal!==this.journal||identity!==this.observationIdentity||this.registryObservationSuspended)throw new ServiceError('stale_split_receipt');
                const prior=saved.value.nativeSplitSnapshot??saved.value.nativeObservation?.snapshot;
                if(response.snapshot!=null&&prior!=null)validateSplitContinuity(catalogObject(prior),catalogObject(response.snapshot));
                if('catalog_split_recover'in intent&&response.receipt!=null&&saved.value.nativeSplitReceipt&&!sameCatalogValue(response.receipt,saved.value.nativeSplitReceipt))throw new ServiceError('historical_split_receipt_changed',true,response);
                await journal.observeCatalog(saved.id,response);results.push(response);
                const known='catalog_split_apply'in intent?response.snapshot!=null:response.receipt!=null&&record(record(response.receipt).outcome).outcome!=='unknown';
                if(known&&(response.snapshot!=null&&splitTerminal(catalogObject(response.snapshot))||'catalog_split_recover'in intent&&response.receipt!=null&&(record(record(response.receipt).outcome).outcome==='restrictive_completion_required'||record(record(response.receipt).outcome).outcome==='already_complete'&&intent.reviewed.finalization==null)))await journal.forget(saved.id);
                continue;
            }
            if('catalog_policy_resume' in saved.value.intent){
                const intent=saved.value.intent,request=intent.catalog_policy_resume;
                const response=await client.catalogPolicyResumeReceipt(intent);
                this.authorized();if(client!==this.client||journal!==this.journal||identity!==this.observationIdentity)throw new ServiceError('stale_catalog_client');
                if(response.receipt===null){await journal.observeCatalog(saved.id,response);results.push(response);continue;}
                const receipt=record(response.receipt),job=record(receipt.job);
                if(saved.value.nativeObservation?.accepted_job_id!==undefined&&saved.value.nativeObservation.accepted_job_id!==job.id)throw new ServiceError('crossed_policy_resume_receipt',true,response);
                // Persist accepted proof before another independent current-state read.
                await journal.observeCatalog(saved.id,response);
                const status=await this.catalogPolicyStatus(request.index_id);
                this.authorized();if(client!==this.client||journal!==this.journal||identity!==this.observationIdentity)throw new ServiceError('stale_catalog_client');
                const observation={...status,accepted_job_id:job.id,status_basis:'current_policy',resume_receipt:response};await journal.observeCatalog(saved.id,observation);results.push(observation);await journal.forget(saved.id);continue;
            }
            if('catalog_source_change_resolve' in saved.value.intent){
                const intent=saved.value.intent,prior=sourceChangeObservation(saved.value.nativeObservation??{})?saved.value.nativeObservation:saved.value.nativeSourceObservation;
                const response=await this.catalogSourceObservation(intent.catalog_source_change_resolve.begin_operation,intent.namespace,prior);
                this.authorized();if(client!==this.client||journal!==this.journal||identity!==this.observationIdentity)throw new ServiceError('stale_catalog_client');
                const complete=client.validateSourceObservation(response,intent.catalog_source_change_resolve.begin_operation,intent.namespace,prior);await journal.observeCatalog(saved.id,response);results.push(response);if(complete)await journal.forget(saved.id);continue;
            }
            if('catalog_index_setup_apply' in saved.value.intent){
                const draft=saved.value.intent.catalog_index_setup_apply.draft,request=catalogObject(draft.request);
                const response=await this.catalogRead({catalog_index_setup_receipt:{caller:request.caller,key:request.key}});
                this.authorized();if(client!==this.client||journal!==this.journal||identity!==this.observationIdentity)throw new ServiceError('stale_catalog_client');
                const complete=client.validateIndexSetupObservation(response,draft,saved.value.intent.request_digest,saved.value.nativeSetupObservation);
                await journal.observeCatalog(saved.id,response);results.push(response);this.setupResumeReceipts.delete(saved.id);
                const receipt=indexSetupObservation(response);if(complete)await journal.forget(saved.id);else if(receipt&&['reserved','prepared'].includes(String(receipt.stage)))this.setupResumeReceipts.set(saved.id,{client,identity,intent:structuredClone(saved.value.intent),response:structuredClone(response)});continue;
            }
            const intent=saved.value.intent;if(!('catalog_apply' in intent))continue;
            const request=intent.catalog_apply.request;
            const response=await this.catalogRead({catalog_receipt:{caller:request.caller,key:request.key}});
            this.authorized();if(client!==this.client||journal!==this.journal||identity!==this.observationIdentity)throw new ServiceError('stale_catalog_client');
            if(response.status==='catalog_receipt_unprepared'){
                client.validateCatalogUnprepared(response,request,intent.request_digest);
                const prior=saved.value.nativeObservation;
                if(saved.value.nativeFenceObservation||prior?.status==='catalog_applied'||prior?.status==='catalog_receipt'&&prior.receipt!=null)throw new ServiceError('catalog_unprepared_conflicts_with_saved_proof',true,response);
                await journal.observeCatalog(saved.id,response);results.push(response);await journal.forget(saved.id);continue;
            }
            const complete=client.validateCatalogObservation(response,request,intent.request_digest,intent.catalog_apply.preview_digest);
            if(saved.value.nativeFenceObservation)client.validateCatalogFenceContinuity(saved.value.nativeFenceObservation,response);
            await journal.observeCatalog(saved.id,response);results.push(response);
            if(complete)await journal.forget(saved.id);
        }
        this.pending();return results;
    }
    private pollActive():void{
        if(this.pollDeadline!==undefined&&Date.now()>=this.pollDeadline)throw new ServiceError('status_poll_timed_out');
    }
    /** One deadline covers the whole poll, rather than a new deadline for each RPC. */
    private async observation<T>(read:()=>Promise<T>,operation='bootstrap'):Promise<T>{
        this.pollActive();if(this.pollDeadline===undefined)return read();
        let timer:ReturnType<typeof setTimeout>|undefined;
        const expired=new Promise<never>((_resolve,reject)=>{timer=setTimeout(()=>{
            this.clearLifecycleCapture();
            this.client?.dispose();this.client=undefined;
            const failure=new ServiceError('status_poll_timed_out');failure.cause=new Error(`Status poll exceeded its shared deadline while reading ${operation}`);reject(failure);
        },Math.max(1,this.pollDeadline!-Date.now()));});
        try{return await Promise.race([read(),expired]);}finally{if(timer)clearTimeout(timer);}
    }
    private async discover():Promise<void>{
        this.authorized();
        if(this.state.capturedLifecycle&&this.lifecycleReconnectAllowed){this.lifecycleReconnectAllowed=false;this.lifecycleReconnectInFlight=true;}
        else this.clearLifecycleCapture();
        try{
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
                this.clearLifecycleCapture();
                this.cachedSnapshot(this.bootstrap.inspection);return;
            }
            this.binding=this.host.runtime.binding(this.bootstrap.inspection,this.bootstrap.installDir);
            const captured=this.state.capturedLifecycle;
            if(captured&&(captured.runtime.installDir!==this.binding.installDir||captured.runtime.controlDir!==this.binding.controlDir||captured.runtime.runtimeSha256!==this.binding.runtimeSha256))this.clearLifecycleCapture();
            const scope=JSON.stringify([this.binding.installDir,this.binding.controlDir,this.binding.executable,this.binding.runtimeSha256,this.binding.version,this.binding.lifecycle]);
            if(this.observationBinding!==scope){
                this.observationBinding=scope;this.observedInstance=undefined;
                this.observationToken=randomUUID();this.clearRegistryObservation(true);
            }
            if(this.binding.payloadVerified!==false&&this.binding.version===this.host.version)this.client=this.host.runtime.client(this.binding);
            this.state.lifecycle=this.binding.lifecycle;
            if(typeof this.bootstrap.inspection.registry_namespace==='string')this.restoreJournal(this.bootstrap.inspection.registry_namespace);
            this.cachedSnapshot(this.bootstrap.inspection);
            if(this.binding.version!==this.host.version){
                try{const snapshot=await this.observation(()=>this.host.runtime.snapshot(this.binding!));this.authorized();this.cachedSnapshot(snapshot);}catch{/* Installer observations remain usable without readable cached project metadata. */}
                this.state.message='Selected native runtime differs from this extension. Review the bundled upgrade or use the matching extension; the pin is preserved.';
            }
        }
        else this.clearLifecycleCapture();
        }catch(error){this.clearLifecycleCapture();throw error;}
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
    async refresh(automatic=false):Promise<void>{
        if(this.disposed||this.state.busy)return;
        this.pollDeadline=Date.now()+15000;this.state.busy=true;this.publish();
        try{this.authorized();if(!this.client)await this.discover();await this.observe(automatic);this.lastPollFailure=undefined;}
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
    private async observe(automatic=false):Promise<void>{
        if(!this.client){await this.savedSnapshot(this.pageStart.after,this.pageStart.revision);return;}
        const wasConnected=this.state.connected;
        let replaced=false;
        try{
            const value=await this.rpc('health','health');
            const instance=identifier(record(value.observation).instance,16);
            if(this.observedInstance!==instance){
                replaced=true;
                this.observedInstance=instance;this.observationToken=randomUUID();
                // Health authenticates the service, not the registry/page from a
                // previous incarnation. Hide those rows until Events authenticates
                // the registry namespace and the project page is observed again.
                this.clearRegistryObservation(true);
            }
            this.state.health={state:'live',value,checkedAt:Date.now()};this.state.connected=true;
            // Health is independently usable while optional observations continue.
            // This reports the authenticated service, not index or inference readiness.
            this.authorized();this.publish();
        }catch(error){
            this.state.health={...this.state.health,state:'failed',code:code(error),checkedAt:Date.now()};
            this.clearLifecycleCapture();
            this.disconnected();this.client?.dispose();this.client=undefined;
            try{await this.savedSnapshot(this.pageStart.after,this.pageStart.revision);}catch{/* Retain the original health failure. */}
            throw error;
        }
        const reusable=(sample:ConsoleObservation|undefined):boolean=>{
            if(!automatic||!wasConnected||replaced||!sample?.value||sample.state==='failed')return false;
            const age=Date.now()-sample.checkedAt;
            return age>=0&&age<5000;
        };
        for(const [name,method,expected] of [['resources','resources','resources'],['model','embedding_status','embedding_status'],['sources','source_status','source_status']] as const){
            if(reusable(this.state[name])){this.state[name]={...this.state[name],state:'cached'};continue;}
            try{this.state[name]={state:'live',value:await this.rpc(method,expected),checkedAt:Date.now()};}
            catch(error){this.state[name]={...this.state[name],state:'failed',code:code(error),checkedAt:Date.now()};}
        }
        if(reusable(this.state.serviceEvents))this.state.serviceEvents={...this.state.serviceEvents!,state:'cached'};
        else try{
            const page=record((await this.rpc({logs:{limit:32}},'logs')).page);
            if(page.schema!==1||!Array.isArray(page.records)||page.records.length>32)throw new ServiceError('invalid_process_log_page');
            this.state.serviceEvents={state:'cached',value:page,checkedAt:Date.now()};
        }catch(error){this.state.serviceEvents={...this.state.serviceEvents,state:'failed',code:code(error),checkedAt:Date.now()};}
        try{await this.events();}catch(error){
            this.state.message=code(error);
            if(error instanceof ServiceError&&error.code==='event_scope_changed')this.clearRegistryObservation();
            if(error instanceof ServiceError&&['event_cursor_expired','event_scope_changed','invalid_event_cursor'].includes(error.code)){
                this.eventCursor=null;this.state.historyOmitted=true;
            }
            // Do not publish project/receipt observations under an unconfirmed
            // registry namespace. The next poll resumes with the reset cursor.
            throw error;
        }
        this.pending();
        try{await this.page(false);}catch(error){this.state.message=code(error);}
        if(this.state.plan)await this.plan(false);
    }
    private clearRegistryObservation(retainedMetadata=false):void{
        if(!this.lifecycleReconnectInFlight)this.clearLifecycleCapture();
        this.registryObservationSuspended=true;
        this.state.page=undefined;this.state.activePage=undefined;this.state.workers=undefined;
        this.state.selected.clear();
        if(retainedMetadata){
            this.state.serviceEvents=undefined;
            for(const name of ['resources','model','sources'] as const){
                if(this.state[name].value)this.state[name]={...this.state[name],state:'cached'};
            }
        }
        this.state.events=[];this.eventCursor=null;this.state.historyOmitted=false;
        this.state.plan=undefined;this.state.preview=undefined;this.state.configurationResults=undefined;
        this.state.configurationResultsOmitted=false;this.state.pending=[];
        this.legacyReview=undefined;this.state.legacyReview=undefined;this.state.legacyReceipts=undefined;this.state.legacyResult=undefined;
        this.pageStart={after:null,revision:null};
    }
    private restoreJournal(namespace:string):void{
        if(this.state.capturedLifecycle&&this.state.capturedLifecycle.namespace!==namespace)this.clearLifecycleCapture();
        this.registryNamespace=identifier(namespace,16);
        const journal=new IntentJournal(this.host.storage,this.binding!.controlDir,identifier(namespace,16));
        if(this.journal?.scope!==journal.scope){
            this.observationToken=randomUUID();this.clearRegistryObservation();
            this.journal=journal;this.state.selected.clear();this.eventCursor=null;this.state.events=[];this.state.page=undefined;this.state.activePage=undefined;
            this.legacyReview=undefined;this.state.legacyReview=undefined;this.state.legacyReceipts=undefined;
            this.state.preview=undefined;
            this.state.configurationResults=undefined;
            this.pageStart={after:null,revision:null};
        }
        if(this.registryObservationSuspended){
            const saved=this.host.storage.get<string>(`airk.service.plan.${journal.scope}`);
            this.state.plan=saved?{value:{id:identifier(saved,32)},items:[],after:null}:undefined;
            this.registryObservationSuspended=false;
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
        this.restoreJournal(identifier(page.namespace,16));
        this.lifecycleReconnectInFlight=false;
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
        let retain=false;
        return this.journal.submit(this.client,id,async response=>{await this.receipt(response);return !retain;},async intent=>{
            this.authorized();if(!this.binding)throw new ServiceError('runtime_binding_not_observed');
            const binding=this.binding,client=this.client,journal=this.journal,identity=this.observationIdentity,namespace=this.registryNamespace,runtime=JSON.stringify(binding);
            const before=this.state.connected&&this.state.health.state==='live'&&this.state.health.value?structuredClone(this.state.health.value):null;
            this.clearLifecycleCapture();
            try{
                const result=await this.host.lifecycle(binding,intent,force);this.authorized();
                if(binding!==this.binding||runtime!==JSON.stringify(this.binding)||client!==this.client||journal!==this.journal||identity!==this.observationIdentity||namespace!==this.registryNamespace||namespace!==intent.lifecycle.key.namespace)throw new ServiceError('stale_lifecycle_response',true,result);
                retain=!this.captureLifecycleResponse(result,intent,binding,before,force);return result;
            }catch(error){
                // Discovery only: expose the saved native phase/force ownership
                // without replaying the uncertain mutation or starting a service.
                try{await this.discover();}catch{/* Keep the original outcome and request. */}
                throw error;
            }
        });
    }
    private captureLifecycleResponse(response:RecordValue,intent:LifecycleIntent,binding:RuntimeBinding,before:RecordValue|null,force:boolean):boolean{
        const native=record(response.record),request=record(native.request),key=record(request.key),expected=intent.lifecycle;
        if(response.status!=='lifecycle'||!['complete','abandoned'].includes(String(native.phase))||request.caller!==expected.caller||request.runtime!==expected.runtime||request.action!==expected.action||request.expected_instance!==expected.expected_instance||key.namespace!==expected.key.namespace||key.epoch!==expected.key.epoch||key.nonce!==expected.key.nonce)throw new ServiceError('crossed_lifecycle_capture',true,response);
        const captured={capturedAt:Date.now(),qualification:'historical_native_lifecycle_response' as const,runtime:{installDir:binding.installDir,controlDir:binding.controlDir,runtimeSha256:binding.runtimeSha256},namespace:expected.key.namespace,before_health:before,force,response:structuredClone(response)};
        try{catalogBound(captured,65536);}catch{
            this.state.capturedLifecycleUnavailable='native_lifecycle_capture_unavailable_budget';this.state.message='Native lifecycle work returned a result, but its complete capture exceeds the display budget. The exact saved request remains tracked for native recovery.';return false;
        }
        this.state.capturedLifecycle=captured;this.lifecycleReconnectAllowed=true;return true;
    }
    async action(message:unknown):Promise<void>{
        const action=consoleAction(message,this.state);if(!action||this.disposed)return;
        if(LOCAL_LOG_ACTIONS.includes(action.action)){await this.localLogs(action.action);return;}
        if(this.state.busy)return;
        const requestedPreview=this.state.preview;
        this.state.busy=true;this.state.message=undefined;this.publish();
        try{
            if(['copyLifecycle','openLifecycle','saveLifecycle'].includes(action.action)){
                this.authorized();if(!this.state.capturedLifecycle)throw new ServiceError('native_lifecycle_capture_unavailable');
                const text=JSON.stringify(this.state.capturedLifecycle,null,2),operation=action.action==='copyLifecycle'?this.host.copy:action.action==='openLifecycle'?this.host.openReport:this.host.saveReport;
                if(!operation)throw new ServiceError('diagnostic_export_unavailable');await operation(text);this.authorized();return;
            }
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
                this.authorized();this.clearLifecycleCapture();this.client?.dispose();this.client=undefined;this.disconnected();
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
                this.authorized();this.clearLifecycleCapture();this.client?.dispose();this.client=undefined;this.disconnected();
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
                    const binding=this.binding,client=this.client,journal=this.journal,identity=this.observationIdentity,namespace=this.registryNamespace;
                    const before=this.state.connected&&this.state.health.state==='live'&&this.state.health.value?structuredClone(this.state.health.value):null;this.clearLifecycleCapture();
                    const result=await this.host.abandonLifecycle(binding,intent,identifier(pending.id,32));this.authorized();
                    if(binding!==this.binding||client!==this.client||journal!==this.journal||identity!==this.observationIdentity||namespace!==this.registryNamespace)throw new ServiceError('stale_lifecycle_response',true,result);
                    const captured=this.captureLifecycleResponse(result,intent,binding,before,false);await this.receipt(result);
                    if(captured)await journal.forget(intent.lifecycle.key.nonce);
                    if(captured)this.state.message=record(result.record).phase==='abandoned'?'Native lifecycle intent abandoned. Automatic starts remain disabled until Start service.':'Native lifecycle response captured. Review its historical complete JSON.';
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
                const saved=this.journal?.list().find(entry=>entry.id===action.id),intent=saved?.value.intent;
                if(intent&&('catalog_apply' in intent||'catalog_index_setup_apply' in intent||'catalog_source_change_resolve' in intent||'catalog_policy_resume' in intent||isSplitIntent(intent)))throw new ServiceError('catalog_reconciliation_required');
                if(await this.host.confirm('Forget this local request key? Native work and native lifecycle intent continue. Its outcome may remain unknown.'))await this.journal?.forget(action.id!);
                return;
            }
            if(action.action==='select'){
                if(action.selected){if(this.state.selected.size>=256)throw new ServiceError('repair_selection_full');this.state.selected.add(action.project!);}
                else this.state.selected.delete(action.project!);return;
            }
            if(action.action==='start'){
                this.clearLifecycleCapture();
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
                const unavailable=this.state.capturedLifecycleUnavailable,ownRuntime=JSON.stringify(this.binding),ownNamespace=this.registryNamespace,ownJournal=this.journal;
                this.client?.dispose();this.client=undefined;this.disconnected();
                if(action.action==='stop')return;
                await this.discover();await this.observe();
                if(unavailable==='native_lifecycle_capture_unavailable_budget'&&ownRuntime===JSON.stringify(this.binding)&&ownNamespace===this.registryNamespace&&ownJournal===this.journal
                    &&this.journal?.list().some(saved=>saved.id===intent.lifecycle.key.nonce&&sameCatalogValue(saved.value.intent,intent)))this.state.capturedLifecycleUnavailable=unavailable;
                return;
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
            case 'queryConcurrency':
            case 'resources':{
                const configuration=await this.host.resources(record(record(this.state.resources.value?.observation).configuration),action.action==='queryConcurrency');
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
    dispose():void{this.clearLifecycleCapture();this.disposed=true;this.client?.dispose();this.client=undefined;this.releaseIdle();}
}
