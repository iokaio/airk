// SPDX-License-Identifier: Apache-2.0
/** External qualification driver only. Raw native replies remain authoritative. */
import {strict as assert} from 'node:assert';
import {validateCombinedCatalogSession} from '../catalogSession';
import {materializeNativeMetadata} from '../nativeMetadata';
import {readResultBody} from '../sourceVerification';
import {sameHttpAuthority, validateHttpConnection} from '../serviceHttp';
import {installedHttpIdentity,InstalledHttpFailure} from './installedEditorHttp';
import {combinedRows} from '../explorerSearch';
import {reviewCombinedScopeSelection} from '../scopeSelection';
export class InstalledNativeReplyError extends Error {
    readonly code?:string;
    constructor(readonly reply:unknown,body?:any){super('Native MCP refused: '+JSON.stringify(reply));const code=body?.error?.code??body?.code??(reply as any)?.error?.data?.code;this.code=typeof code==='string'?code:undefined;}
}
/** Observe restoration only; this helper has no selection or mutation callback. */
export async function installedRestoredDefinition(read:()=>Promise<any[]>,expected:any,retain:(observations:any[])=>Promise<unknown>,deadline:number,options:{now?:()=>number;pause?:(ms:number)=>Promise<void>}={}):Promise<any>{
    const now=options.now??Date.now,pause=options.pause??(ms=>new Promise(resolve=>setTimeout(resolve,ms))),observations:any[]=[];
    const until=Math.min(deadline,now()+90000),remaining=()=>{const left=until-now();assert.ok(Number.isSafeInteger(deadline)&&left>0,'Original restored definition deadline exhausted');return left;};
    const bounded=async<T>(action:()=>Promise<T>):Promise<T>=>{
        const timeout=remaining();let timer:ReturnType<typeof setTimeout>|undefined;
        try{return await Promise.race([action(),new Promise<never>((_,reject)=>{timer=setTimeout(()=>reject(new Error('Original restored definition deadline exhausted')),timeout);})]);}
        finally{if(timer!==undefined)clearTimeout(timer);}
    };
    try{
        remaining();const original=validateCombinedCatalogSession(expected);
        for(let attempt=1;attempt<=128;attempt++){
            remaining();const definitions=await bounded(read);
            const observed:any={observed_unix_ms:now(),count:Array.isArray(definitions)?definitions.length:null};observations.push(observed);remaining();
            await retain(structuredClone(observations));remaining();assert.ok(Array.isArray(definitions),'Actual restored definitions array required');
            assert.ok(definitions.length<=1,'Restored definition crossed the exact singleton');
            if(definitions.length===1){
                const definition=definitions[0],connection=validateHttpConnection(definition.nativeConnection);
                assert.equal(connection.selection.kind,'combined');const selector=validateCombinedCatalogSession(connection.selection.selection);
                observed.selector=structuredClone(selector);await retain(structuredClone(observations));remaining();
                assert.ok(sameHttpAuthority({kind:'combined',selection:selector},{kind:'combined',selection:original}),'Restoration crossed the original immutable authority');
                assert.ok(selector.expected_catalog_revision>=original.expected_catalog_revision,'Restored catalog clock cannot move backward');
                return definition;
            }
            assert.ok(attempt<128,'Restored definition observation budget exhausted');
            assert.ok(remaining()>250,'Original restored definition deadline exhausted');await pause(250);remaining();
        }
    }catch(error){try{await retain(structuredClone(observations));}catch(retention){if(error instanceof Error)error.cause??=retention;}throw error;}
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
/** Start is an explicit user action; close the manual transport before Stop. */
export async function installedLegacyLifecycle(start:()=>Promise<unknown>,read:()=>Promise<void>,stop:()=>Promise<void>,evidence:any,mark:(name:string,value:any)=>Promise<unknown>):Promise<void>{
    await start();
    let hasPrimary=false;
    try{await read();}catch(error){hasPrimary=true;throw error;}finally{
        try{await stop();}catch(error){
            evidence.legacy_stop_failure={error:String(error).slice(0,4096),primary_preserved:hasPrimary};
            try{await mark('retained-evidence',evidence);}catch(retention){
                evidence.legacy_stop_retention_failure=String(retention).slice(0,4096);
            }
            if(!hasPrimary)throw error;
        }
    }
}
export function installedSelector(definition:any,result:any,selection:any):any{
    const identity=installedHttpIdentity(definition),connection=validateHttpConnection(definition.nativeConnection);
    assert.equal(identity.credentialId,connection.connection_id);assert.equal(identity.url,connection.endpoint.url);
    assert.equal(connection.selection.kind,'combined');
    const selector=validateCombinedCatalogSession(connection.selection.selection);
    assert.equal(result.outcome.status,'search_scope_selected');
    assert.deepEqual(result.outcome.selected,{index_ids:[],project_ids:[selection.project]},'Exact visible checkbox selection');
    assert.equal(result.outcome.activation.selected_subset,true);
    assert.ok(sameHttpAuthority({kind:'combined',selection:selector},{kind:'combined',selection:result.outcome.activation.selection.session}),'Complete native reviewed selection; only catalog clock may advance');
    assert.deepEqual(selector.project_ids,[selection.project]);assert.equal(selector.grant_id,selection.grant);
    if(selector.index_ids!==undefined)assert.deepEqual(selector.index_ids,[selection.index]);
    // A whole index-set selector intentionally omits an index restriction.
    // Prove its exact effective union using the complete native activation,
    // rather than adding a restriction to the captured immutable selector.
    const activation=result.outcome.activation,session=validateCombinedCatalogSession(activation.selection.session);
    const choice={grant_id:session.grant_id,expected_grant_revision:session.expected_grant_revision,project_ids:session.project_ids,...(session.index_ids===undefined?{}:{index_ids:session.index_ids})};
    assert.equal(activation.host_context?.status,'catalog_host_context');assert.equal(activation.grant_detail?.status,'catalog_detail');
    assert.ok(Array.isArray(activation.index_details)&&activation.index_details.length===1);
    for(const detail of activation.index_details)assert.equal(detail.status,'catalog_detail');
    const reviewed=reviewCombinedScopeSelection(choice,activation.host_context.descriptor,activation.grant_detail.detail,activation.index_details.map((detail:any)=>detail.detail));
    assert.deepEqual(reviewed.session,session);assert.deepEqual(reviewed.indexes,activation.selection.indexes);
    assert.equal(reviewed.host,activation.selection.host);assert.equal(reviewed.semanticIdentity,activation.selection.semanticIdentity);
    assert.deepEqual(reviewed.indexes.map(index=>index.id),[selection.index]);
    assert.deepEqual(reviewed.indexes[0].backing,selection.backing,'Exact original fixture backing');
    const frozen=activation.grant_detail.detail.item.value.grant.memberships;
    assert.ok(Array.isArray(frozen)&&frozen.length===1,'Exact frozen fixture membership');
    assert.equal(frozen[0].membership.project_id,selection.project);assert.equal(frozen[0].membership.index_id,selection.index);
    assert.deepEqual(frozen[0].membership.scope.map((atom:any)=>({source_root_id:atom.source_root_id,kind:atom.kind,path:atom.path})).sort((a:any,b:any)=>a.source_root_id.localeCompare(b.source_root_id)),
        selection.source_ids.map((source_root_id:string)=>({source_root_id,kind:'subtree',path:''})).sort((a:any,b:any)=>a.source_root_id.localeCompare(b.source_root_id)),'Original complete source-root scope');
    return selector;
}
export function installedToolBody(wire:any):any{
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
    assert.ok(Buffer.byteLength(JSON.stringify(body))<=65536,'Native canonical body bound');return body;
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
export function installedMember(raw:any,selection:any,denied=false):any{
    assert.equal(raw?.schema,'airk.combined-read/1');
    // This is the shipped lossless logical projection, not independent fullreply
    // cryptographic authentication. Untouched raw delivery stays in transport evidence.
    const value=materializeNativeMetadata(raw),scope:any=value.selection;
    assert.deepEqual(scope.project_ids,[selection.project]);assert.deepEqual(scope.index_ids,[selection.index]);
    assert.equal(scope.grant_id,selection.grant);assert.equal(scope.grant_revision,selection.session.expected_grant_revision);
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
    assert.equal(observed?.qualifier,'selected_subset');assert.equal(observed.provenance,'user_reviewed_project_grant');
    assert.equal(observed.grant.id,selection.grant);assert.equal(observed.grant.revision,selection.session.expected_grant_revision);assert.match(observed.fingerprint,/^[a-f0-9]{64}$/);
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
/** Storage preferences may advance only the observed catalog clock. */
export function installedDefinitionContinuity(beforeSafe:any,afterSafe:any):void{
    const before=validateHttpConnection(beforeSafe.nativeConnection);
    assert.equal(before.selection.kind,'combined');
    const after=validateHttpConnection(afterSafe.nativeConnection,before.selection);
    assert.equal(after.selection.kind,'combined');
    const previousSelector=validateCombinedCatalogSession(before.selection.selection),currentSelector=validateCombinedCatalogSession(after.selection.selection);
    assert.ok(currentSelector.expected_catalog_revision>=previousSelector.expected_catalog_revision,'Definition catalog clock cannot move backward');
    const previous=structuredClone(beforeSafe),current=structuredClone(afterSafe);
    delete previous.nativeConnection.selection.selection.expected_catalog_revision;
    delete current.nativeConnection.selection.selection.expected_catalog_revision;
    assert.deepEqual(current,previous,'Storage preference preserves the complete definition except its forward catalog clock');
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
