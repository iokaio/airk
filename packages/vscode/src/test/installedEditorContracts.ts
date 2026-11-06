// SPDX-License-Identifier: Apache-2.0
/** External qualification driver only. Raw native replies remain authoritative. */
import {strict as assert} from 'node:assert';
import {validateCombinedCatalogSession} from '../catalogSession';
import {materializeNativeMetadata} from '../nativeMetadata';
import {readResultBody} from '../sourceVerification';
import {combinedRows} from '../explorerSearch';
export class InstalledNativeReplyError extends Error {
    readonly code?:string;
    constructor(readonly reply:unknown,body?:any){super('Native MCP refused: '+JSON.stringify(reply));const code=body?.error?.code??body?.code??(reply as any)?.error?.data?.code;this.code=typeof code==='string'?code:undefined;}
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
    assert.ok(Array.isArray(definition.args));
    assert.equal(definition.args.filter((arg:string)=>arg==='--combined-session').length,1,'Actual combined native selector');
    for(const arg of ['--catalog-session','--root','--data-dir','--collection','--service-project'])assert.ok(!definition.args.includes(arg),'No reconstructed legacy selector');
    const at=definition.args.indexOf('--combined-session');assert.ok(at>=0&&typeof definition.args[at+1]==='string');
    const selector=validateCombinedCatalogSession(JSON.parse(definition.args[at+1]));
    assert.deepEqual(selector,result.outcome.selection.session,'Complete native reviewed selection');
    assert.deepEqual(selector.index_ids,[selection.index]);assert.deepEqual(selector.project_ids,[selection.project]);assert.equal(selector.grant_id,selection.grant);
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
        if(body.publication!==undefined)assert.deepEqual(body.publication,vector.members[0].publication);
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
    evidence.native.push({transport:client.observations});let cleanup:unknown;
    try{await client.close();}catch(error){cleanup=error;evidence.transport_cleanup_failure=String(error);}
    try{await persist('retained-evidence',evidence);}catch(error){
        evidence.retention_failure=String(error);if(!cleanup)cleanup=error;
        console.error('Private native evidence retention failed:',String(error));
    }
    if(cleanup&&!evidence.failure)throw cleanup; // Preserve a primary failure and disclose failed cleanup separately.
}
