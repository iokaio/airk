// SPDX-License-Identifier: Apache-2.0
/** Pure native read-retry validation shared by every host consumer.
 * Importing this module opens no transport and performs no startup work. */
import { isDeepStrictEqual } from 'node:util';
import { validateCatalogSession } from './catalogSession';

/**
 * Accept only the native "safe to repeat this exact read" replies (publication or
 * selection busy, or an unsubmitted busy admission). Anything else is a real result.
 */
/** Exact counterpart of Core combined_reads::read_retry::validated_retry.
 * The declared pointer retains the original native receipt. A failed sibling in
 * a partial union never grants permission to replay its successful siblings. */
export function singleRequiredMemberRetry(combined: Record<string, any>): Record<string, unknown> | undefined {
    const object=(value:unknown):value is Record<string,any>=>!!value&&typeof value==='object'&&!Array.isArray(value);
    const integer=(value:unknown,min:number,max=Number.MAX_SAFE_INTEGER):value is number=>Number.isSafeInteger(value)&&Number(value)>=min&&Number(value)<=max;
    const fields=(value:Record<string,unknown>,keys:string[])=>Object.keys(value).every(key=>keys.includes(key));
    const id=(value:unknown,prefix:string)=>typeof value==='string'&&new RegExp(`^${prefix}_[0-9a-f]{32}$`).test(value);
    const digest=(value:unknown)=>typeof value==='string'&&/^[0-9a-f]{64}$/.test(value);
    try {
        if(Buffer.byteLength(JSON.stringify(combined),'utf8')>65536||combined.schema!=='airk.combined-read/1'
            ||combined.partial!==true||combined.truncated!==false||!Array.isArray(combined.ranking)||combined.ranking.length!==0
            ||combined.shared_values!==undefined&&combined.shared_values!==null)return;
        const selected=combined.selection;
        if(!object(selected)||!fields(selected,['project_ids','index_ids','grant_id','grant_revision','member_availability','identity'])
            ||selected.member_availability!=='require'||!Array.isArray(selected.index_ids)||selected.index_ids.length!==1||!id(selected.index_ids[0],'idx')
            ||!Array.isArray(selected.project_ids)||selected.project_ids.length<1||selected.project_ids.length>32
            ||!selected.project_ids.every((value:unknown)=>id(value,'prj'))||new Set(selected.project_ids).size!==selected.project_ids.length
            ||!id(selected.grant_id,'grt')||!integer(selected.grant_revision,1)||!digest(selected.identity))return;
        const index=selected.index_ids[0];
        if(!Array.isArray(combined.members)||combined.members.length!==1)return;
        const member=combined.members[0];
        if(!object(member)||member.index_id!==index||member.outcome!=='failed'||member.is_error!==true
            ||member.cutoff!==undefined&&member.cutoff!==null)return;
        const allocation=member.allocation;
        if(!object(allocation)||!fields(allocation,['index_id','limit','max_bytes','deadline_ms','verification_max_sources','verification_max_bytes','verification_deadline_ms'])
            ||allocation.index_id!==index||!integer(allocation.limit,1,100)||!integer(allocation.max_bytes,1,65536)||!integer(allocation.deadline_ms,1,30000)
            ||!integer(allocation.verification_max_sources,0,256)||!integer(allocation.verification_max_bytes,0,1073741824)
            ||!integer(allocation.verification_deadline_ms,0,30000))return;
        const error=combined.error;
        if(!object(error)||error.code!=='member_unavailable'||error.retryable!==false
            ||!Array.isArray(error.failed_members)||error.failed_members.length!==1||error.failed_members[0]!==index)return;
        const vector=combined.publication_vector;
        if(!object(vector)||!fields(vector,['authority','observed_from_unix_ms','observed_to_unix_ms','members'])||!digest(vector.authority)
            ||!integer(vector.observed_from_unix_ms,0)||!integer(vector.observed_to_unix_ms,vector.observed_from_unix_ms)
            ||!Array.isArray(vector.members)||vector.members.length!==1)return;
        const publication=vector.members[0];
        if(!object(publication)||!fields(publication,['index_id','publication'])||publication.index_id!==index
            ||publication.publication!==undefined&&publication.publication!==null)return;
        const result=member.result;
        if(!object(result)||Object.hasOwn(result,'isError')&&result.isError!==true)return;
        let body=result,suffix='';
        if(Object.hasOwn(result,'structuredContent')){
            if(result.isError!==true||!Array.isArray(result.content)||result.content.length!==1
                ||result.content[0]?.type!=='text'||typeof result.content[0]?.text!=='string')return;
            if(!isDeepStrictEqual(JSON.parse(result.content[0].text),result.structuredContent))return;
            body=result.structuredContent;suffix='/structuredContent';
        }else if(Object.hasOwn(result,'content'))return;
        if(!object(body)||Object.hasOwn(body,'query_replay_safe')&&body.query_replay_safe!==true
            ||Object.hasOwn(body,'query_submitted')&&body.query_submitted!==false)return;
        if(Object.hasOwn(body,'request_id')&&(typeof body.request_id!=='string'||body.request_id.length===0||Buffer.byteLength(body.request_id,'utf8')>128))return;
        if(Object.hasOwn(body,'scope_observation')){
            const scope=body.scope_observation;
            if(!object(scope)||!fields(scope,['schema','qualifier','fingerprint','provenance','grant','members'])||scope.schema!==1
                ||scope.qualifier!=='selected_subset'||!digest(scope.fingerprint)||!['user_reviewed_project_grant','imported_legacy_snapshot','imported_standalone_adoption'].includes(scope.provenance)
                ||!object(scope.grant)||!fields(scope.grant,['id','revision'])||scope.grant.id!==selected.grant_id||scope.grant.revision!==selected.grant_revision
                ||!Array.isArray(scope.members)||scope.members.length!==1||Buffer.byteLength(JSON.stringify(scope),'utf8')>16384)return;
            const scoped=scope.members[0];
            if(!object(scoped)||!fields(scoped,['index_id','memberships','source_root_ids','source_revision','policy_epoch','policy_digest'])
                ||scoped.index_id!==index||!integer(scoped.source_revision,1)||!integer(scoped.policy_epoch,0)
                ||(scoped.policy_epoch===0?scoped.policy_digest!==undefined&&scoped.policy_digest!==null:!digest(scoped.policy_digest))
                ||!Array.isArray(scoped.memberships)||scoped.memberships.length<1||scoped.memberships.length>32
                ||!scoped.memberships.every((item:unknown)=>object(item)&&fields(item,['project_id','membership_revision'])
                    &&id(item.project_id,'prj')&&selected.project_ids.includes(item.project_id)&&integer(item.membership_revision,1))
                ||new Set(scoped.memberships.map((item:any)=>item.project_id)).size!==scoped.memberships.length
                ||!Array.isArray(scoped.source_root_ids)||scoped.source_root_ids.length>17
                ||!scoped.source_root_ids.every((source:unknown)=>id(source,'src'))||new Set(scoped.source_root_ids).size!==scoped.source_root_ids.length)return;
        }
        let code:string,delay:unknown,fresh:boolean;
        if(body.status==='retry'){
            if(!fields(body,['status','code','retryable','action','retry_after_ms','request_id','scope_observation','query_replay_safe','query_submitted','fresh_brokerage_required'])
                ||!['publication_busy','selection_busy'].includes(body.code)||body.retryable!==true||body.action!=='repeat_same_request'
                ||Object.hasOwn(body,'fresh_brokerage_required')&&body.fresh_brokerage_required!==false)return;
            code=body.code;delay=body.retry_after_ms;fresh=false;
        }else{
            if(!fields(body,['error','request_id','scope_observation','query_replay_safe','query_submitted','fresh_brokerage_required'])||!object(body.error))return;
            const refusal=body.error;
            if(!fields(refusal,['code','reason','query_submitted','retryable','retry_after_ms','fresh_brokerage_required','query_replay_safe'])
                ||!['direct_query_admission','direct_query_setup'].includes(refusal.code))return;
            code=refusal.code;fresh=code==='direct_query_setup';
            if(refusal.reason!=='busy'||refusal.query_submitted!==false||refusal.retryable!==true||refusal.fresh_brokerage_required!==fresh
                ||Object.hasOwn(refusal,'query_replay_safe')&&refusal.query_replay_safe!==true
                ||Object.hasOwn(body,'fresh_brokerage_required')&&body.fresh_brokerage_required!==fresh)return;
            delay=refusal.retry_after_ms;
        }
        if(!integer(delay,1,1000))return;
        const proposed={schema:1,scope:'same_single_required_member',index_id:index,selection_identity:selected.identity,
            action:'repeat_same_request',query_replay_safe:true,result_pointer:`/members/0/result${suffix}`,code,retry_after_ms:delay,fresh_brokerage_required:fresh};
        if(isDeepStrictEqual(combined.read_retry,proposed))return proposed;
    }catch{/* Unparseable or unbounded metadata never permits another read. */}
    return;
}

export function readRetryDelay(envelope: unknown, expectedScope?: unknown): number | undefined {
    if (!envelope || typeof envelope !== 'object') { return; }
    const value = envelope as Record<string, unknown>, content = value.content;
    if (value.isError !== true || !Array.isArray(content) || content.length !== 1 || content[0]?.type !== 'text' || typeof content[0]?.text !== 'string') { return; }
    let hint: Record<string, any>;
    try { hint = JSON.parse(content[0].text); } catch { return; }
    if (!hint || typeof hint !== 'object') { return; }
    if(value.structuredContent!==undefined&&!isDeepStrictEqual(value.structuredContent,hint))return;
    let delay: unknown;
    if (hint.schema === 'airk.combined-read/1') {
        return singleRequiredMemberRetry(hint)?.retry_after_ms as number|undefined;
    }
    if (hint.error?.code === 'scoped_status_prepare_timeout' || hint.error?.code === 'scoped_connection_unavailable') {
        const error=hint.error,scope=hint.requested_scope;
        const fields=(value:unknown,keys:string[]):value is Record<string,any>=>!!value&&typeof value==='object'&&!Array.isArray(value)
            &&Object.keys(value).every(key=>keys.includes(key));
        if(!fields(hint,['error','requested_scope'])||!fields(error,['code','query_submitted','retryable','retry_after_ms','fresh_brokerage_required','native_control_response','query_replay_safe'])
            ||error.query_submitted!==false||error.retryable!==true||error.fresh_brokerage_required!==true
            ||Object.hasOwn(error,'query_replay_safe')&&error.query_replay_safe!==true
            ||!fields(scope,['confirmed','selection'])||scope.confirmed!==false||expectedScope===undefined
            ||!isDeepStrictEqual(value.structuredContent,hint))return;
        try {
            const observed=validateCatalogSession(scope.selection),expected=validateCatalogSession(expectedScope);
            // The native resolver refreshes only the metadata clock before
            // admission. The original grant, scope and launch arguments remain
            // fixed; a backwards clock or changed authority never permits replay.
            if(observed.expected_catalog_revision<expected.expected_catalog_revision
                ||!isDeepStrictEqual({...observed,expected_catalog_revision:expected.expected_catalog_revision},expected))return;
        }catch{return;}
        if(error.code==='scoped_connection_unavailable'){
            const native=error.native_control_response;
            if(!fields(native,['status','code'])||native.status!=='failure'||!['broker_busy','project_starting_or_unavailable',
                'project_unavailable_or_busy','previous_workers_draining','catalog_revision_changed','catalog_clock_pending',
                'catalog_connection_busy','project_capacity_busy','project_restart_backoff'].includes(native.code))return;
        }else if(error.native_control_response!==undefined)return;
        return Number.isInteger(error.retry_after_ms)&&error.retry_after_ms>=1&&error.retry_after_ms<=1000?error.retry_after_ms:undefined;
    }
    if (hint.status === 'retry') {
        if (hint.retryable !== true || hint.action !== 'repeat_same_request' || !['publication_busy', 'selection_busy'].includes(hint.code)) { return; }
        delay = hint.retry_after_ms;
    } else {
        const error = hint.error;
        if (!error || !['direct_query_admission', 'direct_query_setup'].includes(error.code) || error.reason !== 'busy'
            || error.query_submitted !== false || error.retryable !== true || error.fresh_brokerage_required !== (error.code === 'direct_query_setup')) { return; }
        delay = error.retry_after_ms;
    }
    return Number.isInteger(delay) && Number(delay) >= 1 && Number(delay) <= 1000 ? Number(delay) : undefined;
}
