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
