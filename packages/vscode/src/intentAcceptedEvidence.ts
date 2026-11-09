// SPDX-License-Identifier: Apache-2.0
/** Immutable protocol anchors retain authenticated historical authority. They
 * are not an atomic, globally latest cache of a running operation. */
import {isDeepStrictEqual} from 'node:util';
import type {SavedIntent,ConsoleIntent} from './serviceIntents';
import {catalogFenceObservation,indexSetupObservation,sourceChangeObservation,validatePolicyStatus} from './indexingCatalog';
import {SPLIT_STAGES,SPLIT_JOURNAL_BYTES,validateSplitSnapshot,validateSplitContinuity,validateSplitRecoveryReceipt} from './indexingSplit';
import {validateRefreshJob,validateRefreshReceipt,validateRefreshRefusal} from './referenceRefresh';
import {intentObservationValidators,ServiceError,record} from './serviceClient';

export interface AcceptedIntentEvidence {slot:string;value:unknown}
const fenceSlots=['fence_pending','fence_applied','catalog_metadata_applied'] as const;
const setupSlots=['setup_reserved','setup_prepared','setup_ready'] as const;
const sourceSlots=['source_begin','source_completed'] as const;
const refreshSlots=['refresh_refusal','refresh_admitted','refresh_configured','refresh_result','refresh_terminal'] as const;
const splitSlots=[...SPLIT_STAGES.map(stage=>'split_'+stage),'split_canceled_pending','split_canceled_complete',
    'split_finalization_prepared','split_finalization_publishing','split_finalization_acknowledging','split_finalization_complete'] as const;
export const MAX_ACCEPTED_INTENT_SLOTS=16;
export const MAX_UNRESERVED_ACCEPTED_INTENT_SLOTS=3;
export const MIN_RESERVED_ACCEPTED_INTENT_BYTES=SPLIT_JOURNAL_BYTES;
/** Existing sticky fields only; latest lookups and dispatch state stay mutable. */
export const ACCEPTED_INTENT_FIELDS=['nativeFenceObservation','nativeSetupObservation','nativeSourceObservation',
    'nativeSplitSnapshot','nativeSplitReceipt','nativeRefreshReceipt','nativeRefreshTerminalReceipt','nativePolicyObservation','nativeCatalogObservation'] as const;
// The canonical facade neither constructs a transport nor dispatches requests.
const canonical=intentObservationValidators;
const clone=<T>(value:T):T=>structuredClone(value);
function failure():never{throw new ServiceError('private_intent_accepted_evidence_conflict');}
const terminal=(value:unknown):boolean=>['succeeded','failed','canceled'].includes(String(record(value).state));

export function acceptedIntentSlots(intent:ConsoleIntent):readonly string[]{
    if('catalog_apply'in intent)return fenceSlots;
    if('catalog_index_setup_apply'in intent)return setupSlots;
    if('catalog_source_change_resolve'in intent)return sourceSlots;
    if('catalog_policy_resume'in intent)return ['policy_resume'];
    if('reference_refresh'in intent)return refreshSlots;
    if('catalog_split_apply'in intent)return splitSlots;
    if('catalog_split_recover'in intent)return [...splitSlots,'split_recovery_receipt'];
    return [];
}
function snapshotSlot(input:unknown):string{
    const value=record(input),receipt=record(value.receipt);
    if(value.finalization!=null)return 'split_finalization_'+String(record(value.finalization).progress);
    const state=record(receipt.state);
    if(state.state==='canceled_before_fence')return state.cleanup_pending===true?'split_canceled_pending':'split_canceled_complete';
    return 'split_'+String(receipt.stage);
}
function policyJob(saved:SavedIntent,input:unknown):string{
    if(!('catalog_policy_resume'in saved.intent))failure();
    const value=record(input),intent=saved.intent;
    if(value.status==='catalog_policy_resume_receipt'){
        if(!canonical.policyReceipt(value,intent))failure();
        return String(record(record(value.receipt).job).id);
    }
    if(value.status==='catalog_policy_resumed')canonical.policyResumed(value,intent);
    else {
        validatePolicyStatus(value,intent.catalog_policy_resume.index_id);
        if(!canonical.policyReceipt(record(value.resume_receipt),intent)
            ||value.accepted_job_id!==record(record(record(value.resume_receipt).receipt).job).id)failure();
    }
    return String(value.accepted_job_id);
}
function select(saved:SavedIntent):AcceptedIntentEvidence[]{
    const intent=saved.intent,result:AcceptedIntentEvidence[]=[];
    const current=(body:Record<string,unknown>|undefined,action:(value:Record<string,unknown>)=>void)=>{
        if(!body)return;try{action(body);}catch{/* A full invalid current reply stays diagnosis, never immutable authority. */}
    };
    if('catalog_apply'in intent){
        const classify=(value:Record<string,unknown>,required=false)=>{
            if(!['catalog_applied','catalog_receipt','catalog_outcome_unknown'].includes(String(value.status)))failure();
            const observed=catalogFenceObservation(value);
            if(value.fenced_receipt==null&&!observed&&value.receipt==null){if(required)failure();return;}
            canonical.catalog(value,intent.catalog_apply.request,intent.request_digest,intent.catalog_apply.preview_digest);
            result.push({slot:value.fenced_receipt!=null||observed?.state==='applied'?'fence_applied':observed?'fence_pending':'catalog_metadata_applied',value});
        };
        if(saved.nativeFenceObservation)classify(saved.nativeFenceObservation,true);
        if(saved.nativeCatalogObservation)classify(saved.nativeCatalogObservation,true);
        current(saved.nativeObservation,value=>classify(value));
    }else if('catalog_index_setup_apply'in intent){
        const classify=(value:Record<string,unknown>)=>{
            canonical.setup(value,intent.catalog_index_setup_apply.draft,intent.request_digest);
            const observed=indexSetupObservation(value);if(!observed)failure();
            result.push({slot:'setup_'+String(observed.stage),value});
        };
        if(saved.nativeSetupObservation)classify(saved.nativeSetupObservation);
        current(saved.nativeObservation,classify);
    }else if('catalog_source_change_resolve'in intent){
        // Accepted source proof may reside in either field. Preserve both when
        // completion and the retained begin response are separately available.
        const classify=(value:Record<string,unknown>)=>{
            if(!sourceChangeObservation(value))failure();
            canonical.source(value,intent.catalog_source_change_resolve.begin_operation,intent.namespace);
            const observed=sourceChangeObservation(value)!;
            result.push({slot:observed.completion!=null?'source_completed':'source_begin',value});
        };
        if(saved.nativeSourceObservation)classify(saved.nativeSourceObservation);
        current(saved.nativeObservation,classify);
    }else if('catalog_policy_resume'in intent){
        const classify=(value:Record<string,unknown>)=>{
            if(!(value.status==='catalog_policy_resumed'||value.status==='catalog_policy_resume_receipt'&&value.receipt!=null
                ||value.status==='catalog_policy_status'&&value.resume_receipt!=null))failure();
            policyJob(saved,value);result.push({slot:'policy_resume',value});
        };
        if(saved.nativePolicyObservation)classify(saved.nativePolicyObservation);
        current(saved.nativeObservation,classify);
    }else if('reference_refresh'in intent){
        const classify=(value:Record<string,unknown>)=>{
            if(value.status==='reference_refresh_review_required'){
                validateRefreshRefusal(value,intent);result.push({slot:'refresh_refusal',value});
            }else{
                if(value.status==='job')validateRefreshJob(value.job,intent);else validateRefreshReceipt(value,intent);
                if(value.job==null)failure();
                if(value.configuration!=null)result.push({slot:value.result!=null?'refresh_result':'refresh_configured',value});
                else result.push({slot:'refresh_admitted',value});
                if(terminal(value.job))result.push({slot:'refresh_terminal',value});
            }
        };
        if(saved.nativeRefreshReceipt)classify(saved.nativeRefreshReceipt);
        if(saved.nativeRefreshTerminalReceipt){
            classify(saved.nativeRefreshTerminalReceipt);
            if(!terminal(saved.nativeRefreshTerminalReceipt.job))failure();
        }
        if(!saved.nativeRefreshValidationError)current(saved.nativeRefreshLookup,classify);
    }else if('catalog_split_apply'in intent||'catalog_split_recover'in intent){
        const classify=(input:unknown)=>{
            const value=validateSplitSnapshot(input,'catalog_split_apply'in intent?intent.catalog_split_apply.draft:undefined);
            if('catalog_split_recover'in intent){
                const reviewed=intent.reviewed;
                if(!isDeepStrictEqual(value.request,reviewed.request)||!isDeepStrictEqual(value.baseline,reviewed.baseline)||!isDeepStrictEqual(value.plan,reviewed.plan))failure();
                validateSplitContinuity(reviewed,value);
            }
            result.push({slot:snapshotSlot(value),value});
        };
        if(saved.nativeSplitSnapshot)classify(saved.nativeSplitSnapshot);
        current(saved.nativeObservation,value=>{if(value.snapshot!=null)classify(value.snapshot);});
        if(saved.nativeSplitReceipt){
            if(!('catalog_split_recover'in intent))failure();
            const value=validateSplitRecoveryReceipt(saved.nativeSplitReceipt,intent);
            if(record(value.outcome).outcome==='unknown')failure();
            result.push({slot:'split_recovery_receipt',value});
        }
    }
    return result;
}
export function selectAcceptedIntentEvidence(saved:SavedIntent):readonly AcceptedIntentEvidence[]{
    const allowed=acceptedIntentSlots(saved.intent),result:AcceptedIntentEvidence[]=[];
    for(const item of select(saved)){
        if(!allowed.includes(item.slot))failure();
        const prior=result.find(value=>value.slot===item.slot);
        if(prior){compatible(saved,prior,item);result[result.indexOf(prior)]=clone(preferred(saved,prior,item));continue;}
        result.push(clone(item));
    }
    return result;
}
function asSaved(saved:SavedIntent,item:AcceptedIntentEvidence):SavedIntent{
    if(!acceptedIntentSlots(saved.intent).includes(item.slot))failure();
    const value=withoutAcceptedIntentEvidence(saved),body=record(item.value);
    if(item.slot.startsWith('fence_'))value.nativeFenceObservation=body;
    else if(item.slot==='catalog_metadata_applied'){delete value.nativeObservation;value.nativeCatalogObservation=body;}
    else if(item.slot.startsWith('setup_'))value.nativeSetupObservation=body;
    else if(item.slot.startsWith('source_')){delete value.nativeObservation;value.nativeSourceObservation=body;}
    else if(item.slot==='policy_resume'){delete value.nativeObservation;value.nativePolicyObservation=body;}
    else if(item.slot.startsWith('refresh_'))value.nativeRefreshReceipt=body;
    else if(item.slot==='split_recovery_receipt')value.nativeSplitReceipt=body;
    else value.nativeSplitSnapshot=body;
    const selected=select(value);
    if(!selected.some(candidate=>candidate.slot===item.slot&&isDeepStrictEqual(candidate.value,item.value)))failure();
    return value;
}
function refreshCompatible(saved:SavedIntent,before:unknown,after:unknown):void{
    if(!('reference_refresh'in saved.intent))failure();
    const old=record(before),next=record(after);
    // A pre-admission refusal can precede an admission observed by another
    // window. It is a weaker historical review result, never evidence against
    // an accepted job and never allowed to supersede that job.
    if(old.status==='reference_refresh_review_required'||next.status==='reference_refresh_review_required')return;
    for(const field of ['configuration','result'])if(old[field]!=null&&next[field]!=null&&!isDeepStrictEqual(old[field],next[field]))failure();
    if(old.job!=null&&next.job!=null){
        validateRefreshJob(old.job,saved.intent);validateRefreshJob(next.job,saved.intent);
        if(terminal(old.job)&&terminal(next.job)&&!isDeepStrictEqual(old.job,next.job))failure();
    }
}
/** Compatibility is symmetric: a stale weaker frame is not a contradiction. */
function compatible(saved:SavedIntent,left:AcceptedIntentEvidence,right:AcceptedIntentEvidence):void{
    asSaved(saved,left);asSaved(saved,right);
    const a=record(left.value),b=record(right.value),intent=saved.intent;
    if(left.slot.startsWith('fence_')&&right.slot.startsWith('fence_')){
        const [old,next]=left.slot==='fence_applied'&&right.slot!=='fence_applied'?[b,a]:[a,b];
        canonical.fenceContinuity(old,next);return;
    }
    if(left.slot.startsWith('setup_')&&right.slot.startsWith('setup_')&&'catalog_index_setup_apply'in intent){
        const [old,next]=setupSlots.indexOf(left.slot as typeof setupSlots[number])>setupSlots.indexOf(right.slot as typeof setupSlots[number])?[b,a]:[a,b];
        canonical.setup(next,intent.catalog_index_setup_apply.draft,intent.request_digest,old);return;
    }
    if(left.slot.startsWith('source_')&&right.slot.startsWith('source_')&&'catalog_source_change_resolve'in intent){
        const [old,next]=left.slot==='source_completed'&&right.slot!=='source_completed'?[b,a]:[a,b];
        canonical.source(next,intent.catalog_source_change_resolve.begin_operation,intent.namespace,old);return;
    }
    if(left.slot.startsWith('refresh_')&&right.slot.startsWith('refresh_')){refreshCompatible(saved,a,b);return;}
    if(left.slot==='catalog_metadata_applied'&&right.slot==='catalog_metadata_applied'){
        if(!isDeepStrictEqual(a.receipt,b.receipt))failure();return;
    }
    if(left.slot==='policy_resume'&&right.slot==='policy_resume'){
        if(policyJob(saved,a)!==policyJob(saved,b))failure();
        const ar=a.status==='catalog_policy_resume_receipt'?a:record(a.resume_receipt??{}),br=b.status==='catalog_policy_resume_receipt'?b:record(b.resume_receipt??{});
        if(ar.receipt!=null&&br.receipt!=null){
            const old=record(ar.receipt),next=record(br.receipt);
            for(const field of ['source_job','configuration'])if(!isDeepStrictEqual(old[field],next[field]))failure();
            if(old.result!=null&&next.result!=null&&!isDeepStrictEqual(old.result,next.result))failure();
            if(terminal(old.job)&&terminal(next.job)&&!isDeepStrictEqual(old.job,next.job))failure();
        }
        return;
    }
    if(left.slot==='split_recovery_receipt'||right.slot==='split_recovery_receipt'){
        if(left.slot===right.slot){if(!isDeepStrictEqual(a,b))failure();return;}
        if(!('catalog_split_recover'in intent))failure();
        const acknowledgment=left.slot==='split_recovery_receipt'?a:b,snapshot=left.slot==='split_recovery_receipt'?b:a;
        const outcome=record(acknowledgment.outcome),receipt=record(snapshot.receipt);
        // An older retained snapshot may legitimately predate the accepted
        // recovery. It cannot replace the acknowledgment or authorize replay.
        if(outcome.resulting_receipt!=null&&Number(receipt.revision)<Number(record(outcome.resulting_receipt).revision)){
            validateSplitContinuity(snapshot,{...intent.reviewed,receipt:outcome.resulting_receipt,receipt_digest:outcome.receipt_digest});return;
        }
        if(outcome.outcome==='finalization_accepted'&&Number(record(snapshot.finalization??{}).revision??0)<Number(outcome.finalization_revision)){
            validateSplitContinuity(snapshot,intent.reviewed);return;
        }
        // Validation-only envelope; retain the original complete DTOs rather
        // than exposing this combined observation as a native reply.
        canonical.splitRecovery({status:'catalog_split_recovered',receipt:acknowledgment,snapshot},intent);return;
    }
    if(left.slot.startsWith('split_')&&right.slot.startsWith('split_')){
        const ar=record(a.receipt),br=record(b.receipt),af=a.finalization==null?0:Number(record(a.finalization).revision),bf=b.finalization==null?0:Number(record(b.finalization).revision);
        const [old,next]=Number(ar.revision)>Number(br.revision)||ar.revision===br.revision&&af>bf?[b,a]:[a,b];
        validateSplitContinuity(old,next);return;
    }
    failure();
}
export function acceptedIntentEvidenceDominates(saved:SavedIntent,stronger:AcceptedIntentEvidence,weaker:AcceptedIntentEvidence):boolean{
    compatible(saved,stronger,weaker);
    if(stronger.slot===weaker.slot)return false; // The first whole DTO remains immutable.
    if(stronger.slot.startsWith('fence_'))return stronger.slot==='fence_applied';
    if(stronger.slot.startsWith('setup_'))return setupSlots.indexOf(stronger.slot as typeof setupSlots[number])>setupSlots.indexOf(weaker.slot as typeof setupSlots[number]);
    if(stronger.slot.startsWith('source_'))return stronger.slot==='source_completed';
    if(stronger.slot.startsWith('refresh_')){
        const a=record(stronger.value),b=record(weaker.value);
        if(weaker.slot==='refresh_refusal')return stronger.slot!=='refresh_refusal';
        if(stronger.slot==='refresh_refusal')return false;
        if(weaker.slot==='refresh_terminal'&&!terminal(a.job))return false;
        return (b.configuration==null||a.configuration!=null)&&(b.result==null||a.result!=null)
            &&(b.job==null||a.job!=null)&&(!terminal(b.job)||terminal(a.job));
    }
    if(stronger.slot==='split_recovery_receipt'||weaker.slot==='split_recovery_receipt')return false;
    const a=record(stronger.value),b=record(weaker.value);
    return Number(record(a.receipt).revision)>Number(record(b.receipt).revision)
        ||record(a.receipt).revision===record(b.receipt).revision&&Number(record(a.finalization??{}).revision??0)>Number(record(b.finalization??{}).revision??0);
}
export function withoutAcceptedIntentEvidence(saved:SavedIntent):SavedIntent{
    // A field name alone is not authority to discard a legacy diagnostic. Only
    // canonical fields belonging to this immutable request family are elided;
    // invalid same-family historical proof refuses instead of being erased.
    select(saved);
    const intent=saved.intent,fields:readonly (typeof ACCEPTED_INTENT_FIELDS[number])[]=
        'catalog_apply'in intent?['nativeFenceObservation','nativeCatalogObservation']:
        'catalog_index_setup_apply'in intent?['nativeSetupObservation']:
        'catalog_source_change_resolve'in intent?['nativeSourceObservation']:
        'catalog_policy_resume'in intent?['nativePolicyObservation']:
        'reference_refresh'in intent?['nativeRefreshReceipt','nativeRefreshTerminalReceipt']:
        'catalog_split_apply'in intent||'catalog_split_recover'in intent?['nativeSplitSnapshot','nativeSplitReceipt']:[];
    const result=clone(saved);for(const field of fields)delete result[field];return result;
}
function preferred(saved:SavedIntent,old:AcceptedIntentEvidence|undefined,item:AcceptedIntentEvidence):AcceptedIntentEvidence{
    if(!old)return item;
    if(old.slot!==item.slot){
        if(old.slot.startsWith('refresh_')&&item.slot.startsWith('refresh_')){
            const score=(value:AcceptedIntentEvidence):number=>{
                const body=record(value.value);return body.result!=null?5:body.configuration!=null?4:terminal(body.job??{})?3:body.job!=null?2:1;
            };
            if(score(old)!==score(item))return score(item)>score(old)?item:old;
        }
        return acceptedIntentEvidenceDominates(saved,item,old)?item:old;
    }
    const a=record(old.value),b=record(item.value);
    if(item.slot.startsWith('split_')&&item.slot!=='split_recovery_receipt'){
        const ar=Number(record(a.receipt).revision),br=Number(record(b.receipt).revision);
        if(ar!==br)return br>ar?item:old;
        const af=Number(record(a.finalization??{}).revision??0),bf=Number(record(b.finalization??{}).revision??0);
        if(af!==bf)return bf>af?item:old;
    }
    if(item.slot.startsWith('refresh_')){
        const job=(value:Record<string,unknown>)=>value.job;
        const rank=(value:unknown)=>terminal(value??{})?2:record(value??{}).state==='running'?1:0;
        if(rank(job(a))!==rank(job(b)))return rank(job(b))>rank(job(a))?item:old;
    }
    if(item.slot==='fence_pending'){
        const rank=(value:Record<string,unknown>)=>{
            const pending=record(catalogFenceObservation(value)?.value??{});
            return ['prepared','fence_unknown','fenced'].indexOf(String(pending.stage));
        };
        if(rank(a)!==rank(b))return rank(b)>rank(a)?item:old;
    }
    return item;
}
export function mergeAcceptedIntentEvidence(saved:SavedIntent,evidence:readonly AcceptedIntentEvidence[]):SavedIntent{
    const anchors:AcceptedIntentEvidence[]=[];
    for(const item of [...evidence,...selectAcceptedIntentEvidence(saved)]){
        asSaved(saved,item);
        for(const previous of anchors)compatible(saved,previous,item);
        const same=anchors.find(previous=>previous.slot===item.slot);
        if(!same)anchors.push(clone(item));
        else anchors[anchors.indexOf(same)]=clone(preferred(saved,same,item));
    }
    const result=withoutAcceptedIntentEvidence(saved);
    // Compatible current progress is displayed without rewriting its immutable
    // first-in-phase anchor. A stale weaker lookup still falls back to proof.
    const candidates=[...anchors,...selectAcceptedIntentEvidence(saved)];
    const best=(prefix:string):AcceptedIntentEvidence|undefined=>candidates.filter(item=>item.slot.startsWith(prefix))
        .reduce<AcceptedIntentEvidence|undefined>((old,item)=>preferred(saved,old,item),undefined);
    const fence=best('fence_'),setup=best('setup_'),source=best('source_'),refresh=best('refresh_'),policy=best('policy_'),metadata=best('catalog_');
    if(fence)result.nativeFenceObservation=clone(record(fence.value));
    if(setup)result.nativeSetupObservation=clone(record(setup.value));
    const current=selectAcceptedIntentEvidence({...saved,...Object.fromEntries(ACCEPTED_INTENT_FIELDS.map(field=>[field,undefined]))} as SavedIntent);
    const contains=(proof:AcceptedIntentEvidence):boolean=>current.some(item=>{
        if(item.slot===proof.slot)return preferred(saved,proof,item)===item;
        return acceptedIntentEvidenceDominates(saved,item,proof);
    });
    // These families originally used only nativeObservation. Do not duplicate
    // its full accepted body and reduce the unchanged semantic record capacity.
    if(source&&!contains(source)){
        const prior=current.find(item=>item.slot.startsWith('source_'));
        if(prior&&acceptedIntentEvidenceDominates(saved,source,prior)){
            // The native full completion already contains the accepted begin.
            // Expose that original stronger reply as ordinary progress instead
            // of duplicating two native-size accepted frames during recovery.
            result.nativeObservation=clone(record(source.value));
        }else result.nativeSourceObservation=clone(record(source.value));
    }
    if(refresh)result.nativeRefreshReceipt=clone(record(refresh.value));
    const refreshTerminal=candidates.filter(item=>item.slot==='refresh_terminal')
        .reduce<AcceptedIntentEvidence|undefined>((old,item)=>preferred(saved,old,item),undefined);
    if(refreshTerminal){
        const job=record(refreshTerminal.value).job;
        const carries=(body:Record<string,unknown>|undefined):boolean=>!!body&&terminal(body.job??{})&&isDeepStrictEqual(body.job,job);
        const currentTerminal=current.some(item=>item.slot==='refresh_terminal'&&isDeepStrictEqual(record(item.value).job,job));
        if(!carries(result.nativeRefreshReceipt)&&!currentTerminal)result.nativeRefreshTerminalReceipt=clone(record(refreshTerminal.value));
    }
    if(policy&&!contains(policy))result.nativePolicyObservation=clone(record(policy.value));
    if(metadata&&!contains(metadata))result.nativeCatalogObservation=clone(record(metadata.value));
    const snapshot=candidates.filter(item=>item.slot.startsWith('split_')&&item.slot!=='split_recovery_receipt')
        .reduce<AcceptedIntentEvidence|undefined>((old,item)=>preferred(saved,old,item),undefined);
    if(snapshot)result.nativeSplitSnapshot=clone(record(snapshot.value));
    const recovery=anchors.find(item=>item.slot==='split_recovery_receipt');if(recovery)result.nativeSplitReceipt=clone(record(recovery.value));
    return result;
}
