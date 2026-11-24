// SPDX-License-Identifier: Apache-2.0
/** External qualification driver only. Raw native replies remain authoritative. */
import {strict as assert} from 'node:assert';
import {materializeNativeMetadata} from '../nativeMetadata';
import {readResultBody} from '../sourceVerification';
import {InstalledHttpFailure} from './installedEditorHttp';
import {combinedRows} from '../explorerSearch';
export class InstalledNativeReplyError extends Error {
    readonly code?:string;
    constructor(readonly reply:unknown,body?:any){super('Native MCP refused: '+JSON.stringify(reply));const code=body?.error?.code??body?.code??(reply as any)?.error?.data?.code;this.code=typeof code==='string'?code:undefined;}
}
/** Observe the accepted policy job; never replay Apply or grant preparation. */
export async function installedPolicyReady(index:string,operation:any,read:()=>Promise<{policyStatus:any,indexDetail:any}>,retain:(observations:any[])=>Promise<unknown>,deadline:number):Promise<void>{
    const observations:any[]=[];let pinnedJob:string|undefined,pinnedDigest:string|undefined;
    const remaining=()=>{const value=deadline-Date.now();assert.ok(Number.isSafeInteger(deadline)&&value>0,'Original policy readiness deadline exhausted');return value;};
    const bounded=async<T>(action:()=>Promise<T>):Promise<T>=>{
        const timeout=remaining();let timer:ReturnType<typeof setTimeout>|undefined;
        try{return await Promise.race([action(),new Promise<never>((_,reject)=>{timer=setTimeout(()=>reject(new Error('Original policy readiness deadline exhausted')),timeout);})]);}
        finally{if(timer!==undefined)clearTimeout(timer);}
    };
    try{
        remaining();assert.match(index,/^idx_[a-f0-9]{32}$/);
        assert.equal(operation?.operation,'change_index_policy');assert.equal(operation.id,index);
        assert.ok(Number.isSafeInteger(operation.expected_source_revision)&&operation.expected_source_revision>0);
        const configuration=operation.configuration?.configuration;
        assert.equal(configuration?.operation,'apply_source_policy');assert.match(configuration.operation_id,/^[a-f0-9]{32}$/);
        const desired=structuredClone(configuration.policy),operationId=configuration.operation_id,sourceRevision=operation.expected_source_revision;
        assert.equal(desired.index_id,index);assert.match(desired.owner_binding,/^[a-f0-9]{64}$/);
        for(let attempt=1;attempt<=128;attempt++){
            const observation=await bounded(read);
            assert.ok(Buffer.byteLength(JSON.stringify(observation))<=131072,'Complete policy observation pair bound');
            observations.push(structuredClone(observation));await bounded(()=>retain(structuredClone(observations)));remaining();
            const status=observation.policyStatus,snapshot=status?.snapshot,job=status?.job;
            assert.equal(status?.status,'catalog_policy_status');assert.equal(status.index_id,index);
            const item=observation.indexDetail?.detail?.item;
            assert.equal(observation.indexDetail?.status,'catalog_detail');assert.equal(item?.kind,'index');
            const current=item.value;
            assert.equal(current.id,index);assert.equal(current.backing?.binding,desired.owner_binding);
            assert.equal(current.source_revision,sourceRevision);assert.equal(current.tombstone,false);assert.equal(current.source_import,'complete');
            assert.equal(current.policy?.epoch,desired.epoch);assert.match(current.policy?.digest,/^[a-f0-9]{64}$/);
            pinnedDigest??=current.policy.digest;assert.equal(current.policy.digest,pinnedDigest,'Accepted index policy digest changed');
            assert.ok(snapshot,'Accepted policy snapshot required');assert.deepEqual(snapshot.policy,desired);
            assert.equal(snapshot.operation_id,operationId);assert.match(snapshot.job,/^[a-f0-9]{64}$/);
            pinnedJob??=snapshot.job;assert.equal(snapshot.job,pinnedJob,'Accepted policy job changed');
            assert.equal(job?.id,pinnedJob);assert.equal(job.project,desired.owner_binding);assert.equal(job.kind,'configure');
            assert.ok(['pending','running','succeeded','failed','canceled'].includes(job.state));
            assert.ok(!['failed','canceled'].includes(job.state),'Accepted policy job failed or canceled');
            const report=snapshot.application;
            if(report!==null){
                assert.ok(report&&typeof report==='object');assert.equal(snapshot.application_job,pinnedJob);
                assert.equal(report.operation_id,operationId);assert.equal(report.policy_digest,current.policy.digest);
                if(job.state==='succeeded')return;
            }else assert.equal(snapshot.application_job,null);
            assert.ok(attempt<128,'Policy readiness observation budget exhausted');
            await bounded(()=>new Promise<void>(resolve=>setTimeout(resolve,250)));
        }
    }catch(error){
        try{await retain(structuredClone(observations));}catch(retention){if(error instanceof Error)error.cause??=retention;}
        throw error;
    }
}
/** Only a refused pure review may be prepared anew after observing native idle.
 * This helper never submits Apply or recovers an uncertain accepted operation. */
export async function installedSourceReview(index:string,review:()=>Promise<any>,wait:()=>Promise<void>,retain:(attempts:any[])=>Promise<unknown>,deadline:number):Promise<any>{
    const attempts:any[]=[];
    const remaining=()=>{const value=deadline-Date.now();assert.ok(Number.isSafeInteger(deadline)&&value>0,'Original source review deadline exhausted');return value;};
    const bounded=async<T>(operation:()=>Promise<T>):Promise<T>=>{
        const timeout=remaining();let timer:ReturnType<typeof setTimeout>|undefined;
        try{return await Promise.race([operation(),new Promise<never>((_,reject)=>{timer=setTimeout(()=>reject(new Error('Original source review deadline exhausted')),timeout);})]);}
        finally{if(timer!==undefined)clearTimeout(timer);}
    };
    try{
        assert.match(index,/^idx_[a-f0-9]{32}$/);remaining();
        for(let attempt=1;attempt<=3;attempt++){
            let value:any;
            try{value=await bounded(review);}catch(error){
                attempts.push({attempt,error:error instanceof Error?error.message:'source_review_failed',
                    ...(error&&typeof error==='object'&&'nativeResponse' in error?{nativeResponse:(error as any).nativeResponse}:{})});
                throw error;
            }
            assert.ok(value&&typeof value==='object'&&!Array.isArray(value),'Settled actual source review view required');
            assert.ok(Buffer.byteLength(JSON.stringify(value))<=1048576,'Complete source review view bound');
            attempts.push({attempt,view:structuredClone(value)});await retain(structuredClone(attempts));remaining();
            assert.notEqual(value.busy,true,'Pure source review must have settled');
            if(!value.error){
                assert.ok(value.review&&typeof value.review==='object','Actual native source review required');
                assert.equal(value.review.request?.operation?.operation,'change_index_policy');
                assert.equal(value.review.request.operation.id,index,'Source review crossed the exact index');
                return value;
            }
            assert.ok(value.review===undefined||value.review===null,'A refusal cannot coexist with a prepared review');
            const refusal=value.outcome,busy=refusal?.busy;
            assert.ok(refusal&&typeof refusal==='object'&&!Array.isArray(refusal),'Complete native source refusal required');
            assert.equal(refusal.status,'source_change_busy');assert.equal(refusal.code,'source_change_jobs_pending');
            assert.equal(busy?.index_id,index,'Source busy refusal crossed the exact index');
            for(const field of ['pending_jobs','running_jobs'])assert.ok(Number.isSafeInteger(busy[field])&&busy[field]>=0&&busy[field]<=4294967295,'Native source busy counts must be uint32');
            assert.ok(busy.pending_jobs+busy.running_jobs>0,'Native source busy must identify pending work');
            assert.ok(attempt<3,'Pure source review request budget exhausted');
            await bounded(wait); // Actual native idle observation, never a sleep-based admission claim.
        }
        throw new Error('Pure source review request budget exhausted');
    }catch(error){
        try{await retain(structuredClone(attempts));}catch(retention){
            if(error instanceof Error)error.cause??=retention;
        }
        throw error;
    }
}
export function installedToolBody(wire:any,owner?:{tool:'airk_parse';request:any}):any{
    let bodyBytes=65536;
    if(owner!==undefined){
        assert.equal(owner.tool,'airk_parse','Only an explicit original parse request selects its plan budget');
        const request=owner.request;assert.equal(request?.schema,'airk.parse.request/1');assert.equal(request.mode,'execute');
        const output=request.output??request.plan?.output;
        if(output!==undefined)assert.ok(output&&typeof output==='object'&&!Array.isArray(output),'Original plan output object');
        bodyBytes=output!==undefined&&Object.hasOwn(output,'max_bytes')?output.max_bytes:262144;
        assert.ok(Number.isSafeInteger(bodyBytes)&&bodyBytes>=1024&&bodyBytes<=524288,'Original declared plan output range');
    }
    assert.ok(wire&&typeof wire==='object');assert.equal(wire.error,undefined,'No RPC error in successful tool envelope');
    const result=wire.result;assert.ok(result&&typeof result==='object'&&!Array.isArray(result));
    assert.ok(result.isError===undefined||typeof result.isError==='boolean','Actual typed outer error flag');
    assert.ok(Array.isArray(result.content)&&result.content.length===1&&result.content[0].type==='text'&&typeof result.content[0].text==='string');
    let body:any;
    try{body=JSON.parse(result.content[0].text);}catch(error){
        // Classic remote bridges can return a genuine plaintext error. Preserve
        // that complete refusal without inventing a typed compatibility code.
        if(result.isError===true)throw new InstalledNativeReplyError(wire);
        throw error; // Successful tool bodies must still satisfy the JSON contract.
    }
    if(result.structuredContent!==undefined)assert.deepEqual(result.structuredContent,body,'Complete text and structured native copies agree');
    assert.ok(Buffer.byteLength(JSON.stringify(body))<=bodyBytes,'Native canonical body bound');
    assert.ok(Buffer.byteLength(JSON.stringify(result))<=Math.min(3*bodyBytes+4096,1048576-2048),'Native MCP wire bound');
    assert.ok(Buffer.byteLength(JSON.stringify(wire))<=1048576,'Native public wire bound');
    if(owner!==undefined&&result.isError!==true)assert.equal(body.plan?.output?.max_bytes,bodyBytes,'Returned plan preserves the original output budget');
    return body;
}
function installedPublication(actual:any,expected:any,collection:string):void{
    assert.ok(expected&&typeof expected==='object'&&!Array.isArray(expected));
    assert.deepEqual(Object.keys(expected).sort(),['collection','generation','incarnation','revision','token']);
    assert.ok(typeof expected.collection==='string'&&Buffer.byteLength(expected.collection)>0&&Buffer.byteLength(expected.collection)<=128);
    assert.equal(expected.collection,collection,'Original selected collection publication');
    assert.match(expected.incarnation,/^[a-f0-9]{32}$/);assert.match(expected.revision,/^[a-f0-9]{64}$/);
    assert.ok(Number.isSafeInteger(expected.generation)&&expected.generation>=0);
    assert.equal(expected.token,`airk-r1:${expected.incarnation}:${expected.generation}:${expected.revision}`,'Complete vector publication identity');
    assert.ok(actual&&typeof actual==='object'&&!Array.isArray(actual));
    if(Object.keys(actual).length===1&&Object.hasOwn(actual,'token')){
        // Canonical status_summary intentionally exposes only this exact token,
        // even in full response mode. It encodes incarnation/generation/revision.
        assert.equal(actual.token,expected.token,'Exact native summary publication');
    }else assert.deepEqual(actual,expected,'Complete native publication envelope');
}
/** A report-mode combined read returns a typed, repeatable `publication_busy`
 * member while that index publishes. A publication wait may poll through exactly
 * this reply; every other failed member remains a result for `installedMember`. */
export function installedPublicationBusy(raw:any):boolean{
    if(raw?.schema!=='airk.combined-read/1'||raw.partial!==true||!Array.isArray(raw.members)||raw.members.length!==1||!Array.isArray(raw.ranking)||raw.ranking.length!==0)return false;
    const member=raw.members[0],result=member?.result;
    if(member?.outcome!=='failed'||member.is_error!==true||!result||typeof result!=='object')return false;
    if(result.status==='retry'&&result.code==='publication_busy'&&result.retryable===true
        &&result.action==='repeat_same_request'&&Number.isSafeInteger(result.retry_after_ms)&&result.retry_after_ms>=0)return true;
    // A read admitted against the projection that this publication then replaced is
    // refused as expired and is not replayable. A publication wait makes a new,
    // later observation; it never repeats that submitted query as a retry.
    const error=result.error;
    return !!error&&typeof error==='object'&&error.code==='source_projection_expired'&&error.retryable===false&&error.query_submitted===true&&error.query_replay_safe===false;
}
export function installedMember(raw:any,selection:any,denied=false):any{
    assert.equal(raw?.schema,'airk.combined-read/1');
    // This is the shipped lossless logical projection, not independent fullreply
    // cryptographic authentication. Untouched raw delivery stays in transport evidence.
    const value=materializeNativeMetadata(raw),scope:any=value.selection;
    assert.deepEqual(scope.project_ids,[selection.project]);assert.deepEqual(scope.index_ids,[selection.index]);
    assert.deepEqual(scope.selection,selection.source,'Exact selection authority source');
    assert.equal(scope.member_availability,'report');assert.match(scope.identity,/^[a-f0-9]{64}$/);
    assert.ok(Array.isArray(value.members)&&value.members.length===1);
    const member:any=value.members[0];assert.equal(member.index_id,selection.index);assert.equal(member.allocation?.index_id,selection.index);
    assert.ok(member.allocation.max_bytes>0&&member.allocation.deadline_ms>0);
    assert.equal(member.outcome,denied?'failed':'complete');assert.equal(member.is_error,denied);
    if(Object.hasOwn(member.result,'isError'))assert.equal(member.result.isError,denied,'Complete child error envelope agrees');
    assert.equal(value.partial,denied);assert.equal(typeof value.truncated,'boolean');
    const vector:any=value.publication_vector;assert.ok(Array.isArray(vector.members)&&vector.members.length===1);
    assert.equal(vector.members[0].index_id,selection.index);assert.match(vector.authority,/^[a-f0-9]{64}$/);
    assert.ok(vector.observed_to_unix_ms>=vector.observed_from_unix_ms);
    const delivery=raw.delivery_observation?.receipt;
    assert.equal(delivery?.schema,1);assert.equal(delivery.namespace,selection.session.namespace);
    assert.equal(delivery.scope_identity,vector.authority,'Native delivery/vector domain');
    assert.ok(delivery.catalog_revision>=selection.session.expected_catalog_revision,'Delivery may advance catalog revision');
    const body:any=readResultBody(member.result),observed=body.scope_observation;
    assert.equal(observed?.qualifier,'selected_subset');assert.deepEqual(observed.selection,scope.selection,'Observation agrees with the selection source');assert.match(observed.fingerprint,/^[a-f0-9]{64}$/);
    assert.equal(observed.members.length,1);assert.equal(observed.members[0].index_id,selection.index);
    assert.deepEqual(observed.members[0].memberships.map((row:any)=>row.project_id),[selection.project]);
    if(denied){
        assert.deepEqual(body.error,{schema:1,code:'scoped_read_unavailable',retryable:false});
        assert.equal(vector.members[0].publication,null,'Denied path does not invent an artifact publication');
        assert.ok(Array.isArray(value.ranking));assert.equal(value.ranking.length,0);
        for(const field of ['items','hits','content','evidence'])assert.equal(body[field],undefined,'Denied source carries no evidence');
    }else{
        const rows=combinedRows(value);assert.ok(rows);
        for(const {origin}of rows){
            assert.equal(origin.index_id,selection.index);assert.deepEqual(origin.project_ids,[selection.project]);
            assert.ok(Array.isArray(selection.source_ids)&&selection.source_ids.includes(origin.source_root_id),'Native configured originating source');
            assert.deepEqual(origin.publication,vector.members[0].publication,'Origin retains exact native member publication');
        }
        if(body.publication!==undefined){
            const publication=vector.members[0].publication;
            installedPublication(body.publication,publication,selection.backing.collection);
            for(const field of ['collection','generation'])if(Object.hasOwn(body,field))assert.equal(body[field],publication[field],'Native publication scalar '+field);
        }
    }
    return body; // Derived assertion view only; never replaces canonical export/evidence.
}
export function installedRevocation(body:any,selector:any):void{
    assert.equal(body?.error?.code,'combined_scope_unavailable');assert.equal(body.error.query_submitted,false);
    assert.equal(body.error.retryable,false);assert.equal(body.error.query_replay_safe,false);
    assert.equal(body.requested_scope.confirmed,false);assert.deepEqual(body.requested_scope.selection,selector);
    for(const field of ['items','hits','evidence','result','members'])assert.equal(body[field],undefined);
}
export async function finishInstalledClient(client:{observations:unknown[];close():Promise<unknown>},evidence:any,persist:(name:string,value:unknown)=>Promise<unknown>):Promise<void>{
    const cohort:{transport:unknown[];cleanup?:unknown;cleanup_failure?:string}={transport:client.observations};evidence.native.push(cohort);let cleanup:unknown;
    try{cohort.cleanup=await client.close();}catch(error){cleanup=error;cohort.cleanup_failure=String(error);evidence.transport_cleanup_failure=String(error);}
    try{await persist('retained-evidence',evidence);}catch(error){
        evidence.retention_failure=String(error);if(!cleanup)cleanup=error;
        console.error('Private native evidence retention failed:',String(error));
    }
    if(cleanup&&!evidence.failure)throw cleanup; // Preserve a primary failure and disclose failed cleanup separately.
}

/** A transport reset proves fencing only with the exact completed native deletion. */
export function installedDeliveryFence(failure:unknown,review:any,applied:any,selector:any):void{
    assert.equal(review.request.operation.operation,'delete_project');assert.deepEqual(selector.project_ids,[review.request.operation.id]);
    const receipt=applied.verifiedReceipt;
    assert.equal(receipt.authority_fence_required,true);assert.ok(applied.verifiedFencedReceipt?.proof);
    assert.equal(receipt.caller,review.request.caller);assert.deepEqual(receipt.key,review.request.key);
    assert.equal(receipt.request_digest,review.requestDigest);assert.ok(receipt.catalog_revision>review.request.expected_catalog_revision);
    assert.ok(failure instanceof InstalledHttpFailure);
    if(failure.kind==='network'){
        assert.ok(['ECONNRESET','EPIPE','ERR_STREAM_PREMATURE_CLOSE'].includes(failure.detail.code??''));
        assert.equal(failure.detail.response_bytes,0,'No partial evidence delivered after revocation');
    }else{
        assert.equal(failure.kind,'http');assert.ok([403,503].includes(failure.detail.status??0));
        const body=failure.detail.body as any;assert.equal(body?.schema,'airk.local-api-error/1');
        assert.equal(body.error?.code,failure.detail.status===403?'native_grant_refused':'native_delivery_fenced');
        for(const field of ['items','hits','evidence','result','members'])assert.equal(body[field],undefined);
    }
}
