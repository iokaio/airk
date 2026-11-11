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
import {MAX_IN_FLIGHT,MAX_INVOKE_ARGS,MAX_INVOKE_ARG_BYTES,MAX_INVOKE_DEADLINE_MS,MAX_INVOKE_STDERR_BYTES,MAX_INVOKE_STDIN_BYTES,MAX_REQUEST_FRAME_BYTES,MAX_RESPONSE_FRAME_BYTES,SESSION_PROTOCOL,checkSessionRequest,decodeSessionResponse,encodeSessionRequest,sessionRequestJson,sessionRequests} from '../nativeSession';
import {NativeProcessError} from '../native';

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

// Per-window native session frames. The reviewed Rust definition
// (crates/airk-cli/src/session_protocol.rs) round-trips the same cases.
type SessionCase={name:string;json:Record<string,any>};
const session=JSON.parse(fs.readFileSync(path.join(fixtures,'session_frames.json'),'utf8')) as {schema:string;protocol:number;budgets:Record<string,number>;requests:SessionCase[];invalid_requests:SessionCase[];responses:SessionCase[]};
const sessionCases=(key:'requests'|'invalid_requests'|'responses')=>{assert.ok(session[key].length>0,`${key}: zero selected session cases is a failure`);return session[key];};

test('session frame fixture is the reviewed schema and protocol',()=>{
    assert.equal(session.schema,'airk.wire-fixture.session-frames/1');
    assert.equal(session.protocol,SESSION_PROTOCOL);
});

test('session requests: the extension encoder produces exactly each reviewed request',()=>{
    // Inputs are the fixture's own values; the expected bytes are the fixture.
    const build=(item:SessionCase)=>{
        const [kind]=Object.keys(item.json.request),body=item.json.request[kind];
        switch(kind){
            case 'hello':return sessionRequests.hello();
            case 'control':return sessionRequests.control(body.runtime_dir,body.method,body.runtime_sha256);
            case 'invoke':return sessionRequests.invoke(body.argv,body.deadline_ms,body.stdin);
            case 'cancel':return sessionRequests.cancel(body.target);
            default:throw new Error(`unreviewed session request ${kind}`);
        }
    };
    const kinds=new Set<string>();
    for(const item of sessionCases('requests')){
        kinds.add(Object.keys(item.json.request)[0]);
        const encoded=sessionRequestJson(item.json.id,build(item));
        assert.equal(encoded,JSON.stringify(item.json),item.name);
        const frame=encodeSessionRequest(item.json.id,build(item));
        assert.equal(frame.readUInt32BE(),frame.length-4,item.name);assert.equal(frame.subarray(4).toString('utf8'),encoded,item.name);
    }
    assert.deepEqual([...kinds].sort(),['cancel','control','hello','invoke'],'every request kind has a reviewed case');
});

test('session requests: reviewed invalid requests are never produced or sent',()=>{
    for(const item of sessionCases('requests'))assert.doesNotThrow(()=>checkSessionRequest(item.json),item.name);
    for(const item of sessionCases('invalid_requests')){
        assert.throws(()=>checkSessionRequest(item.json),/native_session_invalid_request/,item.name);
        const request=item.json.request;
        // The encoder only ever writes {id, request}; other top-level shapes cannot be produced.
        if(Object.keys(item.json).sort().join(',')==='id,request'&&request&&typeof request==='object'&&typeof item.json.id==='string')
            assert.throws(()=>encodeSessionRequest(item.json.id,request),/native_session_invalid_request/,item.name);
    }
});

test('session responses: every reviewed response decodes to its exact value',()=>{
    for(const item of sessionCases('responses'))assert.deepEqual(decodeSessionResponse(structuredClone(item.json)),item.json,item.name);
    const failure=sessionCases('responses').find(item=>item.json.outcome.status==='invoked'&&item.json.outcome.success===false);
    assert.ok(failure,'a reviewed failed invocation is present');
    const error=NativeProcessError.fromSession('airk',failure.json.outcome.stderr,1,60000);
    assert.equal(error.nativeMessage,'fixture');assert.equal(error.retryableGateContention,false);
    for(const item of sessionCases('responses'))if(item.json.outcome.status==='failed')assert.equal(item.json.outcome.retryable,item.json.outcome.code==='session_busy',`${item.name}: only session_busy is retryable`);
});

test('session budgets: every extension constant equals the reviewed native budget',()=>{
    assert.deepEqual({max_request_frame_bytes:MAX_REQUEST_FRAME_BYTES,max_invoke_stdin_bytes:MAX_INVOKE_STDIN_BYTES,max_invoke_args:MAX_INVOKE_ARGS,
        max_invoke_arg_bytes:MAX_INVOKE_ARG_BYTES,max_response_frame_bytes:MAX_RESPONSE_FRAME_BYTES,max_invoke_stderr_bytes:MAX_INVOKE_STDERR_BYTES,
        max_in_flight:MAX_IN_FLIGHT,max_invoke_deadline_ms:MAX_INVOKE_DEADLINE_MS},session.budgets,'exact key set and values');
    assert.equal(Object.keys(session.budgets).length,8);
});
