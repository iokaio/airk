// SPDX-License-Identifier: Apache-2.0
import {strict as assert} from 'node:assert';
import {test} from 'node:test';
import {ServiceError as PublicServiceError} from '../serviceClient';
import {ServiceError,retainNativeOutcome,retainNativeOutcomeDuring,retainUnvalidatedNativeReply,isLocalTrackingFailure,isNativeReplyValidationFailure} from '../nativeOutcome';

test('native outcome errors preserve the public class and complete reply with an idempotent original local cause',async()=>{
    assert.equal(PublicServiceError,ServiceError);
    const reply={status:'catalog_applied',receipt:{future_native:{values:[null,{untouched:true}]}},future_envelope:['all','fields']},cause=new ServiceError('private_intent_write_failed');
    const once=retainNativeOutcome(cause,reply),twice=retainNativeOutcome(once,structuredClone(reply));assert.equal(twice,once);assert.equal(twice.cause,cause);assert.equal(twice.code,cause.code);assert.deepEqual(twice.nativeResponse,reply);assert.equal(twice.outcomeUnknown,true);assert.equal(isLocalTrackingFailure(twice),true);
    let dispatched=0;await assert.rejects(retainNativeOutcomeDuring(reply,async()=>{dispatched++;throw cause;}),(error:unknown)=>{assert.ok(error instanceof ServiceError);assert.deepEqual(error.nativeResponse,reply);assert.equal(error.cause,cause);return true;});assert.equal(dispatched,1);
});

test('diagnostic raw replies remain unaccepted while a distinct earlier accepted reply survives through its cause',()=>{
    const malformed={status:'job',job:{id:17},future_malformed:{preserved:true}},cause=new Error('Invalid AIRK service identity.'),invalid=retainUnvalidatedNativeReply(cause,malformed);
    assert.equal(isNativeReplyValidationFailure(invalid),true);assert.equal(isLocalTrackingFailure(invalid),false);assert.equal(retainNativeOutcome(invalid,structuredClone(malformed)),invalid);assert.equal(invalid.cause,cause);
    const accepted={status:'job',job:{id:'11'.repeat(32),project:'22'.repeat(32)},future_acknowledgment:true},outer=retainNativeOutcome(invalid,accepted);
    assert.equal(isLocalTrackingFailure(outer),true);assert.deepEqual(outer.nativeResponse,accepted);assert.equal(outer.cause,invalid);assert.deepEqual((outer.cause as ServiceError).nativeResponse,malformed);
});

test('retained validation replies preserve machine-code errors without promoting free text to a code',()=>{
    const reply={status:'invalid',future_diagnostic:{retained:true}};
    for(const [cause,expected] of [
        [new Error('catalog_response_oversize'),'catalog_response_oversize'],
        [new Error('crossed_split_recovery_receipt'),'crossed_split_recovery_receipt'],
        [new Error('Private path: C:/user/source'),'native_response_validation_failed'],
        [new ServiceError('crossed_catalog_outcome_unknown'),'crossed_catalog_outcome_unknown'],
    ] as const){
        const retained=retainUnvalidatedNativeReply(cause,reply);
        assert.equal(retained.code,expected);assert.equal(retained.cause,cause);
        assert.equal(retained.nativeResponse,reply);assert.equal(isNativeReplyValidationFailure(retained),true);
        assert.equal(isLocalTrackingFailure(retained),false);
    }
});
