// SPDX-License-Identifier: Apache-2.0
const {test}=require('node:test');
const assert=require('node:assert/strict');
const {withFailureStop}=require('../native-editor-failure.cjs');

test('successful client leaves the ordinary final Stop owner in control',async()=>{
    assert.equal(await withFailureStop({run:async()=>42,stop:async()=>assert.fail('unexpected Stop')}),42);
});

test('failed client publishes exactly the real one-dispatch Stop receipt and rethrows',async()=>{
    const original=new Error('client TLS failed'),receipt={capture:{response:{record:{id:'operation'}}}};
    const events=[];
    await assert.rejects(withFailureStop({run:async()=>{throw original;},
        stop:async()=>{events.push('stop');return receipt;},
        publishStop:async value=>{assert.equal(value,receipt);events.push('publish');},
        retainFailure:async value=>{assert.equal(value.stop_receipt_published,true);events.push('retain');}}),error=>error===original);
    assert.deepEqual(events,['stop','publish','retain']);
});

for(const phase of ['stop','publish'])test(`failed ${phase} is retained without replay or a manufactured receipt`,async()=>{
    const original=new Error('client failed'),cleanup=new Error(`${phase} uncertain`);
    let stops=0,publishes=0,retained;
    await assert.rejects(withFailureStop({run:async()=>{throw original;},
        stop:async()=>{stops++;if(phase==='stop')throw cleanup;return {actual:true};},
        publishStop:async()=>{publishes++;throw cleanup;},
        retainFailure:async value=>{retained=value;}}),error=>error===original);
    assert.equal(stops,1);assert.equal(publishes,phase==='stop'?0:1);
    assert.equal(retained.stop_receipt_published,false);assert.equal(retained.cleanup_error,String(cleanup));
});

test('retention failure keeps both the original error and evidence failure',async()=>{
    const original=new Error('client failed'),retention=new Error('disk unavailable');
    await assert.rejects(withFailureStop({run:async()=>{throw original;},stop:async()=>({}),
        publishStop:async()=>{},retainFailure:async()=>{throw retention;}}),
        error=>error instanceof AggregateError&&error.errors[0]===original&&error.errors[1]===retention);
});
