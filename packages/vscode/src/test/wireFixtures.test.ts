// SPDX-License-Identifier: Apache-2.0
// Shared wire fixtures (docs/simplify-plan.md S1.2/S1.3). The same reviewed
// decision cases are asserted against the native Rust rule in airk-core, so the
// hand-written TypeScript mirror is checked against one independent set of
// expectations instead of its own copy. `expected_ts` marks a recorded
// divergence; every other case must match the native decision exactly.
import {test} from 'node:test';
import {strict as assert} from 'node:assert';
import * as fs from 'node:fs';
import * as path from 'node:path';
import {readRetryDelay,singleRequiredMemberRetry} from '../readRetry';
import {ServiceClient} from '../serviceClient';
import type {CatalogRequest} from '../indexingCatalog';
import {ServiceError,isLocalTrackingFailure,isNativeReplyValidationFailure,retainNativeOutcome,retainUnvalidatedNativeReply,validateNativeReply} from '../nativeOutcome';

const fixtures=path.resolve(__dirname,'../../../../fixtures/wire');
type DecisionCase={name:string;rule:string;input:Record<string,unknown>;expected:unknown;expected_ts?:unknown;divergence?:string};
const decisions=JSON.parse(fs.readFileSync(path.join(fixtures,'read_retry_decisions.json'),'utf8')) as {schema:string;cases:DecisionCase[]};

test('read-retry decision fixture is the reviewed schema and is non-empty',()=>{
    assert.equal(decisions.schema,'airk.wire-fixture.read-retry-decisions/1');
    assert.ok(decisions.cases.length>0,'zero selected decision cases is a failure');
    for(const item of decisions.cases)if('expected_ts' in item)assert.ok(item.divergence,`${item.name} records why TypeScript differs`);
});

for(const item of decisions.cases){
    test(`read-retry decision: ${item.name}`,()=>{
        const expected='expected_ts' in item?item.expected_ts:item.expected;
        assert.deepEqual(singleRequiredMemberRetry(structuredClone(item.input))??null,expected,item.rule);
    });
}

test('readRetryDelay refuses a non-integer number literal in the native envelope text',()=>{
    const envelope=(delay:string)=>({isError:true,content:[{type:'text',
        text:`{"status":"retry","code":"selection_busy","retryable":true,"action":"repeat_same_request","retry_after_ms":${delay}}`}]});
    assert.equal(readRetryDelay(envelope('25')),25);
    for(const delay of ['25.0','2.5e1','25e0'])assert.equal(readRetryDelay(envelope(delay)),undefined,delay);
});

// Catalog receipt control read. Native bytes come from the reviewed Rust golden
// (crates/airk-core/src/wire_fixture_tests.rs). The extension speaks the console
// framing `{id, method}`, so only the `method` object is shared with
// `ControlRequest`. Recorded divergences are asserted as they are today.
type ReceiptCase={name:string;inputs:{caller:string;key:{namespace:string;epoch:number;nonce:string};request_digest:string;catalog_revision:number};
    operation_id:string;persisted:string;control_request:string;response:string};
const receipts=JSON.parse(fs.readFileSync(path.join(fixtures,'catalog_receipt.json'),'utf8')) as {schema:string;receipts:ReceiptCase[];response_absent:string};
const receiptCase=(name:string)=>{const found=receipts.receipts.find(item=>item.name===name);assert.ok(found,name);return found;};
const client=()=>new ServiceClient({dispose(){},async request(){throw new Error('fixture validation has no transport effects');}});
const catalogRequest=(item:ReceiptCase)=>({caller:item.inputs.caller,key:item.inputs.key,expected_catalog_revision:item.inputs.catalog_revision-1,
    operation:{operation:'create_project'}}) as unknown as CatalogRequest;
const nativeMethod=(item:ReceiptCase)=>JSON.stringify((JSON.parse(item.control_request) as {method:unknown}).method);

test('catalog receipt fixture is the reviewed schema',()=>{
    assert.equal(receipts.schema,'airk.wire-fixture.catalog-receipt/1');
    assert.ok(receipts.receipts.length>0,'zero selected receipt cases is a failure');
});

test('catalog receipt: extension method object equals the native method (parsed equality)',()=>{
    const item=receiptCase('small_revision'),request=catalogRequest(item);
    // Native deserialization accepts any key order, so this is P8 parsed equality, not P9 bytes.
    assert.deepEqual({catalog_receipt:{caller:request.caller,key:request.key}},JSON.parse(nativeMethod(item)));
});

test('catalog receipt: native response for the matching request is accepted',()=>{
    const item=receiptCase('small_revision');
    assert.equal(client().validateCatalogObservation(JSON.parse(item.response),catalogRequest(item),item.inputs.request_digest,'0f'.repeat(32)),true);
});

test('catalog receipt: an explicit null receipt without a fence is an unprepared observation',()=>{
    const item=receiptCase('small_revision');
    assert.equal(client().validateCatalogObservation(JSON.parse(receipts.response_absent),catalogRequest(item),item.inputs.request_digest,'0f'.repeat(32)),false);
});

test('catalog receipt divergence: integers above 2^53 cannot be echoed or accepted by the extension',()=>{
    const item=receiptCase('above_safe_integer'),request=catalogRequest(item);
    // JSON.parse rounds 2^53+1, so an echoed request key would name a different epoch.
    assert.match(item.control_request,/"epoch":9007199254740993\b/);
    assert.notEqual(String(request.key.epoch),'9007199254740993');
    // Native accepts revisions up to i64::MAX; the extension refuses non-safe integers (conservative).
    assert.throws(()=>client().validateCatalogObservation(JSON.parse(item.response),request,item.inputs.request_digest,'0f'.repeat(32)),/crossed_catalog_receipt/);
});

test('catalog receipt divergence: an absent receipt field is refused although native reads it as null',()=>{
    const item=receiptCase('small_revision');
    assert.throws(()=>client().validateCatalogObservation({status:'catalog_receipt'},catalogRequest(item),item.inputs.request_digest,'0f'.repeat(32)));
});

// Session protocol 1 (`airk service session`) retired with the split; protocol 2
// frames are checked in airkctlSession.test.ts against airkctl_session_frames.json.

// Top-level read-retry envelopes (split C8). The native test client
// (crates/airk-cli/tests/native_read_retry.rs) and the production emitters are
// checked against the same reviewed cases; `expected` is never computed by either.
type EnvelopeCase={name:string;rule:string;envelope:Record<string,unknown>;expected_scope:unknown;
    expected:{retry_after_ms:number;fresh_brokerage_required:boolean}|null};
const envelopes=JSON.parse(fs.readFileSync(path.join(fixtures,'read_retry_envelopes.json'),'utf8')) as {schema:string;cases:EnvelopeCase[]};

test('read-retry envelope fixture is the reviewed schema and is non-empty',()=>{
    assert.equal(envelopes.schema,'airk.wire-fixture.read-retry-envelopes/1');
    assert.ok(envelopes.cases.length>0,'zero selected envelope cases is a failure');
});

for(const item of envelopes.cases){
    test(`read-retry envelope: ${item.name}`,()=>{
        assert.equal(readRetryDelay(structuredClone(item.envelope),item.expected_scope??undefined)??null,item.expected?.retry_after_ms??null,item.rule);
    });
}

// Native reply outcome classes (split C8). The rules under test: the complete
// reply is retained, outcomeUnknown is true exactly for unknown and unvalidated
// replies, and every exchange is sent once; no class authorizes another write.
type OutcomeCase={name:string;rule:string;method:string;expected_status:string;transport:'reply'|'lost_after_send'|'not_sent';
    reply:Record<string,unknown>|null;control_response:boolean;class:'accepted'|'refused'|'unknown'|'unvalidated';code:string|null;
    validator?:{code?:string;message:string}};
const outcomes=JSON.parse(fs.readFileSync(path.join(fixtures,'native_outcomes.json'),'utf8')) as {schema:string;classes:string[];cases:OutcomeCase[]};

test('native outcome fixture is the reviewed schema and covers every class',()=>{
    assert.equal(outcomes.schema,'airk.wire-fixture.native-outcomes/1');
    assert.deepEqual([...new Set(outcomes.cases.map(item=>item.class))].sort(),[...outcomes.classes].sort());
    assert.deepEqual([...outcomes.classes].sort(),['accepted','refused','unknown','unvalidated']);
});

for(const item of outcomes.cases){
    test(`native outcome: ${item.name}`,async()=>{
        let sent=0;
        const client=new ServiceClient({dispose(){},async request(method:unknown){
            sent+=1;assert.deepEqual(method,{[item.method]:{}});
            if(item.transport==='reply'&&item.reply)return structuredClone(item.reply);
            throw new ServiceError(String(item.code),item.transport==='lost_after_send');
        }});
        let response:Record<string,unknown>|undefined,failure:unknown;
        try{response=await client.rpc({[item.method]:{}},item.expected_status);}catch(error){failure=error;}
        assert.equal(sent,1,'every class sends the exchange exactly once');
        if(item.class==='accepted'||item.class==='unvalidated'){
            assert.equal(failure,undefined,item.rule);
            assert.ok(response);assert.deepEqual(response,item.reply,'the complete reply is returned unchanged');
        }
        if(item.class==='accepted'){
            assert.ok(response);
            // A local tracking failure keeps the reply and marks only local tracking unconfirmed.
            const local=retainNativeOutcome(new Error('journal_write_failed'),response);
            assert.ok(isLocalTrackingFailure(local));assert.equal(local.code,'journal_write_failed');
            assert.equal(local.outcomeUnknown,true);assert.equal(local.nativeResponse,response);
            assert.equal(retainNativeOutcome(local,structuredClone(response)),local,'an equal reply is not wrapped twice');
            return;
        }
        if(item.class==='unvalidated'){
            assert.ok(response);const validator=item.validator;assert.ok(validator,'an unvalidated case states its validator failure');
            const stated=Object.assign(new Error(validator.message),validator.code===undefined?{}:{code:validator.code});
            let rejected:unknown;
            try{validateNativeReply(response,()=>{throw stated;});}catch(error){rejected=error;}
            assert.ok(isNativeReplyValidationFailure(rejected),item.rule);
            assert.equal(rejected.code,item.code);assert.equal(rejected.outcomeUnknown,true);
            assert.equal(rejected.nativeResponse,response,'the complete reply is retained');assert.equal(rejected.cause,stated);
            assert.equal(retainUnvalidatedNativeReply(rejected,response),rejected);
            assert.equal(retainNativeOutcome(rejected,response),rejected,'an unvalidated reply is not relabelled as local tracking');
            assert.equal(isLocalTrackingFailure(rejected),false);
            return;
        }
        assert.ok(failure instanceof ServiceError,item.rule);
        assert.equal(failure.code,item.code);
        assert.equal(failure.outcomeUnknown,item.class==='unknown',item.rule);
        if(item.reply)assert.deepEqual(failure.nativeResponse,item.reply,'the complete reply is retained');
        else assert.equal(failure.nativeResponse,undefined);
    });
}


// Active replay binding uses the same independent original-authority/arguments
// cases as the Core validator, beyond the standalone semantic classifier above.
type BindingCase={name:string;rule:string;kind:string;input:Record<string,unknown>;arguments:Record<string,unknown>;
    expected_scope:unknown;expected_selection?:{owner:string;scope_identity:string;catalog_revision:number};expected:{retry_after_ms:number}|null};
const bindingCases=(decisions as typeof decisions&{binding_cases:BindingCase[]}).binding_cases;
test('active read-retry binding fixture has independent cases',()=>{
    assert.ok(Array.isArray(bindingCases)&&bindingCases.length>0,'zero active binding cases is a failure');
});
for(const item of bindingCases??[]){
    test(`active read-retry binding: ${item.name}`,()=>{
        const scope=item.kind==='selection'?{window_scope_selection:item.expected_scope,scope_identity:item.expected_selection?.scope_identity}:item.expected_scope;
        const envelope={isError:true,content:[{type:'text',text:JSON.stringify(item.input)}],structuredContent:item.input};
        const classify=readRetryDelay as (value:unknown,expected?:unknown,args?:Record<string,unknown>,source?:unknown)=>number|undefined;
        assert.equal(classify(structuredClone(envelope),structuredClone(scope),structuredClone(item.arguments),structuredClone(item.expected_selection))??null,
            item.expected?.retry_after_ms??null,item.rule);
    });
}
