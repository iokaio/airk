// SPDX-License-Identifier: Apache-2.0
import {strict as assert} from 'node:assert';
import {test} from 'node:test';
import {readRetryDelay} from './nativeReadRetry';

test('native read fixtures require the complete safe replay contract',()=>{
    const wrap=(hint:unknown)=>({isError:true,content:[{type:'text',text:JSON.stringify(hint)}]});
    const hint={status:'retry',code:'publication_busy',retryable:true,action:'repeat_same_request',retry_after_ms:25};
    assert.equal(readRetryDelay(wrap(hint)),25);
    for(const changed of [{code:'unknown_busy'},{retryable:false},{action:'refresh_revision'},{retry_after_ms:0},{retry_after_ms:1001}]){
        assert.equal(readRetryDelay(wrap({...hint,...changed})),undefined);
    }
    assert.equal(readRetryDelay({...wrap(hint),isError:false}),undefined);
    for(const code of ['direct_query_admission','direct_query_setup']){
        const error={code,reason:'busy',query_submitted:false,retryable:true,retry_after_ms:100,fresh_brokerage_required:code==='direct_query_setup'};
        assert.equal(readRetryDelay(wrap({error})),100);
        for(const changed of [{query_submitted:true},{retryable:false},{fresh_brokerage_required:!error.fresh_brokerage_required}]){
            assert.equal(readRetryDelay(wrap({error:{...error,...changed}})),undefined);
        }
    }
});
