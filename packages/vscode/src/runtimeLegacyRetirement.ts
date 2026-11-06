// SPDX-License-Identifier: Apache-2.0
// Trusted native runtime administration. In-memory only; never a saved evidence session.
import { ServiceError, record, RecordValue, identifier } from './serviceClient';
import { catalogId } from './indexingCatalog';
import { isDeepStrictEqual } from 'node:util';
const LIMIT=64*1024;
export interface LegacyRetirementHost {
    legacyRetirementCandidates(after?:string):Promise<RecordValue>;
    legacyRetirementReview(index:string):Promise<RecordValue>;
    legacyRetirementAuthorize(review:RecordValue,admit:()=>void):Promise<RecordValue>;
    legacyRetirementReceipt(request:RecordValue):Promise<RecordValue|null>;
    legacyRetirementResume(request:RecordValue,admit:()=>void):Promise<RecordValue>;
}
function bounded(value:unknown):RecordValue {
    if(Buffer.byteLength(JSON.stringify(value))>LIMIT)throw new ServiceError('legacy_retirement_reply_oversize');
    return record(value);
}
function nonnegative(value:unknown):number{
    if(typeof value!=='number'||!Number.isSafeInteger(value)||value<0)throw new ServiceError('invalid_legacy_retirement_count');return value;
}
export function validateRetirementReply(value:unknown,expected?:RecordValue):RecordValue {
    const reply=bounded(value);
    if(!['review','authorized','applied'].includes(String(reply.status)))throw new ServiceError('invalid_legacy_retirement_reply');
    const auth=reply.status==='applied'?record(record(reply.receipt).authorization):reply.status==='authorized'?record(reply.authorization):reply;
    const preview=record(auth.preview),request=record(preview.request);
    if(preview.schema!==1||request.schema!==1||preview.source_configuration!=='pending_metadata'
        ||preview.effect!=='retire_and_cancel_with_cleanup_pending')throw new ServiceError('invalid_legacy_retirement_review');
    identifier(auth.review_digest,32);identifier(preview.request_digest,32);catalogId(request.index_id,'idx_');
    const key=record(request.key),registration=record(request.registration),work=record(preview.affected_work);
    identifier(request.caller,16);identifier(request.upgrade_id,16);identifier(key.namespace,16);identifier(key.nonce,16);nonnegative(key.epoch);
    for(const value of [request.plan_digest,request.installer_identity,request.expected_authorizations_digest,
        preview.original_backup_digest,preview.original_legacy_data_digest,work.job_ids_digest])identifier(value,32);
    if(nonnegative(work.jobs)>1024)throw new ServiceError('invalid_legacy_retirement_work');
    identifier(registration.binding,32);
    for(const value of [registration.root,registration.data_root]){
        if(typeof value!=='string'||!value.length||value.length>4096||/[\u0000-\u001f\u007f]/.test(value))throw new ServiceError('invalid_legacy_retirement_registration');
    }
    if(typeof registration.collection!=='string'||!registration.collection.length||registration.collection.length>128)throw new ServiceError('invalid_legacy_retirement_registration');
    if(expected&&!isDeepStrictEqual(request,expected))throw new ServiceError('crossed_legacy_retirement_request');
    if(reply.status==='applied'){
        const receipt=record(reply.receipt);
        if(receipt.schema!==1||receipt.cleanup!=='pending'||nonnegative(receipt.catalog_revision)<1
            ||!isDeepStrictEqual(receipt.canceled_work,work))throw new ServiceError('invalid_legacy_retirement_cleanup');
        identifier(receipt.post_effect_legacy_data_digest,32);
    }
    return reply; // Full original body, including future native fields, is retained and exported.
}
export function retirementRequest(reply:RecordValue):RecordValue {
    const auth=reply.status==='applied'?record(record(reply.receipt).authorization):reply.status==='authorized'?record(reply.authorization):reply;
    return record(record(auth.preview).request);
}
export class LegacyRetirementSession {
    private request?:RecordValue;
    private review?:RecordValue;
    private dispatched=false;
    private generation=0;
    private pageIdentity?:RecordValue;
    last?:RecordValue;
    get uncertainOrAccepted():boolean{return this.dispatched;}
    constructor(private readonly host:LegacyRetirementHost,private readonly trusted:()=>boolean){}
    restore(value:unknown):RecordValue {
        this.check(this.generation);if(this.request)throw new ServiceError('legacy_retirement_session_already_selected');
        const reply=validateRetirementReply(value);
        if(reply.status==='review')throw new ServiceError('legacy_retirement_saved_authorization_required');
        const auth=reply.status==='applied'?record(record(reply.receipt).authorization):record(reply.authorization);
        this.review={status:'review',preview:structuredClone(auth.preview),review_digest:auth.review_digest};
        this.request=structuredClone(retirementRequest(reply));this.dispatched=true;return this.last=reply;
    }
    stop():void {this.generation++;} // Stops UI work; accepted native work is not canceled or replayed.
    private check(g:number):void {if(!this.trusted()||g!==this.generation)throw new ServiceError('legacy_retirement_review_stopped');}
    async candidates(after?:string):Promise<RecordValue>{
        const g=this.generation;this.check(g);if(after)catalogId(after,'idx_');
        const r=bounded(await this.host.legacyRetirementCandidates(after));this.check(g);
        if(r.schema!==1||r.after!==(after??null)||!Array.isArray(r.items)||r.items.length>16
            ||typeof r.subset!=='boolean'||nonnegative(r.total_eligible)>1024)throw new ServiceError('invalid_legacy_retirement_page');
        identifier(r.upgrade_id,16);identifier(r.plan_digest,32);identifier(r.installer_identity,32);identifier(r.namespace,16);
        const identity={upgrade_id:r.upgrade_id,plan_digest:r.plan_digest,installer_identity:r.installer_identity,namespace:r.namespace,total_eligible:r.total_eligible};
        if(after&&(!this.pageIdentity||!isDeepStrictEqual(identity,this.pageIdentity)))throw new ServiceError('legacy_retirement_page_changed');
        this.pageIdentity=identity;
        for(const item of r.items){const row=record(item);catalogId(row.index_id,'idx_');record(row.registration);
            if(row.source_import!=='pending_metadata'||!Object.hasOwn(row,'observation'))throw new ServiceError('invalid_legacy_retirement_candidate');
            if(row.observation!==null){const reply=validateRetirementReply(row.observation),request=retirementRequest(reply);
                if(request.index_id!==row.index_id||request.upgrade_id!==r.upgrade_id||request.plan_digest!==r.plan_digest
                    ||request.installer_identity!==r.installer_identity||!isDeepStrictEqual(request.registration,row.registration))throw new ServiceError('crossed_legacy_retirement_candidate');}
        }
        if(r.continuation!==null){catalogId(r.continuation,'idx_');
            if(!r.items.length||record(r.items[r.items.length-1]).index_id!==r.continuation||r.continuation===after)throw new ServiceError('invalid_legacy_retirement_continuation');}
        if(r.subset!==(!!after||r.continuation!==null))throw new ServiceError('invalid_legacy_retirement_subset');
        return r;
    }
    async prepare(index:string):Promise<RecordValue>{
        if(this.dispatched)throw new ServiceError('check_exact_legacy_retirement_receipt_first');
        const g=this.generation;this.check(g);catalogId(index,'idx_');
        const reply=validateRetirementReply(await this.host.legacyRetirementReview(index));this.check(g);
        if(reply.status!=='review'||retirementRequest(reply).index_id!==index)throw new ServiceError('crossed_legacy_retirement_review');
        this.review=structuredClone(reply);this.request=structuredClone(retirementRequest(reply));return this.last=reply;
    }
    async authorize(confirmed:RecordValue):Promise<RecordValue>{
        const g=this.generation;this.check(g);
        if(this.dispatched||!this.review||!isDeepStrictEqual(confirmed,this.review)||!this.request)throw new ServiceError('legacy_retirement_review_changed');
        this.dispatched=true; // Set before dispatch; a lost reply can only trigger a read-only exact lookup.
        const reply=this.accept(await this.host.legacyRetirementAuthorize(this.review,()=>this.check(g)));this.check(g);
        return this.last=reply;
    }
    async reconcile():Promise<RecordValue|null>{
        const g=this.generation;this.check(g);if(!this.request)throw new ServiceError('legacy_retirement_request_missing');
        const value=await this.host.legacyRetirementReceipt(this.request);this.check(g);
        if(value===null)return null; // Keep last complete evidence; absence never authorizes replay.
        return this.last=this.accept(value);
    }
    async resume():Promise<RecordValue>{
        const g=this.generation;this.check(g);
        if(!this.request||!['authorized','applied'].includes(String(this.last?.status)))throw new ServiceError('legacy_retirement_not_authorized');
        const reply=this.accept(await this.host.legacyRetirementResume(this.request,()=>this.check(g)));this.check(g);
        return this.last=reply;
    }
    private accept(value:unknown):RecordValue {
        const reply=validateRetirementReply(value,this.request);
        const auth=reply.status==='applied'?record(record(reply.receipt).authorization):record(reply.authorization);
        if(!this.review||reply.status==='review'||!isDeepStrictEqual(auth.preview,this.review.preview)
            ||auth.review_digest!==this.review.review_digest)throw new ServiceError('crossed_legacy_retirement_review');
        return reply;
    }
}
export function retirementJson(reply:RecordValue):string{return JSON.stringify(reply,null,2);}
export function retirementMarkdown(reply:RecordValue):string {
    return '# AIRK legacy upgrade retirement\n\nNative runtime administration; source files are preserved. Physical cleanup remains pending. Applied is a historical logical receipt, not current activation readiness or indexing/read authority. No recovery evidence is persisted by this view.\n\n```json\n'+retirementJson(reply)+'\n```\n';
}
