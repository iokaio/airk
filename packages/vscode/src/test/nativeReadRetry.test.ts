// SPDX-License-Identifier: Apache-2.0
import {strict as assert} from 'node:assert';
import {test} from 'node:test';
import {readRetryDelay} from './nativeReadRetry';
import {publishedStatusPending} from './nativeServiceFixture';

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

test('publication fixtures reject unhealthy, foreign and incomplete worker observations',()=>{
    const value={project:'a',index_state:'unavailable',publication:null,files:null,chunks:null,
        index_error:{code:'index_worker_unavailable',message:'No query submitted'},
        runtime:{service_error:null,service:{responsive:true,accepting:true,instance:'owner'},index_worker:{process:null}},
        work:{basis:'instance_bound_service_metadata',serving_worker_unavailable:true,failed_observations:[],worker:{automatic_start_blocked:false,identity:null}}};
    assert.equal(publishedStatusPending(value,'collection-a','a'),true);
    for(const change of [
        {project:'b'},
        {index_error:{code:'unknown',message:'unknown'}},
        {runtime:{...value.runtime,service:null}},
        {runtime:{...value.runtime,service:{...value.runtime.service,responsive:false}}},
        {work:{...value.work,failed_observations:['worker_sample_unavailable_or_wrong_project']}},
        {work:{...value.work,worker:{automatic_start_blocked:true,identity:null}}},
        {work:{...value.work,worker:{automatic_start_blocked:false,identity:{project:'b',service:'owner'}}}},
        {files:0},
    ])assert.throws(()=>publishedStatusPending({...value,...change},'collection-a','a'));
    const workerSide={...value,collection:'collection-a',index_error:{code:'index_status_unavailable',message:'private-source-message'}};
    assert.equal(publishedStatusPending(workerSide,'collection-a','a'),true);
    assert.throws(()=>publishedStatusPending({...workerSide,work:{...value.work,worker:{automatic_start_blocked:true,identity:null}}},'collection-a','a'),error=>!String(error).includes('private-source-message'));
    assert.throws(()=>publishedStatusPending({...workerSide,runtime:[]},'collection-a','a'));
    const published={collection:'collection-a',index_state:'available',publication:{collection:'collection-a',token:'exact'},files:1};
    assert.equal(publishedStatusPending(published,'collection-a','a'),false);
    assert.throws(()=>publishedStatusPending({...published,collection:'b'},'collection-a','a'));
});
