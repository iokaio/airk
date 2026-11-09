// SPDX-License-Identifier: Apache-2.0
/** Native mutation review is separate from evidence reads and explicit requery. */
import {createHash} from 'node:crypto';
import {isDeepStrictEqual} from 'node:util';
import {catalogBound,catalogHex,catalogId,catalogObject,catalogRevision} from './indexingCatalog';
import type {ConfigurationIntent,ProjectConfigurationRequest,RequestKey,ServiceClient} from './serviceClient';
import type {IntentJournal} from './serviceIntents';
import {ServiceError,retainNativeOutcome,retainNativeOutcomeDuring,isLocalTrackingFailure,isNativeReplyValidationFailure} from './nativeOutcome';
type ObjectValue=Record<string,unknown>;
export const REFRESH_SELECTION_BYTES=12*1024;
export const REFRESH_NATIVE_BYTES=64*1024;
export const REFRESH_QUERY_BYTES=16*1024;
// Original result64 + query16 + native request16 + retained acknowledgment64
// + latest full lookup64 + bounded local fields fit before any native dispatch.
export const REFRESH_JOURNAL_BYTES=256*1024;
export interface ReferenceRefreshIntent {reference_refresh:{configure_project:ConfigurationIntent;original_query:ObjectValue|null;original_result:ObjectValue|null;evidence_retention:'memory_only'|'local_approved'}}
export function refreshCapability(value:unknown):boolean {try{return catalogObject(catalogObject(value).capabilities).reference_refresh_contract===1;}catch{return false;}}
export function refreshDigest(value:unknown):string{return createHash('sha256').update(JSON.stringify(value)).digest('hex');}
const REFRESH_QUERY_TOOLS=['airk_search','airk_read_file','airk_list_files','airk_symbols','airk_references','airk_outline','airk_file_info','airk_imports','airk_code_facts','airk_document_facts','airk_changes','airk_context','airk_relationships'] as const;
export function refreshQueryTool(input:unknown):typeof REFRESH_QUERY_TOOLS[number] {
    const tool=REFRESH_QUERY_TOOLS.find(tool=>tool===input);
    if(tool===undefined)throw new Error('invalid_refresh_original_query');
    return tool;
}
function requestEpoch(value:unknown):number {if(typeof value!=='number'||!Number.isSafeInteger(value)||value<0)throw new Error('invalid_refresh_request_epoch');return value;}
function exact(value:ObjectValue,names:string):void {if(Object.keys(value).sort().join(',')!==names.split(',').sort().join(','))throw new Error('invalid_refresh_fields');}
function array(value:unknown,max:number):unknown[] {if(!Array.isArray(value)||!value.length||value.length>max)throw new Error('invalid_refresh_selection');return value;}
function path(value:unknown):string {if(typeof value!=='string'||!value||Buffer.byteLength(value)>4096||/[\\:\u0000-\u001f\u007f]/.test(value)||value.split('/').some(v=>!v||v==='.'||v==='..'))throw new Error('invalid_refresh_path');return value;}
function publication(value:unknown):{text:string;incarnation:string;generation:number} {if(typeof value!=='string')throw new Error('invalid_refresh_publication');const fields=value.split(':');if(fields.length!==4||fields[0]!=='airk-r1'||!/^[1-9][0-9]*$/.test(fields[2]))throw new Error('invalid_refresh_publication');catalogHex(fields[1],32);catalogHex(fields[3],64);const generation=Number(fields[2]);if(!Number.isSafeInteger(generation))throw new Error('invalid_refresh_publication');return {text:value,incarnation:fields[1],generation};}
export function validateRefreshSelection(input:unknown):ObjectValue {
    const value=catalogObject(input);catalogBound(value,REFRESH_SELECTION_BYTES);
    exact(value,'schema,caller,key,original_result_digest,original_evidence_selection_digest,original_query_digest,selection,mode,members');
    if(value.schema!==1||!['selected_only','full_reconcile_reviewed'].includes(String(value.mode)))throw new Error('invalid_refresh_selection');
    catalogHex(value.caller,32);for(const key of ['original_result_digest','original_evidence_selection_digest','original_query_digest'])catalogHex(value[key],64);
    const key=catalogObject(value.key);exact(key,'namespace,epoch,nonce');catalogHex(key.namespace,32);catalogHex(key.nonce,32);requestEpoch(key.epoch);
    const selected=catalogObject(value.selection);exact(selected,'schema,context_key,namespace,expected_catalog_revision,grant_id,expected_grant_revision,project_ids,index_ids');
    if(selected.schema!==1)throw new Error('invalid_refresh_selection_schema');catalogHex(selected.namespace,32);catalogHex(selected.context_key,64);catalogRevision(selected.expected_catalog_revision);catalogRevision(selected.expected_grant_revision);catalogId(selected.grant_id,'grt_');
    const projects=array(selected.project_ids,32).map(v=>catalogId(v,'prj_'));if(new Set(projects).size!==projects.length)throw new Error('duplicate_refresh_project');
    const indexes=array(selected.index_ids,32).map(v=>catalogId(v,'idx_'));if(new Set(indexes).size!==indexes.length)throw new Error('duplicate_refresh_index');
    const members=array(value.members,8);let count=0;const seen=new Set<string>();
    for(const raw of members){const member=catalogObject(raw);exact(member,'index_id,original_publication,sources');const id=catalogId(member.index_id,'idx_');if(!indexes.includes(id)||seen.has(id))throw new Error('crossed_refresh_member');seen.add(id);publication(member.original_publication);
        const paths=new Set<string>();for(const raw of array(member.sources,64)){const source=catalogObject(raw);exact(source,'source_root_id,source_relative_path,canonical_root,indexed_hash');catalogId(source.source_root_id,'src_');const relative=path(source.source_relative_path);catalogHex(source.indexed_hash,64);
            if(typeof source.canonical_root!=='string'||Buffer.byteLength(source.canonical_root)>32768||!/^(?:\/|[A-Za-z]:[\\/]|\\\\)/.test(source.canonical_root)||/[\u0000-\u001f\u007f]/.test(source.canonical_root))throw new Error('invalid_refresh_original_root');
            const identity=JSON.stringify([source.source_root_id,relative]);if(paths.has(identity)||++count>64)throw new Error('duplicate_or_oversize_refresh_sources');paths.add(identity);
        }
    }return value;
}
// Core RefreshSelection::identity serializes typed structs in declaration
// order, not arbitrary JS insertion order or alphabetically sorted JSON keys.
// Every request object is already deny_unknown_fields; validate before ordering.
export function refreshSelectionCanonicalJson(input:unknown):string {
    const v=validateRefreshSelection(input),key=catalogObject(v.key),scope=catalogObject(v.selection);
    const nativeText=(value:unknown):string=>{
        if(typeof value!=='string'||/\p{Cc}/u.test(value)||Array.from(value).some(c=>{const n=c.codePointAt(0)!;return n>=0xd800&&n<=0xdfff;}))throw new Error('invalid_refresh_native_string');
        return value;
    };
    const members=(v.members as unknown[]).map(raw=>{
        const member=catalogObject(raw);
        return {index_id:member.index_id,original_publication:member.original_publication,sources:(member.sources as unknown[]).map(raw=>{
            const source=catalogObject(raw);
            return {source_root_id:source.source_root_id,source_relative_path:nativeText(source.source_relative_path),canonical_root:nativeText(source.canonical_root),indexed_hash:source.indexed_hash};
        })};
    });
    return JSON.stringify({schema:v.schema,caller:v.caller,key:{namespace:key.namespace,epoch:key.epoch,nonce:key.nonce},
        original_result_digest:v.original_result_digest,original_evidence_selection_digest:v.original_evidence_selection_digest,original_query_digest:v.original_query_digest,
        selection:{schema:scope.schema,context_key:scope.context_key,namespace:scope.namespace,expected_catalog_revision:scope.expected_catalog_revision,grant_id:scope.grant_id,expected_grant_revision:scope.expected_grant_revision,project_ids:scope.project_ids,index_ids:scope.index_ids},
        mode:v.mode,members});
}
export function refreshSelectionIdentity(input:unknown):string {
    const hash=createHash('sha256').update(Buffer.from('AIRK-DIGEST-1\0'));
    for(const field of [Buffer.from('airk/catalog-refresh-selection/1'),Buffer.from(refreshSelectionCanonicalJson(input),'utf8')]){
        const size=Buffer.alloc(8);size.writeBigUInt64BE(BigInt(field.length));hash.update(size).update(field);
    }
    return hash.digest('hex');
}
export function refreshConfiguration(input:unknown):ProjectConfigurationRequest {
    const value=catalogObject(input),configuration=catalogObject(value.configuration);catalogBound(value,16384);
    if(!['configuration,incarnation,settings','configuration,incarnation,prepare,settings'].includes(Object.keys(value).sort().join(','))||value.prepare!==undefined&&value.prepare!==false)throw new Error('invalid_refresh_configuration');
    catalogHex(value.incarnation,32);catalogHex(value.settings,64);exact(configuration,'operation,selection');if(configuration.operation!=='refresh_references')throw new Error('invalid_refresh_operation');validateRefreshSelection(configuration.selection);return input as ProjectConfigurationRequest;
}
export function validateRefreshIntent(input:unknown):ReferenceRefreshIntent {
    const value=catalogObject(input);exact(value,'reference_refresh');const review=catalogObject(value.reference_refresh);exact(review,'configure_project,original_query,original_result,evidence_retention');if(!['memory_only','local_approved'].includes(String(review.evidence_retention)))throw new Error('invalid_refresh_evidence_retention');const configuration=catalogObject(review.configure_project);exact(configuration,'project,caller,key,request');catalogHex(configuration.project,64);catalogHex(configuration.caller,32);
    const request=refreshConfiguration(configuration.request),selection=catalogObject(request.configuration.selection);if(request.prepare!==undefined)throw new Error('noncanonical_refresh_prepare_field');if(configuration.caller!==selection.caller||!isDeepStrictEqual(configuration.key,selection.key))throw new Error('crossed_refresh_request_key');
    if(review.original_query===null||review.original_result===null){if(review.original_query!==null||review.original_result!==null||review.evidence_retention!=='memory_only')throw new Error('incomplete_refresh_original');return input as ReferenceRefreshIntent;}
    const query=catalogObject(review.original_query);catalogBound(query,REFRESH_QUERY_BYTES);exact(query,'tool,arguments');refreshQueryTool(query.tool);catalogObject(query.arguments);
    const result=catalogObject(review.original_result);catalogBound(result,REFRESH_NATIVE_BYTES);if(selection.original_query_digest!==refreshDigest(query)||selection.original_result_digest!==refreshDigest(result)||selection.original_evidence_selection_digest!==refreshDigest(selection.members))throw new Error('changed_refresh_original_evidence');
    return input as ReferenceRefreshIntent;
}
export function refreshIntentKey(intent:ReferenceRefreshIntent):RequestKey{return intent.reference_refresh.configure_project.key;}
export function refreshJobId(intent:ReferenceRefreshIntent):string {
    const request=intent.reference_refresh.configure_project,key=request.key,epoch=Buffer.alloc(8);epoch.writeBigUInt64LE(BigInt(key.epoch));const digest=createHash('sha256').update(Buffer.from('AIRK-DIGEST-1\0'));
    for(const field of [Buffer.from('airk-service-job/1'),Buffer.from(request.project,'hex'),Buffer.from(request.caller,'hex'),Buffer.from(key.namespace,'hex'),epoch,Buffer.from(key.nonce,'hex')]){const size=Buffer.alloc(8);size.writeBigUInt64BE(BigInt(field.length));digest.update(size).update(field);}return digest.digest('hex');
}
export function validateRefreshJob(input:unknown,intent:ReferenceRefreshIntent):ObjectValue {
    const job=catalogObject(input);if(job.id!==refreshJobId(intent)||job.project!==intent.reference_refresh.configure_project.project||job.kind!=='configure'||!['pending','running','succeeded','failed','canceled'].includes(String(job.state)))throw new Error('crossed_refresh_job');return job;
}
function reviewReason(input:unknown):ObjectValue {const review=catalogObject(input);exact(review,'reasons');const reasons=array(review.reasons,11);const known=['no_current_index','pending_recovery','configuration_changed','extractor_changed','incomplete_scan','directory_selection','control_input','dependency_input','unqualified_control_provenance','control_inputs_changed','control_context_changed'];if(reasons.some(reason=>!known.includes(String(reason)))||new Set(reasons).size!==reasons.length)throw new Error('invalid_refresh_review_reason');return review;}
export function validateRefreshRefusal(input:unknown,intent:ReferenceRefreshIntent):ObjectValue {
    const value=catalogObject(input);catalogBound(value,REFRESH_NATIVE_BYTES);const expected=intent.reference_refresh.configure_project;
    if(value.status!=='reference_refresh_review_required'||value.project!==expected.project||value.caller!==expected.caller||!isDeepStrictEqual(value.key,expected.key)||!isDeepStrictEqual(value.request,expected.request))throw new Error('crossed_refresh_review_refusal');reviewReason(value.review);return value;
}
export function validateRefreshReceipt(input:unknown,intent:ReferenceRefreshIntent):ObjectValue {
    const value=catalogObject(input);catalogBound(value,REFRESH_NATIVE_BYTES);if(value.status!=='reference_refresh_receipt'||typeof value.expired!=='boolean')throw new Error('invalid_refresh_receipt');
    if(value.job===null){if(value.configuration!==null||value.result!==null)throw new Error('unbound_refresh_receipt');return value;}
    const job=validateRefreshJob(value.job,intent);if(value.expired)throw new Error('crossed_refresh_receipt');
    if(value.configuration===null){if(value.result!==null)throw new Error('unbound_refresh_result');return value;}
    const work=catalogObject(value.configuration);catalogHex(work.engine,64);if(!isDeepStrictEqual(work.request,intent.reference_refresh.configure_project.request))throw new Error('changed_refresh_accepted_configuration');
    if(value.result!==null){const result=catalogObject(value.result),selection=catalogObject(intent.reference_refresh.configure_project.request.configuration.selection),members=array(selection.members,8),member=catalogObject(members[0]);
        if(result.selection_digest!==refreshSelectionIdentity(selection))throw new Error('crossed_refresh_selection_digest');
        if(result.schema==='airk.reference-refresh-review-required/1'){
            if(result.original_query_digest!==selection.original_query_digest||result.publication_applied!==false||result.requery_executed!==false||job.state==='succeeded')throw new Error('crossed_refresh_refusal_result');reviewReason(result.review);return value;
        }
        if(result.schema!=='airk.reference-refresh-result/1'||result.original_query_digest!==selection.original_query_digest||result.effective_mode!==selection.mode||result.effective_scope!==(selection.mode==='selected_only'?'selected_sources':'whole_index_explicitly_reviewed')||result.requery_executed!==false||result.before_publication!==member.original_publication)throw new Error('crossed_refresh_result');
        const before=publication(result.before_publication),after=publication(result.after_publication);if(after.incarnation!==before.incarnation||after.generation<before.generation||after.generation===before.generation&&after.text!==before.text||catalogObject(catalogObject(result.status).publication).token!==after.text||catalogObject(result.report).generation!==after.generation)throw new Error('crossed_refresh_publication');
        if(job.state==='succeeded'&&(catalogObject(job.outcome).state!=='succeeded'||catalogObject(job.outcome).revision!==after.text))throw new Error('crossed_refresh_completion');
    }return value;
}
export function refreshComplete(receipt:ObjectValue):boolean{return receipt.job!==null&&catalogObject(receipt.job).state==='succeeded'&&receipt.configuration!==null&&receipt.result!==null;}
export interface RefreshFlowHost {
    trusted():boolean;
    persistEvidenceAllowed?():boolean;
    confirm(details:string,action:string):Promise<boolean>;
    // Existing canonical native context read/resolve; must not issue any grant.
    reconfirm(selection:ObjectValue):Promise<ObjectValue>;
    // Exactly one existing canonical evidence request, same full tool arguments.
    query(query:ObjectValue,selection:ObjectValue):Promise<ObjectValue>;
}
export class ReferenceRefreshFlow {
    private submitting=false;
    private readonly originalEvidence=new Map<string,{query:ObjectValue;result:ObjectValue}>();
    constructor(private readonly client:ServiceClient,private readonly journal:IntentJournal,private readonly host:RefreshFlowHost){}
    private requireTrust():void{if(!this.host.trusted())throw new Error('refresh_workspace_not_trusted');}
    async reviewAndSubmit(input:ReferenceRefreshIntent,capabilities:unknown):Promise<string|undefined>{
        if(this.submitting)throw new Error('refresh_review_already_in_flight');this.submitting=true;
        try{return await this.reviewAndSubmitOnce(input,capabilities);}finally{this.submitting=false;}
    }
    private async reviewAndSubmitOnce(input:ReferenceRefreshIntent,capabilities:unknown):Promise<string|undefined>{
        const intent=structuredClone(validateRefreshIntent(input));
        this.requireTrust();if(intent.reference_refresh.original_query===null||intent.reference_refresh.original_result===null)throw new Error('refresh_original_evidence_not_available');if(intent.reference_refresh.evidence_retention==='local_approved'&&!this.host.persistEvidenceAllowed?.())throw new Error('refresh_evidence_persistence_not_authorized');if(!refreshCapability(capabilities))throw new Error('native_reference_refresh_capability_required');
        const originalQuery=intent.reference_refresh.original_query,originalResult=intent.reference_refresh.original_result;
        if(originalQuery===null||originalResult===null)throw new Error('refresh_original_evidence_not_available');
        if(this.originalEvidence.size>=64)throw new Error('refresh_volatile_evidence_capacity');
        const key=refreshIntentKey(intent);
        for(const existing of this.journal.list())if('reference_refresh'in existing.value.intent){
            const previous=existing.value.intent.reference_refresh.configure_project;
            if(isDeepStrictEqual(previous.key,key))throw new Error('refresh_already_tracked_check_exact_receipt');
            const known=existing.value.nativeRefreshReceipt;let job=known?.job==null?undefined:catalogObject(known.job);
            if(known?.status==='reference_refresh_review_required')continue;
            const terminal=existing.value.nativeRefreshTerminalReceipt??existing.value.nativeRefreshLookup;
            if(terminal&&(!job||!['succeeded','failed','canceled'].includes(String(job.state)))){
                // A terminal job-only reply does not replace a richer full
                // configuration/result receipt. Validate its separate original
                // frame against this exact prior intent before using its state.
                try{
                    if(terminal.status==='job')validateRefreshJob(terminal.job,existing.value.intent);else validateRefreshReceipt(terminal,existing.value.intent);
                    const candidate=terminal.job==null?undefined:catalogObject(terminal.job);
                    if(candidate&&['succeeded','failed','canceled'].includes(String(candidate.state)))job=candidate;
                }catch{/* An unqualified current lookup never settles the prior request. */}
            }
            if(previous.project===intent.reference_refresh.configure_project.project&&(!job||!['succeeded','failed','canceled'].includes(String(job.state))))throw new Error('refresh_prior_outcome_requires_exact_receipt');
        }

        const selection=catalogObject(intent.reference_refresh.configure_project.request.configuration.selection);if(array(selection.members,8).length!==1)throw new Error('native_refresh_requires_one_member');
        const current=await this.host.reconfirm(catalogObject(selection.selection));this.requireTrust();if(!isDeepStrictEqual(current,selection.selection))throw new Error('refresh_scope_changed_review_required');
        if(!await this.host.confirm(refreshReviewText(intent),'Refresh reviewed files'))return;
        this.requireTrust();const id=await this.journal.prepare('Refresh reviewed references',intent);
        // Stop/trust/scope may change while the exact durable reservation writes.
        // Retain that row for read-only receipt reconciliation; never dispatch stale work.
        this.requireTrust();
        this.originalEvidence.set(id,{query:structuredClone(originalQuery),result:structuredClone(originalResult)});
        // Persist terminal space before dispatch. Never replay from a saved row.
        let reply:ObjectValue;
        try{reply=await this.client.submitReferenceRefresh(intent);}
        catch(error){throw this.unknownOutcome(await this.retainRejectedLookup(id,error));}
        try{await retainNativeOutcomeDuring(reply,()=>this.journal.observeRefresh(id,reply));}
        catch(error){throw this.unknownOutcome(error);}
        if(reply.status==='reference_refresh_review_required')throw new Error('refresh_scope_changed_review_required',{cause:reply});
        return id;
    }
    /** The journal independently validates this diagnostic frame. Invalid RPC
     * replies must not become historical authority, but must remain visible as
     * the latest lookup. Keep this outside the local-persistence catch so an
     * already-recorded validation failure is never written a second time. */
    private async retainRejectedLookup(id:string,error:unknown):Promise<unknown>{
        const native=(error as {nativeResponse?:ObjectValue})?.nativeResponse;
        if(native&&!isLocalTrackingFailure(error))try{await this.journal.observeRefresh(id,native);}catch(local){return retainNativeOutcome(local,native);}
        return error;
    }
    private unknownOutcome(error:unknown):ServiceError{
        const outcome=new ServiceError('refresh_outcome_unknown_check_exact_receipt',true,(error as {nativeResponse?:ObjectValue})?.nativeResponse);outcome.cause=error;
        if(isLocalTrackingFailure(error))Object.assign(outcome,{localTrackingUnconfirmed:true});
        if(isNativeReplyValidationFailure(error))Object.assign(outcome,{nativeReplyUnvalidated:true});return outcome;
    }
    async check(id:string):Promise<ObjectValue>{
        this.requireTrust();const saved=this.journal.list().find(value=>value.id===id);if(!saved||!('reference_refresh'in saved.value.intent))throw new Error('refresh_intent_not_found');
        let reply:ObjectValue;
        try{reply=await this.client.referenceRefreshReceipt(saved.value.intent);}
        catch(error){throw await this.retainRejectedLookup(id,error);}
        return retainNativeOutcomeDuring(reply,async()=>{await this.journal.observeRefresh(id,reply);this.requireTrust();return reply;});
    }
    exportObservation(id:string,requery?:ObjectValue):{json:string;markdown:string}{
        this.requireTrust();const saved=this.journal.list().find(value=>value.id===id);
        if(!saved||!('reference_refresh'in saved.value.intent))throw new Error('refresh_intent_not_found');
        const intent=structuredClone(saved.value.intent),memory=this.originalEvidence.get(id);
        if(memory){intent.reference_refresh.original_query=structuredClone(memory.query);intent.reference_refresh.original_result=structuredClone(memory.result);}
        validateRefreshIntent(intent);
        return refreshExport(intent,saved.value.nativeRefreshLookup,saved.value.nativeRefreshReceipt,requery,saved.value.nativeRefreshTerminalReceipt);
    }
    async requery(id:string):Promise<{original:ObjectValue;current:ObjectValue;native_receipt:ObjectValue}>{
        this.requireTrust();const saved=this.journal.list().find(value=>value.id===id);if(!saved||!('reference_refresh'in saved.value.intent))throw new Error('refresh_intent_not_found');
        const intent=saved.value.intent,original=this.originalEvidence.get(id)??(intent.reference_refresh.original_query!==null&&intent.reference_refresh.original_result!==null?{query:intent.reference_refresh.original_query,result:intent.reference_refresh.original_result}:undefined);
        if(!original)throw new Error('refresh_original_query_not_retained_reopen_and_review');
        const receipt=await this.check(id);if(!refreshComplete(receipt))throw new Error('fresh_completed_refresh_receipt_required');
        const selected=catalogObject(catalogObject(intent.reference_refresh.configure_project.request.configuration.selection).selection),current=await this.host.reconfirm(selected);this.requireTrust();
        // A fresh catalog revision is expected; every actual authority selector
        // stays exact. Native resolve remains responsible for current liveness.
        const before={...selected},after={...current};delete before.expected_catalog_revision;delete after.expected_catalog_revision;
        if(!isDeepStrictEqual(before,after))throw new Error('refresh_requery_scope_changed_review_required');
        if(!await this.host.confirm('Run the same saved query once against the newly confirmed current selection? Preserve the original result and verification settings.','Requery current index'))throw new Error('refresh_requery_not_confirmed');
        this.requireTrust();const response=await this.host.query(structuredClone(original.query),structuredClone(current));this.requireTrust();catalogBound(response,REFRESH_NATIVE_BYTES);
        return {original:structuredClone(original.result),current:structuredClone(response),native_receipt:structuredClone(receipt)};
    }
}
export function refreshReviewText(intent:ReferenceRefreshIntent):string {const selection=catalogObject(intent.reference_refresh.configure_project.request.configuration.selection);return `${selection.mode==='selected_only'?'Refresh only selected files.':'Full index reconciliation was explicitly selected; unrelated source files in this index may also update.'}\nThe original result stays historical. ${intent.reference_refresh.evidence_retention==='memory_only'?'Query/result text stays in memory; restart recovery retains only native metadata and cannot rerun a missing query.':'This review saves the complete original query/result locally under the approved persistence setting.'} This starts one native job; any later requery requires confirmation.\n${JSON.stringify(selection.members,null,2)}`;}
export function refreshExport(intent:ReferenceRefreshIntent,latest:ObjectValue|undefined,known:ObjectValue|undefined,requery?:ObjectValue,terminal?:ObjectValue):{json:string;markdown:string}{
    const value={evidence_retention:intent.reference_refresh.evidence_retention,reviewed_native_request:intent.reference_refresh.configure_project,original_query:intent.reference_refresh.original_query,original_historical_result:intent.reference_refresh.original_result,latest_receipt_lookup:latest??null,last_known_historical_native_receipt:known??null,...(terminal?{last_known_terminal_native_receipt:terminal}:{}),current_requery_result:requery??null};
    return {json:JSON.stringify(value,null,2),markdown:`# Refresh from reviewed references\n\nOriginal evidence, historical accepted proof, latest lookup and new query are separate observations.\n\n\`\`\`json\n${JSON.stringify(value,null,2)}\n\`\`\`\n`};
}
